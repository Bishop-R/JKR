//! The concussion rifle's alternate fire (OpenJK `codemp/game/g_weapon.c:3080-3339`): a
//! rail-like beam of up to three traces through whatever it strikes, twenty-five to what
//! takes damage without knockback or hit location, then for a player the shove and the
//! knockdown of `knockdown::concussion_shove`. A mover — glass, a breakable brush, a door —
//! stops it after the blow, and any other solid before; the shooter is shoved backwards
//! for a quarter second; one temp entity for the whole beam. An NPC is hurt but not
//! knocked down, and Galak's armour (ten points, the beam stopped) is not modelled.

use crate::damage::{Attacker, DAMAGE_NO_HIT_LOC, DAMAGE_NO_KNOCKBACK, DamageRequest};
use crate::event_entity::EventEntity;
use crate::pmove::MovementTrace;
use crate::shot_target::ShotTarget;
use jkr_protocol::{PlayerState, legacy_direction_to_byte};

/// `CONC_ALT_DAMAGE`, `DISRUPTOR_ALT_TRACES`, the shot's range, `MOD_CONC_ALT`.
const CONC_ALT_DAMAGE: i32 = 25;
const ALT_TRACES: usize = 3;
const SHOT_RANGE: f32 = 8_192.0;
const MOD_CONC_ALT: u32 = 30;
/// `MASK_SHOT`, `SURF_NOIMPACT`, `MAX_CLIENTS`.
const MASK_SHOT: u32 = 0x1 | 0x100 | 0x200 | 0x1000;
const SURF_NOIMPACT: u32 = 0x10;
const MAX_CLIENTS: u16 = 32;
/// `EV_CONC_ALT_IMPACT` and the fields its entity carries: `owner`, `angles`,
/// `origin2`, `angles2`.
const EV_CONC_ALT_IMPACT: u32 = 84;
const ES_OWNER: usize = 40;
const ES_ANGLES: [usize; 3] = [25, 9, 24];
const ES_ORIGIN2: [usize; 3] = [56, 60, 53];
const ES_ANGLES2: [usize; 3] = [82, 51, 84];
/// `PMF_DUCKED`, `ENTITYNUM_NONE`.
const PMF_DUCKED: u16 = 1;
const ENTITY_NONE: u16 = 1_023;

/// The world as the beam traces and strikes it.
pub trait ConcussionTargets {
    /// `trap->Trace` from `start` to `end` in a box of one, passing `skip`.
    fn trace(&mut self, skip: u16, start: [f32; 3], end: [f32; 3], mask: u32) -> MovementTrace;
    /// A player entity that takes damage (a corpse too): whether it counts for the
    /// accuracy (`LogAccuracyHit`), its `FL_NO_KNOCKBACK`, whether it holds a saber;
    /// `None` for anything else.
    fn player(&self, number: u16) -> Option<StruckPlayer>;
    /// `Jedi_DodgeEvasion(other, shooter, &tr, HL_NONE)`: without a hit location nobody
    /// dodges, though `g_forceDodge 2` still draws for it ([`crate::force_dodge`]).
    fn dodges(&mut self, number: u16) -> bool;
    /// What the beam makes of `number` (`g_entities[tr.entityNum]`).
    fn target(&mut self, number: u16) -> ShotTarget;
    /// `accuracy_hits++` for a player struck that counts.
    fn count_hit(&mut self);
    /// `G_Damage` on `number`, whatever it is.
    fn hurt(&mut self, number: u16, request: DamageRequest);
    /// The shove and the knockdown after the damage (`knockdown::concussion_shove`, with
    /// `FL_NO_KNOCKBACK` as it was before the damage).
    fn shove(
        &mut self,
        number: u16,
        no_knockback: bool,
        forward: [f32; 3],
        shooter: u16,
        shooter_origin: [f32; 3],
    );
    /// `G_TempEntity`.
    fn raise(&mut self, event: EventEntity);
}

/// A player the beam struck, as the fire reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StruckPlayer {
    pub counts: bool,
    pub no_knockback: bool,
    /// `s.weapon == WP_SABER`: the beam asks it to dodge, without a hit location.
    pub wields_saber: bool,
}

/// `WP_FireConcussionAlt` by `state`'s player: the shooter shoved 200 back along the
/// view (its ground lost, `pm_time` 250 — 100 hunkered down) on the wire state; the
/// traces from the muzzle 8192 along the view (`MASK_SHOT`, a box of one, the shooter
/// then each thing struck passed), a player struck hurt (twenty-five, no knockback, no
/// hit location, `MOD_CONC_ALT`, counted when living) and shoved, the traces going on
/// through it (and through one that dodges) — stopped by the world, the sky, a mover;
/// then the beam's one temp entity at the last end: the normal's byte, the owner, the
/// direction from the muzzle as the angles, the muzzle as `origin2`, the view's forward
/// as `angles2`. (`W_TraceSetStart` with no box is a no-op for a player; Galak Mech and
/// breakables are not here.)
pub fn fire_alt(
    state: &mut PlayerState,
    muzzle: [f32; 3],
    forward: [f32; 3],
    attacker: Attacker,
    level_time: i32,
    targets: &mut dyn ConcussionTargets,
) {
    let shooter = state.client_num();
    let mut velocity = state.velocity();
    for axis in 0..3 {
        velocity[axis] += -200.0 * forward[axis];
    }
    state.set_velocity(velocity);
    state.set_ground_entity_num(ENTITY_NONE);
    state.set_movement_time(if state.movement_flags() & PMF_DUCKED != 0 {
        100
    } else {
        250
    });
    let shooter_origin = state.origin();
    let mut start = muzzle;
    let mut skip = shooter;
    let mut trace = MovementTrace::miss(muzzle);
    for _ in 0..ALT_TRACES {
        let end: [f32; 3] = std::array::from_fn(|axis| start[axis] + SHOT_RANGE * forward[axis]);
        trace = targets.trace(skip, start, end, MASK_SHOT);
        let render_impact = trace.surface_flags & SURF_NOIMPACT == 0;
        if trace.entity_number == shooter {
            start = trace.end_position;
            skip = trace.entity_number;
            continue;
        }
        if trace.fraction >= 1.0 {
            break;
        }
        if !render_impact {
            break;
        }
        let struck = trace.entity_number;
        let player = (struck < MAX_CLIENTS)
            .then(|| targets.player(struck))
            .flatten();
        if player.is_some_and(|player| player.wields_saber) && targets.dodges(struck) {
            start = trace.end_position;
            skip = struck;
            continue;
        }
        // A solid that takes no damage stops the beam; what takes damage is hurt, and a
        // mover (glass, a breakable brush, a door) stops it after the blow.
        let target = targets.target(struck);
        if !target.takes_damage && !target.glass_or_mover {
            break;
        }
        if player.is_some_and(|player| player.counts) {
            targets.count_hit();
        }
        if target.takes_damage {
            let request = DamageRequest {
                level_time,
                attacker: Some(attacker),
                direction: Some(forward),
                point: Some(trace.end_position),
                damage: CONC_ALT_DAMAGE,
                flags: DAMAGE_NO_KNOCKBACK | DAMAGE_NO_HIT_LOC,
                means: MOD_CONC_ALT,
            };
            targets.hurt(struck, request);
        }
        if let Some(player) = player {
            targets.shove(
                struck,
                player.no_knockback,
                forward,
                shooter,
                shooter_origin,
            );
        }
        if target.glass_or_mover {
            break;
        }
        start = trace.end_position;
        skip = struck;
    }
    let direction: [f32; 3] = std::array::from_fn(|axis| trace.end_position[axis] - muzzle[axis]);
    let mut event = EventEntity {
        event: EV_CONC_ALT_IMPACT,
        parameter: u32::from(legacy_direction_to_byte(trace.plane_normal)),
        origin: trace.end_position,
        client: None,
        broadcast: false,
        extra: [(0, 0); 12],
    };
    event.extra[0] = (ES_OWNER, u32::from(shooter));
    for axis in 0..3 {
        event.extra[1 + axis] = (ES_ANGLES[axis], direction[axis].to_bits());
        event.extra[4 + axis] = (ES_ORIGIN2[axis], muzzle[axis].to_bits());
        event.extra[7 + axis] = (ES_ANGLES2[axis], forward[axis].to_bits());
    }
    targets.raise(event);
}
