//! The portable E-Web (OpenJK `codemp/game/g_items.c:1450-2050`: `ItemUse_UseEWeb`,
//! `EWeb_Create`, `EWebThink` and its helpers, `EWebDie`, `EWebPain`, `EWebDisattach`;
//! `G_RunItem` and `G_RunExPhys` for its fall).
//!
//! Its owner sets it down 48 units ahead along their yaw, where its 64 by 64 by 48 box fits
//! and stands on the world within 18 units below; else a failure sound plays. Its owner is
//! held on it (`emplacedIndex`, the emplaced gun's weapon) while it unfolds for half a
//! second; then each frame its cannon turns toward the owner's view at four degrees a frame
//! at most, its owner is moved to stand 32 units behind the cannon's turning bone (their legs
//! strafing while moved), and its barrel pitches to 0.8 of the view's. While the owner holds
//! attack it fires every tenth of a second from its muzzle bolt: a turret bolt of 20 at 1200,
//! the owner's. Using it again, backing off it (the movement clears `emplacedIndex`) or no
//! room for its owner puts it away: the owner's weapons come back, and it is freed. Shot to
//! 0 health, or once its owner died, left, spawned again or changed teams, it blows up (90
//! within 128, its own) and the owner loses it for good. Its health is the owner's
//! between deployments (`ewebHealth`).
//!
//! It links before its box is set: until it next moves other moves find it only near its
//! origin (`linked_box`), and clients are sent a one-unit box in `solid`. While airborne
//! it falls as an item (`G_RunItem`, whose trace passes its owner) and by its extra physics
//! (`G_RunExPhys`, gravity 3, mass 0.09, bounce 1.1, whose traces meet its owner).
//!
//! Left out: being pushed by movers (`G_MoverPush` moves a `physicsObject`); an E-Web whose
//! model files are missing still deploys, its bolts at its origin.

use crate::entity_clip::{BoxObstacle, linked_box, packed_solid};
use crate::ex_physics::{Body, Physics, Step};
use crate::item_physics::{ItemBounce, ItemMove};
use crate::map_turret_g2::{bone_anim, set_bone_angles};
use crate::map_turret_world::{
    BoltIndex, CONTENTS_LIGHTSABER, DAMAGE_DEATH_KNOCKBACK, MASK_SHOT, Shot, TurretHost,
    WP_EMPLACED_GUN, WP_TURRET, es, play_effect, play_effect_id, publish_net_health,
};
use crate::means_of_death::{MOD_SUICIDE, MOD_TURBLAST};
use crate::player_angle_math::{angle_subtract, vector_angles};
use jkr_protocol::{EntityState, LEGACY_ENTITY_FIELDS};

/// Its model.
pub const MODEL: &[u8] = b"models/map_objects/hoth/eweb_model.glm";
/// Its box (`r.mins`, `r.maxs`, set after its first link).
pub const BOUNDS: ([f32; 3], [f32; 3]) = ([-32.0, -32.0, -24.0], [32.0, 32.0, 24.0]);
/// `MASK_PLAYERSOLID`: its contents, its clip mask, and what its placing traces meet.
pub const CONTENTS: u32 = 0x1111;
/// `EWEB_HEALTH`, `EWEB_DEATH_DMG`, `EWEB_DEATH_RADIUS`, `EWEB_MISSILE_DAMAGE`,
/// `EWEB_USE_DEBOUNCE`.
const HEALTH: i32 = 200;
const DEATH_DAMAGE: f32 = 90.0;
const DEATH_RADIUS: f32 = 128.0;
const MISSILE_DAMAGE: i32 = 20;
const USE_DEBOUNCE: i32 = 1_000;
/// Its bolt's speed and life (`CreateMissile(bPoint, d, 1200.0f, 10000, ...)`).
const MISSILE_SPEED: f32 = 1_200.0;
const MISSILE_LIFE: i32 = 10_000;
/// "max degrees we can turn per update".
const TURN_CAP: f32 = 4.0;
/// `G_RunExPhys(self, gravity, mass, bounce, ...)` in `EWebThink`.
const PHYSICS: Physics = Physics {
    gravity: 3.0,
    mass: 0.09,
    bounce: 1.1,
};
/// The sound of no room, the muzzle flash.
const FAIL_SOUND: &[u8] = b"sound/interface/shieldcon_empty";
const MUZZLE_FLASH: &[u8] = b"turret/muzzle_flash.efx";
/// `EFFECT_EXPLOSION_DETPACK`; `CHAN_AUTO`.
const EFFECT_EXPLOSION_DETPACK: u32 = 6;
const CHAN_AUTO: u32 = 0;
/// `ENTITYNUM_WORLD`, `ENTITYNUM_NONE`.
const ENTITYNUM_WORLD: u16 = 1_022;
const ENTITYNUM_NONE: u16 = 1_023;
/// `BUTTON_ATTACK`; `HANDEXTEND_NONE`; `HI_EWEB`.
const BUTTON_ATTACK: u16 = 1;
const HANDEXTEND_NONE: u32 = 0;
const HI_EWEB: i32 = crate::holdables::HI_EWEB;
/// `BOTH_STRAFE_LEFT1`, `BOTH_STRAFE_RIGHT1`; `SETANIM_FLAG_OVERRIDE`, `SETANIM_FLAG_HOLD`.
const BOTH_STRAFE_LEFT1: u16 = 1_122;
const BOTH_STRAFE_RIGHT1: u16 = 1_123;
const SETANIM_FLAG_OVERRIDE: u8 = crate::pmove_anim::SETANIM_FLAG_OVERRIDE;
const SETANIM_FLAG_HOLD: u8 = crate::pmove_anim::SETANIM_FLAG_HOLD;
/// `s.origin2`, the yaw arc its owner's view is held to (`BG_EmplacedView`).
const ES_ORIGIN2_X: usize = 56;
const ARC: f32 = 360.0;
/// `s.solid`.
const ES_SOLID: usize = 26;

/// What a client keeps of its E-Web beyond its player state (`gclient_t`'s `ewebIndex`,
/// `ewebTime` and `ewebHealth`); every spawn zeroes them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Kit {
    /// The entity number of its E-Web; 0 for none.
    pub index: u16,
    /// Until when a use does nothing.
    pub time: i32,
    /// The health its next E-Web has; -1 once one died, 0 for a fresh one.
    pub health: i32,
}

/// The owner as the E-Web reads and moves them; the caller copies it in and back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct User {
    pub number: u16,
    pub kit: Kit,
    /// `inuse` and a client; and `pers.connected == CON_CONNECTED`.
    pub in_use: bool,
    pub connected: bool,
    /// `ent->health`; `sess.sessionTeam`; `pers.cmd.buttons`.
    pub health: i32,
    pub team: i32,
    pub buttons: u16,
    /// `r.mins`, `r.maxs`.
    pub bounds: ([f32; 3], [f32; 3]),
    /// `ps.origin` (and `r.currentOrigin`, which `G_SetOrigin` moves with it).
    pub origin: [f32; 3],
    /// `ps.viewangles`.
    pub view: [f32; 3],
    /// `ps.weapon`, `stats[STAT_WEAPONS]`, `stats[STAT_HOLDABLE_ITEMS]`,
    /// `stats[STAT_HOLDABLE_ITEM]`, `emplacedIndex`.
    pub weapon: u32,
    pub weapons: i32,
    pub items: i32,
    pub selected: i32,
    pub emplaced: u16,
    /// `legsAnim`, `legsTimer`, `weaponTime`, `forceHandExtend`.
    pub legs_anim: u16,
    pub legs_timer: i32,
    pub weapon_time: i32,
    pub hand_extend: u32,
    /// The legs' animation the E-Web asked to play on them (`G_SetAnim(owner, &cmd,
    /// SETANIM_LEGS, anim, flags, 0)`), for the caller to play with the owner's lengths.
    pub legs_request: Option<(u16, u8)>,
}

/// What its think does next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Think {
    /// `EWebThink`.
    Run,
    /// `G_FreeEntity`: put away or blown up.
    Free,
}

/// A deployed E-Web: its wire state and the fields of it the reference reads.
#[derive(Clone, Debug, PartialEq)]
pub struct EWeb {
    pub number: u16,
    /// Its wire state (`s`).
    pub state: EntityState,
    /// `r.currentOrigin`.
    pub origin: [f32; 3],
    /// `r.ownerNum`.
    pub owner: u16,
    pub health: i32,
    pub max_health: i32,
    pub takes_damage: bool,
    /// `epVelocity`, `epGravFactor`.
    pub velocity: [f32; 3],
    pub grav_factor: f32,
    /// `ent->angle`: the cannon's turn from where it was set down.
    pub yaw: f32,
    /// `genericValue8`: until when it unfolds.
    pub unfolded_at: i32,
    /// `genericValue5`, `genericValue3`: its next shot, and whether it plays its firing
    /// frames.
    pub next_shot: i32,
    pub firing: bool,
    /// `genericValue11`: the owner's weapons when it was set down.
    pub weapons: i32,
    /// `genericValue10`, `genericValue9`: its muzzle bolt and its turning bone's.
    pub muzzle: BoltIndex,
    pub seat: BoltIndex,
    /// `r.absmin`, `r.absmax`: where it was last linked, and with what box (none until it
    /// first moves).
    pub linked: ([f32; 3], [f32; 3]),
    pub think: Think,
    pub next_think: i32,
    /// `G_FreeEntity` has taken it.
    pub freed: bool,
}

/// What a use of the E-Web comes to (`ItemUse_UseEWeb`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use {
    /// Too soon after the last, busy, or on another emplaced gun.
    Refused,
    /// The owner's E-Web is to be put away ([`disattach`]).
    PutAway,
    /// One is to be set down ([`create`]), in an entity the caller holds ready.
    Deploy,
}

/// `ItemUse_UseEWeb` (`g_items.c:2012-2045`): nothing within a second of the last use, while
/// the weapon or the hand is busy, or on an emplaced gun that is not the owner's E-Web; else
/// the owner's E-Web put away, or one set down — and a second to wait either way.
pub fn use_eweb(user: &mut User, level_time: i32) -> Use {
    if user.kit.time > level_time
        || user.weapon_time > 0
        || user.hand_extend != HANDEXTEND_NONE
        || (user.emplaced != 0 && user.kit.index == 0)
    {
        return Use::Refused;
    }
    user.kit.time = level_time + USE_DEBOUNCE;
    if user.kit.index != 0 {
        Use::PutAway
    } else {
        Use::Deploy
    }
}

/// `EWeb_Create` (`g_items.c:1887-2008`): the E-Web `user` sets down as entity `number`,
/// its owner held on it; `None` where it has no room or no ground (the failure sound
/// played), and the caller lets the entity go.
pub fn create(
    user: &mut User,
    number: u16,
    level_time: i32,
    host: &mut dyn TurretHost,
) -> Option<EWeb> {
    let fail = host.sound_index(FAIL_SOUND);
    let facing = [0.0, user.view[1], 0.0];
    let forward = crate::pmove::flight::flight_axes(facing).0.to_array();
    let mut start = user.origin;
    // "allow some fudge".
    start[2] += 12.0;
    let mut at: [f32; 3] = std::array::from_fn(|axis| start[axis] + 48.0 * forward[axis]);
    let ahead = host.trace_box(start, BOUNDS, at, user.number, CONTENTS);
    if ahead.all_solid || ahead.start_solid || ahead.fraction != 1.0 {
        // "can't spawn here, we are in solid".
        host.raise(crate::weapon_fire::sound_event(
            user.origin,
            CHAN_AUTO,
            fail,
        ));
        return None;
    }
    let mut down = at;
    down[2] -= 18.0;
    let ground = host.trace_box(at, BOUNDS, down, user.number, CONTENTS);
    if ground.start_solid
        || ground.all_solid
        || ground.fraction == 1.0
        || ground.entity_number < ENTITYNUM_WORLD
    {
        // "didn't hit ground."
        host.raise(crate::weapon_fire::sound_event(
            user.origin,
            CHAN_AUTO,
            fail,
        ));
        return None;
    }
    at = ground.end_position;
    let mut eweb = EWeb {
        number,
        state: EntityState::zero(0, &LEGACY_ENTITY_FIELDS),
        origin: at,
        owner: user.number,
        health: 0,
        max_health: HEALTH,
        takes_damage: true,
        velocity: [0.0; 3],
        grav_factor: 0.0,
        yaw: 0.0,
        unfolded_at: 0,
        next_shot: 0,
        firing: false,
        weapons: user.weapons,
        muzzle: -1,
        seat: -1,
        linked: linked_box(at, ([0.0; 3], [0.0; 3])),
        think: Think::Run,
        next_think: level_time,
        freed: false,
    };
    let _ = eweb.state.set_number(number);
    crate::item_physics::set_origin(&mut eweb.state, &mut eweb.origin, at);
    for axis in 0..3 {
        eweb.state
            .set_raw_field(es::APOS_BASE[axis], facing[axis].to_bits());
        // "consider "angle 0" for constraint".
        eweb.state
            .set_raw_field(es::ANGLES[axis], facing[axis].to_bits());
    }
    eweb.state.set_raw_field(es::OWNER, u32::from(user.number));
    eweb.state.set_raw_field(es::TEAM_OWNER, user.team as u32);
    // "refresh the owner's e-web health if its last e-web did not exist or was killed".
    if user.kit.health <= 0 {
        user.kit.health = HEALTH;
    }
    eweb.health = user.kit.health;
    publish_net_health(&mut eweb.state, eweb.health, eweb.max_health);
    eweb.state.set_raw_field(es::MODEL_GHOUL2, 1);
    eweb.state.set_raw_field(es::G2_RADIUS, 128);
    let model = host.model_index(MODEL);
    eweb.state.set_raw_field(es::MODEL_INDEX, u32::from(model));
    host.init_model(number, MODEL);
    set_bone_angles(&mut eweb.state, number, b"cannon_Yrot", [0.0; 3], host);
    set_bone_angles(&mut eweb.state, number, b"cannon_Xrot", [0.0; 3], host);
    eweb.muzzle = host.add_bolt(number, b"*cannonflash");
    eweb.seat = host.add_bolt(number, b"cannon_Yrot");
    eweb.state.set_raw_field(ES_ORIGIN2_X, ARC.to_bits());
    // Linked before its box is set: found by a point's box, sent as a one-unit one.
    link(&mut eweb, ([0.0; 3], [0.0; 3]));
    // "start the "unfolding" anim", and no use "until the anim is done playing".
    bone_anim(&mut eweb.state, number, 4, 20, host);
    eweb.unfolded_at = level_time + 500;
    user.kit.index = number;
    user.emplaced = number;
    Some(eweb)
}

/// `EWebDisattach` (`g_items.c:1454-1469`): `user` let go of `eweb` — their weapons back
/// (none if dead) — and `eweb` freed at its next think.
pub fn disattach(user: &mut User, eweb: Option<&mut EWeb>, level_time: i32) {
    user.kit.index = 0;
    user.emplaced = 0;
    if let Some(eweb) = eweb {
        user.weapons = if user.health > 0 { eweb.weapons } else { 0 };
        eweb.think = Think::Free;
        eweb.next_think = level_time;
    } else {
        // Its entity is gone already (a no-drop volume took it): the reference reads the
        // freed slot, whose weapons are zeroed.
        user.weapons = 0;
    }
}

/// What a frame or a blow of the E-Web leaves for the caller.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Outcome {
    /// It blew up: its blast, for the caller to deal before [`after_blast`].
    pub blast: Option<Blast>,
}

/// `EWebDie`'s `G_RadiusDamage(origin, eweb, 90, 128, eweb, eweb, MOD_SUICIDE)`: the E-Web
/// its attacker, its inflictor and the one it spares.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blast {
    pub origin: [f32; 3],
    pub damage: f32,
    pub radius: f32,
    pub means: u32,
}

/// `G_RunItem` for the E-Web (`g_items.c:3218-3279`) at `level_time` after the frame at
/// `previous_time`: airborne, its fall along its trajectory first (its owner passed), then
/// its think when due (`EWebThink`, or freeing), then the bounce where the fall was cut
/// short; a no-drop volume frees it as it is. `user` is its owner, `None` once not in use.
#[allow(clippy::too_many_arguments)]
pub fn run(
    eweb: &mut EWeb,
    user: Option<&mut User>,
    level_time: i32,
    previous_time: i32,
    host: &mut dyn TurretHost,
) -> Outcome {
    let owner = eweb.owner;
    let moved =
        crate::item_physics::fall(&mut eweb.state, &mut eweb.origin, level_time, |from, to| {
            host.trace_box(from, BOUNDS, to, owner, CONTENTS)
        });
    let ItemMove::Moved(moved) = moved else {
        return run_think(eweb, user, level_time, host);
    };
    link(eweb, BOUNDS);
    let outcome = run_think(eweb, user, level_time, host);
    if eweb.freed {
        return outcome;
    }
    let bounced = crate::item_physics::bounce(
        &mut eweb.state,
        &mut eweb.origin,
        &moved,
        level_time,
        previous_time,
        |point| host.point_contents(point, ENTITYNUM_NONE),
    );
    if bounced == ItemBounce::NoDrop {
        eweb.freed = true;
    }
    outcome
}

/// `G_RunThink`: its think, when due.
fn run_think(
    eweb: &mut EWeb,
    user: Option<&mut User>,
    level_time: i32,
    host: &mut dyn TurretHost,
) -> Outcome {
    if eweb.next_think <= 0 || eweb.next_think > level_time {
        return Outcome::default();
    }
    eweb.next_think = 0;
    match eweb.think {
        Think::Free => {
            eweb.freed = true;
            Outcome::default()
        }
        Think::Run => think(eweb, user, level_time, host),
    }
}

/// `EWebThink` (`g_items.c:1804-1883`).
fn think(
    eweb: &mut EWeb,
    user: Option<&mut User>,
    level_time: i32,
    host: &mut dyn TurretHost,
) -> Outcome {
    let Some(user) = user.filter(|user| user.in_use) else {
        // "something happened to the owner, let's explode".
        return die(eweb);
    };
    if !user.connected || user.kit.index != eweb.number || user.health < 1 {
        return die(eweb);
    }
    if user.emplaced != eweb.number {
        // "just go back to the inventory then".
        disattach(user, Some(eweb), level_time);
        return Outcome::default();
    }
    let angles = read3(&eweb.state, es::ANGLES);
    let arc = f32::from_bits(eweb.state.raw_field(ES_ORIGIN2_X).unwrap_or(0));
    let (held, yaw) = crate::emplaced::emplaced_view(user.view, angles, arc);
    if held != 0 {
        user.view[1] = yaw;
    }
    user.weapon = WP_EMPLACED_GUN;
    user.weapons = WP_EMPLACED_GUN as i32;
    if eweb.unfolded_at < level_time {
        update_bone_angles(eweb, user, level_time, host);
        if user.kit.index == 0 {
            // "was removed during position function".
            return Outcome::default();
        }
        if user.buttons & BUTTON_ATTACK != 0 {
            if eweb.next_shot < level_time {
                fire(eweb, user.number, level_time, host);
                // "cheap firing anim".
                bone_anim(&mut eweb.state, eweb.number, 2, 4, host);
                eweb.firing = true;
                eweb.next_shot = level_time + 100;
            }
        } else if eweb.next_shot < level_time && eweb.firing {
            // "reset the anim back to non-firing".
            bone_anim(&mut eweb.state, eweb.number, 0, 1, host);
            eweb.firing = false;
        }
    }
    run_ex_phys(eweb, host);
    eweb.next_think = level_time;
    Outcome::default()
}

/// `EWebUpdateBoneAngles` (`g_items.c:1765-1798`): the cannon turned toward the view (four
/// degrees at most), the owner placed behind it, and — still on it — the barrel pitched.
fn update_bone_angles(
    eweb: &mut EWeb,
    user: &mut User,
    level_time: i32,
    host: &mut dyn TurretHost,
) {
    let angles = read3(&eweb.state, es::ANGLES);
    let ideal = angle_subtract(user.view[1], angles[1]);
    let increment = angle_subtract(ideal, eweb.yaw).clamp(-TURN_CAP, TURN_CAP);
    eweb.yaw += increment;
    set_bone_angles(
        &mut eweb.state,
        eweb.number,
        b"cannon_Yrot",
        [eweb.yaw, 0.0, 0.0],
        host,
    );
    position_user(eweb, user, level_time, host);
    if user.kit.index == 0 {
        return;
    }
    let pitch = angle_subtract(user.view[0], angles[0]) * 0.8;
    set_bone_angles(
        &mut eweb.state,
        eweb.number,
        b"cannon_Xrot",
        [0.0, 0.0, pitch],
        host,
    );
}

/// `EWebPositionUser` (`g_items.c:1697-1762`): the owner moved to 32 units behind the
/// cannon's turning bone, at the E-Web's height and four up, then down to the ground within
/// seven — strafing while moved, standing still once not; no room there and the E-Web is
/// put away.
fn position_user(eweb: &mut EWeb, user: &mut User, level_time: i32, host: &mut dyn TurretHost) {
    let apos = read3(&eweb.state, es::APOS_BASE);
    let bolt = host.bolt_matrix(eweb.number, eweb.seat, apos, eweb.origin, [0.0; 3]);
    // `NEGATIVE_X`: the first column, negated.
    let behind = [-bolt[0][0], -bolt[1][0], -bolt[2][0]];
    let mut place: [f32; 3] = std::array::from_fn(|row| bolt[row][3] + 32.0 * behind[row]);
    place[2] = eweb.origin[2];
    place[2] += 4.0;
    let there = host.trace_box(user.origin, user.bounds, place, user.number, CONTENTS);
    if there.start_solid || there.all_solid || there.fraction != 1.0 {
        // "can't move here.. stop using the thing I guess".
        disattach(user, Some(eweb), level_time);
        return;
    }
    let mut below = place;
    below[2] -= 7.0;
    let ground = host.trace_box(place, user.bounds, below, user.number, CONTENTS);
    if ground.start_solid || ground.all_solid {
        return;
    }
    let moved: [f32; 3] = std::array::from_fn(|axis| user.origin[axis] - ground.end_position[axis]);
    let length = (moved[0] * moved[0] + moved[1] * moved[1] + moved[2] * moved[2]).sqrt();
    if length > 1.0 {
        // "we moved, do some animating".
        let mut toward = vector_angles(moved);
        toward[1] = angle_subtract(user.view[1], toward[1]);
        let mut flags = SETANIM_FLAG_HOLD;
        let legs = if toward[1] > 0.0 {
            if user.legs_anim == BOTH_STRAFE_RIGHT1 {
                // "reset to change direction".
                flags |= SETANIM_FLAG_OVERRIDE;
            }
            BOTH_STRAFE_LEFT1
        } else {
            if user.legs_anim == BOTH_STRAFE_LEFT1 {
                flags |= SETANIM_FLAG_OVERRIDE;
            }
            BOTH_STRAFE_RIGHT1
        };
        user.legs_request = Some((legs, flags));
    } else if user.legs_anim == BOTH_STRAFE_RIGHT1 || user.legs_anim == BOTH_STRAFE_LEFT1 {
        // "don't keep animating in place".
        user.legs_timer = 0;
    }
    user.origin = ground.end_position;
    host.move_entity(user.number, user.origin);
}

/// `EWebFire` (`g_items.c:1654-1694`): a bolt from the muzzle along it, started 16 units
/// back into the box; the owner's, passing the E-Web; and the muzzle flash.
fn fire(eweb: &mut EWeb, owner: u16, level_time: i32, host: &mut dyn TurretHost) {
    if eweb.muzzle == -1 {
        return;
    }
    let apos = read3(&eweb.state, es::APOS_BASE);
    let bolt = host.bolt_matrix(eweb.number, eweb.muzzle, apos, eweb.origin, [0.0; 3]);
    let point = [bolt[0][3], bolt[1][3], bolt[2][3]];
    // `NEGATIVE_Y`: the second column, negated.
    let direction = [-bolt[0][1], -bolt[1][1], -bolt[2][1]];
    let start: [f32; 3] = std::array::from_fn(|axis| point[axis] + -16.0 * direction[axis]);
    let mut missile = crate::weapon_fire::create_missile_by(
        owner,
        start,
        direction,
        MISSILE_SPEED,
        level_time,
        false,
    );
    missile.free_at = level_time + MISSILE_LIFE;
    missile.state.set_raw_field(es::WEAPON, WP_TURRET);
    missile.damage = MISSILE_DAMAGE;
    missile.method_of_death = MOD_TURBLAST;
    missile.clip_mask = MASK_SHOT | CONTENTS_LIGHTSABER;
    missile.pass_through = Some(eweb.number);
    missile.bounce_count = 8;
    host.launch(Shot {
        missile,
        dflags: DAMAGE_DEATH_KNOCKBACK,
        parentless: false,
    });
    let flash = host.effect_index(MUZZLE_FLASH);
    play_effect_id(host, u32::from(flash), point, vector_angles(direction));
}

/// `G_RunExPhys(self, 3, 0.09, 1.1, qfalse, NULL, 0)`: placed and linked where it moved or
/// stopped.
fn run_ex_phys(eweb: &mut EWeb, host: &mut dyn TurretHost) {
    let ground = eweb.state.raw_field(es::GROUND_ENTITY).unwrap_or(0) as u16;
    let mut body = Body {
        origin: eweb.origin,
        velocity: eweb.velocity,
        grav_factor: eweb.grav_factor,
        ground,
    };
    let me = eweb.number;
    let step = crate::ex_physics::run(&mut body, PHYSICS, &mut |from, to| {
        host.trace_box(from, BOUNDS, to, me, CONTENTS)
    });
    eweb.state
        .set_raw_field(es::GROUND_ENTITY, u32::from(body.ground));
    eweb.velocity = body.velocity;
    eweb.grav_factor = body.grav_factor;
    match step {
        Step::Still | Step::Stuck => {}
        Step::Stopped => link(eweb, BOUNDS),
        Step::Moved { .. } => {
            crate::item_physics::set_origin(&mut eweb.state, &mut eweb.origin, body.origin);
            link(eweb, BOUNDS);
        }
    }
}

/// `G_Damage` on the E-Web for `take` (`object_take`'s): its health bar, then its death, or
/// its pain — the owner told its health (`EWebPain`).
pub fn damage(eweb: &mut EWeb, user: Option<&mut User>, take: i32) -> Outcome {
    eweb.health -= take;
    if eweb.health <= 0 {
        eweb.health = eweb.health.max(-999);
    }
    publish_net_health(&mut eweb.state, eweb.health, eweb.max_health);
    if eweb.health <= 0 {
        return die(eweb);
    }
    // `EWebPain`: "update the owner's health status of me".
    if let Some(user) = user.filter(|user| user.in_use) {
        user.kit.health = eweb.health;
    }
    Outcome::default()
}

/// `EWebDie` (`g_items.c:1484-1517`) up to its blast, which the caller deals before
/// [`after_blast`].
fn die(eweb: &EWeb) -> Outcome {
    Outcome {
        blast: Some(Blast {
            origin: eweb.origin,
            damage: DEATH_DAMAGE,
            radius: DEATH_RADIUS,
            means: MOD_SUICIDE,
        }),
    }
}

/// The rest of `EWebDie` once `blast` is dealt: the det pack's explosion where it blew up,
/// and the owner (`user`, while in use) let go of it, its E-Web gone for good — the holdable
/// taken away, and the next one chosen if it was the chosen one.
pub fn after_blast(
    eweb: &mut EWeb,
    user: Option<&mut User>,
    blast: Blast,
    level_time: i32,
    host: &mut dyn TurretHost,
) {
    play_effect(
        host,
        EFFECT_EXPLOSION_DETPACK,
        blast.origin,
        [1.0, 0.0, 0.0],
    );
    let Some(user) = user.filter(|user| user.in_use) else {
        return;
    };
    disattach(user, Some(eweb), level_time);
    // "make sure it resets next time we spawn one".
    user.kit.health = -1;
    // "take it away from him, it is gone forever."
    user.items &= !(1 << HI_EWEB);
    if user.selected > 0 && crate::holdables::tag_of(user.selected as u32) == HI_EWEB {
        user.selected = crate::holdables::cycle(user.items as u32, 0, 1) as i32;
    }
}

/// `trap->LinkEntity` with the box it has now: `s.solid` packed from it, and the box other
/// moves find it by.
fn link(eweb: &mut EWeb, bounds: ([f32; 3], [f32; 3])) {
    eweb.linked = linked_box(eweb.origin, bounds);
    eweb.state.set_raw_field(ES_SOLID, packed_solid(bounds));
}

impl EWeb {
    /// The E-Web in other moves' way: its box at its origin, found by the box it was last
    /// linked with.
    pub fn obstacle(&self) -> BoxObstacle {
        BoxObstacle {
            entity: self.number,
            origin: self.origin,
            bounds: BOUNDS,
            contents: CONTENTS,
            model: None,
            linked_box: Some(self.linked),
        }
    }
}

fn read3(state: &EntityState, fields: [usize; 3]) -> [f32; 3] {
    fields.map(|field| f32::from_bits(state.raw_field(field).unwrap_or(0)))
}

impl EWeb {
    /// The E-Web, entity `number`, as a mover's push takes it (a physics object,
    /// `EWeb_Create`, `g_items.c:1921`): it blocks a mover it is in the way of — unarmed
    /// (`s.weapon` none), it is no one a stuck mover breaks.
    pub fn pushee(&self, number: u16) -> crate::path_movers::Pushee {
        let fate = crate::path_movers::Fate::Blocks;
        crate::item_physics::pushee(number, &self.state, BOUNDS, fate, false)
    }

    /// `G_TryPushingEntity` moving the E-Web to `to` ([`crate::item_physics::shove`]),
    /// linked there.
    pub fn shove(&mut self, to: [f32; 3], carried: bool) {
        crate::item_physics::shove(&mut self.state, &mut self.origin, to, carried);
        link(self, BOUNDS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_use_is_refused_while_busy_and_puts_away_a_deployed_one() {
        let mut user = User {
            number: 0,
            kit: Kit::default(),
            in_use: true,
            connected: true,
            health: 100,
            team: 0,
            buttons: 0,
            bounds: ([-15.0, -15.0, -24.0], [15.0, 15.0, 40.0]),
            origin: [0.0; 3],
            view: [0.0; 3],
            weapon: 3,
            weapons: 0x19,
            items: 1 << HI_EWEB,
            selected: crate::holdables::index_of(HI_EWEB) as i32,
            emplaced: 0,
            legs_anim: 0,
            legs_timer: 0,
            weapon_time: 300,
            hand_extend: 0,
            legs_request: None,
        };
        assert_eq!(use_eweb(&mut user, 1_000), Use::Refused);
        user.weapon_time = 0;
        assert_eq!(use_eweb(&mut user, 1_000), Use::Deploy);
        assert_eq!(user.kit.time, 2_000);
        user.kit.index = 56;
        user.emplaced = 56;
        assert_eq!(use_eweb(&mut user, 1_500), Use::Refused);
        assert_eq!(use_eweb(&mut user, 2_050), Use::PutAway);
        disattach(&mut user, None, 2_050);
        assert_eq!((user.kit.index, user.emplaced), (0, 0));
    }
}
