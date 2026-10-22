//! Small stock game commands whose results are information or voice events.

use super::{NativeGame, Told};
use jkr_game_jka::{event_entity::EventEntity, voice_commands};
use jkr_network::LegacyTokens;

impl NativeGame {
    pub(super) fn information_command(&mut self, client: usize, text: &[u8], time: i32) -> bool {
        let mut words = LegacyTokens::new(text);
        let Some(command) = words.next() else {
            return false;
        };
        let voice = command.eq_ignore_ascii_case(b"voice_cmd");
        if !voice && !command.eq_ignore_ascii_case(b"where") {
            return false;
        }
        if self.refused_at_intermission(client, command) {
            return true;
        }
        let Some(peer) = self.peer(client) else {
            return true;
        };
        if !voice {
            let origin = if peer.playing() {
                peer.state.origin()
            } else {
                peer.entity.state().event_origin()
            };
            self.told.push(Told::One(
                client,
                format!(
                    "print \"({} {} {})\n\"",
                    origin[0] as i32, origin[1] as i32, origin[2] as i32
                )
                .into_bytes(),
            ));
            return true;
        }
        if self.gametype < jkr_game_jka::match_end::GT_TEAM {
            return true;
        }
        let Some(argument) = words.next() else {
            return true;
        };
        if peer.session.team == 3 || peer.temp_spectate >= time {
            self.told.push(Told::One(
                client,
                b"print \"@@@NOVOICECHATASSPEC\n\"".to_vec(),
            ));
            return true;
        }
        let Some(sound) = voice_commands::command_sound(argument) else {
            return true;
        };
        let index = self.sound_index(sound.as_bytes());
        let mut event = EventEntity {
            event: 75,
            parameter: u32::from(index),
            origin: [0.0; 3],
            client: None,
            broadcast: true,
            extra: [(0, 0); 12],
        };
        // EV_VOICECMD_SOUND addresses its speaker through groundEntityNum.
        event.extra[0] = (22, client as u32);
        let _ = self.pool.spawn_temporary(event.state(), time, None);
        true
    }
}
