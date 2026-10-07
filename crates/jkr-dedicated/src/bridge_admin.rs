//! jaPRO's admin commands on this server ([`crate::admin`]): the levels' options, and
//! `amlogin`, `amlogout`, `amkick` and `ampsay`. The other `am*` admin commands, account
//! logins and bans are not ported yet.

use super::*;
use crate::admin::{self, Found, Login};

/// The levels' options: name, jaPRO's default, description (all archived).
const OPTIONS: [(&[u8], &[u8], &[u8]); 7] = [
    (
        b"g_juniorAdminLevel",
        b"0",
        b"jaPRO: junior admin powers, a bit each",
    ),
    (
        b"g_fullAdminLevel",
        b"0",
        b"jaPRO: full admin powers, a bit each",
    ),
    (
        b"g_juniorAdminPass",
        b"",
        b"jaPRO: the junior admins' amlogin password",
    ),
    (
        b"g_fullAdminPass",
        b"",
        b"jaPRO: the full admins' amlogin password",
    ),
    (
        b"g_juniorAdminMsg",
        b"",
        b"jaPRO: told to everyone when a junior admin logs in",
    ),
    (
        b"g_fullAdminMsg",
        b"",
        b"jaPRO: told to everyone when a full admin logs in",
    ),
    (
        b"sv_maxTeamSize",
        b"0",
        b"jaPRO: the most players a team game's side takes (0: no limit)",
    ),
];
/// `GT_FFA`, `GT_DUEL`, `GT_POWERDUEL`, `GT_TEAM`.
const GT_FFA: i32 = 0;
const GT_DUEL: i32 = 3;
const GT_POWERDUEL: i32 = 4;
const GT_TEAM: i32 = 6;

impl NativeGame {
    /// The admin levels' options, with jaPRO's defaults: no powers, no passwords.
    pub(super) fn register_admin(&mut self) {
        for (name, value, about) in OPTIONS {
            self.cvars
                .get(name, value, crate::cvars::CVAR_ARCHIVE, Some(about));
        }
    }

    /// jaPRO's admin commands from `client`: whether `text`'s command was one.
    pub(super) fn admin_command(&mut self, client: usize, text: &[u8], level_time: i32) -> bool {
        let words: Vec<&[u8]> = text
            .split(|byte| byte.is_ascii_whitespace())
            .filter(|word| !word.is_empty())
            .collect();
        let Some(command) = words.first() else {
            return false;
        };
        let command = command.to_ascii_lowercase();
        match command.as_slice() {
            b"amlogin" => self.admin_login(client, &words),
            b"amlogout" => {
                let had = self
                    .peer_mut(client)
                    .is_some_and(|peer| admin::logout(&mut peer.session.account_flags));
                let text: &[u8] = if had {
                    b"print \"You are no longer an admin.\n\""
                } else {
                    b"print \"You are not logged in as an admin!\n\""
                };
                self.told.push(Told::One(client, text.to_vec()));
            }
            b"amkick" => self.admin_kick(client, &words),
            b"amlockteam" => self.admin_lock_team(client, &words),
            b"amforceteam" => {
                if !self.refused_at_intermission(client, b"amforceteam") {
                    self.admin_force_team(client, &words, level_time);
                }
            }
            b"ampsay" => {
                if !self.refused_at_intermission(client, b"ampsay")
                    && self.admin_allowed(client, admin::A_CSPRINT, "amPsay")
                {
                    if words.len() < 2 {
                        let usage = b"print \"Usage: /amPsay <message>.\n\"".to_vec();
                        self.told.push(Told::One(client, usage));
                    } else {
                        let message = admin::centre_text(&words[1..].join(&b' '));
                        let line = [b"cp \"".as_slice(), &message, b"\""].concat();
                        self.told.push(Told::Everyone(line));
                    }
                }
            }
            _ => return false,
        }
        true
    }

    /// `Cmd_Amlogin_f`: the full or junior level's powers for its password.
    fn admin_login(&mut self, client: usize, words: &[&[u8]]) {
        let usage = b"print \"Usage: amLogin <password>\n\"".to_vec();
        if words.len() == 1 {
            self.told.push(Told::One(client, usage));
            return;
        }
        if words.len() != 2 {
            return;
        }
        let level = |name: &[u8], pass: &[u8]| {
            (
                self.cvars.string(pass).to_vec(),
                self.cvars.integer(name) as u32,
            )
        };
        let full = level(b"g_fullAdminLevel", b"g_fullAdminPass");
        let junior = level(b"g_juniorAdminLevel", b"g_juniorAdminPass");
        let messages = [b"g_fullAdminMsg".as_slice(), b"g_juniorAdminMsg"]
            .map(|name| self.cvars.string(name).to_vec());
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let outcome = admin::login(
            &mut peer.session.account_flags,
            words[1],
            (&full.0, full.1),
            (&junior.0, junior.1),
        );
        let name = peer.name.clone();
        let told = match outcome {
            Login::Usage => Told::One(client, usage),
            Login::Wrong => Told::One(
                client,
                b"print \"^3Failed to log in: Incorrect password!\n\"".to_vec(),
            ),
            Login::Already => Told::One(
                client,
                b"print \"You are already logged in. Type in /amLogout to remove admin status.\n\""
                    .to_vec(),
            ),
            Login::Granted { full } => {
                let message = &messages[usize::from(!full)];
                if message.is_empty() {
                    let level = if full { "full" } else { "junior" };
                    let line = format!(
                        "print \"^2You are now logged in with {level} admin privileges.\n\""
                    );
                    Told::One(client, line.into_bytes())
                } else {
                    let line = [
                        b"print \"".as_slice(),
                        &name,
                        b"^7 has logged in ",
                        message,
                        b"\n\"",
                    ]
                    .concat();
                    Told::Everyone(line)
                }
            }
        };
        self.told.push(told);
    }

    /// `Cmd_Amkick_f`: `clientkick` for the player named, if it does not hold the power too.
    fn admin_kick(&mut self, client: usize, words: &[&[u8]]) {
        if !self.admin_allowed(client, admin::A_ADMINKICK, "amKick") {
            return;
        }
        if words.len() != 2 {
            let usage = b"print \"Usage: /amKick <client>.\n\"".to_vec();
            self.told.push(Told::One(client, usage));
            return;
        }
        let Some(target) = self.admin_target(client, words[1]) else {
            return;
        };
        let their_flags = self
            .peer(target)
            .map_or(0, |peer| peer.session.account_flags);
        if !admin::usable_on(their_flags, target == client, admin::A_ADMINKICK) {
            return;
        }
        let command = format!("clientkick {target}\n");
        self.commands
            .add_text(command.as_bytes(), &mut super::bridge_bots::console);
    }

    /// `G_AdminAllowed` without race mode or cheats: whether `client` holds `power`, having
    /// told it otherwise.
    fn admin_allowed(&mut self, client: usize, power: u32, name: &str) -> bool {
        let allowed = self
            .peer(client)
            .is_some_and(|peer| peer.session.account_flags & power != 0);
        if !allowed {
            let line = format!("print \"You are not authorized to use this command ({name}).\n\"");
            self.told.push(Told::One(client, line.into_bytes()));
        }
        allowed
    }

    /// `JP_ClientNumberFromString` for `client`'s `text`, having told it why there is none.
    fn admin_target(&mut self, client: usize, text: &[u8]) -> Option<usize> {
        let present: Vec<(usize, bool, Vec<u8>)> = (0..self.players.places())
            .filter_map(|slot| {
                self.peer(slot)
                    .map(|peer| (slot, peer.begun, peer.name.clone()))
            })
            .collect();
        let clients: Vec<(usize, bool, &[u8])> = present
            .iter()
            .map(|(slot, begun, name)| (*slot, *begun, name.as_slice()))
            .collect();
        let quoted =
            |prefix: &str, suffix: &str| [prefix.as_bytes(), text, suffix.as_bytes()].concat();
        let told = match admin::find_client(text, &clients) {
            Found::Client(slot) => return Some(slot),
            Found::Inactive(slot) => {
                format!("print \"Client '{slot}' is not active\n\"").into_bytes()
            }
            Found::Ambiguous => quoted("print \"More than one user '", "' on the server\n\""),
            Found::Missing { say: true } => quoted("print \"User '", "' is not on the server\n\""),
            Found::Missing { say: false } => return None,
        };
        self.told.push(Told::One(client, told));
        None
    }

    /// `Cmd_Amlockteam_f`: a team locked, or opened again, for everyone to hear.
    fn admin_lock_team(&mut self, client: usize, words: &[&[u8]]) {
        if !self.admin_allowed(client, admin::A_LOCKTEAM, "amLockTeam") {
            return;
        }
        if self.gametype < GT_TEAM && self.gametype != GT_FFA {
            let line = b"print \"You can not use this command in this gametype (amLockTeam).\n\"";
            self.told.push(Told::One(client, line.to_vec()));
            return;
        }
        let team = (words.len() == 2)
            .then(|| admin::team_word(words[1]))
            .flatten();
        let Some(team) = team else {
            let usage = b"print \"Usage: /amLockTeam <team>\n\"".to_vec();
            self.told.push(Told::One(client, usage));
            return;
        };
        let locked = self.team_locks.toggle(team);
        let name = ["Free", "Red", "Blue", "Spectator"][team as usize];
        let line = match (team, locked) {
            // (sic: jaPRO's unlocked spectators are lower case)
            (admin::TEAM_SPECTATOR, false) => {
                "print \"The spectator team is now unlocked.\n\"".to_owned()
            }
            (_, true) => format!("print \"The {name} team is now locked.\n\""),
            (_, false) => format!("print \"The {name} team is now unlocked.\n\""),
        };
        self.told.push(Told::Everyone(line.into_bytes()));
    }

    /// `Cmd_Amforceteam_f`: a player, or everyone (`-1`), sent to a team past its locks
    /// and limits (`SetTeam(ent, team, qtrue)`).
    fn admin_force_team(&mut self, client: usize, words: &[&[u8]], level_time: i32) {
        if !self.admin_allowed(client, admin::A_FORCETEAM, "amForceTeam") {
            return;
        }
        let usage = b"print \"Usage: amForceTeam <client> <team>\n\"";
        if words.len() != 3 {
            self.told.push(Told::One(client, usage.to_vec()));
            return;
        }
        let team_game = self.gametype >= GT_TEAM;
        if !team_game && self.gametype != GT_FFA {
            let line = b"print \"You can not use this command in this gametype (amForceTeam).\n\"";
            self.told.push(Told::One(client, line.to_vec()));
            return;
        }
        let everyone = words[1] == b"-1";
        let target = if everyone {
            None
        } else {
            let Some(target) = self.admin_target(client, words[1]) else {
                return;
            };
            let their_flags = self
                .peer(target)
                .map_or(0, |peer| peer.session.account_flags);
            if !admin::usable_on(their_flags, target == client, admin::A_FORCETEAM) {
                return;
            }
            Some(target)
        };
        let team = admin::team_word(words[2])
            .filter(|&team| team_game || !matches!(team, admin::TEAM_RED | admin::TEAM_BLUE));
        let Some(team) = team else {
            self.told.push(Told::One(client, usage.to_vec()));
            return;
        };
        let (word, shown): (&[u8], &str) = match team {
            admin::TEAM_RED => (b"red", "^1Red"),
            admin::TEAM_BLUE => (b"blue", "^4Blue"),
            admin::TEAM_SPECTATOR => (b"spectator", "^3Spectator"),
            _ => (b"free", "^2Free"),
        };
        let moved: Vec<usize> = match target {
            Some(target) => vec![target],
            None => (0..self.players.places())
                .filter(|&slot| self.peer(slot).is_some_and(|peer| peer.begun))
                .collect(),
        };
        for slot in moved {
            let Some((on, name)) = self
                .peer(slot)
                .map(|peer| (peer.session.team, peer.name.clone()))
            else {
                continue;
            };
            if on == team {
                continue;
            }
            self.set_team_to(slot, word, level_time);
            if !everyone {
                let tail = format!(" ^7has been forced to the {shown} ^7team.\n\"");
                let line = [b"print \"".as_slice(), &name, tail.as_bytes()].concat();
                self.told.push(Told::Everyone(line));
            }
        }
    }

    /// jaPRO's `SetTeam` refusals for `client`'s own `team` command, where stock's
    /// `Cmd_Team_f` would go on to `SetTeam`: a locked team, or a full one.
    pub(super) fn locked_team(
        &mut self,
        client: usize,
        arguments: &[&[u8]],
        sides: jkr_game_jka::client_begin::Sides,
        level_time: i32,
    ) -> Option<&'static [u8]> {
        let [word] = arguments else {
            return None;
        };
        let peer = self.peer(client)?;
        // `Cmd_Team_f`'s own checks come first: the switch timer and the duels.
        if peer.session.switch_team_time > level_time
            || (self.gametype == GT_DUEL && peer.session.team == admin::TEAM_FREE)
            || self.gametype == GT_POWERDUEL
        {
            return None;
        }
        let team_game = self.gametype >= GT_TEAM;
        let named = jkr_game_jka::client_begin::team_for_word(word, sides);
        let team = if team_game || named == admin::TEAM_SPECTATOR {
            named
        } else {
            admin::TEAM_FREE
        };
        let on_team = (0..self.players.places())
            .filter(|&slot| {
                self.peer(slot)
                    .is_some_and(|peer| peer.session.team == team)
            })
            .count() as i32;
        let max_size = self.cvars.integer(b"sv_maxTeamSize");
        admin::team_refusal(team, team_game, self.team_locks, max_size, on_team)
    }
}
