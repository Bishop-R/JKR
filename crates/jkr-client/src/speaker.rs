//! Automatic `ET_SPEAKER` scheduling (`codemp/cgame/cg_ents.c`, `CG_Speaker`).

use jkr_protocol::EntityState;

/// Per-map cgame `miscTime` values. Leaving the PVS does not reset a timer.
pub struct SpeakerClock {
    deadlines: Box<[i32]>,
    previous_time: i32,
}

impl Default for SpeakerClock {
    fn default() -> Self {
        Self {
            deadlines: vec![0; 1024].into_boxed_slice(),
            previous_time: 0,
        }
    }
}

impl SpeakerClock {
    /// Reset on a backwards presentation seek, without replaying missed sounds.
    pub fn begin_frame(&mut self, time: i32) {
        if time < self.previous_time {
            self.deadlines.fill(0);
        }
        self.previous_time = time;
    }

    /// True once per elapsed interval; `random` supplies stock's [-1, 1] sample.
    /// The float-to-int truncation applies to the complete new deadline.
    pub fn due(&mut self, entity: &EntityState, time: i32, random: impl FnOnce() -> f32) -> bool {
        if entity.entity_type() != 9 || entity.client_num() == 0 {
            return false;
        }
        let Some(deadline) = self.deadlines.get_mut(usize::from(entity.number())) else {
            return false;
        };
        if time < *deadline {
            return false;
        }
        let wait = entity.integer_field(83).unwrap_or(0).saturating_mul(100);
        *deadline = (time.saturating_add(wait) as f32
            + f32::from(entity.client_num()) * 100.0 * random()) as i32;
        true
    }
}
