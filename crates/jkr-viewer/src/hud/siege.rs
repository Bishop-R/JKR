//! The Siege round on the HUD (`CG_Draw2D`'s Siege part, `cg_draw.c:8168-8336`): the
//! notice before a round begins, the round clock and the local player's respawn wave.
//!
//! Stock centre-prints the notice; here it takes the warmup line, the HUD's place for the
//! same news. The clock is red when the timed side is the viewer's and green otherwise,
//! as `CG_DrawSiegeTimer` colours it; the layout gives each colour its own widget.

use super::*;
use jkr_client::siege_notices::SiegeNotices;
use jkr_client::siege_round::{ClockRules, RoundNotice, SiegeRound};
use std::fmt::Write as _;

/// Retained Siege presentation.
pub(super) struct State {
    round: SiegeRound,
    /// The round clock, `m:ss`; empty when there is none.
    pub(super) clock: String,
    /// Whether the timed side is the viewer's.
    pub(super) clock_own: bool,
    /// The respawn wave, `m:ss`; empty when none is coming.
    pub(super) respawn: String,
}

impl Default for State {
    fn default() -> Self {
        Self {
            round: SiegeRound::default(),
            clock: String::with_capacity(8),
            clock_own: false,
            respawn: String::with_capacity(8),
        }
    }
}

/// What one frame of the Siege HUD reads.
pub(crate) struct SiegeInputs<'a> {
    pub(crate) game: &'a GameState,
    /// `cg.time`.
    pub(crate) time: i32,
    /// The viewer's `PERS_TEAM`.
    pub(crate) team: u8,
    pub(crate) notices: Option<&'a SiegeNotices>,
    /// Whole seconds until the viewer's respawn wave.
    pub(crate) respawn: Option<i32>,
    pub(crate) intermission: bool,
}

impl HudOverlay {
    /// One frame of the Siege round. Runs after [`Self::update_match_information`], whose
    /// warmup line a Siege notice takes over.
    pub(crate) fn update_siege(&mut self, input: SiegeInputs<'_>, localization: &Localization) {
        self.siege.clock.clear();
        self.siege.respawn.clear();
        // `CG_Draw2D` returns before its Siege part during an intermission.
        if input.intermission {
            return;
        }
        self.siege.round.observe(input.game, input.time);
        let timed = input.notices.map_or([0; 2], SiegeNotices::timed);
        let rules = ClockRules::read(input.game, timed);
        let frame = self.siege.round.frame(input.time, input.team, rules);
        if let Some(notice) = frame.notice {
            self.warmup_text.clear();
            match notice {
                RoundNotice::Waiting => self.warmup_text.push_str(text(
                    localization,
                    "WAITING_FOR_PLAYERS",
                    "Waiting for players",
                )),
                RoundNotice::BeginsIn(seconds) => {
                    let begins = text(localization, "ROUNDBEGINSIN", "Round begins in");
                    let _ = write!(self.warmup_text, "{begins} {seconds}...");
                }
            }
            self.warmup_text.make_ascii_uppercase();
        }
        if let Some(clock) = frame.clock {
            let _ = write!(
                self.siege.clock,
                "{}:{:02}",
                clock.seconds / 60,
                clock.seconds % 60
            );
            self.siege.clock_own = clock.own;
        }
        if let Some(seconds) = input.respawn {
            let _ = write!(self.siege.respawn, "{}:{:02}", seconds / 60, seconds % 60);
        }
    }
}

/// An `MP_INGAME` string, or its English text when the tables lack it.
fn text<'a>(localization: &'a Localization, key: &str, english: &'a str) -> &'a str {
    localization
        .strings
        .get(key)
        .map_or(english, String::as_str)
}
