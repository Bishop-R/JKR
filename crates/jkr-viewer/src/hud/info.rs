//! Match-information projection into retained HUD storage.

use super::*;
use jkr_client::{CrosshairName, KillFeed, WarmupText};
use std::fmt::Write as _;

impl HudOverlay {
    pub(crate) fn update_match_information(
        &mut self,
        game: &GameState,
        server_time: i32,
        feed: &KillFeed,
        lagometer: &jkr_client::LagometerSamples,
        crosshair: Option<CrosshairName>,
        interrupted: bool,
        localization: &Localization,
    ) {
        self.update_kill_feed(game, feed, server_time, localization);
        self.icons.obituary(feed);
        self.crosshair_name.clear();
        self.crosshair_alpha = 0.0;
        self.crosshair_teammate = false;
        if let Some(target) = crosshair {
            family::name(
                &mut self.crosshair_name,
                &client_name_and_team(game, target.client_num).0,
                self.family.name_colors,
            );
            self.crosshair_alpha = target.alpha;
            self.crosshair_teammate = target.teammate;
        }
        self.match_timer.clear();
        family::timer(&mut self.match_timer, game, server_time, self.family);
        self.warmup_text.clear();
        match jkr_client::legacy_warmup_text(game, server_time) {
            WarmupText::Hidden => {}
            WarmupText::WaitingForPlayers => {
                self.warmup_text.push_str("WAITING FOR PLAYERS");
            }
            WarmupText::StartsIn(seconds) => {
                let _ = write!(self.warmup_text, "STARTS IN: {seconds}");
            }
        }
        self.interrupted = interrupted;
        self.lagometer.clone_from(lagometer);
    }

    /// Only the newest obituary is presented, and only briefly; the full
    /// list lives in the console log.
    fn update_kill_feed(
        &mut self,
        game: &GameState,
        feed: &KillFeed,
        server_time: i32,
        localization: &Localization,
    ) {
        self.kill_len = 0;
        self.kill_alpha = 0.0;
        let Some(event) = feed.newest(0) else { return };
        let age = server_time.saturating_sub(event.server_time).max(0) as u64;
        self.kill_alpha = transient_alpha(age, KILL_HOLD_MS, KILL_FADE_MS);
        if self.kill_alpha <= 0.0 {
            return;
        }
        self.kill_len = 1;
        let target = client_name_and_team(game, event.target).0;
        let attacker = client_name_and_team(game, event.attacker).0;
        let key = event.attacker_message.unwrap_or(event.message);
        let phrase = localization.strings.get(key).map_or(key, String::as_str);
        self.kill_rows[0].clear();
        if event.attacker_message.is_some() {
            family::obituary(
                &mut self.kill_rows[0],
                &target,
                &attacker,
                phrase,
                self.family.kill_reverse,
            );
        } else {
            let _ = write!(self.kill_rows[0], "{target} {phrase}");
        }
    }
}
