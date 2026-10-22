//! Player-state side of ICARUS setters (`codemp/game/g_ICARUScb.c`).
//!
//! NPC weapon changes and custom-gravity flags have separate ownership. On a player,
//! ChangeWeapon intentionally does nothing: Q3_SetWeapon only changes the inventory.

use crate::script_world::ClientAction;
use jkr_protocol::PlayerState;

/// Apply a setter wholly represented by player state. Return false for actions that
/// require health, Force, animation or sound ownership in the host.
pub fn apply(state: &mut PlayerState, action: &ClientAction, command_angles: [i32; 3]) -> bool {
    match *action {
        ClientAction::Teleport(origin) => {
            state.set_origin([origin[0], origin[1], origin[2] + 1.0]);
            state.set_velocity([0.0; 3]);
            hold_knockback(state, 160);
            state.set_raw_field(17, state.entity_flags() ^ crate::EF_TELEPORT_BIT);
        }
        ClientAction::SetViewAngles(angles) => crate::triggers::face(state, angles, command_angles),
        ClientAction::AddVelocity { axis, speed } => {
            if axis >= 3 || !speed.is_finite() {
                return true;
            }
            let mut velocity = state.velocity();
            velocity[axis] += speed;
            state.set_velocity(velocity);
            hold_knockback(state, 500);
        }
        ClientAction::SetGravity(gravity) => state.set_gravity(gravity as i32),
        ClientAction::SetModelScale(scale) => {
            state.set_raw_field(123, scale as u32);
        }
        ClientAction::SetNoDraw(hidden) => {
            let flags = state.entity_flags();
            state.set_raw_field(17, if hidden { flags | 256 } else { flags & !256 });
        }
        ClientAction::SetWeapon(ref name) => {
            if let Some(weapon) = crate::weapon_data::weapon_by_name(name.as_bytes()) {
                state.stats[4] = 1 << weapon;
            }
        }
        _ => return false,
    }
    true
}

fn hold_knockback(state: &mut PlayerState, time: i16) {
    state.set_movement_time(time);
    state.set_movement_flags(state.movement_flags() | 64);
}
