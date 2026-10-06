//! The `cg_scoreboardStyle` setting and the classic scoreboard's options.
//!
//! `modern` is JKR's floating table ([`super::view`]); `classic` follows the
//! retail scoreboard as EternalJK-derived clients draw it ([`super::classic`]),
//! with their `cg_smallScoreboard`, `cg_showClientIDs`,
//! `cg_drawScoreboardIcons` and `cg_drawScoreboardPlayerCount` options.

use crate::console::ViewerConsole;

/// Archived cvar naming the scoreboard style.
pub(crate) const CVAR: &str = "cg_scoreboardStyle";

/// Layout family of the scoreboard.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ScoreboardStyle {
    /// JKR's floating table beside the chat column.
    #[default]
    Modern,
    /// The retail layout: centred columns, team bands and the client ID.
    Classic,
}

impl ScoreboardStyle {
    /// Values the settings screen offers, in [`ScoreboardStyle`] order; the
    /// first is the default.
    pub(crate) const NAMES: [&'static str; 2] = ["modern", "classic"];

    /// Read the cvar value: `classic` (any case) or `1` selects the classic
    /// style; anything else, including a missing or mistyped value, the
    /// modern one.
    pub(crate) fn from_cvar(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some(text) if text.eq_ignore_ascii_case("classic") || text == "1" => Self::Classic,
            _ => Self::Modern,
        }
    }

    /// The player's current style.
    pub(crate) fn from_console(console: Option<&ViewerConsole>) -> Self {
        Self::from_cvar(console.and_then(|console| console.text_value(CVAR)))
    }
}

/// Classic scoreboard options, sampled once per frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ClassicOptions {
    /// `cg_smallScoreboard`: always use the interleaved (small) rows.
    pub(crate) small: bool,
    /// `cg_showClientIDs`: the client ID column.
    pub(crate) client_ids: bool,
    /// `cg_drawScoreboardIcons`: each player's head icon before the name.
    pub(crate) icons: bool,
    /// `cg_drawScoreboardPlayerCount`: 0 off, 1 host name and counts,
    /// 2 counts only (team games always show "N vs. M").
    pub(crate) player_count: i64,
}

impl Default for ClassicOptions {
    fn default() -> Self {
        Self {
            small: false,
            client_ids: true,
            icons: true,
            player_count: 1,
        }
    }
}

impl ClassicOptions {
    /// Read the options, keeping the defaults for missing cvars.
    pub(crate) fn from_console(console: Option<&ViewerConsole>) -> Self {
        let defaults = Self::default();
        let Some(console) = console else {
            return defaults;
        };
        Self {
            small: console
                .bool_cvar("cg_smallScoreboard")
                .unwrap_or(defaults.small),
            client_ids: console
                .bool_cvar("cg_showClientIDs")
                .unwrap_or(defaults.client_ids),
            icons: console
                .bool_cvar("cg_drawScoreboardIcons")
                .unwrap_or(defaults.icons),
            player_count: console
                .integer_cvar("cg_drawScoreboardPlayerCount")
                .unwrap_or(defaults.player_count),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_needs_an_explicit_value() {
        assert_eq!(ScoreboardStyle::from_cvar(None), ScoreboardStyle::Modern);
        assert_eq!(
            ScoreboardStyle::from_cvar(Some("")),
            ScoreboardStyle::Modern
        );
        assert_eq!(
            ScoreboardStyle::from_cvar(Some("clasic")),
            ScoreboardStyle::Modern
        );
        assert_eq!(
            ScoreboardStyle::from_cvar(Some(" Classic ")),
            ScoreboardStyle::Classic
        );
        assert_eq!(
            ScoreboardStyle::from_cvar(Some("1")),
            ScoreboardStyle::Classic
        );
    }

    #[test]
    fn offered_names_parse_in_order() {
        let parsed = ScoreboardStyle::NAMES.map(|name| ScoreboardStyle::from_cvar(Some(name)));
        assert_eq!(parsed, [ScoreboardStyle::Modern, ScoreboardStyle::Classic]);
        assert_eq!(parsed[0], ScoreboardStyle::default());
    }
}
