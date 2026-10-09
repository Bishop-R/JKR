//! EternalJK's `cg_autoKillWhenFalling` (`cg_draw.c:10404-10409`): a player falling to its
//! death asks to die at once (`kill`, at most every half second) and presses attack to come
//! back, instead of watching the screen fade for three seconds.

use super::*;
use std::time::{Duration, Instant};

/// How often `kill` is sent while the fall lasts (`cg.lastAutoKillTime`).
const KILL_INTERVAL: Duration = Duration::from_millis(500);

impl ViewerConsole {
    /// One look at the local player, `falling` to its death and in the game: with
    /// `cg_autoKillWhenFalling` on, `kill` (unless one went out within half a second) and an
    /// attack press of two frames.
    pub(crate) fn auto_kill_when_falling(&mut self, falling: bool, now: Instant) {
        if !falling || self.integer_cvar("cg_autoKillWhenFalling").unwrap_or(0) == 0 {
            return;
        }
        if self
            .last_auto_kill
            .is_none_or(|last| now.saturating_duration_since(last) > KILL_INTERVAL)
        {
            self.queue_script("kill\n");
        }
        self.queue_script("+attack;wait 2;-attack\n");
        self.last_auto_kill = Some(now);
    }
}
