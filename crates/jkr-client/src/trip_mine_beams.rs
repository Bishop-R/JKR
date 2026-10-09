//! The laser of an armed trip mine (`CG_General`, `codemp/cgame/cg_ents.c:1903-1935`).
//!
//! A mine stuck to a surface and armed (`laserTrapStick`, `codemp/game/g_weapon.c`: `s.time`
//! -1 and `EF_FIRING`, the surface normal in `pos.trDelta`) plays `tripMine/laserMP` 6.6
//! units in front of its body along that normal on every client frame; a proximity mine
//! (`bolt2` 1) plays `tripMine/glowbit` there instead. While the viewed player's Force
//! Sight is on, the laser plays twice more per Sight level, so it shows brighter.

use crate::map_effects::angle_vectors_forward;
use crate::{legacy_evaluate_trajectory, legacy_evaluate_trajectory_angles};
use jkr_protocol::{EntityState, PlayerState};

/// `ET_GENERAL` (`codemp/game/bg_public.h`).
const ET_GENERAL: u8 = 1;
/// `WP_TRIP_MINE` (`codemp/game/bg_weapons.h:45`).
const WP_TRIP_MINE: u8 = 13;
/// `EF_FIRING` (`codemp/game/bg_public.h:752`).
const EF_FIRING: u32 = 1 << 9;
/// `FP_SEE`'s bit in the playerstate's `fd.forcePowersActive`.
const FORCE_SIGHT_ACTIVE: u32 = 1 << 14;
/// The playerstate's `fd.forcePowerLevel[FP_SEE]` wire field (`msg.cpp:1396`).
const PS_FORCE_SIGHT_LEVEL: usize = 109;
/// `VectorMA( ent.origin, 6.6f, ent.axis[0], beamOrg )`.
const BEAM_FORWARD: f32 = 6.6;

/// The laser: `tripMine/laserMP`, as the effect library names it.
pub const TRIP_MINE_LASER_EFFECT: &str = "tripmine/lasermp";
/// A proximity mine's glow: `tripMine/glowbit`.
pub const TRIP_MINE_GLOW_EFFECT: &str = "tripmine/glowbit";

/// One armed mine's effect for a client frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TripMineBeam {
    /// [`TRIP_MINE_LASER_EFFECT`] or [`TRIP_MINE_GLOW_EFFECT`].
    pub effect_name: &'static str,
    /// `beamOrg`: 6.6 units in front of the mine.
    pub origin: [f32; 3],
    /// The effect's forward axis, the mine's `pos.trDelta`.
    pub direction: [f32; 3],
    /// How many times the frame plays it: one, and two more per Force Sight level.
    pub plays: u8,
}

/// `entity`'s beam at client time `time`, seen with the snapshot playerstate `player`;
/// `None` for anything that is not an armed trip mine.
pub fn trip_mine_beam(
    entity: &EntityState,
    player: &PlayerState,
    time: i32,
) -> Option<TripMineBeam> {
    if entity.entity_type() != ET_GENERAL
        || entity.weapon() != WP_TRIP_MINE
        || entity.time() != -1
        || entity.e_flags() & EF_FIRING == 0
    {
        return None;
    }
    let origin = legacy_evaluate_trajectory(
        entity.trajectory_base(),
        entity.trajectory_delta(),
        entity.trajectory_type(),
        entity.trajectory_time(),
        entity.trajectory_duration(),
        time,
    );
    let angles = legacy_evaluate_trajectory_angles(
        entity.angular_trajectory_base(),
        entity.angular_trajectory_delta(),
        entity.angular_trajectory_type(),
        entity.angular_trajectory_time(),
        entity.angular_trajectory_duration(),
        time,
    );
    let forward = angle_vectors_forward(angles);
    let (effect_name, plays) = if entity.bolt2() == 1 {
        (TRIP_MINE_GLOW_EFFECT, 1)
    } else if player.force_powers_active() & FORCE_SIGHT_ACTIVE != 0 {
        let level = player.raw_field(PS_FORCE_SIGHT_LEVEL).unwrap_or(0).min(3) as u8;
        (TRIP_MINE_LASER_EFFECT, 1 + 2 * level)
    } else {
        (TRIP_MINE_LASER_EFFECT, 1)
    };
    Some(TripMineBeam {
        effect_name,
        origin: std::array::from_fn(|axis| origin[axis] + BEAM_FORWARD * forward[axis]),
        direction: entity.trajectory_delta(),
        plays,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jkr_protocol::LEGACY_ENTITY_FIELDS;

    const ES_ENTITY_TYPE: usize = 8;
    const ES_WEAPON: usize = 14;
    const ES_EFLAGS: usize = 19;
    const ES_BOLT2: usize = 63;
    const ES_TIME: usize = 65;
    const ES_POS_BASE: [usize; 3] = [2, 1, 4];
    const ES_POS_DELTA: [usize; 3] = [6, 7, 10];
    const ES_APOS_BASE: [usize; 3] = [5, 3, 33];
    const PS_FORCE_POWERS_ACTIVE: usize = 82;

    /// A mine stuck to a wall facing +Y at (100, 200, 300), armed or not.
    fn mine(armed: bool, proximity: bool) -> EntityState {
        let mut state = EntityState::zero(77, &LEGACY_ENTITY_FIELDS);
        state.set_raw_field(ES_ENTITY_TYPE, u32::from(ET_GENERAL));
        state.set_raw_field(ES_WEAPON, u32::from(WP_TRIP_MINE));
        for (axis, value) in [100.0_f32, 200.0, 300.0].into_iter().enumerate() {
            state.set_raw_field(ES_POS_BASE[axis], value.to_bits());
        }
        state.set_raw_field(ES_POS_DELTA[1], 1.0_f32.to_bits());
        state.set_raw_field(ES_APOS_BASE[1], 90.0_f32.to_bits());
        if armed {
            state.set_raw_field(ES_EFLAGS, EF_FIRING);
            state.set_raw_field(ES_TIME, (-1_i32) as u32);
        }
        state.set_raw_field(ES_BOLT2, u32::from(proximity));
        state
    }

    fn player(sight_level: Option<u32>) -> PlayerState {
        let mut player = PlayerState::default();
        if let Some(level) = sight_level {
            player.set_raw_field(PS_FORCE_POWERS_ACTIVE, FORCE_SIGHT_ACTIVE);
            player.set_raw_field(PS_FORCE_SIGHT_LEVEL, level);
        }
        player
    }

    /// The laser plays 6.6 units in front of the mine along its normal, twice more per
    /// Force Sight level while Sight is on; a proximity mine glows instead.
    #[test]
    fn armed_mines_play_their_laser() {
        let beam = trip_mine_beam(&mine(true, false), &player(None), 1_000).unwrap();
        assert_eq!(beam.effect_name, TRIP_MINE_LASER_EFFECT);
        assert_eq!(beam.direction, [0.0, 1.0, 0.0]);
        assert_eq!(beam.plays, 1);
        let expected = [100.0, 206.6, 300.0];
        for (axis, value) in beam.origin.into_iter().enumerate() {
            assert!((value - expected[axis]).abs() < 1e-3, "{:?}", beam.origin);
        }
        let seeing = trip_mine_beam(&mine(true, false), &player(Some(2)), 1_000).unwrap();
        assert_eq!(seeing.plays, 5);
        let proximity = trip_mine_beam(&mine(true, true), &player(Some(3)), 1_000).unwrap();
        assert_eq!((proximity.effect_name, proximity.plays), (TRIP_MINE_GLOW_EFFECT, 1));
    }

    /// Nothing shows before the mine is armed, nor for anything else.
    #[test]
    fn only_armed_trip_mines_have_a_beam() {
        assert_eq!(trip_mine_beam(&mine(false, false), &player(None), 0), None);
        let mut other = mine(true, false);
        other.set_raw_field(ES_WEAPON, 14);
        assert_eq!(trip_mine_beam(&other, &player(None), 0), None);
    }
}
