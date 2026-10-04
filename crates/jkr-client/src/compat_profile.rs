//! Explicit compatibility profile selected from the legacy serverinfo string.
//!
//! Protocol 26 reserves configstring zero as `CS_SERVERINFO`
//! (`codemp/qcommon/q_shared.h:941`).  Stock cgame reads that info string in
//! `CG_ParseServerinfo` (`codemp/cgame/cg_servercmds.c`) at gamestate time.
//! JKR makes the same boundary explicit so BaseJKA and mod behavior cannot
//! silently bleed into each other.

use jkr_network::LegacyUserInfo;
use jkr_protocol::{GameState, InfoString};
use std::fmt;

/// TaystJK/jaPRO USERINFO cvars and defaults declared by
/// `codemp/cgame/cg_xcvar.h:195-207` at TaystJK commit 5802c999, minus
/// `cp_sbRGB1`/`cp_sbRGB2`, which the profile adapter emits itself when a
/// blade colour selects RGB (see `PlayerProfile::legacy_userinfo`), and
/// `cp_pluginDisable`, the player's own setting ([`CompatProfile::userinfo_for`]).
const TAYSTJK_USERINFO: [(&str, &str); 5] = [
    ("cg_displayCameraPosition", "1 80 16"),
    ("cg_displayNetSettings", "125 0 125"),
    ("cjp_client", "1.4JAPRO"),
    ("cp_clanPwd", "none"),
    ("cp_cosmetics", "0"),
];

/// JA+ client-plugin USERINFO keys, by which a JA+ server recognises a plugin
/// user and serves it plugin features such as custom RGB blades.
///
/// - `cjp_client` is the JA+ 1.4B4 plugin's own value: its `cgamex86.dll`
///   registers `cjp_client` `"1.4B4"` with `CVAR_USERINFO | CVAR_ROM`, and the
///   JA+ server's `jampgamex86.dll` names `1.4B4` as the latest plugin it
///   knows, so it does not ask for an update.
/// - `cp_clanPwd` `"none"` is the default of both the plugin and EternalJK.
///
/// `cp_pluginDisable` is the player's setting and is added per profile by
/// [`CompatProfile::userinfo_for`], defaulting to [`PLUGIN_DISABLE_DEFAULT`].
///
/// `cp_sbRGB1`/`cp_sbRGB2` are added by the userinfo codec whenever a blade
/// selects RGB, for every profile. The plugin's other USERINFO cvars
/// (`cp_login`, `cp_holster`) are left out, as EternalJK leaves them out.
const JAPLUS_USERINFO: [(&str, &str); 2] = [("cjp_client", "1.4B4"), ("cp_clanPwd", "none")];

/// `cp_pluginDisable` sent to plugin servers when the player has not set one:
/// EternalJK's value (`codemp/cgame/cg_xcvar.h:166`) rather than the JA+
/// plugin's 0. Bits 9 (holstered saber) and 10 (ledge grab) opt out of JA+
/// features drawn with the plugin's extra animations and attachments, which
/// this client, like EternalJK, lacks (`cg_consolecmds.c:1160-1185`).
pub const PLUGIN_DISABLE_DEFAULT: u32 = 1_536;

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
    /// BaseJKA and unknown modules receive only stock keys. JA+ receives the
    /// client-plugin identity ([`JAPLUS_USERINFO`]). TaystJK is the public
    /// jaPRO-derived profile whose source declares these USERINFO cvars
    /// (`codemp/cgame/cg_xcvar.h:195-207`).
    pub fn userinfo_extensions(&self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::TaystJk => &TAYSTJK_USERINFO,
            Self::JaPlus { .. } => &JAPLUS_USERINFO,
            Self::BaseJka | Self::Unknown(_) => &[],
        }
    }

    /// The player's userinfo as this profile sends it: plugin servers (JA+,
    /// TaystJK/jaPRO) get the player's `cp_pluginDisable`, or
    /// [`PLUGIN_DISABLE_DEFAULT`] when it is unset, and other servers none.
    /// Used for the connect userinfo and every later `userinfo` update alike.
    pub fn userinfo_for(&self, userinfo: &LegacyUserInfo) -> LegacyUserInfo {
        let mut userinfo = userinfo.clone();
        userinfo.plugin_disable = match self {
            Self::JaPlus { .. } | Self::TaystJk => {
                Some(userinfo.plugin_disable.unwrap_or(PLUGIN_DISABLE_DEFAULT))
            }
            Self::BaseJka | Self::Unknown(_) => None,
        };
        userinfo
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

    /// Detect a profile from an already parsed serverinfo string, or from a
    /// connectionless `getinfo` reply, which carries no `gamename` and names
    /// the mod directory `game` instead of `fs_game` (OpenJK
    /// `codemp/server/sv_main.cpp` `SVC_Info`).
    pub fn from_server_info(info: &InfoString) -> Self {
        let gamename = info.get("gamename").unwrap_or_default().trim();
        let fs_game = info
            .get("fs_game")
            .or_else(|| info.get("game"))
            .unwrap_or_default()
            .trim();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn info(text: &str) -> InfoString {
        InfoString::parse(text).unwrap()
    }

    #[test]
    fn getinfo_game_directory_selects_the_mod_profile() {
        // A getinfo reply has `game` but neither `gamename` nor `fs_game`.
        let ja_plus = info(r"\hostname\JoF\mapname\mp/ffa3\game\japlus");
        assert!(matches!(
            CompatProfile::from_server_info(&ja_plus),
            CompatProfile::JaPlus { version: Some(version) } if version == "japlus"
        ));
        let japro = info(r"\hostname\x\game\japro");
        assert_eq!(
            CompatProfile::from_server_info(&japro),
            CompatProfile::TaystJk
        );
        let base = info(r"\hostname\x\mapname\mp/ffa3");
        assert_eq!(
            CompatProfile::from_server_info(&base),
            CompatProfile::BaseJka
        );
        let other = info(r"\hostname\x\game\MBII");
        assert_eq!(
            CompatProfile::from_server_info(&other),
            CompatProfile::Unknown("MBII".to_owned())
        );
    }

    #[test]
    fn serverinfo_fs_game_takes_precedence_over_game() {
        let both = info(r"\fs_game\japlus\game\base");
        assert!(matches!(
            CompatProfile::from_server_info(&both),
            CompatProfile::JaPlus { .. }
        ));
    }

    #[test]
    fn ja_plus_identifies_as_the_client_plugin() {
        let keys = CompatProfile::JaPlus { version: None }.userinfo_extensions();
        assert_eq!(keys, &[("cjp_client", "1.4B4"), ("cp_clanPwd", "none")]);
        // 1536 opts out of exactly holstered sabers (bit 9) and ledge grab (bit 10).
        assert_eq!(PLUGIN_DISABLE_DEFAULT, (1 << 9) | (1 << 10));
        assert!(CompatProfile::BaseJka.userinfo_extensions().is_empty());
        assert!(
            CompatProfile::Unknown("MBII".to_owned())
                .userinfo_extensions()
                .is_empty()
        );
    }

    #[test]
    fn ja_plus_userinfo_carries_plugin_keys_and_rgb_blades() {
        let mut user = jkr_network::LegacyUserInfo::with_name("Sol");
        user.color1 = 6;
        user.saber_rgb = [Some(0x00_80_ff), None];
        let profile = CompatProfile::JaPlus { version: None };
        let payload = jkr_network::legacy_userinfo_payload_with_extensions(
            &profile.userinfo_for(&user),
            profile.userinfo_extensions(),
        )
        .unwrap();
        let parsed = info(&payload);
        assert_eq!(parsed.get("cjp_client"), Some("1.4B4"));
        assert_eq!(parsed.get("cp_pluginDisable"), Some("1536"));
        assert_eq!(parsed.get("cp_clanPwd"), Some("none"));
        assert_eq!(parsed.get("color1"), Some("6"));
        assert_eq!(parsed.get("cp_sbRGB1"), Some("33023"));
        assert_eq!(parsed.get("cp_sbRGB2"), None);
        // Far inside the 1024-byte MAX_INFO_STRING with stock values.
        assert!(payload.len() < 512, "{} bytes", payload.len());
    }

    #[test]
    fn plugin_disable_follows_the_player_on_plugin_servers_only() {
        let payload = |profile: &CompatProfile, user: &jkr_network::LegacyUserInfo| {
            let payload = jkr_network::legacy_userinfo_payload_with_extensions(
                &profile.userinfo_for(user),
                profile.userinfo_extensions(),
            )
            .unwrap();
            info(&payload).get("cp_pluginDisable").map(str::to_owned)
        };
        let mut user = jkr_network::LegacyUserInfo::with_name("Sol");
        let ja_plus = CompatProfile::JaPlus { version: None };
        // Unset: the default, on both plugin profiles.
        assert_eq!(payload(&ja_plus, &user).as_deref(), Some("1536"));
        assert_eq!(
            payload(&CompatProfile::TaystJk, &user).as_deref(),
            Some("1536")
        );
        // The player's value replaces it, exactly once in the payload.
        user.plugin_disable = Some(512);
        assert_eq!(payload(&ja_plus, &user).as_deref(), Some("512"));
        assert_eq!(
            payload(&CompatProfile::TaystJk, &user).as_deref(),
            Some("512")
        );
        let full = jkr_network::legacy_userinfo_payload_with_extensions(
            &ja_plus.userinfo_for(&user),
            ja_plus.userinfo_extensions(),
        )
        .unwrap();
        assert_eq!(full.matches("cp_pluginDisable").count(), 1);
        // Stock and unknown servers never receive it.
        assert_eq!(payload(&CompatProfile::BaseJka, &user), None);
        assert_eq!(
            payload(&CompatProfile::Unknown("MBII".to_owned()), &user),
            None
        );
    }
}
