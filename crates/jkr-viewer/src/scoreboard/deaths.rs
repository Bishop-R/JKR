//! Session-local death counts for jaPRO scoreboard modes 2 and 3.
use super::*;

/// Fixed counters, consuming the existing obituary feed exactly once.
pub(super) struct Deaths {
    counts: [i32; 64],
    consumed: u64,
    time: i32,
    mode: i64,
    gametype: i32,
}

impl Default for Deaths {
    fn default() -> Self {
        Self {
            counts: [0; 64],
            consumed: 0,
            time: 0,
            mode: 1,
            gametype: 0,
        }
    }
}

impl Scoreboard {
    /// Sample the cvar and accept new obituary counters without rebuilding the name cache.
    pub(crate) fn observe_deaths(
        &mut self,
        tracker: &jkr_client::ObituaryTracker,
        snapshot: &jkr_protocol::Snapshot,
        game: &GameState,
        console: Option<&crate::console::ViewerConsole>,
    ) {
        let d = &mut self.deaths;
        if snapshot.server_time < d.time || tracker.decoded() < d.consumed {
            d.counts.fill(0);
            d.consumed = 0;
        }
        d.time = snapshot.server_time;
        d.mode = console
            .and_then(|c| c.integer_cvar("cg_scoredeaths"))
            .unwrap_or(1);
        d.gametype = game
            .config_string(0)
            .and_then(|b| jkr_client::LegacyClientInfo::new(b).integer("g_gametype"))
            .unwrap_or(0);
        for offset in 0..(tracker.decoded() - d.consumed).min(8) as usize {
            if let Some(event) = tracker.feed().newest(offset)
                && let Some(count) = d.counts.get_mut(usize::from(event.target))
            {
                *count = count.saturating_add(1);
            }
        }
        d.consumed = tracker.decoded();
    }
}

impl Deaths {
    /// Without a JA+ plugin handshake, mode 1 is deliberately hidden (stock rule).
    pub(super) fn count(&self, client: u8) -> Option<i32> {
        (matches!(self.mode, 2 | 3) && !matches!(self.gametype, 3 | 7))
            .then(|| self.counts.get(usize::from(client)).copied())
            .flatten()
    }
}
