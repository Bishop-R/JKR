//! jaPRO's password admins, without its account database: `amlogin` grants the full or
//! the junior admin powers (`g_fullAdminLevel`, `g_juniorAdminLevel`, a bit per power) to
//! whoever gives that level's password (`Cmd_Amlogin_f`, `g_cmds.c`); a command asks
//! `G_AdminAllowed` whether its caller holds its power, and `G_AdminUsableOn` whether its
//! target is fair game; players are named as `JP_ClientNumberFromString` reads them.

/// The account bits that are admin powers (`JAPRO_MAX_ADMIN_BITS`, 0-22).
pub const ADMIN_BITS: u32 = (1 << 23) - 1;
/// `JAPRO_ACCOUNTFLAG_A_ADMINKICK`, `JAPRO_ACCOUNTFLAG_A_CSPRINT`.
pub const A_ADMINKICK: u32 = 1 << 4;
pub const A_CSPRINT: u32 = 1 << 9;
/// `JAPRO_ACCOUNTFLAG_A_CHANGEMAP`, `_FORCETEAM`, `_LOCKTEAM`, `_VSTR`, `_RENAME`,
/// `_KILLVOTE`.
pub const A_CHANGEMAP: u32 = 1 << 8;
/// `JAPRO_ACCOUNTFLAG_A_FREEZE`.
pub const A_FREEZE: u32 = 1 << 1;
pub const A_FORCETEAM: u32 = 1 << 10;
pub const A_LOCKTEAM: u32 = 1 << 11;
pub const A_VSTR: u32 = 1 << 12;
pub const A_RENAME: u32 = 1 << 14;
pub const A_KILLVOTE: u32 = 1 << 21;
/// `TEAM_FREE`, `TEAM_RED`, `TEAM_BLUE`, `TEAM_SPECTATOR`.
pub const TEAM_FREE: i32 = 0;
pub const TEAM_RED: i32 = 1;
pub const TEAM_BLUE: i32 = 2;
pub const TEAM_SPECTATOR: i32 = 3;
/// `MAX_NAME_LENGTH`: `SanitizeString2` reads no further.
const MAX_NAME_LENGTH: usize = 32;

/// The team `amLockTeam` and `amForceTeam` name with `word`, any case.
pub fn team_word(word: &[u8]) -> Option<i32> {
    let is = |words: &[&[u8]]| words.iter().any(|name| word.eq_ignore_ascii_case(name));
    if is(&[b"red", b"r"]) {
        Some(TEAM_RED)
    } else if is(&[b"blue", b"b"]) {
        Some(TEAM_BLUE)
    } else if is(&[b"s", b"spectator", b"spec", b"spectate"]) {
        Some(TEAM_SPECTATOR)
    } else if is(&[b"f", b"free", b"join", b"j", b"enter"]) {
        Some(TEAM_FREE)
    } else {
        None
    }
}

/// The level's team locks (`level.isLockedfree` .. `isLockedspec`), which a map clears.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TeamLocks([bool; 4]);

impl TeamLocks {
    /// `amLockTeam`: `team` locked if it was open, opened if it was locked; whether it is
    /// locked now.
    pub fn toggle(&mut self, team: i32) -> bool {
        let lock = &mut self.0[team as usize & 3];
        *lock = !*lock;
        *lock
    }

    pub fn locked(self, team: i32) -> bool {
        self.0[team as usize & 3]
    }
}

/// jaPRO's `SetTeam` refusals for a player's own request (`g_cmds.c:1185-1270`), the team
/// its word leads to being `team` (in a team game `PickTeam`'s choice for any other word):
/// that team locked, or in a team game full (`sv_maxTeamSize`, `max_size` > 0, against the
/// players `on_team` already there).
pub fn team_refusal(
    team: i32,
    team_game: bool,
    locks: TeamLocks,
    max_size: i32,
    on_team: i32,
) -> Option<&'static [u8]> {
    if team == TEAM_SPECTATOR {
        return locks
            .locked(team)
            .then_some(b"print \"^7The ^3Spectator ^7Access has been locked!\n\"".as_slice());
    }
    if !team_game {
        return locks
            .locked(TEAM_FREE)
            .then_some(b"print \"^7The ^3Free ^7team is locked!\n\"".as_slice());
    }
    let (locked, full): (&[u8], &[u8]) = if team == TEAM_RED {
        (
            b"print \"^7The ^1Red ^7team is locked!\n\"",
            b"print \"^7The ^1Red ^7team is full!\n\"",
        )
    } else {
        (
            b"print \"^7The ^4Blue ^7team is locked!\n\"",
            b"print \"^7The ^4Blue ^7team is full!\n\"",
        )
    };
    if locks.locked(team) {
        Some(locked)
    } else if max_size != 0 && on_team >= max_size {
        Some(full)
    } else {
        None
    }
}

/// What `amlogin <password>` came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Login {
    /// No password given.
    Usage,
    /// New powers granted, full or junior.
    Granted {
        full: bool,
    },
    /// The level's powers were all held already.
    Already,
    Wrong,
}

/// `Cmd_Amlogin_f` for `password`, the levels' `(password, powers)`: the full level's first,
/// then the junior's, both compared without regard to case as jaPRO does. `flags` gains the
/// level's powers.
pub fn login(flags: &mut u32, password: &[u8], full: (&[u8], u32), junior: (&[u8], u32)) -> Login {
    if password.is_empty() {
        return Login::Usage;
    }
    let level = if password.eq_ignore_ascii_case(full.0) {
        Some((true, full.1))
    } else if password.eq_ignore_ascii_case(junior.0) {
        Some((false, junior.1))
    } else {
        None
    };
    let Some((full, powers)) = level else {
        return Login::Wrong;
    };
    let added = powers & ADMIN_BITS & !*flags;
    *flags |= added;
    if added != 0 {
        Login::Granted { full }
    } else {
        Login::Already
    }
}

/// `Cmd_Amlogout_f`: every admin power given up; whether there was one.
pub fn logout(flags: &mut u32) -> bool {
    let had = *flags & ADMIN_BITS != 0;
    *flags &= !ADMIN_BITS;
    had
}

/// `G_AdminUsableOn`: a power may be used on anyone who does not hold it, and on oneself.
pub fn usable_on(their_flags: u32, themselves: bool, power: u32) -> bool {
    themselves || their_flags & power == 0
}

/// `SanitizeString2`: a name as matched, colour codes (`^` and a digit) and control
/// characters dropped, lower case, at most 31 characters of the original read.
pub fn sanitize(name: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(name.len());
    let mut index = 0;
    while index < name.len() && name[index] != 0 && index < MAX_NAME_LENGTH - 1 {
        let byte = name[index];
        if byte == b'^' {
            let colour = name.get(index + 1).is_some_and(u8::is_ascii_digit);
            index += if colour { 2 } else { 1 };
            continue;
        }
        if byte >= 32 {
            out.push(byte.to_ascii_lowercase());
        }
        index += 1;
    }
    out
}

/// Whom `JP_ClientNumberFromString` finds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Found {
    Client(usize),
    /// A slot number with no player in the game: "Client 'n' is not active".
    Inactive(usize),
    /// "More than one user '...' on the server".
    Ambiguous,
    /// "User '...' is not on the server" (said only when `text` does not read as a number).
    Missing {
        say: bool,
    },
}

/// `JP_ClientNumberFromString` for `text` among `clients` (slot, in the game, name): 0-31
/// as one or two digits are slot numbers; anything else is a part of one present player's
/// name, colours and case aside.
pub fn find_client(text: &[u8], clients: &[(usize, bool, &[u8])]) -> Found {
    let slot = match text {
        [digit] if digit.is_ascii_digit() => Some(usize::from(digit - b'0')),
        [b'1' | b'2', digit] if digit.is_ascii_digit() => {
            Some(usize::from((text[0] - b'0') * 10 + (digit - b'0')))
        }
        [b'3', b'0' | b'1'] => Some(30 + usize::from(text[1] - b'0')),
        _ => None,
    };
    if let Some(slot) = slot {
        let active = clients
            .iter()
            .any(|&(client, in_game, _)| client == slot && in_game);
        return if active {
            Found::Client(slot)
        } else {
            Found::Inactive(slot)
        };
    }
    let wanted = sanitize(text);
    let mut matches = clients
        .iter()
        .filter(|(_, _, name)| contains(&sanitize(name), &wanted));
    match (matches.next(), matches.next()) {
        (Some(&(client, _, _)), None) => Found::Client(client),
        (Some(_), Some(_)) => Found::Ambiguous,
        _ => Found::Missing {
            say: atoi(text) == 0,
        },
    }
}

/// `strstr(haystack, needle) != NULL`.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty()
        || haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

/// C's `atoi`: leading blanks, a sign, digits.
fn atoi(text: &[u8]) -> i64 {
    let text = text.trim_ascii_start();
    let (sign, digits) = match text.first() {
        Some(b'-') => (-1, &text[1..]),
        Some(b'+') => (1, &text[1..]),
        _ => (1, text),
    };
    sign * digits
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .fold(0_i64, |value, byte| {
            value
                .saturating_mul(10)
                .saturating_add(i64::from(byte - b'0'))
        })
}

/// `Cmd_Ampsay_f`'s message: the words after the command with each `\n` written out turned
/// into a line break.
pub fn centre_text(words: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(words.len());
    let mut index = 0;
    while index < words.len() {
        if words[index] == b'\\' && words.get(index + 1) == Some(&b'n') {
            out.push(b'\n');
            index += 2;
        } else {
            out.push(words[index]);
            index += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The full password before the junior's, case aside; powers already held grant nothing.
    #[test]
    fn a_password_grants_its_level_once() {
        let mut flags = 0;
        let levels = (
            (b"Secret".as_slice(), 0b11_0000),
            (b"junior".as_slice(), 0b1_0000),
        );
        assert_eq!(login(&mut flags, b"", levels.0, levels.1), Login::Usage);
        assert_eq!(login(&mut flags, b"nope", levels.0, levels.1), Login::Wrong);
        assert_eq!(
            login(&mut flags, b"JUNIOR", levels.0, levels.1),
            Login::Granted { full: false }
        );
        assert_eq!(flags, 0b1_0000);
        assert_eq!(
            login(&mut flags, b"junior", levels.0, levels.1),
            Login::Already
        );
        assert_eq!(
            login(&mut flags, b"secret", levels.0, levels.1),
            Login::Granted { full: true }
        );
        assert_eq!(flags, 0b11_0000);
        // Empty passwords (the defaults) match nothing anyone can type.
        let mut none = 0;
        assert_eq!(
            login(&mut none, b"x", (b"", u32::MAX), (b"", u32::MAX)),
            Login::Wrong
        );
        assert!(logout(&mut flags) && flags == 0 && !logout(&mut flags));
    }

    /// Slot numbers up to 31, else a unique part of a name, colours and case aside.
    #[test]
    fn players_are_found_by_slot_or_part_of_a_name() {
        let clients: [(usize, bool, &[u8]); 3] = [
            (0, true, b"^1Kyle ^7Katarn"),
            (3, true, b"Tavion"),
            (12, false, b"Padawan"),
        ];
        assert_eq!(find_client(b"3", &clients), Found::Client(3));
        assert_eq!(find_client(b"12", &clients), Found::Inactive(12));
        assert_eq!(find_client(b"31", &clients), Found::Inactive(31));
        assert_eq!(find_client(b"katarn", &clients), Found::Client(0));
        assert_eq!(find_client(b"a", &clients), Found::Ambiguous);
        assert_eq!(
            find_client(b"desann", &clients),
            Found::Missing { say: true }
        );
        assert_eq!(find_client(b"40", &clients), Found::Missing { say: false });
        assert_eq!(sanitize(b"^1Ky^xle\x01"), b"kyxle");
        assert_eq!(centre_text(b"Hello\\nworld"), b"Hello\nworld");
        assert!(
            usable_on(A_ADMINKICK, true, A_ADMINKICK)
                && !usable_on(A_ADMINKICK, false, A_ADMINKICK)
        );
    }

    /// A locked team refuses whoever asks for it; a full one only in team games.
    #[test]
    fn locked_and_full_teams_refuse_a_join() {
        let mut locks = TeamLocks::default();
        assert_eq!(team_word(b"SPECTATE"), Some(TEAM_SPECTATOR));
        assert_eq!(team_word(b"enter"), Some(TEAM_FREE));
        assert_eq!(team_word(b"green"), None);
        assert!(locks.toggle(TEAM_RED));
        assert_eq!(
            team_refusal(TEAM_RED, true, locks, 0, 0),
            Some(b"print \"^7The ^1Red ^7team is locked!\n\"".as_slice())
        );
        assert_eq!(team_refusal(TEAM_BLUE, true, locks, 0, 9), None);
        assert_eq!(
            team_refusal(TEAM_BLUE, true, locks, 2, 2),
            Some(b"print \"^7The ^4Blue ^7team is full!\n\"".as_slice())
        );
        assert_eq!(team_refusal(TEAM_FREE, false, locks, 1, 5), None);
        assert!(!locks.toggle(TEAM_RED));
        locks.toggle(TEAM_FREE);
        assert!(team_refusal(TEAM_FREE, false, locks, 0, 0).is_some());
    }
}
