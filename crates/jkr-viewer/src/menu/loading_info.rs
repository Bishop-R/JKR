//! What stock's loading screen tells about the server and the match (`CG_DrawInformation`,
//! OpenJK `codemp/cgame/cg_info.c:150-330`): the server's name, whether it is pure, its
//! message of the day, the map's own message, a cheats warning, the game type with its
//! limits, the Force and weapon settings, and the game type's rules. JKR shows these in
//! its own loading notice; this gathers the lines from the gamestate.

use crate::Localization;
use jkr_protocol::{GameState, info_value};

/// `CS_SERVERINFO`, `CS_SYSTEMINFO`, `CS_MESSAGE`, `CS_MOTD`.
const CS_SERVERINFO: usize = 0;
const CS_SYSTEMINFO: usize = 1;
const CS_MESSAGE: usize = 3;
const CS_MOTD: usize = 4;
/// `gametype_t`.
const GT_HOLOCRON: i32 = 1;
const GT_JEDIMASTER: i32 = 2;
const GT_DUEL: i32 = 3;
const GT_POWERDUEL: i32 = 4;
const GT_SINGLE_PLAYER: i32 = 5;
const GT_TEAM: i32 = 6;
const GT_SIEGE: i32 = 7;
const GT_CTF: i32 = 8;
const GT_CTY: i32 = 9;
/// `forceMasteryLevels` (`bg_misc.c:153-163`): the `MASTERY0`..`MASTERY7` strings' English.
const MASTERY: [&str; 8] = [
    "Uninitiated",
    "Initiate",
    "Padawan",
    "Jedi",
    "Jedi Adept",
    "Jedi Guardian",
    "Jedi Knight",
    "Jedi Master",
];

/// The loading notice's lines for `game`, in the order stock draws them.
pub(crate) fn lines(game: &GameState, localization: &Localization) -> Vec<String> {
    let text = |key: &str, english: &str| {
        localization
            .strings
            .get(key)
            .map_or_else(|| english.to_owned(), Clone::clone)
    };
    let config = |index| game.config_string(index).unwrap_or_default();
    let (info, system) = (config(CS_SERVERINFO), config(CS_SYSTEMINFO));
    let value = |from: &[u8], key: &str| {
        info_value(from, key.as_bytes())
            .map(|value| String::from_utf8_lossy(value).into_owned())
            .unwrap_or_default()
    };
    let number = |key: &str| atoi(&value(info, key));
    let plain = |raw: &[u8]| {
        super::map_picker::strip_colours(&localization.translate(&String::from_utf8_lossy(raw)))
    };
    let mut lines = Vec::new();
    let mut push = |line: String| {
        if !line.trim().is_empty() {
            lines.push(line);
        }
    };
    push(plain(value(info, "sv_hostname").as_bytes()));
    if value(system, "sv_pure").starts_with('1') {
        push(text("PURE_SERVER", "Pure Server"));
    }
    push(plain(config(CS_MOTD)));
    push(plain(config(CS_MESSAGE)));
    if value(system, "sv_cheats").starts_with('1') {
        push(text("CHEATSAREENABLED", "Cheats are enabled"));
    }
    let gametype = number("g_gametype");
    push(gametype_name(gametype).to_owned());
    let limit = |key: &str, label: &str, english: &str| {
        let limit = number(key);
        (limit != 0).then(|| format!("{} {limit}", text(label, english)))
    };
    if gametype != GT_SIEGE {
        push(limit("timelimit", "TIMELIMIT", "Time Limit").unwrap_or_default());
        if gametype < GT_CTF {
            push(limit("fraglimit", "FRAGLIMIT", "Frag Limit").unwrap_or_default());
            if matches!(gametype, GT_DUEL | GT_POWERDUEL) {
                push(limit("duel_fraglimit", "WINLIMIT", "Win Limit").unwrap_or_default());
            }
        }
    }
    if gametype >= GT_CTF {
        push(limit("capturelimit", "CAPTURELIMIT", "Capture Limit").unwrap_or_default());
    }
    if gametype >= GT_TEAM && number("g_forceBasedTeams") != 0 {
        push(text("FORCEBASEDTEAMS", "Force-based teams"));
    }
    if gametype != GT_SIEGE {
        let no_force = number("g_forcePowerDisable") != 0;
        let rank = number("g_maxForceRank");
        if !no_force {
            let level = if rank > 0 && (rank as usize) < MASTERY.len() {
                rank as usize
            } else {
                MASTERY.len() - 1
            };
            let mastery = text(&format!("MASTERY{level}"), MASTERY[level]);
            push(format!(
                "{} {mastery}",
                text("MAXFORCERANK", "Max Force Rank:")
            ));
        }
        let weapons = if matches!(gametype, GT_DUEL | GT_POWERDUEL) {
            number("g_duelWeaponDisable")
        } else {
            number("g_weaponDisable")
        };
        if gametype != GT_JEDIMASTER && weapons != 0 {
            push(text("SABERONLYSET", "Saber only"));
        }
        if no_force {
            push(text("NOFPSET", "No Force powers"));
        }
    }
    for key in rules(gametype) {
        push(text(key, ""));
    }
    lines
}

/// `BG_GetGametypeString` (`bg_misc.c:3193-3222`).
fn gametype_name(gametype: i32) -> &'static str {
    match gametype {
        0 => "Free For All",
        GT_HOLOCRON => "Holocron",
        GT_JEDIMASTER => "Jedi Master",
        GT_DUEL => "Duel",
        GT_POWERDUEL => "Power Duel",
        GT_SINGLE_PLAYER => "Cooperative",
        GT_TEAM => "Team Deathmatch",
        GT_SIEGE => "Siege",
        GT_CTF => "Capture The Flag",
        GT_CTY => "Capture The Ysalimiri",
        _ => "Unknown Gametype",
    }
}

/// The rules lines the loading screen gives each game type (`cg_info.c:315-370`).
fn rules(gametype: i32) -> &'static [&'static str] {
    match gametype {
        0 => &["RULES_FFA_1"],
        GT_HOLOCRON => &["RULES_HOLO_1", "RULES_HOLO_2"],
        GT_JEDIMASTER => &["RULES_JEDI_1", "RULES_JEDI_2"],
        GT_DUEL => &["RULES_DUEL_1", "RULES_DUEL_2"],
        GT_POWERDUEL => &["RULES_POWERDUEL_1", "RULES_POWERDUEL_2"],
        GT_TEAM => &["RULES_TEAM_1", "RULES_TEAM_2"],
        GT_CTF => &["RULES_CTF_1", "RULES_CTF_2"],
        GT_CTY => &["RULES_CTY_1", "RULES_CTY_2"],
        _ => &[],
    }
}

/// C's `atoi`: the leading whole number, 0 for none.
fn atoi(text: &str) -> i32 {
    let text = text.trim_start();
    let (sign, digits) = match text.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, text.strip_prefix('+').unwrap_or(text)),
    };
    let end = digits
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(digits.len());
    digits[..end]
        .parse::<i32>()
        .map_or(0, |number| sign * number)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A duel server's name, its message of the day, the game type, the limits it sets and
    /// the duel's rules, in stock's order; nothing for what is unset.
    #[test]
    fn a_duel_server_is_described_as_stock_describes_it() {
        let mut game = GameState::empty_local(0);
        let info = b"\\sv_hostname\\^1My ^7Server\\g_gametype\\3\\fraglimit\\0\\duel_fraglimit\\5\\timelimit\\10\\g_maxForceRank\\7";
        let _ = game.replace_config_string(CS_SERVERINFO, info.to_vec());
        let _ = game.replace_config_string(CS_MOTD, b"Welcome".to_vec());
        let lines = lines(&game, &Localization::default());
        assert_eq!(
            lines,
            [
                "My Server",
                "Welcome",
                "Duel",
                "Time Limit 10",
                "Win Limit 5",
                "Max Force Rank: Jedi Master",
            ]
        );
    }
}
