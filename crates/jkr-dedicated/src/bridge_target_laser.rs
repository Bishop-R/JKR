//! jaPRO's target laser (`TryTargettingLaser`, `g_active.c:3337-3362` and `5553-5555` of
//! jaPRO; `g_allowTargetLaser`, archived, announced, 0): while a living player holds
//! `+button15`, a line from just ahead of its eyes to the first solid thing, 8192 units at
//! most, is shown to everyone for a quarter second, every quarter second (`G_TestLine`).
//! It is green in jaPRO's call; the wire keeps only the colour's low byte, so every client
//! draws it white.

use super::*;
use crate::collision::{Void, WithPlayers, WorldCollision};
use jkr_game_jka::event_entity::EventEntity;

/// `BUTTON_TARGET` (`+button15`), `CONTENTS_SOLID`.
const BUTTON_TARGET: u16 = 32_768;
const CONTENTS_SOLID: u32 = 1;
const LASER_MS: i32 = 250;
const LASER_COLOR: u32 = 0x00ff_00;
pub(super) const ALLOW_TARGET_LASER: &[u8] = b"g_allowTargetLaser";

impl NativeGame {
    /// `ClientThink_real`'s laser after the use key, for `client`.
    pub(super) fn target_laser(&mut self, client: usize, level_time: i32) {
        if self.cvars.integer(ALLOW_TARGET_LASER) == 0 {
            return;
        }
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        if peer.last_command.buttons & BUTTON_TARGET == 0
            || peer.health <= 0
            || peer.target_laser_until > level_time
        {
            return;
        }
        peer.target_laser_until = level_time + LASER_MS;
        let state = &peer.state;
        let view = jkr_game_jka::pmove::flight::flight_axes(state.view_angles()).0;
        let origin = state.origin();
        let eye = [origin[0], origin[1], origin[2] + state.view_height() as f32];
        let start: [f32; 3] = std::array::from_fn(|axis| eye[axis] + view[axis] * 16.0);
        let end: [f32; 3] = std::array::from_fn(|axis| start[axis] + view[axis] * 8192.0);
        self.gather_obstacles(client);
        let Self { map, obstacles, .. } = &*self;
        let hit = match map {
            Some(map) => WithPlayers {
                world: WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                },
                players: obstacles,
            }
            .trace(start, [0.0; 3], [0.0; 3], end, CONTENTS_SOLID),
            None => WithPlayers {
                world: Void,
                players: obstacles,
            }
            .trace(start, [0.0; 3], [0.0; 3], end, CONTENTS_SOLID),
        };
        let line = EventEntity::test_line(start, hit.end_position, LASER_COLOR, LASER_MS);
        let _ = self.pool.spawn_temporary(line.state(), level_time, None);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Identity, NativeGame};
    use super::{ALLOW_TARGET_LASER, BUTTON_TARGET};
    use jkr_network::LegacyGameHost;
    use jkr_protocol::UserCommand;

    /// Holding the target button with the laser allowed sends everyone a test line from
    /// the player's eyes, at most every quarter second; without the cvar, nothing.
    #[test]
    fn the_target_button_shows_a_line() {
        let identity = Identity {
            hostname: b"test".to_vec(),
            mapname: b"mp/ffa3".to_vec(),
        };
        let cvars = crate::cvars::Cvars::server();
        let mut game = NativeGame::with_cvars(identity, None, 4, 4, cvars).unwrap();
        let userinfo = b"\\name\\a\\rate\\25000\\snaps\\40\\model\\kyle/default\\forcepowers\\7-1-032330000000001333\\color1\\4\\color2\\4\\handicap\\100\\cg_predictItems\\1\\saber1\\single_1\\saber2\\none\\char_color_red\\255\\char_color_green\\255\\char_color_blue\\255\\ip\\127.0.0.1:1";
        game.client_connect(0, userinfo).unwrap();
        game.enter_world(0, &UserCommand::default(), 100);
        let lines = |game: &NativeGame| {
            game.pool
                .entities()
                .filter(|(_, _, state, _)| u32::from(state.entity_type()) == 18 + 112)
                .count()
        };
        game.peer_mut(0).unwrap().last_command.buttons = BUTTON_TARGET;
        game.target_laser(0, 200);
        assert_eq!(lines(&game), 0);
        game.cvars.set(ALLOW_TARGET_LASER, b"1");
        game.target_laser(0, 300);
        game.target_laser(0, 400);
        assert_eq!(lines(&game), 1);
        game.target_laser(0, 550);
        assert_eq!(lines(&game), 2);
    }
}
