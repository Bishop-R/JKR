//! Stock event-driven player indicators and local action timing.

use jkr_protocol::{EntityState, GameState};

/// Map-lifetime feedback; the caller supplies already deduplicated occurrences.
#[derive(Default)]
pub struct Feedback {
    voice_until: [i32; 32],
    timer: Option<(i32, i32)>,
}

impl Feedback {
    /// Apply `EV_VOICECMD_SOUND` / `EV_LOCALTIMER` to their intended player.
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
            _ => {}
        }
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
