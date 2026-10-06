//! Spoken team orders remain chat messages, independently of console output.

use super::*;
use jkr_protocol::{EntityState, GameState, PlayerState};

impl ChatOverlay {
    pub(super) fn voice_command(
        &mut self,
        event: &EntityState,
        game: &GameState,
        player: &PlayerState,
        localization: &crate::Localization,
    ) {
        let speaker = event.ground_entity_num();
        if speaker >= 32 {
            return;
        }
        let Some(info) = game.config_string(1131 + usize::from(speaker)) else {
            return;
        };
        if jkr_client::LegacyClientInfo::new(info).integer("t") != Some(i32::from(player.team())) {
            return;
        }
        let sound = game
            .config_string(811 + usize::from(event.event_parameter()))
            .and_then(|s| std::str::from_utf8(s).ok())
            .unwrap_or("");
        if sound.is_empty() {
            return;
        }
        let text = jkr_game_jka::voice_commands::sound_index(sound).map_or_else(
            || "voice chat".to_owned(),
            |index| {
                localization.translate(&format!(
                    "@@@MENUS_{}",
                    jkr_game_jka::voice_commands::LABELS[index]
                ))
            },
        );
        self.receive(
            ServerEventKind::TeamChat,
            text,
            Some(speaker),
            Instant::now(),
        );
    }
}
