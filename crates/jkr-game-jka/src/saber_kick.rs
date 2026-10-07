//! Kicks landing (`w_saber.c:7480-7963`). A staff's kicks, the hilt bash and the rolling
//! get-ups are animations the movement plays; `WP_SaberPositionUpdate` makes them strike,
//! first thing every frame, while a client's legs play one (`BG_KickingAnim`):
//! `G_KickSomeMofos` draws the kick's damage and push, and at the moments each animation
//! lands its kick traces a small box from the kicker's middle along the way its foot (or
//! hand) points ([`kick_trace`]); `G_KickTrace` then hits what the box struck — not the same
//! entity twice in one kick — with a punch sound, a fifth of the damage and a toss that may
//! knock the target down ([`land`], [`toss`]).
//!
//! `d_saberKickTweak` (on by default) makes the trace test the models (`G2TRFLAG_*`), cuts
//! the damage to a fifth and spares a saber user standing at the ready the knockdown.

use crate::knockdown::{Knockdown, in_knockdown, knockdownable};
use crate::player_death::Rng;
use crate::pmove_hand_extend::HANDEXTEND_KNOCKDOWN;
use jkr_protocol::PlayerState;

/// `STAFF_KICK_RANGE`.
const STAFF_KICK_RANGE: f32 = 16.0;
/// The kick's box: two units each way (`kickMins`, `kickMaxs`).
pub const KICK_BOX: [f32; 3] = [2.0; 3];
/// The animations that land kicks (`anims.h`).
const BOTH_A7_KICK_F: u16 = 887;
const BOTH_A7_KICK_B: u16 = 888;
const BOTH_A7_KICK_R: u16 = 889;
const BOTH_A7_KICK_L: u16 = 890;
const BOTH_A7_KICK_S: u16 = 891;
const BOTH_A7_KICK_BF: u16 = 892;
const BOTH_A7_KICK_RL: u16 = 894;
const BOTH_A7_KICK_F_AIR: u16 = 895;
const BOTH_A7_KICK_B_AIR: u16 = 896;
const BOTH_A7_KICK_R_AIR: u16 = 897;
const BOTH_A7_KICK_L_AIR: u16 = 898;
const BOTH_A7_HILT: u16 = 1_266;
const BOTH_GETUP_BROLL_B: u16 = 1_239;
const BOTH_GETUP_BROLL_F: u16 = 1_240;
const BOTH_GETUP_FROLL_B: u16 = 1_243;
const BOTH_GETUP_FROLL_F: u16 = 1_244;
/// The game's generator as a kick draws from it: `Q_irand` and `flrand`.
pub trait Draws {
    fn irand(&mut self, low: i32, high: i32) -> i32;
    fn flrand(&mut self, low: f32, high: f32) -> f32;
}

impl Draws for Rng {
    fn irand(&mut self, low: i32, high: i32) -> i32 {
        Rng::irand(self, low, high)
    }

    fn flrand(&mut self, low: f32, high: f32) -> f32 {
        Rng::flrand(self, low, high)
    }
}

/// `HANDEXTEND_KNOCKDOWN`'s length from a toss.
const TOSS_KNOCKDOWN_MS: i32 = 700;
/// The wire fields a toss reads and writes: `velocity`, `forceHandExtend`,
/// `forceDodgeAnim`, `pm_flags`, `weaponTime`, `weapon`, `groundEntityNum`.
const PS_FORCE_HAND_EXTEND: usize = 80;
const PS_FORCE_DODGE_ANIM: usize = 89;
/// `WP_SABER`; `ENTITYNUM_NONE`.
const WP_SABER: u8 = 3;
const ENTITYNUM_NONE: u16 = 1_023;

/// A kicker's last hit (`jediKickIndex`, `jediKickTime`): the entity is not hit again
/// before the time, the end of the kick that hit it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KickMemory {
    pub index: u16,
    pub until: i32,
}

/// The bolt a kick takes its way from (`renderInfo`'s `footRBolt`, `footLBolt`,
/// `handRBolt`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KickBolt {
    RightFoot,
    LeftFoot,
    RightHand,
}

impl KickBolt {
    /// The model's surface bolt (`*r_leg_foot`, `*l_leg_foot`, `*r_hand`).
    pub fn name(self) -> &'static str {
        match self {
            Self::RightFoot => "*r_leg_foot",
            Self::LeftFoot => "*l_leg_foot",
            Self::RightHand => "*r_hand",
        }
    }
}

/// What `G_KickSomeMofos` reads of the kicker this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kicker {
    pub legs_anim: u16,
    pub torso_anim: u16,
    /// `BG_AnimLength(localAnimIndex, legsAnim)`.
    pub animation_length: i32,
    pub legs_timer: i32,
    /// `ps.viewangles[YAW]`.
    pub yaw: f32,
    /// `ps.origin` and `r.currentOrigin`.
    pub origin: [f32; 3],
    pub current_origin: [f32; 3],
    /// `r.maxs`.
    pub maxs: [f32; 3],
    /// `level.framenum & 1`.
    pub odd_frame: bool,
    /// The model has the kick bolts (`renderInfo`'s are set): without them the reference
    /// guesses the way, in windows of its own for the spin and the double kick.
    pub bolts: bool,
}

/// A kick traced this frame: the box's sweep and what it carries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KickTrace {
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub direction: [f32; 3],
    /// `kickDamage` (the tweak takes a fifth of it) and `kickPush`.
    pub damage: i32,
    pub push: i32,
}

/// Where the kick goes when it goes: by a bolt, or "guessed" along the yaw.
enum Aim {
    Bolt(KickBolt, Origin),
    Guess(f32, Axis),
}

/// The origin a bolt's way is measured from: `ps.origin` (the hilt bash and the
/// roll-ups) or `r.currentOrigin`.
#[derive(Clone, Copy)]
enum Origin {
    Player,
    Current,
}

/// Which of the yaw's axes a guess takes, and its sign.
#[derive(Clone, Copy)]
enum Axis {
    Forward,
    Back,
    Right,
    Left,
}

/// `G_KickSomeMofos` up to its trace (`w_saber.c:7651-7963`): the damage (`Q_irand(10, 15)`)
/// and push (`flrand(50, 100)`, whole) drawn every frame, a stronger push for the spin and
/// the double kicks, then whether the animation lands a kick now and along which way —
/// `bolt` answers `G_GetBoltPosition` for the kicker, asked only when a kick lands by one.
/// The trace goes from the kicker's middle as far as its reach (`maxs[0] * 1.5 + 24`, more
/// for the double kicks); `None` when no kick lands this frame.
pub fn kick_trace(
    kicker: &Kicker,
    rng: &mut impl Draws,
    mut bolt: impl FnMut(KickBolt) -> [f32; 3],
) -> Option<KickTrace> {
    let elapsed = kicker.animation_length - kicker.legs_timer;
    let remaining = kicker.legs_timer;
    let mut distance = kicker.maxs[0] * 1.5 + STAFF_KICK_RANGE + 8.0;
    let damage = rng.irand(10, 15);
    let mut push = rng.flrand(50.0, 100.0) as i32;
    let front = elapsed >= 250 && remaining >= 250;
    // A plain kick by a bolt, or along the yaw without one.
    let plain = |foot: KickBolt, from: Origin, axis: Axis| {
        if kicker.bolts {
            Aim::Bolt(foot, from)
        } else {
            Aim::Guess(0.0, axis)
        }
    };
    let aim = if kicker.torso_anim == BOTH_A7_HILT {
        front.then(|| plain(KickBolt::RightHand, Origin::Player, Axis::Forward))
    } else {
        match kicker.legs_anim {
            BOTH_GETUP_BROLL_B | BOTH_GETUP_BROLL_F | BOTH_GETUP_FROLL_B | BOTH_GETUP_FROLL_F => {
                front.then(|| plain(KickBolt::RightFoot, Origin::Player, Axis::Forward))
            }
            BOTH_A7_KICK_F_AIR | BOTH_A7_KICK_B_AIR | BOTH_A7_KICK_R_AIR | BOTH_A7_KICK_L_AIR => {
                (elapsed >= 100 && remaining >= 250)
                    .then(|| plain(KickBolt::RightFoot, Origin::Current, Axis::Forward))
            }
            BOTH_A7_KICK_F => {
                front.then(|| plain(KickBolt::RightFoot, Origin::Current, Axis::Forward))
            }
            BOTH_A7_KICK_B => {
                front.then(|| plain(KickBolt::RightFoot, Origin::Current, Axis::Back))
            }
            BOTH_A7_KICK_R => {
                front.then(|| plain(KickBolt::RightFoot, Origin::Current, Axis::Right))
            }
            BOTH_A7_KICK_L => front.then(|| plain(KickBolt::LeftFoot, Origin::Current, Axis::Left)),
            BOTH_A7_KICK_S => {
                push = rng.flrand(75.0, 125.0) as i32;
                if kicker.bolts {
                    (550..=1050)
                        .contains(&elapsed)
                        .then_some(Aim::Bolt(KickBolt::RightFoot, Origin::Current))
                } else {
                    spin_guess(elapsed)
                }
            }
            BOTH_A7_KICK_BF => {
                push = rng.flrand(75.0, 125.0) as i32;
                distance += 20.0;
                if kicker.bolts {
                    ((750..850).contains(&elapsed) || (1400..1500).contains(&elapsed))
                        .then_some(Aim::Bolt(KickBolt::RightFoot, Origin::Current))
                } else {
                    match elapsed {
                        250..=349 => Some(Aim::Guess(0.0, Axis::Forward)),
                        350..=449 => Some(Aim::Guess(0.0, Axis::Back)),
                        _ => None,
                    }
                }
            }
            BOTH_A7_KICK_RL => {
                push = rng.flrand(75.0, 125.0) as i32;
                distance += 10.0;
                // "tracing constantly on these things": a foot each frame.
                Some(if kicker.odd_frame {
                    plain(KickBolt::RightFoot, Origin::Current, Axis::Right)
                } else {
                    plain(KickBolt::LeftFoot, Origin::Current, Axis::Left)
                })
            }
            _ => None,
        }
    }?;
    let direction = match aim {
        Aim::Bolt(which, from) => {
            let point = bolt(which);
            let origin = match from {
                Origin::Player => kicker.origin,
                Origin::Current => kicker.current_origin,
            };
            let mut direction = [point[0] - origin[0], point[1] - origin[1], 0.0];
            crate::saber_clash::normalize(&mut direction);
            direction
        }
        Aim::Guess(offset, axis) => along(kicker.yaw + offset, axis),
    };
    let current = kicker.current_origin;
    let start = [current[0], current[1], current[2] + kicker.maxs[2] * 0.5];
    let end = std::array::from_fn(|axis| start[axis] + distance * direction[axis]);
    Some(KickTrace {
        start,
        end,
        direction,
        damage,
        push,
    })
}

/// The spin kick's guess for a model without the bolt (`w_saber.c:7843-7889`): a way
/// each tenth of a second, round from the front — every other one 45 degrees on, as the
/// reference turns the yaw for them (its back-left turns the yaw for the forward, its
/// front-left the right reversed).
fn spin_guess(elapsed: i32) -> Option<Aim> {
    let (offset, axis) = match elapsed {
        400..=499 => (0.0, Axis::Forward),
        500..=599 => (45.0, Axis::Forward),
        600..=699 => (0.0, Axis::Right),
        700..=799 => (45.0, Axis::Right),
        800..=899 => (0.0, Axis::Back),
        900..=999 => (45.0, Axis::Forward),
        1000..=1099 => (0.0, Axis::Left),
        1100..=1199 => (45.0, Axis::Left),
        _ => return None,
    };
    Some(Aim::Guess(offset, axis))
}

/// `AngleVectors((0, yaw, 0))`'s forward or right, or either reversed.
fn along(yaw: f32, axis: Axis) -> [f32; 3] {
    let (forward, right, _) = crate::npc_sniper::angle_vectors([0.0, yaw, 0.0]);
    match axis {
        Axis::Forward => forward,
        Axis::Back => forward.map(|value| -value),
        Axis::Right => right,
        Axis::Left => right.map(|value| -value),
    }
}

/// What the kick's box struck (`trace_t`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Struck {
    pub fraction: f32,
    pub start_solid: bool,
    pub all_solid: bool,
    pub entity: u16,
    pub end: [f32; 3],
}

/// The entity struck, as `G_KickTrace` reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Target {
    pub in_use: bool,
    pub takes_damage: bool,
    pub client: bool,
}

/// The struck client after the blow, as the toss reads it: its health, whether it flies
/// from a knockback already (`PMF_TIME_KNOCKBACK`), and whether the kicker may treat it as
/// an enemy (`G_CanBeEnemy`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AfterBlow {
    pub health: i32,
    pub knocked_back: bool,
    pub enemy: bool,
}

/// What landing a kick asks of the world, whose generator its draws share with the blow's.
pub trait Landing: Draws {
    /// `G_Sound(kicker, CHAN_AUTO, G_SoundIndex(name))`.
    fn sound(&mut self, name: &str);
    /// `g_entities[number]` as the kick reads it.
    fn target(&mut self, number: u16) -> Target;
    /// `G_Damage(target, kicker, kicker, direction, point, amount, DAMAGE_NO_KNOCKBACK,
    /// MOD_MELEE)`, a client struck credited to the kicker for ten seconds (`otherKiller`).
    /// Whether the blow got past `G_Damage`'s refusals (a duel, a protected rider, ...) to
    /// where it normalizes the direction in place — the kick's own, which the toss then
    /// takes.
    fn damage(&mut self, number: u16, direction: [f32; 3], point: [f32; 3], amount: i32) -> bool;
    /// The struck client after the blow; `None` for anything else.
    fn after_blow(&mut self, number: u16) -> Option<AfterBlow>;
    /// `G_TossTheMofo(target, direction, strength)` ([`toss`]).
    fn toss(&mut self, number: u16, direction: [f32; 3], strength: f32);
}

/// `G_KickTrace` after its trace (`w_saber.c:7562-7648`): a strike, not the entity this
/// kick already hit, is remembered till the kick ends; the punch sounds (a hilt bash
/// slams); what takes damage takes the kick's (a fifth of it with `tweak`); and a client
/// the kicker may treat as an enemy, not flying already, is tossed — four times the push
/// when the blow killed it, else the push, or 300 one time in three when the push is 75
/// or more. The struck entity, if any.
#[allow(clippy::too_many_arguments)]
pub fn land(
    kick: &KickTrace,
    struck: Struck,
    memory: &mut KickMemory,
    kicker: &Kicker,
    level_time: i32,
    tweak: bool,
    world: &mut impl Landing,
) -> Option<u16> {
    if struck.fraction >= 1.0 || struck.start_solid || struck.all_solid {
        return None;
    }
    if memory.until > level_time && struck.entity == memory.index {
        return None;
    }
    *memory = KickMemory {
        index: struck.entity,
        until: level_time + kicker.legs_timer,
    };
    if kicker.torso_anim == BOTH_A7_HILT {
        world.sound("sound/movers/objects/saber_slam");
    } else {
        let name = format!("sound/weapons/melee/punch{}", world.irand(1, 4));
        world.sound(&name);
    }
    let target = world.target(struck.entity);
    if !target.in_use {
        return Some(struck.entity);
    }
    let mut direction = kick.direction;
    if target.takes_damage {
        let amount = if tweak {
            (kick.damage as f32 * 0.2) as i32
        } else {
            kick.damage
        };
        if world.damage(struck.entity, direction, struck.end, amount) {
            crate::saber_clash::normalize(&mut direction);
        }
    }
    let Some(after) = world
        .after_blow(struck.entity)
        .filter(|after| target.client && !after.knocked_back && after.enemy)
    else {
        return Some(struck.entity);
    };
    let push = kick.push as f32;
    let strength = if after.health <= 0 {
        push * 4.0
    } else if push >= 75.0 && world.irand(0, 2) == 0 {
        300.0
    } else {
        push
    };
    world.toss(struck.entity, direction, strength);
    Some(struck.entity)
}

/// A client as `G_CanBeEnemy` reads it: its number, the one it duels, its team.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Side {
    pub number: u16,
    /// `ps.duelIndex` while `ps.duelInProgress`.
    pub duel: Option<u16>,
    /// `sess.sessionTeam`.
    pub team: i32,
}

/// `G_CanBeEnemy` (`w_saber.c:106-131`): not one duelling someone else, nor one that
/// duels someone else; anyone outside the team games; with `g_friendlyFire` anyone;
/// else one of another team (`OnSameTeam`'s team games).
pub fn can_be_enemy(me: Side, other: Side, gametype: i32, friendly_fire: bool) -> bool {
    /// `GT_TEAM`.
    const GT_TEAM: i32 = 6;
    if me.duel.is_some_and(|with| with != other.number)
        || other.duel.is_some_and(|with| with != me.number)
    {
        return false;
    }
    gametype < GT_TEAM || friendly_fire || me.team != other.team
}

/// `G_TossTheMofo` (`w_saber.c:7512-7535`) on a client's state: thrown along `direction`
/// at `strength` and 200 up; and, alive, not down already, knock-downable and — with
/// `tweak` — not a saber user standing at the ready nor already down or getting up
/// (`G_KickDownable`), knocked down for 700 ms. A vehicle is not tossed (the caller's
/// check).
pub fn toss(
    state: &mut PlayerState,
    memory: &mut Knockdown,
    health: i32,
    direction: [f32; 3],
    strength: f32,
    level_time: i32,
    tweak: bool,
) {
    let mut velocity = state.velocity();
    for (value, way) in velocity.iter_mut().zip(direction) {
        *value += strength * way;
    }
    velocity[2] = 200.0;
    state.set_velocity(velocity);
    let down = state.raw_field(PS_FORCE_HAND_EXTEND).unwrap_or(0) as u8 == HANDEXTEND_KNOCKDOWN;
    if health > 0 && !down && knockdownable(state) && kick_downable(state, tweak) {
        state.set_raw_field(PS_FORCE_HAND_EXTEND, u32::from(HANDEXTEND_KNOCKDOWN));
        memory.hand_extend_time = level_time + TOSS_KNOCKDOWN_MS;
        state.set_raw_field(PS_FORCE_DODGE_ANIM, 0);
    }
}

/// `G_KickDownable` (`w_saber.c:7484-7510`) for a client: with `tweak`, not one down or
/// getting up, nor a saber user standing at the ready (no swing running, on the ground).
fn kick_downable(state: &PlayerState, tweak: bool) -> bool {
    if !tweak {
        return true;
    }
    if in_knockdown(state.leg_animation()) || in_knockdown(state.torso_animation()) {
        return false;
    }
    !(state.weapon_time() <= 0
        && state.weapon() == WP_SABER
        && state.ground_entity_num() != ENTITYNUM_NONE)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A client standing on the ground holding `weapon`, idle.
    fn standing(weapon: u8) -> PlayerState {
        let mut state = PlayerState::zero();
        state.set_raw_field(47, u32::from(weapon));
        state.set_ground_entity_num(1_022);
        state
    }

    #[test]
    fn a_toss_throws_and_knocks_down_all_but_a_saber_at_the_ready() {
        let mut memory = Knockdown::default();
        let mut pistol = standing(4);
        toss(
            &mut pistol,
            &mut memory,
            100,
            [0.5, 0.75, 0.0],
            40.0,
            1_000,
            true,
        );
        assert_eq!(pistol.velocity(), [20.0, 30.0, 200.0]);
        assert_eq!(
            pistol.raw_field(PS_FORCE_HAND_EXTEND),
            Some(u32::from(HANDEXTEND_KNOCKDOWN))
        );
        assert_eq!(memory.hand_extend_time, 1_700);
        // A saber held still on the ground is only thrown (`G_KickDownable`)...
        let mut saber = standing(WP_SABER);
        let mut memory = Knockdown::default();
        toss(
            &mut saber,
            &mut memory,
            100,
            [1.0, 0.0, 0.0],
            50.0,
            1_000,
            true,
        );
        assert_eq!(saber.velocity(), [50.0, 0.0, 200.0]);
        assert_eq!(saber.raw_field(PS_FORCE_HAND_EXTEND), Some(0));
        // ...unless the tweak is off; and the dead are not knocked down.
        toss(
            &mut saber,
            &mut memory,
            100,
            [1.0, 0.0, 0.0],
            50.0,
            1_000,
            false,
        );
        assert_eq!(
            saber.raw_field(PS_FORCE_HAND_EXTEND),
            Some(u32::from(HANDEXTEND_KNOCKDOWN))
        );
        let mut dead = standing(4);
        toss(
            &mut dead,
            &mut memory,
            0,
            [1.0, 0.0, 0.0],
            200.0,
            1_000,
            true,
        );
        assert_eq!(dead.raw_field(PS_FORCE_HAND_EXTEND), Some(0));
    }

    #[test]
    fn duels_and_teams_decide_who_a_kick_may_toss() {
        let side = |number, duel, team| Side { number, duel, team };
        // Free for all: anyone, but not across a duel.
        assert!(can_be_enemy(side(0, None, 0), side(1, None, 0), 0, false));
        assert!(!can_be_enemy(
            side(0, Some(2), 0),
            side(1, None, 0),
            0,
            false
        ));
        assert!(!can_be_enemy(
            side(0, None, 0),
            side(1, Some(2), 0),
            0,
            false
        ));
        assert!(can_be_enemy(
            side(0, Some(1), 0),
            side(1, Some(0), 0),
            0,
            false
        ));
        // Team games: only the other team, unless friendly fire is on.
        assert!(!can_be_enemy(side(0, None, 1), side(1, None, 1), 6, false));
        assert!(can_be_enemy(side(0, None, 1), side(1, None, 2), 6, false));
        assert!(can_be_enemy(side(0, None, 1), side(1, None, 1), 6, true));
    }

    #[test]
    fn the_kick_animations_are_the_reference_numbers() {
        let named = |name: &str| {
            crate::legacy_animation::NAMES
                .iter()
                .position(|known| *known == name)
                .map(|index| index as u16)
        };
        for (number, name) in [
            (BOTH_A7_KICK_F, "BOTH_A7_KICK_F"),
            (BOTH_A7_KICK_B, "BOTH_A7_KICK_B"),
            (BOTH_A7_KICK_R, "BOTH_A7_KICK_R"),
            (BOTH_A7_KICK_L, "BOTH_A7_KICK_L"),
            (BOTH_A7_KICK_S, "BOTH_A7_KICK_S"),
            (BOTH_A7_KICK_BF, "BOTH_A7_KICK_BF"),
            (BOTH_A7_KICK_RL, "BOTH_A7_KICK_RL"),
            (BOTH_A7_KICK_F_AIR, "BOTH_A7_KICK_F_AIR"),
            (BOTH_A7_KICK_B_AIR, "BOTH_A7_KICK_B_AIR"),
            (BOTH_A7_KICK_R_AIR, "BOTH_A7_KICK_R_AIR"),
            (BOTH_A7_KICK_L_AIR, "BOTH_A7_KICK_L_AIR"),
            (BOTH_A7_HILT, "BOTH_A7_HILT"),
            (BOTH_GETUP_BROLL_B, "BOTH_GETUP_BROLL_B"),
            (BOTH_GETUP_BROLL_F, "BOTH_GETUP_BROLL_F"),
            (BOTH_GETUP_FROLL_B, "BOTH_GETUP_FROLL_B"),
            (BOTH_GETUP_FROLL_F, "BOTH_GETUP_FROLL_F"),
        ] {
            assert_eq!(named(name), Some(number), "{name}");
            assert!(crate::saber_rules::kicking(number), "{name} kicks");
        }
    }
}
