//! Items a player drops (`g_items.c`, `g_combat.c`): `LaunchItem`, the entity a dropped
//! item is — thrown, falling under gravity, stopping where it lands (`G_RunItem`,
//! `G_BounceItem`), freed after thirty seconds unless a player picks it up first — and
//! what makes one: `Drop_Item`, `TossClientItems` (the weapon in hand and every powerup
//! running, at death) and `TossClientWeapon` (a Force pull taking the weapon away).
//!
//! A dropped item is an [`items::Pickup`](crate::items::Pickup) with a [`Dropped`] part,
//! touched and taken as a placed one is. jaPRO's flag throwing changes how flags fly
//! ([`crate::flag_throw`]).

use crate::event_entity::EventEntity;
use crate::items::{ITEMS, Kind, Pickup};
use crate::player_death::Rng;
use crate::pmove::MovementTrace;
use jkr_protocol::{EntityState, LEGACY_ENTITY_FIELDS, PlayerState};

/// `ITEM_RADIUS`: a dropped item's box is thirty units each way.
pub const ITEM_RADIUS: f32 = 15.0;
/// A dropped item is freed this long after it was launched (`G_FreeEntity` as its think).
const LIFETIME: i32 = 30_000;
const ET_ITEM: u32 = 2;
const TR_STATIONARY: u32 = 0;
const TR_GRAVITY: u32 = 6;
const EF_DROPPEDWEAPON: u32 = 1 << 25;
const EF_NODRAW: u32 = 1 << 8;
const CONTENTS_TRIGGER: u32 = 0x400;
const CONTENTS_NODROP: u32 = 0x800;
/// `MASK_PLAYERSOLID & ~CONTENTS_BODY`.
const ITEM_MASK: u32 = 0x1 | 0x10 | 0x1000;
const EV_DESTROY_WEAPON_MODEL: u32 = 104;
const ENTITY_NUMBER_NONE: u32 = 1_023;
const GT_TEAM: i32 = 6;
const GT_SIEGE: i32 = 7;
const WP_NONE: u32 = 0;
const WP_BRYAR_PISTOL: u32 = 4;
const WP_BOWCASTER: u32 = 7;
const WP_THERMAL: u32 = 12;
const WP_TRIP_MINE: u32 = 13;
const WP_DET_PACK: u32 = 14;
const WP_EMPLACED_GUN: u32 = 17;
const WP_TURRET: u32 = 18;
const WEAPON_DROPPING: u32 = 2;
const EV_NOAMMO: u32 = 25;
const STAT_WEAPONS: usize = 4;
const PS_EXTERNAL_EVENT: usize = 56;
const PS_EXTERNAL_EVENT_PARM: usize = 64;
const EVENT_BITS: u32 = 0x300;
const EVENT_BIT1: u32 = 0x100;
const PS_WEAPON: usize = 47;
const PS_WEAPON_STATE: usize = 33;
const ES_TYPE: usize = 8;
const ES_EFLAGS: usize = 19;
const ES_MODEL: usize = 46;
const ES_MODEL2: usize = 41;
const ES_GROUND_ENTITY: usize = 22;
const ES_GENERIC1: usize = 86;
const ES_POWERUPS: usize = 77;
const ES_WEAPON: usize = 14;
const ES_POS_TYPE: usize = 23;
const ES_POS_TIME: usize = 0;
pub(crate) const ES_POS_BASE: [usize; 3] = [2, 1, 4];
const ES_POS_DELTA: [usize; 3] = [6, 7, 10];
pub(crate) const ES_APOS_BASE: [usize; 3] = [5, 3, 33];
pub(crate) const ES_ANGLES: [usize; 3] = [25, 9, 24];

/// What a dropped item keeps beyond a placed one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dropped {
    /// `nextthink` with `G_FreeEntity`: when it is freed if nobody took it.
    pub free_at: i32,
    /// `count`: how much of it there is (a powerup's seconds, a tossed weapon's ammo); 0
    /// for the item's own quantity.
    pub count: i32,
    /// Picked up: freed at the next frame (`freeAfterEvent` with no event of its own).
    pub taken: bool,
    /// `physicsBounce`: the share of its speed a bounce keeps — none (it stops dead), but
    /// for a flag thrown under jaPRO's rules ([`crate::flag_throw`]).
    pub bounce: f32,
}

/// `LaunchItem` (`g_items.c:2693-2771`): an `ET_ITEM` entity thrown from `origin` at
/// `velocity`, a trigger thirty units wide, marked as dropped (`modelindex2`), turned
/// along its flight (weapons and powerups on their side), freed in thirty seconds.
pub fn launch(item: usize, origin: [f32; 3], velocity: [f32; 3], level_time: i32) -> Pickup {
    let row = ITEMS[item];
    let mut state = EntityState::zero(0, &LEGACY_ENTITY_FIELDS);
    state.set_raw_field(ES_TYPE, ET_ITEM);
    state.set_raw_field(ES_MODEL, item as u32);
    state.set_raw_field(ES_MODEL2, 1);
    state.set_raw_field(ES_POS_TYPE, TR_GRAVITY);
    state.set_raw_field(ES_POS_TIME, level_time as u32);
    for axis in 0..3 {
        state.set_raw_field(ES_POS_BASE[axis], origin[axis].to_bits());
        state.set_raw_field(ES_POS_DELTA[axis], velocity[axis].to_bits());
    }
    if matches!(row.kind, Kind::Weapon | Kind::Powerup) {
        state.set_raw_field(ES_EFLAGS, EF_DROPPEDWEAPON);
    }
    let (_, yaw) = crate::damage::vector_to_angles(velocity);
    // The tag is held against weapon numbers whatever the item is: the boon (tag 14, the
    // det pack's number) lies pitched down like a det pack.
    let tag = row.tag as u32;
    let pitch: f32 = if matches!(tag, WP_TRIP_MINE | WP_DET_PACK) {
        -90.0
    } else {
        0.0
    };
    let roll: f32 = if matches!(tag, WP_BOWCASTER | WP_DET_PACK | WP_THERMAL) {
        0.0
    } else {
        -90.0
    };
    for (index, value) in ES_ANGLES.into_iter().zip([pitch, yaw, roll]) {
        state.set_raw_field(index, value.to_bits());
    }
    let bounds = ([-ITEM_RADIUS; 3], [ITEM_RADIUS; 3]);
    let dropped = Dropped {
        free_at: level_time + LIFETIME,
        count: 0,
        taken: false,
        bounce: 0.0,
    };
    Pickup {
        state,
        item,
        origin,
        bounds,
        contents: CONTENTS_TRIGGER,
        respawn_at: 0,
        wait: 0.0,
        random: 0.0,
        dropped: Some(dropped),
        allow_npc: false,
        no_global_sound: false,
        targetname: None,
        team: Vec::new(),
    }
}

/// `Drop_Item` (`g_items.c:2777-2791`): thrown forward along the player's yaw turned by
/// `angle`, 150 ahead and 200 up, the lift varied by fifty either way.
pub fn drop_item(
    item: usize,
    player: &EntityState,
    angle: f32,
    level_time: i32,
    rng: &mut Rng,
) -> Pickup {
    let read = |index: usize| f32::from_bits(player.raw_field(index).unwrap_or(0));
    let yaw = read(ES_APOS_BASE[1]) + angle;
    let (forward, _) = crate::pmove::flight::flight_axes([0.0, yaw, read(ES_APOS_BASE[2])]);
    let mut velocity = forward.to_array().map(|axis| axis * 150.0);
    velocity[2] += 200.0 + rng.flrand(-1.0, 1.0) * 50.0;
    let origin = ES_POS_BASE.map(read);
    launch(item, origin, velocity, level_time)
}

/// `BG_FindItemForWeapon`: the item-list row of the weapon, which `RegisterItem` marks.
pub fn item_for_weapon(weapon: u32) -> Option<usize> {
    ITEMS
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, row)| row.kind == Kind::Weapon && row.tag == weapon as i32)
        .map(|(index, _)| index)
}

/// `BG_FindItemForPowerup`: a powerup or a team item with the powerup's tag.
pub fn item_for_powerup(powerup: usize) -> Option<usize> {
    ITEMS
        .iter()
        .position(|row| matches!(row.kind, Kind::Powerup | Kind::Team) && row.tag == powerup as i32)
}

/// What a player's death drops.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tossed {
    /// `s.bolt2`: the weapon the body no longer shows; untouched in siege.
    pub bolt2: Option<u32>,
    /// `EV_DESTROY_WEAPON_MODEL`, told to everyone, when the weapon was dropped.
    pub event: Option<EventEntity>,
    /// The items, in the order they were spawned.
    pub items: Vec<Pickup>,
}

/// `TossClientItems` (`g_combat.c:587-652`) for a player that dies: the weapon in hand
/// (the one being switched to, if the pistol is being put away) unless it is the saber,
/// the pistol or fists, or has no ammo; and, outside team games, every powerup still
/// running, each 45 degrees further round, counting its seconds left. Nothing in siege.
/// Under jaPRO's flag throwing a flag keeps its carrier's momentum
/// ([`crate::flag_throw::drop_carried`]); under its gun game (`gun_game`) only a flag
/// drops.
pub fn toss_client_items(
    state: &PlayerState,
    entity: &EntityState,
    command_weapon: u8,
    gametype: i32,
    level_time: i32,
    rng: &mut Rng,
    rules: crate::flag_throw::FlagRules,
    gun_game: bool,
) -> Tossed {
    let mut tossed = Tossed::default();
    if gametype == GT_SIEGE {
        return tossed;
    }
    let mut weapon = entity.raw_field(ES_WEAPON).unwrap_or(0);
    if weapon == WP_BRYAR_PISTOL {
        if state.raw_field(PS_WEAPON_STATE) == Some(WEAPON_DROPPING) {
            weapon = u32::from(command_weapon);
        }
        if state.stats[STAT_WEAPONS] & (1u32 << (weapon & 31)) == 0 {
            weapon = WP_NONE;
        }
    }
    tossed.bolt2 = Some(weapon);
    let ammo = |weapon: u32| {
        state.ammo[crate::weapon_data::LEGACY_WEAPON_DATA[weapon as usize].ammo_index] as i32
    };
    if !gun_game
        && weapon > WP_BRYAR_PISTOL
        && weapon != WP_EMPLACED_GUN
        && weapon != WP_TURRET
        && ammo(weapon) != 0
        && let Some(item) = item_for_weapon(weapon)
    {
        let client = u32::from(state.client_num());
        tossed.event = Some(EventEntity {
            event: EV_DESTROY_WEAPON_MODEL,
            parameter: client,
            origin: [0.0; 3],
            client: None,
            broadcast: true,
            extra: [(0, 0); 12],
        });
        tossed
            .items
            .push(drop_item(item, entity, 0.0, level_time, rng));
    }
    if gametype != GT_TEAM {
        let mut angle = 45.0;
        for powerup in 1..state.powerups.len() {
            let until = state.powerups[powerup] as i32;
            if until <= level_time {
                continue;
            }
            let Some(item) = item_for_powerup(powerup) else {
                continue;
            };
            // jaPRO's gun game drops nothing (`TossClientItems` returns at once); a flag
            // still drops here, where jaPRO loses it.
            if gun_game && ITEMS[item].kind != Kind::Team {
                continue;
            }
            let mut drop = if rules.throw && ITEMS[item].kind == Kind::Team {
                crate::flag_throw::drop_carried(item, entity, state.velocity(), angle, level_time)
            } else {
                drop_item(item, entity, angle, level_time, rng)
            };
            if rules.throw && rules.suicide_drops && ITEMS[item].kind == Kind::Team {
                drop.contents = crate::flag_throw::THROWN_CONTENTS;
            }
            if let Some(dropped) = drop.dropped.as_mut() {
                dropped.count = ((until - level_time) / 1_000).max(1);
            }
            tossed.items.push(drop);
            angle += 45.0;
        }
    }
    tossed
}

/// `TossClientWeapon` (`g_combat.c:488-575`): the weapon in hand thrown along `direction`
/// at `speed` — none in siege, and never the saber, the pistol, fists, an emplaced gun
/// or a turret, nor one without its item's worth of ammo. The thrower cannot take it back
/// for a second and a half; it carries at most its item's ammo, taken from the player's,
/// and a weapon with no ammo left (all but the thermals, mines and det packs at once) is
/// gone from the player's hands for the first it still has (`EV_NOAMMO`).
pub fn toss_client_weapon(
    state: &mut PlayerState,
    entity: &mut EntityState,
    direction: [f32; 3],
    speed: f32,
    gametype: i32,
    level_time: i32,
) -> Option<Pickup> {
    let weapon = entity.raw_field(ES_WEAPON).unwrap_or(0);
    if gametype == GT_SIEGE
        || weapon <= WP_BRYAR_PISTOL
        || weapon == WP_EMPLACED_GUN
        || weapon == WP_TURRET
    {
        return None;
    }
    let item = item_for_weapon(weapon)?;
    let quantity = ITEMS[item].quantity;
    let ammo_index = crate::weapon_data::LEGACY_WEAPON_DATA[weapon as usize].ammo_index;
    let short = state.ammo[ammo_index] as i32 - quantity;
    if short < 0 && quantity + short <= 0 {
        return None;
    }
    let velocity = direction.map(|axis| axis * speed);
    let mut launched = launch(item, state.origin(), velocity, level_time);
    launched
        .state
        .set_raw_field(ES_GENERIC1, u32::from(state.client_num()));
    launched
        .state
        .set_raw_field(ES_POWERUPS, (level_time + 1_500) as u32);
    let mut count = quantity;
    let left = state.ammo[ammo_index] as i32 - quantity;
    if left < 0 {
        count += left;
        state.ammo[ammo_index] = 0;
    } else {
        state.ammo[ammo_index] = left as u32;
    }
    if let Some(dropped) = launched.dropped.as_mut() {
        dropped.count = count;
    }
    let no_ammo = (state.ammo[ammo_index] as i32) < 1 && weapon != WP_DET_PACK;
    if no_ammo || !matches!(weapon, WP_THERMAL | WP_DET_PACK | WP_TRIP_MINE) {
        state.stats[STAT_WEAPONS] &= !(1u32 << weapon);
        let next = (0..crate::weapon_data::LEGACY_WEAPON_COUNT as u32)
            .find(|other| *other != WP_NONE && state.stats[STAT_WEAPONS] & (1 << other) != 0)
            .unwrap_or(WP_NONE);
        entity.set_raw_field(ES_WEAPON, next);
        state.set_raw_field(PS_WEAPON, next);
        // `G_AddEvent` on a player: its external event, the sequence bits stepped on.
        let bits = (state.raw_field(PS_EXTERNAL_EVENT).unwrap_or(0) & EVENT_BITS)
            .wrapping_add(EVENT_BIT1)
            & EVENT_BITS;
        state.set_raw_field(PS_EXTERNAL_EVENT, EV_NOAMMO | bits);
        state.set_raw_field(PS_EXTERNAL_EVENT_PARM, weapon);
    }
    Some(launched)
}

/// A dropped item set moving from where it is (jaPRO's pushes: a blast, a missile, a
/// Force push or pull): `s.pos` under gravity from now, from `r.currentOrigin`, with
/// `velocity` added to its delta (`add`) or in its place.
pub fn kick(pickup: &mut Pickup, velocity: [f32; 3], add: bool, level_time: i32) {
    let state = &mut pickup.state;
    state.set_raw_field(ES_POS_TYPE, TR_GRAVITY);
    state.set_raw_field(ES_POS_TIME, level_time as u32);
    for axis in 0..3 {
        let delta = if add {
            f32::from_bits(state.raw_field(ES_POS_DELTA[axis]).unwrap_or(0)) + velocity[axis]
        } else {
            velocity[axis]
        };
        state.set_raw_field(ES_POS_BASE[axis], pickup.origin[axis].to_bits());
        state.set_raw_field(ES_POS_DELTA[axis], delta.to_bits());
    }
}

/// What a frame did to a dropped item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DroppedRun {
    /// It is where it is.
    Stays,
    /// It came to rest this frame.
    Landed,
    /// Freed: it was picked up.
    Freed,
    /// It came to rest in a no-drop volume (`CONTENTS_NODROP`).
    Lost,
    /// Its thirty seconds are up (`G_FreeEntity`, or a flag's `Team_DroppedFlagThink`).
    Expired,
}

/// `G_RunItem` (`g_items.c:3218-3272`) for a dropped item, at `level_time` after
/// `previous_time`: a taken item goes; one at rest only waits out its time; one in flight
/// is traced along its trajectory through the world (`trace` passes client 0, the entity
/// `LaunchItem` leaves as its owner), and where it strikes something it stops dead
/// (`G_BounceItem`: `physicsBounce` is never set, so the bounce keeps no speed) — at rest
/// a unit above a floor, or dropping straight down from a wall. With jaPRO's
/// `flag_throw` a flag bounces on ([`crate::flag_throw::stops`]), and a flag flying
/// fast is slowed ([`crate::flag_throw::drag`]).
pub fn run(
    pickup: &mut Pickup,
    level_time: i32,
    previous_time: i32,
    trace: &mut dyn FnMut([f32; 3], [f32; 3], [f32; 3], [f32; 3], u32) -> MovementTrace,
    contents: &dyn Fn([f32; 3]) -> u32,
    flag_throw: bool,
) -> DroppedRun {
    let Some(dropped) = pickup.dropped else {
        return DroppedRun::Stays;
    };
    if dropped.taken {
        return DroppedRun::Freed;
    }
    let state = &mut pickup.state;
    let read = |state: &EntityState, index: usize| state.raw_field(index).unwrap_or(0);
    if read(state, ES_GROUND_ENTITY) == ENTITY_NUMBER_NONE && read(state, ES_POS_TYPE) != TR_GRAVITY
    {
        state.set_raw_field(ES_POS_TYPE, TR_GRAVITY);
        state.set_raw_field(ES_POS_TIME, level_time as u32);
    }
    let expired = level_time >= dropped.free_at;
    if read(state, ES_POS_TYPE) == TR_STATIONARY {
        return if expired {
            DroppedRun::Expired
        } else {
            DroppedRun::Stays
        };
    }
    let flag = ITEMS[pickup.item].kind == Kind::Team;
    if flag {
        let mut delta = ES_POS_DELTA.map(|index| f32::from_bits(read(state, index)));
        crate::flag_throw::drag(&mut delta);
        for axis in 0..2 {
            state.set_raw_field(ES_POS_DELTA[axis], delta[axis].to_bits());
        }
    }
    let base = ES_POS_BASE.map(|index| f32::from_bits(read(state, index)));
    let delta = ES_POS_DELTA.map(|index| f32::from_bits(read(state, index)));
    let kind = read(state, ES_POS_TYPE) as u8;
    let start = read(state, ES_POS_TIME) as i32;
    let origin =
        crate::trajectory::legacy_evaluate_trajectory(base, delta, kind, start, 0, level_time);
    let mut found = trace(
        pickup.origin,
        pickup.bounds.0,
        pickup.bounds.1,
        origin,
        ITEM_MASK,
    );
    pickup.origin = found.end_position;
    if found.start_solid {
        found.fraction = 0.0;
    }
    if expired {
        return DroppedRun::Expired;
    }
    if found.fraction == 1.0 {
        return DroppedRun::Stays;
    }
    if contents(pickup.origin) & CONTENTS_NODROP != 0 {
        return DroppedRun::Lost;
    }
    // `G_BounceItem`: the velocity reflected on the plane and scaled by `physicsBounce`,
    // 0 but for a thrown flag — the signs survive as negative zeroes.
    let hit_time = previous_time + ((level_time - previous_time) as f32 * found.fraction) as i32;
    let velocity =
        crate::trajectory::legacy_evaluate_trajectory_delta(delta, kind, start, 0, hit_time);
    let normal = found.plane_normal;
    let along = velocity[0] * normal[0] + velocity[1] * normal[1] + velocity[2] * normal[2];
    let mut bounced = [0.0; 3];
    for axis in 0..3 {
        let reflected = velocity[axis] + normal[axis] * (-2.0 * along);
        bounced[axis] = reflected * dropped.bounce;
        state.set_raw_field(ES_POS_DELTA[axis], bounced[axis].to_bits());
    }
    let stops = if flag && flag_throw {
        crate::flag_throw::stops(bounced[2])
    } else {
        normal[2] > 0.0 && bounced[2] < 40.0
    };
    if stops {
        let mut rest = found.end_position;
        rest[2] += 1.0;
        let rest = rest.map(|axis| axis as i32 as f32);
        // `G_SetOrigin`: stationary, the delta cleared.
        state.set_raw_field(ES_POS_TYPE, TR_STATIONARY);
        state.set_raw_field(ES_POS_TIME, 0);
        for axis in 0..3 {
            state.set_raw_field(ES_POS_BASE[axis], rest[axis].to_bits());
            state.set_raw_field(ES_POS_DELTA[axis], 0);
        }
        pickup.origin = rest;
        state.set_raw_field(ES_GROUND_ENTITY, u32::from(found.entity_number));
        return DroppedRun::Landed;
    }
    let moved: [f32; 3] = std::array::from_fn(|axis| pickup.origin[axis] + normal[axis]);
    pickup.origin = moved;
    for axis in 0..3 {
        state.set_raw_field(ES_POS_BASE[axis], moved[axis].to_bits());
    }
    state.set_raw_field(ES_POS_TIME, level_time as u32);
    DroppedRun::Stays
}

/// `Touch_Item`'s own rules for a dropped item, before the pickup's: a tossed weapon whose
/// thrower's second and a half is up forgets its thrower; until then the thrower cannot
/// take it back (`BG_CanItemBeGrabbed`).
pub fn refuses(pickup: &mut Pickup, client: u16, level_time: i32) -> bool {
    if pickup.dropped.is_none() {
        return false;
    }
    let until = pickup.state.raw_field(ES_POWERUPS).unwrap_or(0) as i32;
    if ITEMS[pickup.item].kind == Kind::Weapon && until != 0 && until < level_time {
        pickup.state.set_raw_field(ES_GENERIC1, 0);
        pickup.state.set_raw_field(ES_POWERUPS, 0);
    }
    let until = pickup.state.raw_field(ES_POWERUPS).unwrap_or(0) as i32;
    until > level_time && pickup.state.raw_field(ES_GENERIC1).unwrap_or(0) == u32::from(client)
}

/// `Touch_Item`'s end for a dropped item that was taken: drawn no more, touched no more,
/// freed at the next frame (`freeAfterEvent` with no event of its own). Its respawn think
/// is set as for any item, and never runs.
pub fn taken(pickup: &mut Pickup, respawn: i32, level_time: i32) {
    let flags = pickup.state.raw_field(ES_EFLAGS).unwrap_or(0);
    pickup.state.set_raw_field(ES_EFLAGS, flags | EF_NODRAW);
    pickup.contents = 0;
    pickup.respawn_at = if respawn <= 0 {
        0
    } else {
        level_time + respawn * 1_000
    };
    if let Some(dropped) = pickup.dropped.as_mut() {
        dropped.taken = true;
    }
}

/// Whether `G_RunItem` moves dropped item `pickup`: it falls, or it was left with no
/// ground, which sets it falling.
pub fn moving(pickup: &Pickup) -> bool {
    let read = |index: usize| pickup.state.raw_field(index).unwrap_or(0);
    pickup.dropped.is_some()
        && (read(ES_POS_TYPE) != TR_STATIONARY || read(ES_GROUND_ENTITY) == ENTITY_NUMBER_NONE)
}

/// Dropped item `pickup`, entity `number`, as a mover's push takes it (`physicsObject`,
/// `G_MoverPush`): where it lies (`s.pos.trBase`), its box and what it lies on; a dropped
/// weapon is freed where it would block. `None` for an item the map placed, or one taken.
pub fn pushee(number: u16, pickup: &Pickup) -> Option<crate::path_movers::Pushee> {
    use crate::path_movers::{Fate, Pushee};
    pickup.dropped.filter(|dropped| !dropped.taken)?;
    let read = |index: usize| pickup.state.raw_field(index).unwrap_or(0);
    Some(Pushee {
        number,
        origin: ES_POS_BASE.map(|index| f32::from_bits(read(index))),
        bounds: pickup.bounds,
        ground: read(ES_GROUND_ENTITY) as u16,
        client: false,
        fate: if read(ES_EFLAGS) & EF_DROPPEDWEAPON != 0 {
            Fate::Freed
        } else {
            Fate::Blocks
        },
    })
}

/// `G_TryPushingEntity` moving dropped item `pickup` to `to` (`g_mover.c:224-250`):
/// `s.pos.trBase` and `r.currentOrigin` there, and its ground gone unless it rode the
/// mover — which sets it falling.
pub fn shove(pickup: &mut Pickup, to: [f32; 3], carried: bool) {
    for (index, value) in ES_POS_BASE.into_iter().zip(to) {
        pickup.state.set_raw_field(index, value.to_bits());
    }
    pickup.origin = to;
    if !carried {
        pickup
            .state
            .set_raw_field(ES_GROUND_ENTITY, ENTITY_NUMBER_NONE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path_movers::Fate;

    /// A dropped weapon is freed where a mover would be blocked by it, any other dropped
    /// item blocks it; shoved off, an item loses its ground and falls, carried it keeps it.
    #[test]
    fn a_dropped_item_is_pushed_as_a_physics_object() {
        let weapon = item_for_weapon(WP_BOWCASTER).unwrap();
        let mut pickup = launch(weapon, [0.0, 0.0, 64.0], [0.0; 3], 1_000);
        pickup.state.set_raw_field(ES_EFLAGS, EF_DROPPEDWEAPON);
        pickup.state.set_raw_field(ES_POS_TYPE, TR_STATIONARY);
        pickup.state.set_raw_field(ES_GROUND_ENTITY, 77);
        assert!(!moving(&pickup));
        let pushee = pushee(300, &pickup).unwrap();
        assert_eq!(
            (pushee.fate, pushee.ground, pushee.client),
            (Fate::Freed, 77, false)
        );
        pickup.state.set_raw_field(ES_EFLAGS, 0);
        assert_eq!(super::pushee(300, &pickup).unwrap().fate, Fate::Blocks);
        shove(&mut pickup, [8.0, 0.0, 80.0], true);
        assert_eq!(pickup.origin, [8.0, 0.0, 80.0]);
        assert!(!moving(&pickup));
        shove(&mut pickup, [16.0, 0.0, 80.0], false);
        assert!(moving(&pickup));
        let base = ES_POS_BASE.map(|index| f32::from_bits(pickup.state.raw_field(index).unwrap()));
        assert_eq!(base, [16.0, 0.0, 80.0]);
    }
}
