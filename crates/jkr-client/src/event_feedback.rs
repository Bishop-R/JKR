//! Stock event-driven player indicators and local action timing.

use jkr_protocol::{EntityState, GameState};

/// Map-lifetime feedback; the caller supplies already deduplicated occurrences.
#[derive(Default)]
pub struct Feedback {
    voice_until: [i32; 32],
    timer: Option<(i32, i32)>,
    /// `cg_siegeDeathTime`: when the local player's respawn wave comes.
    respawn: Option<i32>,
}

impl Feedback {
    /// Apply `EV_VOICECMD_SOUND` / `EV_LOCALTIMER` / `EV_SIEGESPEC` to their intended
    /// player.
    pub fn observe(
        &mut self,
        event: u16,
        entity: &EntityState,
        game: &GameState,
        local: u16,
        time: i32,
    ) {
        match event {
            75 if game
                .config_string(811 + usize::from(entity.event_parameter()))
                .is_some_and(|s| !s.is_empty()) =>
            {
                if let Some(deadline) = self
                    .voice_until
                    .get_mut(usize::from(entity.ground_entity_num()))
                {
                    *deadline = time.saturating_add(1000);
                }
            }
            43 if entity.raw_field(40) == Some(u32::from(local)) => {
                let start = entity.integer_field(65).unwrap_or(0);
                let duration = entity.integer_field(61).unwrap_or(0);
                self.timer = (duration > 0).then_some((start.saturating_add(duration), duration));
            }
            // `EV_SIEGESPEC` (`cg_event.c:1827-1834`): `time` is the wave.
            191 if entity.raw_field(40) == Some(u32::from(local)) => {
                self.respawn = entity.integer_field(65);
            }
            _ => {}
        }
    }

    /// Whole seconds until the local player's respawn wave (`CG_DrawSiegeDeathTimer`),
    /// down to zero during its last second; hidden once it has come.
    pub fn respawn_remaining(&self, time: i32) -> Option<i32> {
        let left = self.respawn?.wrapping_sub(time);
        (left >= 0).then_some(left / 1_000)
    }

    /// When `client` last gave a voice command (`vChatTime`), zero for never.
    pub fn spoke_at(&self, client: u16) -> i32 {
        self.voice_until
            .get(usize::from(client))
            .filter(|&&until| until != 0)
            .map_or(0, |until| until.wrapping_sub(1_000))
    }

    /// The voice sprite lasts one second and takes precedence over typing.
    pub fn speaking(&self, client: u16, time: i32) -> bool {
        self.voice_until
            .get(usize::from(client))
            .is_some_and(|&until| time < until)
    }

    /// Remaining action fraction. Future, cancelled and expired timers are hidden.
    pub fn timer_remaining(&self, time: i32) -> Option<f32> {
        let (end, duration) = self.timer?;
        let remaining = end.saturating_sub(time);
        (remaining > 0 && remaining <= duration).then_some(remaining as f32 / duration as f32)
    }
}

/// Decode targeted `EV_SCREENSHAKE`; timing and magnitude follow `CGCam_Shake`.
pub fn screen_shake(event: u16, entity: &EntityState, local: u16) -> Option<(f32, i32)> {
    let target = entity.model_index();
    if event != 42 || target != 0 && i32::from(target) - 1 != i32::from(local) {
        return None;
    }
    let intensity = entity.angles()[0];
    intensity
        .is_finite()
        .then_some((intensity.min(16.0), entity.integer_field(65).unwrap_or(0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `EV_SIEGESPEC` as a server sends it: the wave's time and the player it is for.
    fn siege_spec(owner: u32, wave: i32) -> EntityState {
        let mut state = EntityState::zero(0, &jkr_protocol::LEGACY_ENTITY_FIELDS);
        state.set_raw_field(65, wave as u32);
        state.set_raw_field(40, owner);
        state
    }

    #[test]
    fn a_respawn_wave_counts_down_for_its_own_player_only() {
        let game = GameState::empty_local(3);
        let mut feedback = Feedback::default();
        feedback.observe(191, &siege_spec(4, 30_000), &game, 3, 20_000);
        assert_eq!(feedback.respawn_remaining(20_500), None);
        feedback.observe(191, &siege_spec(3, 30_000), &game, 3, 20_000);
        assert_eq!(feedback.respawn_remaining(20_500), Some(9));
        assert_eq!(feedback.respawn_remaining(29_999), Some(0));
        assert_eq!(feedback.respawn_remaining(30_001), None);
    }
}
