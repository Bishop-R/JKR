//! Opening, closing and re-sorting motion for the classic scoreboard.
//!
//! The retail scoreboard fades out over 200 ms once released (`FADE_TIME`,
//! `CG_FadeColor` in `CG_DrawOldScoreboard`). The classic style keeps that
//! fade, adds a short fade and drop on opening, and moves rows to their new
//! places when the server re-sorts the scores instead of jumping. Everything
//! is fixed storage stepped by frame time.

use std::time::Instant;

/// Opening time in seconds.
const OPEN_SECONDS: f32 = 0.12;
/// Closing time in seconds: retail's `FADE_TIME`.
const CLOSE_SECONDS: f32 = 0.2;
/// Time constant of a row moving to a new place, in seconds.
const ROW_SECONDS: f32 = 0.07;

/// Presence and per-client row positions.
pub(super) struct Motion {
    /// 0 hidden, 1 fully shown.
    shown: f32,
    last: Option<Instant>,
    /// Step of the latest frame, in seconds.
    step: f32,
    rows: [f32; 32],
    placed: [bool; 32],
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            shown: 0.0,
            last: None,
            step: 0.0,
            rows: [0.0; 32],
            placed: [false; 32],
        }
    }
}

impl Motion {
    /// Step presence to `now`: rising while `requested`, falling otherwise.
    /// Returns whether the scoreboard draws this frame. When it may not draw
    /// at all (`allowed` false: console, menus, `cg_drawScores 0`) it hides
    /// at once.
    pub(super) fn present(&mut self, requested: bool, allowed: bool, now: Instant) -> bool {
        let step = self
            .last
            .map_or(0.0, |last| now.duration_since(last).as_secs_f32())
            .min(0.1);
        self.last = Some(now);
        self.step = step;
        if !allowed {
            self.hide();
            return false;
        }
        let was_hidden = self.shown <= 0.0;
        self.shown = if requested {
            (self.shown + step / OPEN_SECONDS)
                .min(1.0)
                .max(if was_hidden {
                    // The first frame already shows a little, so a tap is seen.
                    0.05
                } else {
                    0.0
                })
        } else {
            (self.shown - step / CLOSE_SECONDS).max(0.0)
        };
        if was_hidden && self.shown > 0.0 {
            // Rows take their places at once when the board opens.
            self.placed = [false; 32];
        }
        self.shown > 0.0
    }

    /// Hide at once.
    pub(super) fn hide(&mut self) {
        self.shown = 0.0;
    }

    /// Smoothstep opacity of the whole board.
    pub(super) fn opacity(&self) -> f32 {
        let t = self.shown.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// Where `client`'s row is drawn this frame, moving towards `target`.
    pub(super) fn row(&mut self, client: u8, target: f32) -> f32 {
        let slot = usize::from(client).min(31);
        if !self.placed[slot] {
            self.placed[slot] = true;
            self.rows[slot] = target;
            return target;
        }
        let blend = 1.0 - (-self.step / ROW_SECONDS).exp();
        let current = self.rows[slot] + (target - self.rows[slot]) * blend;
        // Settle exactly once within a hundredth of a pixel.
        self.rows[slot] = if (target - current).abs() < 0.01 {
            target
        } else {
            current
        };
        self.rows[slot]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn opens_quickly_and_fades_out_like_retail() {
        let mut motion = Motion::default();
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        assert!(motion.present(true, true, start));
        assert!(motion.opacity() > 0.0 && motion.opacity() < 0.1);
        // Fully shown within the 120 ms opening at 60 FPS.
        for ms in (16..=128).step_by(16) {
            assert!(motion.present(true, true, at(ms)));
        }
        assert_eq!(motion.opacity(), 1.0);
        // Released: still drawn while fading, gone after 200 ms.
        assert!(motion.present(false, true, at(144)));
        assert!(motion.opacity() < 1.0);
        let mut ms = 144;
        while motion.present(false, true, at(ms)) {
            ms += 16;
            assert!(ms <= 144 + 220, "still shown at {ms} ms");
        }
        // Not allowed: hidden at once.
        assert!(motion.present(true, true, at(1_000)));
        assert!(!motion.present(true, false, at(1_016)));
    }

    #[test]
    fn rows_snap_on_opening_and_glide_after() {
        let mut motion = Motion::default();
        let start = Instant::now();
        motion.present(true, true, start);
        assert_eq!(motion.row(3, 100.0), 100.0);
        motion.present(true, true, start + Duration::from_millis(16));
        let moved = motion.row(3, 200.0);
        assert!(moved > 100.0 && moved < 200.0, "{moved}");
        for frame in 2..80 {
            motion.present(true, true, start + Duration::from_millis(16 * frame));
            motion.row(3, 200.0);
        }
        assert_eq!(motion.row(3, 200.0), 200.0);
        // Closed and reopened: snaps again.
        motion.present(false, false, start + Duration::from_secs(2));
        motion.present(true, true, start + Duration::from_secs(3));
        assert_eq!(motion.row(3, 50.0), 50.0);
    }
}
