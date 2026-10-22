//! The jetpack's flight (`pm_type` `PM_JETPACK`, which the game sets while a player's
//! jetpack is on): `PmoveSingle`'s own block (`bg_pmove.c:10712-10811`). Gravity is cut to a
//! tenth within 128 units of the ground and to a quarter above; the legs hold the in-air
//! loop for the way the player steers; holding jump adds up to 12 to the climb each move
//! (less the higher a rising player is, never past 256) and lights the flames
//! (`EF_JETPACK_FLAMING`); otherwise the fall is held to 100 and the player is lifted 2 each
//! move while within 64 units of the ground. The other moves treat a jetpacker as off the
//! ground and give it more air control while thrusting (`PM_AirMove`).
//!
//! Left out: the random shove a spinning saber gets (`Q_irand(-100, 100)` on each horizontal
//! axis), which needs the game's generator; the client cannot predict it either.

use super::{Bounds, MovementCollision, MovementState, UserCommand};

/// `PM_JETPACK`.
pub const PM_JETPACK: u8 = 1;
/// `JETPACK_HOVER_HEIGHT`.
const HOVER_HEIGHT: f32 = 64.0;
/// `EF_JETPACK_FLAMING`.
const EF_JETPACK_FLAMING: u32 = 1 << 30;
/// `MASK_SOLID`.
const MASK_SOLID: u32 = 1;
/// `BOTH_INAIR1`, `BOTH_INAIRBACK1`, `BOTH_INAIRLEFT1`, `BOTH_INAIRRIGHT1`.
const BOTH_INAIR1: u16 = 1_139;
const BOTH_INAIRBACK1: u16 = 1_143;
const BOTH_INAIRLEFT1: u16 = 1_146;
const BOTH_INAIRRIGHT1: u16 = 1_149;

/// Whether the move is a jetpack's.
pub fn active(state: &MovementState) -> bool {
    state.movement_type == PM_JETPACK
}

/// `PM_GroundDistance` (`bg_saber.c:1936-1950`) with the move's box: how far the ground is
/// below the player, 4096 at most.
pub(super) fn ground_distance(
    state: &MovementState,
    bounds: Bounds,
    collision: &impl MovementCollision,
) -> f32 {
    let mut down = state.origin;
    down[2] -= 4_096.0;
    let trace = collision.trace(
        state.origin,
        bounds.minimums,
        bounds.maximums,
        down,
        MASK_SOLID,
    );
    let gap: [f32; 3] = std::array::from_fn(|axis| state.origin[axis] - trace.end_position[axis]);
    (gap[0] * gap[0] + gap[1] * gap[1] + gap[2] * gap[2]).sqrt()
}

/// `pm->ps->gravity *= 0.1f` within 128 units of the ground, `0.25f` above: a float
/// product truncated to the integer gravity.
pub(super) fn lightened_gravity(gravity: f32, ground_distance: f32) -> f32 {
    let scale = if ground_distance < HOVER_HEIGHT + 64.0 {
        0.1_f32
    } else {
        0.25_f32
    };
    ((gravity as i32) as f32 * scale) as i32 as f32
}

/// The legs' in-air loop for the way the player steers, and the climb or the hover.
pub(super) fn steer(state: &mut MovementState, command: &UserCommand, ground_distance: f32) {
    let legs = if command.right_move > 0 {
        BOTH_INAIRRIGHT1
    } else if command.right_move < 0 {
        BOTH_INAIRLEFT1
    } else if command.forward_move < 0 {
        BOTH_INAIRBACK1
    } else {
        BOTH_INAIR1
    };
    crate::pmove_anim::continue_legs(state, legs);
    if command.up_move > 0 && state.velocity[2] < 256.0 {
        // "cap upward velocity off at 256"; a rising player adds less the higher it is.
        let add = if state.velocity[2] > 0.0 {
            12.0 - ground_distance / 64.0
        } else {
            12.0
        };
        if add > 0.0 {
            state.velocity[2] += add;
        }
        state.entity_flags |= EF_JETPACK_FLAMING;
    } else {
        state.entity_flags &= !EF_JETPACK_FLAMING;
        if state.velocity[2] < 256.0 {
            if state.velocity[2] < -100.0 {
                state.velocity[2] = -100.0;
            }
            if ground_distance < HOVER_HEIGHT {
                // "always hovering off the ground somewhat while jetpack is active".
                state.velocity[2] += 2.0;
            }
        }
    }
}

/// `PM_AirMove`'s jetpack air control (`bg_pmove.c:3200-3214`): less than usual while not
/// thrusting, twice the command's while thrusting.
pub(super) fn air_control(up_move: i8) -> f32 {
    if up_move <= 0 { 0.8 } else { 2.0 }
}
