//! jaPRO's match options on this server: `g_mercyRule` (a team game ends early on a big
//! lead, [`jkr_game_jka::match_end::Limits::mercy`], kept from the cvar hook) and
//! `g_allowTeamSuicide` (`kill` in a team game; registered on so a stock game allows it,
//! while jaPRO's own default refuses it).

use super::*;

impl NativeGame {
    /// The match options, archived and announced.
    pub(super) fn register_game_options(&mut self) {
        let options: [(&[u8], &[u8], &[u8]); 2] = [
            (
                b"g_mercyRule",
                b"0",
                b"jaPRO: past half the frag limit, a team leading by this share of it wins",
            ),
            (
                b"g_allowTeamSuicide",
                b"1",
                b"jaPRO: kill is allowed in a team game (jaPRO's default is 0)",
            ),
        ];
        for (name, value, about) in options {
            self.cvars
                .get_announced(name, value, crate::cvars::CVAR_ARCHIVE, Some(about));
        }
    }
}
