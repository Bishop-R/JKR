//! The `noclip` cheat for players: `Cmd_Noclip_f` (OpenJK `codemp/game/g_cmds.c`), which
//! toggles `client->noclip`, and what `ClientThink_real` (`g_active.c`) makes of the flag.
//!
//! The command is `CMD_CHEAT | CMD_ALIVE | CMD_NOINTERMISSION`: the server checks those
//! gates before it toggles. While the flag is on, every think puts the player in
//! `PM_NOCLIP` — ahead of a disintegration, of death and of a grip — and `Pmove` flies it
//! through everything (`PM_NoclipMove`, [`crate::pmove`]). The client predicts the same
//! move from the `pm_type` the snapshot carries. The flag also keeps the player out of
//! `G_TouchTriggers`, out of `P_WorldEffects`' drowning and burning
//! ([`crate::world_effects`]) and out of `G_Damage`. `ClientSpawn` clears the whole
//! client, so a respawn or a team change ends it.

/// `PM_NORMAL`, `PM_NOCLIP`, `PM_DEAD` (`bg_public.h`).
const PM_NORMAL: u8 = 0;
const PM_NOCLIP: u8 = 3;
const PM_DEAD: u8 = 5;

/// `Cmd_Noclip_f`: the flag toggled, and the line the player is printed
/// (`print "noclip ON\n"` or `print "noclip OFF\n"`).
pub fn toggle(noclip: &mut bool) -> &'static [u8] {
    *noclip = !*noclip;
    if *noclip {
        b"print \"noclip ON\n\""
    } else {
        b"print \"noclip OFF\n\""
    }
}

/// `ClientThink_real`'s movement type (`g_active.c`, the `client->noclip` chain) as far
/// as noclip decides it, for a player in the world whose type is `current`: `PM_NOCLIP`
/// while the flag is on; once it is off, the type the chain then picks for a player
/// noclip left in `PM_NOCLIP` — still `PM_NOCLIP` when `disintegrated`
/// (`EF_DISINTEGRATION`), `PM_DEAD` at no health, else `PM_NORMAL` (a grip's own type
/// is [`crate::force_dark::gripped_movement_type`]'s to restore). Returns the new type
/// when it changes; every other type is left to the rules that own it.
pub fn movement_type(noclip: bool, current: u8, health: i32, disintegrated: bool) -> Option<u8> {
    let wanted = if noclip {
        PM_NOCLIP
    } else if current != PM_NOCLIP || disintegrated {
        return None;
    } else if health <= 0 {
        PM_DEAD
    } else {
        PM_NORMAL
    };
    (wanted != current).then_some(wanted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pmove::{MovementCollision, MovementConfig, MovementTrace, Predictor};
    use jkr_protocol::{PlayerState, UserCommand};

    #[test]
    fn toggle_prints_the_reference_lines() {
        let mut noclip = false;
        assert_eq!(toggle(&mut noclip), b"print \"noclip ON\n\"");
        assert!(noclip);
        assert_eq!(toggle(&mut noclip), b"print \"noclip OFF\n\"");
        assert!(!noclip);
    }

    #[test]
    fn noclip_comes_first_and_leaves_to_the_right_type() {
        // On: over a normal, floating (gripped) or dead player.
        for current in [PM_NORMAL, 2, PM_DEAD] {
            assert_eq!(movement_type(true, current, 100, false), Some(PM_NOCLIP));
        }
        assert_eq!(movement_type(true, PM_NOCLIP, 100, false), None);
        // Off: back to walking, or to death; a disintegrated body keeps floating.
        assert_eq!(movement_type(false, PM_NOCLIP, 100, false), Some(PM_NORMAL));
        assert_eq!(movement_type(false, PM_NOCLIP, 0, false), Some(PM_DEAD));
        assert_eq!(movement_type(false, PM_NOCLIP, 0, true), None);
        // Off and never on: nothing of noclip's to undo.
        for current in [PM_NORMAL, 2, crate::PM_SPECTATOR, PM_DEAD] {
            assert_eq!(movement_type(false, current, 100, false), None);
        }
    }

    /// A world that is solid everywhere: any move that traced would stop dead.
    struct Solid;

    impl MovementCollision for Solid {
        fn point_contents(&self, _point: [f32; 3]) -> u32 {
            1
        }

        fn trace(
            &self,
            start: [f32; 3],
            _minimums: [f32; 3],
            _maximums: [f32; 3],
            _end: [f32; 3],
            _content_mask: u32,
        ) -> MovementTrace {
            MovementTrace {
                fraction: 0.0,
                start_solid: true,
                all_solid: true,
                entity_number: 1_022,
                plane_normal: [0.0, 0.0, 1.0],
                ..MovementTrace::miss(start)
            }
        }
    }

    fn noclipping_player() -> PlayerState {
        let mut state = PlayerState::default();
        state.set_movement_type(PM_NOCLIP);
        state.set_client_num(0);
        state.set_speed(250.0);
        state.set_base_speed(250);
        state.set_origin([0.0, 0.0, 0.0]);
        state.stats[0] = 100;
        state
    }

    /// The server's bare `Pmove` and the client's prediction move a noclipping player
    /// the same, bit for bit, at 125/142/250/333 FPS command steps, and through solid
    /// walls: what the server sends back is what the client predicted.
    #[test]
    fn server_and_prediction_fly_alike_through_walls() {
        let server = MovementConfig {
            authoritative: true,
            ..Default::default()
        };
        for step in [8, 7, 4, 3] {
            let state = noclipping_player();
            let mut moved = Predictor::from_player_state(&state, server);
            let mut predicted = Predictor::from_player_state(&state, MovementConfig::default());
            for index in 1..=(1_000 / step) {
                let command = UserCommand {
                    server_time: index * step,
                    forward_move: 127,
                    up_move: if index % 3 == 0 { 127 } else { 0 },
                    buttons: if index > 60 { 1 } else { 0 },
                    angles: [0, 4_096, 0],
                    ..Default::default()
                };
                moved.predict_command(command, &Solid);
                predicted.predict_command(command, &Solid);
                let (server, client) = (moved.state(), predicted.state());
                assert_eq!(server.origin, client.origin, "step {step} command {index}");
                assert_eq!(
                    server.velocity, client.velocity,
                    "step {step} command {index}"
                );
                assert_eq!(server.movement_type, PM_NOCLIP);
            }
            let origin = moved.state().origin;
            let travelled = origin.iter().map(|axis| axis * axis).sum::<f32>().sqrt();
            assert!(travelled > 1_000.0, "step {step}: only {travelled} units");
        }
    }
}
