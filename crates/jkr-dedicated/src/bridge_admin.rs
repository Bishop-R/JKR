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
            b"amsay" => self.admin_say(client, text),
            b"amtele" => {
                if !self.refused_at_intermission(client, b"amtele") {
                    self.admin_tele(client, &words, level_time);
                }
            }
            b"amtelemark" => {
                if !self.refused_at_intermission(client, b"amtelemark") {
                    self.admin_telemark(client);
                }
            }
            b"amban" => self.admin_ban(client, &words),
            b"amgrantadmin" => self.admin_grant(client, &words),
            b"amlockteam" => self.admin_lock_team(client, &words),
            b"amrename" => {
                if !self.refused_at_intermission(client, b"amrename") {
                    self.admin_rename(client, &words, level_time);
                }
            }
            b"amkillvote" => self.admin_kill_vote(client),
            b"amfreeze" => {
                if !self.refused_at_intermission(client, b"amfreeze") {
                    self.admin_freeze(client, &words, level_time);
                }
            }
            b"ammap" => {
                if !self.refused_at_intermission(client, b"ammap") {
                    self.admin_map(client, &words);
                }
            }
            b"amvstr" => {
                if !self.refused_at_intermission(client, b"amvstr") {
                    self.admin_vstr(client, &words);
                }
            }
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

    /// `Cmd_Amrename_f`: a player's name changed as if it had sent it, and kept for five
    /// seconds.
    fn admin_rename(&mut self, client: usize, words: &[&[u8]], level_time: i32) {
        if !self.admin_allowed(client, admin::A_RENAME, "amRename") {
            return;
        }
        if words.len() != 3 {
            let usage = b"print \"Usage: /amRename <client> <newname>.\n\"".to_vec();
            self.told.push(Told::One(client, usage));
            return;
        }
        let Some(target) = self.admin_target(client, words[1]) else {
            return;
        };
        let Some(userinfo) = self.peer(target).map(|peer| peer.userinfo.clone()) else {
            return;
        };
        let userinfo = jkr_protocol::info_set_value(&userinfo, b"name", words[2]);
        self.userinfo_changed(target, &userinfo);
        if let Some(peer) = self.peer_mut(target) {
            peer.netname_time = level_time + 5_000;
        }
    }

    /// `Cmd_Amkillvote_f`: the running vote ended unpassed, its strings cleared.
    fn admin_kill_vote(&mut self, client: usize) {
        if !self.admin_allowed(client, admin::A_KILLVOTE, "amKillVote") {
            return;
        }
        if self.vote.time != 0 {
            let line = b"print \"^1Vote has been killed!\n\"".to_vec();
            self.told.push(Told::Everyone(line));
        }
        self.vote.execute_time = 0;
        self.vote.gametype_to = None;
        self.vote.time = 0;
        self.vote.display.clear();
        self.vote.string.clear();
        let cleared = [
            jkr_game_jka::vote::CS_VOTE_TIME,
            jkr_game_jka::vote::CS_VOTE_STRING,
            jkr_game_jka::vote::CS_VOTE_YES,
            jkr_game_jka::vote::CS_VOTE_NO,
        ]
        .map(|index| jkr_game_jka::vote::Said::ConfigString(index, Vec::new()));
        self.tell_vote(cleared.into(), None);
    }

    /// `Cmd_Ammap_f`: the game type and a map this server has, announced and run through
    /// the console. (jaPRO's game type check never refuses, and reads its first digit only.)
    fn admin_map(&mut self, client: usize, words: &[&[u8]]) {
        if !self.admin_allowed(client, admin::A_CHANGEMAP, "amMap") {
            return;
        }
        if words.len() != 3 {
            let usage = b"print \"Usage: /amMap <gametype #> <map>.\n\"".to_vec();
            self.told.push(Told::One(client, usage));
            return;
        }
        let map = words[2];
        if map.iter().any(|byte| matches!(byte, b';' | b'\r' | b'\n')) {
            let line = b"print \"Invalid map string.\n\"".to_vec();
            self.told.push(Told::One(client, line));
            return;
        }
        let gametype = words[1]
            .first()
            .filter(|digit| digit.is_ascii_digit())
            .map_or(0, |digit| i32::from(digit - b'0'));
        let path = format!("maps/{}.bsp", String::from_utf8_lossy(map));
        if !self.config_files.has(&path) {
            return;
        }
        let Some(name) = self.peer(client).map(|peer| peer.name.clone()) else {
            return;
        };
        let line = [
            b"print \"^3Map change triggered by ^7".as_slice(),
            &name,
            b"\n\"",
        ]
        .concat();
        self.told.push(Told::Everyone(line));
        self.log(&format!(
            "Map change triggered by ^7{}\n",
            String::from_utf8_lossy(&name)
        ));
        let commands = [
            format!("g_gametype {gametype}\n").into_bytes(),
            [b"map ".as_slice(), map, b"\n"].concat(),
        ];
        for command in commands {
            self.commands
                .add_text(&command, &mut super::bridge_bots::console);
        }
    }

    /// `Cmd_Amvstr_f`: a variable's commands run through the console, if it holds any.
    fn admin_vstr(&mut self, client: usize, words: &[&[u8]]) {
        if !self.admin_allowed(client, admin::A_VSTR, "amVstr") {
            return;
        }
        if words.len() != 2 {
            let usage = b"print \"Usage: /amVstr <vstr>.\n\"".to_vec();
            self.told.push(Told::One(client, usage));
            return;
        }
        if words[1]
            .iter()
            .any(|byte| matches!(byte, b';' | b'\r' | b'\n'))
        {
            let line = b"print \"Invalid vstr string.\n\"".to_vec();
            self.told.push(Told::One(client, line));
            return;
        }
        let name = super::bridge_bots::bridge_minplayers::clean_str(&words[1].to_ascii_lowercase());
        if self.cvars.string(&name).is_empty() {
            return;
        }
        let Some(by) = self.peer(client).map(|peer| peer.name.clone()) else {
            return;
        };
        let line = [
            b"print \"^3Vstr (".as_slice(),
            &name,
            b"^3) executed by ^7",
            &by,
            b"\n\"",
        ]
        .concat();
        self.told.push(Told::Everyone(line));
        self.log(&format!(
            "Vstr ({}^7) executed by ^7{}\n",
            String::from_utf8_lossy(&name),
            String::from_utf8_lossy(&by)
        ));
        let command = [b"vstr ".as_slice(), &name, b"\n"].concat();
        self.commands
            .add_text(&command, &mut super::bridge_bots::console);
    }

    /// `Cmd_Amfreeze_f`: a player frozen where it stands and unhurt, or let go again; or
    /// everyone at once (`+all`, `-all`).
    fn admin_freeze(&mut self, client: usize, words: &[&[u8]], level_time: i32) {
        if !self.admin_allowed(client, admin::A_FREEZE, "amFreeze") {
            return;
        }
        if words.len() != 2 {
            let usage =
                b"print \"Usage: /amFreeze <client> or /amFreeze +all or /amFreeze -all.\n\"";
            self.told.push(Told::One(client, usage.to_vec()));
            return;
        }
        let all = if words[1].eq_ignore_ascii_case(b"+all") {
            Some(true)
        } else if words[1].eq_ignore_ascii_case(b"-all") {
            Some(false)
        } else {
            None
        };
        if let Some(freeze) = all {
            let usable: Vec<usize> = (0..self.players.places())
                .filter(|&slot| {
                    self.peer(slot).is_some_and(|peer| {
                        admin::usable_on(
                            peer.session.account_flags,
                            slot == client,
                            admin::A_FREEZE,
                        )
                    })
                })
                .collect();
            for slot in usable {
                if freeze {
                    if let Some(peer) = self.peer_mut(slot) {
                        peer.admin_frozen = true;
                    }
                    // (sic: jaPRO shakes and thunders even for the already frozen)
                    self.freeze_effects(slot, level_time);
                } else if self.peer(slot).is_some_and(|peer| peer.admin_frozen) {
                    self.unfreeze(slot);
                }
            }
            let line: &[u8] = if freeze {
                b"cp \"You have all been frozen\n\""
            } else {
                b"cp \"You have all been unfrozen\n\""
            };
            self.told.push(Told::Everyone(line.to_vec()));
            return;
        }
        let Some(target) = self.admin_target(client, words[1]) else {
            return;
        };
        let their_flags = self
            .peer(target)
            .map_or(0, |peer| peer.session.account_flags);
        if !admin::usable_on(their_flags, target == client, admin::A_FREEZE) {
            return;
        }
        let Some(peer) = self.peer_mut(target) else {
            return;
        };
        peer.noclip = false;
        if peer.admin_frozen {
            self.unfreeze(target);
            let line = b"cp \"You have been unfrozen\n\"".to_vec();
            self.told.push(Told::One(target, line));
        } else {
            peer.admin_frozen = true;
            let line = b"cp \"You have been frozen\n\"".to_vec();
            self.told.push(Told::One(target, line));
            self.freeze_effects(target, level_time);
        }
    }

    /// A frozen player let go: its saber may be thrown and its Force used again, and it
    /// moves again (jaPRO's chain ends in `PM_NORMAL` at the next think; nothing else
    /// here leaves a player in `PM_FREEZE`).
    fn unfreeze(&mut self, slot: usize) {
        const PS_SABER_CAN_THROW: usize = 49;
        const PS_FORCE_RESTRICTED: usize = 115;
        const PM_NORMAL: u8 = 0;
        if let Some(peer) = self.peer_mut(slot) {
            peer.admin_frozen = false;
            peer.state.set_raw_field(PS_SABER_CAN_THROW, 1);
            peer.state.set_raw_field(PS_FORCE_RESTRICTED, 0);
            if peer.state.movement_type() == jkr_game_jka::japro_emotes::PM_FREEZE {
                peer.state.set_movement_type(PM_NORMAL);
            }
            peer.movement = peer.movement.reseeded(&peer.state);
        }
    }

    /// The freeze's show: the player's own view shaken (`G_ScreenShake(origin, target, 3,
    /// 2000, qtrue)`) and a thunderclap where it stands (`G_Sound(target, CHAN_AUTO, ...)`).
    fn freeze_effects(&mut self, slot: usize, level_time: i32) {
        const EV_GENERAL_SOUND: u32 = 76;
        let Some(origin) = self.peer(slot).map(|peer| peer.state.origin()) else {
            return;
        };
        let shake = EventEntity::screen_shake(origin, Some(slot as u16), 3.0, 2_000, true);
        if let Some(id) = self.pool.spawn_temporary(shake.state(), level_time, None) {
            self.pool.set_broadcast(id, shake.broadcast);
        }
        let sound = self.sound_index_for(b"sound/ambience/thunder_close1");
        let thunder = EventEntity {
            event: EV_GENERAL_SOUND,
            parameter: u32::from(sound),
            origin,
            client: None,
            broadcast: false,
            extra: [(0, 0); 12],
        };
        let _ = self.pool.spawn_temporary(thunder.state(), level_time, None);
    }
}

impl NativeGame {
    /// jaPRO's `noclip <player>` for an admin: that player's noclip toggled, it told; not
    /// for one riding a vehicle.
    pub(super) fn admin_noclip(&mut self, client: usize, name: &[u8]) {
        let Some(target) = self.admin_target(client, name) else {
            return;
        };
        let Some(peer) = self.peer_mut(target) else {
            return;
        };
        if peer.state.vehicle_entity_num() != 0 {
            return;
        }
        let message = jkr_game_jka::noclip::toggle(&mut peer.noclip);
        self.told.push(Told::One(target, message.to_vec()));
    }
}

impl NativeGame {
    /// `Cmd_Amban_f`: the player's address added to the bans (`AddIP`, kept in
    /// `g_banIPs`) and the player kicked; never one holding the power too.
    fn admin_ban(&mut self, client: usize, words: &[&[u8]]) {
        if !self.admin_allowed(client, admin::A_ADMINBAN, "amBan") {
            return;
        }
        if words.len() != 2 {
            let usage = b"print \"Usage: /amBan <client>.\n\"".to_vec();
            self.told.push(Told::One(client, usage));
            return;
        }
        let Some(target) = self.admin_target(client, words[1]) else {
            return;
        };
        let Some((their_flags, address)) = self
            .peer(target)
            .map(|peer| (peer.session.account_flags, peer.address.clone()))
        else {
            return;
        };
        if !admin::usable_on(their_flags, target == client, admin::A_ADMINBAN) {
            return;
        }
        // `sess.IP`: the address without its port.
        let ip = address
            .split(|&byte| byte == b':')
            .next()
            .unwrap_or_default();
        let ban_ips = self.ip_filter.add(ip, &mut super::bridge_bots::console);
        self.set_cvar("g_banIPs", &String::from_utf8_lossy(&ban_ips));
        let command = format!("clientkick {target}\n");
        self.commands
            .add_text(command.as_bytes(), &mut super::bridge_bots::console);
    }

    /// `Cmd_Amgrantadmin_f`: the junior level's powers given to a player, or with `none`
    /// every admin power taken from it.
    fn admin_grant(&mut self, client: usize, words: &[&[u8]]) {
        if !self.admin_allowed(client, admin::A_GRANTADMIN, "amGrantAdmin") {
            return;
        }
        if words.len() != 2 && words.len() != 3 {
            let usage = b"print \"Usage: /amGrantAdmin <client>.\n\"".to_vec();
            self.told.push(Told::One(client, usage));
            return;
        }
        let Some(target) = self.admin_target(client, words[1]) else {
            return;
        };
        let junior = self.cvars.integer(b"g_juniorAdminLevel") as u32 & admin::ADMIN_BITS;
        let Some(peer) = self.peer_mut(target) else {
            return;
        };
        if !admin::usable_on(
            peer.session.account_flags,
            target == client,
            admin::A_GRANTADMIN,
        ) {
            return;
        }
        match words.get(2) {
            None => {
                peer.session.account_flags |= junior;
                if junior != 0 {
                    let line = b"print \"You have been granted Junior admin privileges.\n\"";
                    self.told.push(Told::One(target, line.to_vec()));
                }
            }
            Some(word) if word.eq_ignore_ascii_case(b"none") => {
                peer.session.account_flags &= !admin::ADMIN_BITS;
            }
            Some(_) => {}
        }
    }
}

impl NativeGame {
    /// jaPRO's `G_AdminAllowed` for a power cheats may stand in for: allowed with the power;
    /// otherwise told so and refused. (Without the power jaPRO hands a cheater its race
    /// teleport, which is not ported.)
    fn admin_or_cheats(&mut self, client: usize, power: u32) -> bool {
        if self
            .peer(client)
            .is_some_and(|peer| peer.session.account_flags & power != 0)
        {
            return true;
        }
        if !self.settings.cheats {
            let line = b"print \"Cheats are not enabled on this server.\n\"".to_vec();
            self.told.push(Told::One(client, line));
        }
        false
    }

    /// `Cmd_Amtelemark_f`: the teleport marker set where the admin stands (a little higher
    /// for one following someone), facing as it faces, and told.
    fn admin_telemark(&mut self, client: usize) {
        const TEAM_SPECTATOR: i32 = 3;
        const SPECTATOR_FOLLOW: i32 = 2;
        if !self.admin_or_cheats(client, admin::A_TELEMARK) {
            return;
        }
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let mut origin = peer.state.origin();
        if peer.session.team == TEAM_SPECTATOR && peer.session.spectator_state == SPECTATOR_FOLLOW {
            origin[2] += 58.0;
        }
        let [pitch, yaw, _] = peer.state.view_angles();
        peer.telemark = (origin, [yaw, pitch]);
        let line = format!(
            "print \"Teleport Marker: ^3<{}, {}, {}> {}, {}\n\"",
            origin[0] as i32, origin[1] as i32, origin[2] as i32, yaw as i32, pitch as i32
        );
        self.told.push(Told::One(client, line.into_bytes()));
    }

    /// `Cmd_Amtele_f`'s admin forms: to the marker; to a player; one player to another; to
    /// a place (with a yaw); a player to a place (with a yaw). Never a player holding the
    /// power, nor to a spectator without the power to see hidden players.
    fn admin_tele(&mut self, client: usize, words: &[&[u8]], level_time: i32) {
        const TEAM_SPECTATOR: i32 = 3;
        const SPECTATOR_FOLLOW: i32 = 2;
        let following = self.peer(client).is_some_and(|peer| {
            peer.session.team == TEAM_SPECTATOR && peer.session.spectator_state == SPECTATOR_FOLLOW
        });
        if following || !self.admin_or_cheats(client, admin::A_ADMINTELE) {
            return;
        }
        let number = |word: &[u8]| atoi_f32(word);
        let usable = |game: &Self, target: usize| {
            game.peer(target).is_some_and(|peer| {
                admin::usable_on(
                    peer.session.account_flags,
                    target == client,
                    admin::A_ADMINTELE,
                )
            })
        };
        // A spectator is only found by one who may see hidden players.
        let visible = |game: &Self, target: usize| {
            game.peer(target)
                .is_some_and(|peer| peer.session.team != TEAM_SPECTATOR)
                || game
                    .peer(client)
                    .is_some_and(|peer| peer.session.account_flags & admin::A_SEEHIDDEN != 0)
        };
        let above = |game: &Self, target: usize| {
            game.peer(target).map(|peer| {
                let origin = peer.state.origin();
                [origin[0], origin[1], origin[2] + 96.0]
            })
        };
        match words.len() {
            1 => {
                let Some((origin, [yaw, pitch])) = self.peer(client).map(|peer| peer.telemark)
                else {
                    return;
                };
                if origin == [0.0; 3] && yaw == 0.0 {
                    let line = b"print \"No telemark set!\n\"".to_vec();
                    self.told.push(Told::One(client, line));
                    return;
                }
                self.admin_teleport(client, origin, [pitch, yaw, 0.0], level_time);
            }
            2 => {
                let Some(target) = self.admin_target(client, words[1]) else {
                    return;
                };
                if !visible(self, target) {
                    return;
                }
                if let Some(origin) = above(self, target) {
                    self.admin_teleport(client, origin, [0.0; 3], level_time);
                }
            }
            3 => {
                let first = self.admin_target(client, words[1]);
                let second = self.admin_target(client, words[2]);
                let (Some(first), Some(second)) = (first, second) else {
                    return;
                };
                if !visible(self, second) || !usable(self, first) {
                    return;
                }
                if let Some(origin) = above(self, second) {
                    self.admin_teleport(first, origin, [0.0; 3], level_time);
                }
            }
            4 => {
                let origin = [number(words[1]), number(words[2]), number(words[3])];
                self.admin_teleport(client, origin, [0.0; 3], level_time);
            }
            5 => match self.admin_target(client, words[1]) {
                // `amtele x y z yaw`: no player by the first word.
                None => {
                    let origin = [number(words[1]), number(words[2]), number(words[3])];
                    let yaw = number(words[4]);
                    self.admin_teleport(client, origin, [0.0, yaw, 0.0], level_time);
                }
                Some(target) => {
                    if usable(self, target) {
                        let origin = [number(words[2]), number(words[3]), number(words[4])];
                        self.admin_teleport(target, origin, [0.0; 3], level_time);
                    }
                }
            },
            6 => {
                let Some(target) = self.admin_target(client, words[1]) else {
                    return;
                };
                if usable(self, target) {
                    let origin = [number(words[2]), number(words[3]), number(words[4])];
                    let yaw = number(words[5]);
                    self.admin_teleport(target, origin, [0.0, yaw, 0.0], level_time);
                }
            }
            _ => {
                let usage = b"print \"Usage: /amTele or /amTele <client> or /amTele <client> <client> or /amTele <X> <Y> <Z> <YAW> or /amTele <player> <X> <Y> <Z> <YAW>.\n\"";
                self.told.push(Told::One(client, usage.to_vec()));
            }
        }
    }
}

/// `atoi` as a float: jaPRO reads teleport coordinates as whole numbers.
fn atoi_f32(word: &[u8]) -> f32 {
    let text = String::from_utf8_lossy(word);
    let text = text.trim_start();
    let end = text
        .char_indices()
        .take_while(|&(index, c)| c.is_ascii_digit() || (index == 0 && (c == '-' || c == '+')))
        .count();
    text[..end].parse::<i32>().unwrap_or(0) as f32
}

impl NativeGame {
    /// jaPRO's admin-only voting (`Cmd_CallVote_f`, `g_cmds.c:3154-3157`): once either
    /// admin level holds the call-vote power, only those who hold it call votes. Whether
    /// `client` was refused (and told). Votes switched off are refused by stock first.
    pub(super) fn admin_only_vote_refused(&mut self, client: usize) -> bool {
        let levels =
            self.cvars.integer(b"g_fullAdminLevel") | self.cvars.integer(b"g_juniorAdminLevel");
        if self.cvars.integer(b"g_allowVote") == 0 || levels as u32 & admin::A_CALLVOTE == 0 {
            return false;
        }
        !self.admin_allowed(client, admin::A_CALLVOTE, "callVote")
    }

    /// `Cmd_Amsay_f`: `G_Say(ent, NULL, SAY_ADMIN, text)`, a message the sender and the
    /// admins holding the power to read it see, marked `<Admin>` in yellow (with the
    /// sender's place in a team game).
    fn admin_say(&mut self, client: usize, text: &[u8]) {
        const ESCAPE: u8 = 0x19;
        let arguments: Vec<&[u8]> = jkr_network::LegacyTokens::new(text).collect();
        if arguments.len() < 2 {
            return;
        }
        let mut message = jkr_game_jka::chat::concat_args(&arguments, 1);
        message.truncate(jkr_game_jka::chat::MAX_SAY_TEXT - 1);
        let Some(name) = self.peer(client).map(|peer| peer.name.clone()) else {
            return;
        };
        self.log(&format!(
            "sayadmin: {}: {}\n",
            String::from_utf8_lossy(&name),
            String::from_utf8_lossy(&message)
        ));
        // `Q_strstrip(text, "\n\r", "  ")`.
        for byte in &mut message {
            if matches!(*byte, b'\n' | b'\r') {
                *byte = b' ';
            }
        }
        let tag = [
            [ESCAPE].as_slice(),
            b"^3<Admin>^7(",
            &name,
            b"^7",
            &[ESCAPE],
            b")",
            &[ESCAPE],
            b": ",
        ]
        .concat();
        let location = self.location_of(client);
        let line = match &location {
            Some(place) => [
                b"lchat \"".as_slice(),
                &tag,
                b"\" \"",
                place,
                b"\" \"3\" \"",
                &message,
                format!("\" {client}").as_bytes(),
            ]
            .concat(),
            None => [
                b"chat \"".as_slice(),
                &tag,
                b"^3",
                &message,
                format!("\" {client}").as_bytes(),
            ]
            .concat(),
        };
        println!(
            "{}{}",
            String::from_utf8_lossy(&tag),
            String::from_utf8_lossy(&message)
        );
        for other in 0..self.players.places() {
            let reads = self.peer(other).is_some_and(|peer| {
                peer.begun
                    && (other == client || peer.session.account_flags & admin::A_READAMSAY != 0)
            });
            if reads {
                self.told.push(Told::One(other, line.clone()));
            }
        }
    }
}

/// jaPRO's admin freeze in `ClientThink_real`'s movement chain, ahead of the emote's hold:
/// frozen in place (`PM_FREEZE`), its saber's wound times cleared, unable to throw it or
/// use the Force, its saber put away; not while it duels. Whether the player is frozen,
/// which takes the chain's place for the emote hold.
pub(super) fn freeze_hold(peer: &mut Peer) -> bool {
    const PS_SABER_CAN_THROW: usize = 49;
    const PS_SABER_HOLSTERED: usize = 81;
    const PS_WEAPON_TIME: usize = 10;
    const PS_FORCE_RESTRICTED: usize = 115;
    const WP_SABER: u8 = 3;
    if !peer.admin_frozen || !super::bridge_emotes::reaches_holds(peer) {
        return false;
    }
    if peer.state.duel_in_progress() {
        return true;
    }
    peer.state
        .set_movement_type(jkr_game_jka::japro_emotes::PM_FREEZE);
    peer.saber_cut.attack_wound = 0;
    peer.saber_cut.idle_wound = 0;
    peer.state.set_raw_field(PS_SABER_CAN_THROW, 0);
    peer.state.set_raw_field(PS_FORCE_RESTRICTED, 1);
    if peer.state.weapon() == WP_SABER && peer.state.saber_holstered() < 2 {
        peer.state.set_raw_field(PS_SABER_HOLSTERED, 2);
        peer.state.set_raw_field(PS_WEAPON_TIME, 400);
    }
    peer.movement = peer.movement.reseeded(&peer.state);
    true
}
