//! The Force readout's flash (`cg.forceHUDTotalFlashTime`, `cg_event.c:1991-1994`, drawn
//! by `CG_DrawForcePower` and `CG_DrawSimpleForcePower`, `cg_draw.c`): a saber move that
//! costs Force (a kata, a lunge and the like) tried without the points for it (`EV_NOAMMO`
//! with a saber in hand, from `BG_EnoughForcePowerForMove`) turns the Force readouts red
//! for a second, and while the status shows, the no-Force sound plays at once and every
//! 400 ms after.

use std::cell::Cell;

/// `cgs.media.noforceSound`.
pub(crate) const SOUND: &str = "sound/weapons/force/noforce";
const FLASH_MS: u64 = 1_000;
const SOUND_MS: u64 = 400;

/// The flash's window and the sound's next time (`forceHUDNextFlashTime`).
#[derive(Default)]
pub(crate) struct ForceFlash {
    pending: bool,
    until: u64,
    next_sound: u64,
    sound: Cell<bool>,
}

impl ForceFlash {
    /// The local player's saber move failed for want of Force points.
    pub(crate) fn start(&mut self) {
        self.pending = true;
    }

    /// Advance to `time_ms`, the HUD's clock; `drawn` while the status shows, which is
    /// when stock's readout plays the sound.
    pub(super) fn update(&mut self, time_ms: u64, drawn: bool) {
        if std::mem::take(&mut self.pending) {
            self.until = time_ms + FLASH_MS;
        }
        if time_ms >= self.until {
            self.next_sound = 0;
        } else if drawn && self.next_sound < time_ms {
            self.next_sound = time_ms + SOUND_MS;
            self.sound.set(true);
        }
    }

    /// Whether the Force readouts are red at `time_ms`.
    pub(super) fn red(&self, time_ms: u64) -> bool {
        time_ms < self.until
    }

    /// Whether the no-Force sound is due, once, for the audio's frame.
    pub(crate) fn take_sound(&self) -> bool {
        self.sound.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_force_move_flashes_a_second_with_three_sounds() {
        let mut flash = ForceFlash::default();
        flash.start();
        let mut sounds = Vec::new();
        for time in (1_000..2_200).step_by(50) {
            flash.update(time, true);
            if flash.take_sound() {
                sounds.push(time);
            }
        }
        assert_eq!(sounds, [1_000, 1_450, 1_900]);
        assert!(flash.red(1_999));
        assert!(!flash.red(2_000));
        // Not drawn, no sound.
        flash.start();
        flash.update(3_000, false);
        assert!(flash.red(3_000) && !flash.take_sound());
    }
}
