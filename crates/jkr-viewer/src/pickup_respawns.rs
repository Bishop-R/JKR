//! An item's rez-in once it has respawned (`CG_Item`, `cg_ents.c:2214-2270`, timed from
//! `EV_ITEM_RESPAWN`'s `cent->miscTime`, `cg_event.c:2757-2760`): for `ITEM_SCALEUP_TIME`
//! it fades in by alpha (`RF_FORCE_ENT_ALPHA`, though not Force Boon, whose sprite blends
//! otherwise) and is drawn again under `powerups/rezout`, tinted (0, 200, 85). Flags,
//! placeholders and dropped weapons have none.

use std::time::{Duration, Instant};

/// `cgs.media.itemRespawningRezOut` (`cg_main.c:1209`).
pub(crate) const SHADER: &str = "powerups/rezout";
/// The pass's `RF_RGB_TINT` colour.
pub(crate) const TINT: [f32; 4] = [0.0, 200.0 / 255.0, 85.0 / 255.0, 1.0];
/// `ITEM_SCALEUP_TIME`.
const SCALE_UP: Duration = Duration::from_millis(1_000);
/// Items rezzing in at once; more forget the oldest.
const MAX_RECENT: usize = 64;

/// The items that respawned less than a second ago.
pub(crate) struct Respawns {
    recent: Vec<(u16, Instant)>,
}

impl Default for Respawns {
    fn default() -> Self {
        Self {
            recent: Vec::with_capacity(MAX_RECENT),
        }
    }
}

impl Respawns {
    /// `EV_ITEM_RESPAWN` on item `entity` at `now`.
    pub(crate) fn mark(&mut self, entity: u16, now: Instant) {
        self.recent.retain(|(number, at)| {
            *number != entity && now.saturating_duration_since(*at) < SCALE_UP
        });
        if self.recent.len() == MAX_RECENT {
            self.recent.remove(0);
        }
        self.recent.push((entity, now));
    }

    /// How far through its rez-in item `entity` is at `now`, from 0 to 1; `None` outside one.
    pub(crate) fn progress(&self, entity: u16, now: Instant) -> Option<f32> {
        self.recent
            .iter()
            .rev()
            .find(|(number, _)| *number == entity)
            .map(|(_, at)| now.saturating_duration_since(*at))
            .filter(|elapsed| *elapsed < SCALE_UP)
            .map(|elapsed| elapsed.as_millis() as f32 / SCALE_UP.as_millis() as f32)
    }
}

/// The item pass's alpha, `msec / ITEM_SCALEUP_TIME` as a byte and never 0.
pub(crate) fn alpha(progress: f32) -> u8 {
    ((progress * 255.0) as i32).clamp(1, 255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_rezzes_in_over_a_second() {
        let start = Instant::now();
        let mut respawns = Respawns::default();
        respawns.mark(70, start);
        assert_eq!(respawns.progress(70, start), Some(0.0));
        assert_eq!(alpha(0.0), 1);
        let half = respawns.progress(70, start + Duration::from_millis(500));
        assert_eq!(half, Some(0.5));
        assert_eq!(alpha(0.5), 127);
        assert_eq!(respawns.progress(70, start + SCALE_UP), None);
        assert_eq!(respawns.progress(71, start), None);
    }
}
