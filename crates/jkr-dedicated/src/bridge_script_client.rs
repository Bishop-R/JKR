//! ICARUS player actions that need authoritative animation, Force and sound owners.

use super::*;
use jkr_game_jka::script_world::ClientAction;

impl NativeGame {
    pub(super) fn script_client_action(
        &mut self,
        client: usize,
        action: &ClientAction,
        time: i32,
    ) -> bool {
        if matches!(action, ClientAction::ToggleSaber) {
            let Some(origin) = self.peer(client).map(|peer| peer.state.origin()) else {
                return true;
            };
            let command = UserCommand {
                generic_command: jkr_game_jka::generic_commands::GENCMD_SABERSWITCH,
                ..Default::default()
            };
            self.generic_command(client, &command, origin, time);
            return true;
        }
        let Some(peer) = self.peer_mut(client) else {
            return true;
        };
        if jkr_game_jka::script_client_state::apply(
            &mut peer.state,
            action,
            peer.last_command.angles,
        ) {
            if matches!(action, ClientAction::SetNoDraw(true)) {
                peer.riding.contents = 0;
                peer.entity.linked(peer.movement.box_bounds(), false);
            }
            peer.movement = peer.movement.reseeded(&peer.state);
            return true;
        }
        match *action {
            ClientAction::SetForcePowerLevel { power, level } => {
                let Ok(power) = usize::try_from(power) else {
                    return true;
                };
                let Some(value) = peer.force.levels.get_mut(power) else {
                    return true;
                };
                let Ok(level) = u8::try_from(level) else {
                    return true;
                };
                *value = level;
                let known = peer.state.raw_field(51).unwrap_or(0);
                let known = if level == 0 {
                    known & !(1 << power)
                } else {
                    known | (1 << power)
                };
                peer.state.set_raw_field(51, known);
                if power == 1 {
                    peer.state.set_raw_field(52, u32::from(level));
                }
                if power == 14 {
                    peer.state.set_raw_field(109, u32::from(level));
                }
                if let Some(force) = &mut peer.session.force {
                    force.levels[power] = level;
                    force.known = known;
                }
                peer.movement = peer.movement.reseeded(&peer.state);
            }
            ClientAction::SetAnimation { torso, animation } => {
                use jkr_game_jka::pmove_anim::{
                    SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_FLAG_RESTART, SETANIM_LEGS,
                    SETANIM_TORSO,
                };
                let Ok(animation) = u16::try_from(animation) else {
                    return true;
                };
                peer.movement = peer.movement.reseeded(&peer.state);
                peer.movement.set_animation_parts(
                    if torso { SETANIM_TORSO } else { SETANIM_LEGS },
                    animation,
                    SETANIM_FLAG_RESTART | SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                );
                peer.movement.write_player_state(&mut peer.state);
            }
            _ => return false,
        }
        true
    }
}
