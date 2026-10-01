//! Explicit compatibility profile selected from the legacy serverinfo string.
//!
//! Protocol 26 reserves configstring zero as `CS_SERVERINFO`
//! (`codemp/qcommon/q_shared.h:941`).  Stock cgame reads that info string in
//! `CG_ParseServerinfo` (`codemp/cgame/cg_servercmds.c`) at gamestate time.
//! JKR makes the same boundary explicit so BaseJKA and mod behavior cannot
//! silently bleed into each other.

use jkr_protocol::{GameState, InfoString};
use std::fmt;

/// TaystJK/jaPRO USERINFO cvars and defaults declared by
/// `codemp/cgame/cg_xcvar.h:195-207` at TaystJK commit 5802c999, minus
/// `cp_sbRGB1`/`cp_sbRGB2`, which the profile adapter emits itself when a
/// blade colour selects RGB (see `PlayerProfile::legacy_userinfo`).
const TAYSTJK_USERINFO: [(&str, &str); 6] = [
    ("cp_pluginDisable", "1536"),
    ("cg_displayCameraPosition", "1 80 16"),
    ("cg_displayNetSettings", "125 0 125"),
    ("cjp_client", "1.4JAPRO"),
    ("cp_clanPwd", "none"),
    ("cp_cosmetics", "0"),
];

/// Server/game compatibility policy selected for one client session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompatProfile {
    /// The unmodified Jedi Academy multiplayer game module.
    BaseJka,
    /// JA+ game module; the optional version is copied from observed serverinfo.
    JaPlus { version: Option<String> },
    /// TaystJK or its public jaPRO game-module lineage.
    TaystJk,
    /// A protocol-26 game module for which JKR has no explicit policy yet.
    Unknown(String),
}

impl CompatProfile {
    /// Adapter-owned fields appended to the stock connection userinfo.
    ///
    /// BaseJKA, JA+, and unknown modules receive only stock keys. TaystJK is
    /// the public jaPRO-derived profile whose source declares these
    /// USERINFO cvars (`codemp/cgame/cg_xcvar.h:195-207`).
    pub fn userinfo_extensions(&self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::TaystJk => &TAYSTJK_USERINFO,
            Self::BaseJka | Self::JaPlus { .. } | Self::Unknown(_) => &[],
        }
    }

    /// Detect a profile from `CS_SERVERINFO` (`gamename`, `fs_game`, `version`).
    ///
    /// Matching is deliberately conservative: only identifiers containing an
    /// observed product name opt into JA+ or TaystJK behavior.  Everything else
    /// remains BaseJKA only when both game identifiers are empty/base-like.
    pub fn from_game_state(game_state: &GameState) -> Self {
        game_state
            .config_string(0)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .and_then(|text| InfoString::parse(text).ok())
            .map_or_else(
                || Self::Unknown(String::new()),
                |info| Self::from_server_info(&info),
            )
    }

    /// Detect a profile from an already parsed serverinfo string.
    pub fn from_server_info(info: &InfoString) -> Self {
        let gamename = info.get("gamename").unwrap_or_default().trim();
        let fs_game = info.get("fs_game").unwrap_or_default().trim();
        let version = info.get("version").unwrap_or_default().trim();
        if contains_identifier(gamename, "taystjk")
            || contains_identifier(fs_game, "taystjk")
            || contains_identifier(version, "taystjk")
            || gamename.eq_ignore_ascii_case("japro")
            || fs_game.eq_ignore_ascii_case("japro")
        {
            return Self::TaystJk;
        }
        if contains_identifier(gamename, "japlus")
            || contains_identifier(fs_game, "japlus")
            || contains_identifier(version, "japlus")
            || contains_identifier(gamename, "ja+")
            || contains_identifier(version, "ja+")
        {
            let identified_version = [gamename, version, fs_game]
                .into_iter()
                .filter(|value| {
                    contains_identifier(value, "japlus") || contains_identifier(value, "ja+")
                })
                .max_by_key(|value| value.len())
                .map(str::to_owned);
            return Self::JaPlus {
                version: identified_version,
            };
        }
        if is_base_identifier(gamename) && is_base_identifier(fs_game) {
            Self::BaseJka
        } else {
            Self::Unknown(if !gamename.is_empty() {
                gamename.to_owned()
            } else if !fs_game.is_empty() {
                fs_game.to_owned()
            } else {
                version.to_owned()
            })
        }
    }
}

impl fmt::Display for CompatProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BaseJka => formatter.write_str("BaseJKA"),
            Self::JaPlus {
                version: Some(version),
            } => write!(formatter, "JA+ ({version})"),
            Self::JaPlus { version: None } => formatter.write_str("JA+"),
            Self::TaystJk => formatter.write_str("TaystJK"),
            Self::Unknown(name) if name.is_empty() => formatter.write_str("Unknown"),
            Self::Unknown(name) => write!(formatter, "Unknown ({name})"),
        }
    }
}

fn contains_identifier(value: &str, identifier: &str) -> bool {
    value.to_ascii_lowercase().contains(identifier)
}

fn is_base_identifier(value: &str) -> bool {
    value.is_empty()
        || value.eq_ignore_ascii_case("base")
        || value.eq_ignore_ascii_case("basejka")
        || value.eq_ignore_ascii_case("jamp")
}
