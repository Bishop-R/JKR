//! The portable assault sentry (OpenJK `codemp/game/g_items.c:538-1131`: `ItemUse_Sentry`,
//! `SP_PAS`, `pas_think` and their helpers; `G_RunItem`, `G_BounceItem` and `G_RunObject`
//! for its physics).
//!
//! Its owner sets it down 64 units ahead along their yaw, at their origin's height. It is
//! a `physicsObject` under gravity, but its own move's trace passes nobody, not even the
//! sentry, so its own solid box holds it where it was set down: it falls only while a
//! client stands inside it and it has gone non-solid. Once awake it fires at the nearest
//! client within 800 units it can see, other than its owner, its owner's team and vehicles:
//! after a wind-up of about a second, a bolt of ten every 0.3 seconds (it thinks every
//! tenth of one, and its 200 ms debounce is compared strictly). Thirty-nine bolts, or a
//! minute after it was set down, and it shuts down; it blows up two seconds later (thirty
//! within 256 units, credited to its owner). It is freed without a blast when its owner
//! leaves or changes teams. The clients turn its gun themselves
//! (`TurretClientRun`); the entity tells them its enemy (`bolt2`) and whether it fires
//! (`fireflag`: 1 firing, 2 shut down).
//!
//! Left out: `pas_think`'s own aim (`speed`, `random`), which changes nothing but a loop
//! sound nothing sets; `G_RunObject`'s push triggers after a shot.

use crate::item_physics::{TR_GRAVITY, TR_STATIONARY};
use crate::map_turret_world::{
    CONTENTS_BODY, CONTENTS_LIGHTSABER, DAMAGE_DEATH_KNOCKBACK, ET_GENERAL, FRAMETIME, MASK_SHOT,
    Shot, Sighted, TurretHost, WP_TURRET, body_sound, es, find, length_squared, play_effect, sub,
};
use crate::means_of_death::{MOD_SENTRY, MOD_UNKNOWN};
use jkr_protocol::{EntityState, LEGACY_ENTITY_FIELDS};

/// `TURRET_RADIUS`, `TURRET_LIFETIME`, `TURRET_DEATH_DELAY`, `TURRET_AMMO_COUNT`.
const RADIUS: f32 = 800.0;
const LIFETIME: i32 = 60_000;
const DEATH_DELAY: i32 = 2_000;
const AMMO: i32 = 40;
/// Its box (`r.mins`, `r.maxs`) and its health.
pub const BOUNDS: ([f32; 3], [f32; 3]) = ([-8.0, -8.0, 0.0], [8.0, 8.0, 24.0]);
const HEALTH: i32 = 50;
/// `MASK_SOLID` (its clip mask), `CONTENTS_SOLID`.
const MASK_SOLID: u32 = 0x1;
const CONTENTS_SOLID: u32 = 0x1;
/// `MAX_CLIENTS`, `ENTITYNUM_NONE`.
const MAX_CLIENTS: u16 = 32;
const ENTITYNUM_NONE: u16 = 1_023;
/// `GT_TEAM`.
const GT_TEAM: i32 = 6;
/// `EFFECT_EXPLOSION_PAS`.
const EFFECT_EXPLOSION_PAS: u32 = 3;
/// What a sentry's bolt is: its speed, damage and life (`WP_FireTurretMissile`).
const BOLT_SPEED: f32 = 2_300.0;
const BOLT_DAMAGE: i32 = 10;
const BOLT_LIFE: i32 = 10_000;

/// What the sentry thinks next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Think {
    /// `pas_think`.
    Watch,
    /// `sentryExpire`: shut down, it blows up.
    Expire,
    /// `G_FreeEntity`: its owner is gone.
    Free,
}

/// A deployed sentry: its wire state and the fields of it the reference reads.
#[derive(Clone, Debug, PartialEq)]
pub struct Sentry {
    /// Its wire state (`s`), numbered once the caller has a place for it.
    pub state: EntityState,
    /// `r.currentOrigin`.
    pub origin: [f32; 3],
    /// `genericValue3` and `genericValue2` (also `alliedTeam`): who set it down, and their
    /// team then.
    pub owner: u16,
    pub owner_team: i32,
    pub health: i32,
    pub takes_damage: bool,
    /// `r.contents`: solid, but nothing while a client stands inside it.
    pub contents: u32,
    /// `count`: forty at first; each shot takes one, and the one that would leave none
    /// shuts it down instead.
    pub count: i32,
    pub enemy: Option<u16>,
    /// `damage`: its first think is spent waking.
    pub awake: bool,
    /// `genericValue8`: when it was set down.
    pub placed_at: i32,
    pub think: Think,
    pub next_think: i32,
    /// `attackDebounceTime`, `painDebounceTime`, `aimDebounceTime`, `bounceCount`.
    pub attack_debounce: i32,
    pub pain_debounce: i32,
    pub aim_debounce: i32,
    pub bounce_count: i32,
    /// `G_FreeEntity` has taken it.
    pub freed: bool,
}

/// The owner as a sentry reads them: a client still in the game, and their team now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Owner {
    pub team: i32,
}

/// What a sentry's frame or blow left for the caller.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// The owner's `ps.fd.sentryDeployed` is to be cleared (`turret_die`, `turret_free`).
    pub owner_undeployed: bool,
}

/// `ItemUse_Sentry` and `SP_PAS` (`g_items.c:1014-1131`): the sentry `owner` sets down 64
/// units ahead of `origin` along `yaw`, its startup sound played. The caller marks its
/// owner `sentryDeployed` and numbers its state.
pub fn place(
    owner: u16,
    origin: [f32; 3],
    yaw: f32,
    owner_team: i32,
    gametype: i32,
    level_time: i32,
    host: &mut dyn TurretHost,
) -> Sentry {
    let forward = crate::pmove::flight::flight_axes([0.0, yaw, 0.0])
        .0
        .to_array();
    let at: [f32; 3] = std::array::from_fn(|axis| origin[axis] + forward[axis] * 64.0);
    let mut state = EntityState::zero(0, &LEGACY_ENTITY_FIELDS);
    let model = host.model_index(b"models/items/psgun.glm");
    state.set_raw_field(es::MODEL_INDEX, u32::from(model));
    state.set_raw_field(es::G2_RADIUS, 30);
    state.set_raw_field(es::MODEL_GHOUL2, 1);
    state.set_raw_field(es::TYPE, ET_GENERAL);
    // `G_SetOrigin`, then a fall from there.
    set3(&mut state, es::POS_BASE, at);
    state.set_raw_field(es::POS_TYPE, TR_GRAVITY);
    state.set_raw_field(es::POS_TIME, level_time as u32);
    state.set_raw_field(es::OWNER, u32::from(owner));
    state.set_raw_field(es::SHOULD_TARGET, 1);
    let team_owner = if gametype >= GT_TEAM { owner_team } else { 16 };
    state.set_raw_field(es::TEAM_OWNER, team_owner as u32);
    // "a sort of hack to indicate that this model needs special turret things done to it"
    state.set_raw_field(es::BOLT1, 1);
    state.set_raw_field(es::BOLT2, u32::from(ENTITYNUM_NONE));
    let mut sentry = Sentry {
        state,
        origin: at,
        owner,
        owner_team,
        health: HEALTH,
        takes_damage: true,
        contents: CONTENTS_SOLID,
        count: AMMO,
        enemy: None,
        awake: false,
        placed_at: level_time,
        think: Think::Watch,
        next_think: level_time + FRAMETIME,
        attack_debounce: 0,
        pain_debounce: 0,
        aim_debounce: 0,
        bounce_count: 0,
        freed: false,
    };
    link(&mut sentry);
    body_sound(host, at, b"sound/chars/turret/startup.wav");
    sentry
}

/// `G_RunItem` for sentry `me` (`g_items.c:3218-3279`), at `level_time` after the frame
/// at `previous_time`: falling, the move from where it was — through the host's box trace,
/// which passes nobody and so meets the sentry's own box while it is solid — its think
/// when due, and where the move stopped, its bounce; a `CONTENTS_NODROP` volume frees it
/// (`turret_free`). `owner` is its owner while they are in the game.
#[allow(clippy::too_many_arguments)]
pub fn run(
    sentry: &mut Sentry,
    me: u16,
    owner: Option<Owner>,
    targets: &[Sighted],
    level_time: i32,
    previous_time: i32,
    host: &mut dyn TurretHost,
) -> Outcome {
    let moved = crate::item_physics::fall(
        &mut sentry.state,
        &mut sentry.origin,
        level_time,
        |from, to| host.trace_box(from, BOUNDS, to, ENTITYNUM_NONE, MASK_SOLID),
    );
    let crate::item_physics::ItemMove::Moved(moved) = moved else {
        return think(sentry, me, owner, targets, level_time, previous_time, host);
    };
    link(sentry);
    let outcome = think(sentry, me, owner, targets, level_time, previous_time, host);
    if sentry.freed {
        return outcome;
    }
    let bounced = crate::item_physics::bounce(
        &mut sentry.state,
        &mut sentry.origin,
        &moved,
        level_time,
        previous_time,
        |point| host.point_contents(point, ENTITYNUM_NONE),
    );
    if bounced == crate::item_physics::ItemBounce::NoDrop {
        // `turret_free`.
        sentry.freed = true;
        return Outcome {
            owner_undeployed: owner.is_some(),
        };
    }
    outcome
}

/// `G_RunThink`: the sentry's think, if due.
fn think(
    sentry: &mut Sentry,
    me: u16,
    owner: Option<Owner>,
    targets: &[Sighted],
    level_time: i32,
    previous_time: i32,
    host: &mut dyn TurretHost,
) -> Outcome {
    if sentry.next_think <= 0 || sentry.next_think > level_time {
        return Outcome::default();
    }
    sentry.next_think = 0;
    match sentry.think {
        Think::Watch => {
            watch(sentry, me, owner, targets, level_time, previous_time, host);
            Outcome::default()
        }
        Think::Expire => die(sentry, me, owner, host),
        Think::Free => {
            sentry.freed = true;
            Outcome::default()
        }
    }
}

/// `pas_think` (`g_items.c:731-961`).
fn watch(
    sentry: &mut Sentry,
    me: u16,
    owner: Option<Owner>,
    targets: &[Sighted],
    level_time: i32,
    previous_time: i32,
    host: &mut dyn TurretHost,
) {
    // A client inside it, four units in from its sides: it goes non-solid to let them out.
    let inner = (
        std::array::from_fn(|axis| sentry.origin[axis] + BOUNDS.0[axis] + 4.0),
        std::array::from_fn(|axis| sentry.origin[axis] + BOUNDS.1[axis] - 4.0),
    );
    let trapped = targets
        .iter()
        .any(|target| target.number < MAX_CLIENTS && overlaps(target.bounds, inner));
    if trapped {
        sentry.contents = 0;
        sentry.state.set_raw_field(es::FIREFLAG, 0);
        sentry.next_think = level_time + FRAMETIME;
        return;
    }
    sentry.contents = CONTENTS_SOLID;
    if owner.is_none_or(|owner| owner.team != sentry.owner_team) {
        sentry.think = Think::Free;
        sentry.next_think = level_time;
        return;
    }
    if !sentry.awake {
        sentry.awake = true;
        sentry.next_think = level_time + FRAMETIME;
        return;
    }
    if sentry.placed_at + LIFETIME < level_time {
        shut_down(sentry, level_time, host);
        return;
    }
    sentry.next_think = level_time + FRAMETIME;
    if sentry.enemy.is_some() {
        adjust_enemy(sentry, me, targets, level_time, host);
    }
    if sentry.enemy == Some(me)
        || find(targets, sentry.enemy).is_none_or(|enemy| !enemy.client || enemy.health < 1)
    {
        sentry.enemy = None;
    }
    if sentry.enemy.is_none() {
        find_enemies(sentry, me, targets, level_time, host);
    }
    let bolt2 = sentry.enemy.unwrap_or(ENTITYNUM_NONE);
    sentry.state.set_raw_field(es::BOLT2, u32::from(bolt2));
    match find(targets, sentry.enemy) {
        Some(enemy) if sentry.attack_debounce < level_time => {
            sentry.count -= 1;
            if sentry.count != 0 {
                let at = enemy.origin;
                fire(sentry, me, at, level_time, previous_time, host);
                sentry.state.set_raw_field(es::FIREFLAG, 1);
                sentry.attack_debounce = level_time + 200;
            } else {
                shut_down(sentry, level_time, host);
            }
        }
        _ => {
            sentry.state.set_raw_field(es::FIREFLAG, 0);
        }
    }
}

/// Out of shots or time: the shutdown sound, no enemy, the shut-down flag, and its end
/// two seconds on (`sentryExpire`).
fn shut_down(sentry: &mut Sentry, level_time: i32, host: &mut dyn TurretHost) {
    body_sound(host, sentry.origin, b"sound/chars/turret/shutdown.wav");
    sentry
        .state
        .set_raw_field(es::BOLT2, u32::from(ENTITYNUM_NONE));
    sentry.state.set_raw_field(es::FIREFLAG, 2);
    sentry.think = Think::Expire;
    sentry.next_think = level_time + DEATH_DELAY;
}

/// `pas_find_enemies` (`g_items.c:570-662`): of the clients within its radius
/// (`G_RadiusList` from where it was set down), the nearest it can see that may be hurt —
/// winding up, with its startup sound, where it has been quiet; while it is still alert from
/// an enemy it lost, it pings once a second.
fn find_enemies(
    sentry: &mut Sentry,
    me: u16,
    targets: &[Sighted],
    level_time: i32,
    host: &mut dyn TurretHost,
) {
    if sentry.aim_debounce > level_time && sentry.pain_debounce < level_time {
        body_sound(host, sentry.origin, b"sound/chars/turret/ping.wav");
        sentry.pain_debounce = level_time + 1_000;
    }
    let from = base(sentry);
    let mut best = RADIUS * RADIUS;
    for target in targets {
        if target.number == me
            || !target.takes_damage
            || distance_to_box(from, target.bounds) >= RADIUS
            || !target.client
            || target.health <= 0
            || target.no_target
            || (sentry.owner_team != 0 && target.session_team == sentry.owner_team)
            || target.number == sentry.owner
            || !host.in_pvs(from, target.origin)
            || target.vehicle
        {
            continue;
        }
        let sight = host.trace(from, target.origin, me, MASK_SHOT);
        if sight.all_solid
            || sight.start_solid
            || !(sight.fraction == 1.0 || sight.entity_number == target.number)
        {
            continue;
        }
        let distance = length_squared(sub(target.origin, sentry.origin));
        if distance < best {
            if sentry.attack_debounce + 100 < level_time {
                body_sound(host, sentry.origin, b"sound/chars/turret/startup.wav");
                // `level.time + 900 + Q_flrand(0.0f, 1.0f) * 200`, a float.
                let wind = host.rng().flrand(0.0, 1.0) * 200.0;
                sentry.attack_debounce = ((level_time + 900) as f32 + wind) as i32;
            }
            sentry.enemy = Some(target.number);
            best = distance;
        }
    }
}

/// `pas_adjust_enemy` (`g_items.c:665-718`): an enemy dead, or no longer in its sight, is
/// let go — not again within about half a second — with the shutdown sound and five
/// seconds of pinging.
fn adjust_enemy(
    sentry: &mut Sentry,
    me: u16,
    targets: &[Sighted],
    level_time: i32,
    host: &mut dyn TurretHost,
) {
    let keep = match find(targets, sentry.enemy) {
        Some(enemy) if enemy.health > 0 => {
            let mut at = enemy.origin;
            if enemy.client {
                at[2] -= 15.0;
            }
            let sight = host.trace(base(sentry), at, me, MASK_SHOT);
            !((sight.all_solid
                || sight.start_solid
                || sight.fraction < 0.9
                || sight.entity_number == me)
                && sight.entity_number != enemy.number)
        }
        _ => false,
    };
    if !keep && sentry.bounce_count < level_time && sentry.enemy.is_some() {
        sentry.enemy = None;
        body_sound(host, sentry.origin, b"sound/chars/turret/shutdown.wav");
        // `level.time + 500 + Q_flrand(0.0f, 1.0f) * 150`, a float.
        let wait = host.rng().flrand(0.0, 1.0) * 150.0;
        sentry.bounce_count = ((level_time + 500) as f32 + wait) as i32;
        sentry.aim_debounce = level_time + 5_000;
    }
}

/// `pas_fire` (`g_items.c:544-566`): a turret bolt (`WP_FireTurretMissile`, the owner's)
/// from 24 units up toward the same height on the enemy at `enemy`, 16 units along the
/// way, passing through the sentry. `G_RunObject` after it puts a sentry at rest back under
/// gravity; its own box holds it still.
fn fire(
    sentry: &mut Sentry,
    me: u16,
    enemy: [f32; 3],
    level_time: i32,
    previous_time: i32,
    host: &mut dyn TurretHost,
) {
    let mut from = sentry.origin;
    from[2] += 24.0;
    let mut at = enemy;
    at[2] += 24.0;
    let mut forward = sub(at, from);
    crate::player_angle_math::normalize(&mut forward);
    // `CreateMissile` snaps its start.
    let start = crate::weapon_fire::snap_vector(std::array::from_fn(|axis| {
        from[axis] + forward[axis] * 16.0
    }));
    let mut missile = crate::weapon_fire::create_missile_by(
        sentry.owner,
        start,
        forward,
        BOLT_SPEED,
        level_time,
        false,
    );
    missile.state.set_raw_field(es::WEAPON, WP_TURRET);
    missile.free_at = level_time + BOLT_LIFE;
    missile.damage = BOLT_DAMAGE;
    missile.method_of_death = MOD_SENTRY;
    missile.clip_mask = MASK_SHOT | CONTENTS_LIGHTSABER;
    missile.pass_through = Some(me);
    missile.bounce_count = 8;
    host.launch(Shot {
        missile,
        dflags: DAMAGE_DEATH_KNOCKBACK,
        parentless: false,
    });
    if pos_type(sentry) == TR_STATIONARY {
        sentry.state.set_raw_field(es::POS_TYPE, TR_GRAVITY);
        set3(&mut sentry.state, es::POS_BASE, sentry.origin);
        sentry
            .state
            .set_raw_field(es::POS_TIME, previous_time as u32);
    }
}

/// `turret_die` (`g_items.c:963-996`), sentry `me`'s death: with its owner gone it is only
/// freed; else it blows up where it was set down — the effect, thirty within 256 units its
/// owner's — and its owner may set down another.
pub fn die(
    sentry: &mut Sentry,
    me: u16,
    owner: Option<Owner>,
    host: &mut dyn TurretHost,
) -> Outcome {
    sentry.freed = true;
    if owner.is_none() {
        return Outcome::default();
    }
    sentry.takes_damage = false;
    sentry.health = 0;
    let at = base(sentry);
    play_effect(host, EFFECT_EXPLOSION_PAS, at, [0.0, 0.0, 1.0]);
    host.radius_damage(at, Some(sentry.owner), 30.0, 256.0, Some(me), MOD_UNKNOWN);
    Outcome {
        owner_undeployed: true,
    }
}

/// `G_Damage`'s last part on sentry `me` (after [`crate::map_turret_world::object_take`]):
/// `take` off its health, and at none its death.
pub fn damage(
    sentry: &mut Sentry,
    me: u16,
    take: i32,
    owner: Option<Owner>,
    host: &mut dyn TurretHost,
) -> Outcome {
    sentry.health -= take;
    if sentry.health <= 0 {
        return die(sentry, me, owner, host);
    }
    Outcome::default()
}

/// `trap->LinkEntity`'s part the clients see: `s.solid`, its box packed for their
/// prediction while it is solid.
fn link(sentry: &mut Sentry) {
    let solid = if sentry.contents & (CONTENTS_SOLID | CONTENTS_BODY) != 0 {
        crate::entity_clip::packed_solid(BOUNDS)
    } else {
        0
    };
    sentry.state.set_raw_field(es::SOLID, solid);
}

/// `s.pos.trType`.
fn pos_type(sentry: &Sentry) -> u32 {
    read(&sentry.state, es::POS_TYPE)
}

/// `s.pos.trBase`: where it was set down, or came to rest.
fn base(sentry: &Sentry) -> [f32; 3] {
    read3(&sentry.state, es::POS_BASE)
}

fn read(state: &EntityState, field: usize) -> u32 {
    state.raw_field(field).unwrap_or(0)
}

fn read3(state: &EntityState, fields: [usize; 3]) -> [f32; 3] {
    fields.map(|field| f32::from_bits(read(state, field)))
}

fn set3(state: &mut EntityState, fields: [usize; 3], value: [f32; 3]) {
    for (field, value) in fields.into_iter().zip(value) {
        state.set_raw_field(field, value.to_bits());
    }
}

/// How far `point` is from the box's nearest point (`G_RadiusList`).
fn distance_to_box(point: [f32; 3], (low, high): ([f32; 3], [f32; 3])) -> f32 {
    let gap: [f32; 3] = std::array::from_fn(|axis| {
        if point[axis] < low[axis] {
            low[axis] - point[axis]
        } else if point[axis] > high[axis] {
            point[axis] - high[axis]
        } else {
            0.0
        }
    });
    length_squared(gap).sqrt()
}

/// Whether two boxes meet, edges included (`SV_AreaEntities`).
fn overlaps(a: ([f32; 3], [f32; 3]), b: ([f32; 3], [f32; 3])) -> bool {
    (0..3).all(|axis| a.0[axis] <= b.1[axis] && a.1[axis] >= b.0[axis])
}

impl Sentry {
    /// The sentry, entity `number`, as a mover's push takes it (a physics object,
    /// `SP_PAS`, `g_items.c:1044`): it blocks a mover it is in the way of — no owner
    /// (`r.ownerNum`) makes it one a stuck mover breaks.
    pub fn pushee(&self, number: u16) -> crate::path_movers::Pushee {
        let fate = crate::path_movers::Fate::Blocks;
        crate::item_physics::pushee(number, &self.state, BOUNDS, fate, false)
    }

    /// `G_TryPushingEntity` moving the sentry to `to` ([`crate::item_physics::shove`]).
    pub fn shove(&mut self, to: [f32; 3], carried: bool) {
        crate::item_physics::shove(&mut self.state, &mut self.origin, to, carried);
    }
}
