//! The game log on this server (`G_LogPrintf`): each line printed on the console as a
//! dedicated server prints it, and appended to `g_log` in the home directory with the
//! level's stamp before it — opened as a level starts, closed as it ends.

use super::NativeGame;
use jkr_game_jka::game_log::{self, Who};
use std::io::Write;

/// `SECURITY_LOG` (`g_local.h`).
const SECURITY_LOG: &str = "security.log";

/// A server command too long for a legacy client to read (over 1022 bytes): whom it was
/// for (-1 for everyone) and the command.
fn too_long(told: &super::Told) -> Option<(i32, &[u8])> {
    const MAX_COMMAND: usize = 1_022;
    match told {
        super::Told::Everyone(text) if text.len() > MAX_COMMAND => Some((-1, text)),
        super::Told::One(client, text) if text.len() > MAX_COMMAND => Some((*client as i32, text)),
        _ => None,
    }
}

/// `pers.guid` for a userinfo: its `ja_guid`, else `NOGUID`.
pub(super) fn guid(userinfo: &[u8]) -> Vec<u8> {
    jkr_protocol::info_value(userinfo, b"ja_guid")
        .filter(|value| !value.is_empty())
        .map_or_else(|| b"NOGUID".to_vec(), <[u8]>::to_vec)
}

impl NativeGame {
    /// `G_LogPrintf`: `line` on the console, and stamped in the log file if one is open.
    pub(super) fn log(&mut self, line: &str) {
        let _ = std::io::stdout().write_all(line.as_bytes());
        let level_time = self.last_frame_time.max(self.level_start_time);
        if let Some(file) = &mut self.log_file {
            let stamped = game_log::stamp(level_time, self.level_start_time) + line;
            // A log that cannot be written is not the match's concern.
            let _ = file.write_all(stamped.as_bytes());
        }
    }

    /// `G_InitGame`'s log: `g_log` opened for appending in the home directory (nothing
    /// without a name or a home), then the level's first two lines.
    pub(super) fn open_log(&mut self) {
        let name = String::from_utf8_lossy(self.cvars.string(b"g_log")).into_owned();
        let print = |text: String| {
            let _ = std::io::stdout().write_all(text.as_bytes());
        };
        self.log_file = None;
        if name.is_empty() {
            print("Not logging game events to disk.\n".to_owned());
        } else {
            match self.config_files.append_home(&name) {
                Some(file) => {
                    self.log_file = Some(file);
                    print(format!("Logging to {name}\n"));
                }
                None => print(format!("WARNING: Couldn't open logfile: {name}\n")),
            }
        }
        for line in game_log::init_game(&self.status_info.clone()) {
            self.log(&line);
        }
        // `g_securityLog` 1 or 2 (appended, flushed or not: every line is written at once).
        self.security_file = None;
        if self.cvars.integer(b"g_securityLog") == 0 {
            print("Not logging security events to disk.\n".to_owned());
        } else {
            match self.config_files.append_home(SECURITY_LOG) {
                Some(file) => {
                    self.security_file = Some(file);
                    print(format!("Logging to {SECURITY_LOG}\n"));
                }
                None => print(format!("WARNING: Couldn't open logfile: {SECURITY_LOG}\n")),
            }
        }
    }

    /// `G_SecurityLogPrintf` (`g_main.c:1541-1564`): `line` on the console, and in
    /// `security.log` behind the date and time (UTC) while it is open.
    pub(super) fn security_log(&mut self, line: &str) {
        let _ = std::io::stdout().write_all(line.as_bytes());
        if let Some(file) = &mut self.security_file {
            let since_epoch = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            let [year, month, day, hour, minute, second] = super::bridge_demos::utc(since_epoch);
            let stamped = format!(
                "[{year:04}-{month:02}-{day:02}] [{hour:02}:{minute:02}:{second:02}] {line}"
            );
            let _ = file.write_all(stamped.as_bytes());
        }
    }

    /// `trap_SendServerCommand` (`g_syscalls.c:101-108`): a command longer than a legacy
    /// client can read (1022 bytes) is not sent, and the security log says so.
    pub(super) fn refuse_long_commands(&mut self) {
        if !self.told.iter().any(|told| too_long(told).is_some()) {
            return;
        }
        let refused: Vec<(i32, String)> = self
            .told
            .iter()
            .filter_map(too_long)
            .map(|(client, text)| (client, String::from_utf8_lossy(text).into_owned()))
            .collect();
        self.told.retain(|told| too_long(told).is_none());
        for (client, text) in refused {
            self.security_log(&format!(
                "trap_SendServerCommand( {client}, ... ) length exceeds 1022.\n"
            ));
            self.security_log(&format!("text [{text}]\n"));
        }
    }

    /// `G_ShutdownGame` as the server quits: the log's last line, the file closed.
    pub fn shut_down(&mut self) {
        self.close_log();
    }

    /// `G_ShutdownGame`'s log: the last line, and the file closed.
    pub(super) fn close_log(&mut self) {
        self.log(&game_log::shutdown_game());
        self.log_file = None;
        if self.security_file.is_some() {
            self.security_log("ShutdownGame\n\n");
            self.security_file = None;
        }
    }

    /// Who a player is to the log: slot, address, guid and name.
    pub(super) fn log_who(&self, client: usize) -> Option<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        let peer = self.peer(client)?;
        // `ClientConnect`: a bot's guid is `BOT`.
        let guid = if peer.bot {
            b"BOT".to_vec()
        } else {
            guid(&peer.userinfo)
        };
        Some((peer.address.clone(), guid, peer.name.clone()))
    }

    /// A line about one player, built from who it is.
    pub(super) fn log_about(&mut self, client: usize, line: impl FnOnce(Who) -> String) {
        let Some((address, guid, name)) = self.log_who(client) else {
            return;
        };
        let text = line(Who {
            client,
            address: &address,
            guid: &guid,
            name: &name,
        });
        self.log(&text);
    }

    /// `player_die`'s line.
    pub(super) fn log_kill(&mut self, victim: usize, attacker: Option<u16>, means: u32) {
        let victim_name = self
            .peer(victim)
            .map(|peer| peer.name.clone())
            .unwrap_or_default();
        let attacker = attacker.map(|number| {
            (
                i32::from(number),
                self.peer(usize::from(number))
                    .map(|peer| peer.name.clone())
                    .unwrap_or_default(),
            )
        });
        let line = game_log::kill(
            attacker
                .as_ref()
                .map(|(number, name)| (*number, name.as_slice())),
            victim,
            &victim_name,
            means,
        );
        self.log(&line);
    }

    /// `LogExit`: why the match ended, the team scores in a team game, and every
    /// connected player in the game by rank, with its score and ping.
    pub(super) fn log_exit(&mut self, reason: &str) {
        self.log(&game_log::exit(reason));
        let team_game = self.gametype >= jkr_game_jka::match_end::GT_TEAM;
        if team_game {
            self.log(&game_log::team_scores(
                self.team_scores[0],
                self.team_scores[1],
            ));
        }
        let (sorted, _) = self.sorted();
        for client in sorted.into_iter().map(usize::from) {
            let Some(peer) = self
                .peer(client)
                .filter(|peer| peer.begun && peer.session.team != i32::from(super::TEAM_SPECTATOR))
            else {
                continue;
            };
            let score = peer.state.persistent[super::PERS_SCORE] as i32;
            let line = game_log::score(
                team_game.then_some(peer.session.team),
                score,
                peer.ping,
                &guid(&peer.userinfo),
                client,
                &peer.name,
            );
            self.log(&line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Identity, NativeGame, Told};
    use jkr_network::LegacyGameHost;

    /// A game with one client connected but not yet in the game.
    fn connected() -> NativeGame {
        let identity = Identity {
            hostname: b"test".to_vec(),
            mapname: b"mp/ffa3".to_vec(),
        };
        let cvars = crate::cvars::Cvars::server();
        let mut game = NativeGame::with_cvars(identity, None, 4, 4, cvars).unwrap();
        let userinfo = b"\\name\\a\\rate\\25000\\snaps\\40\\model\\kyle/default\\forcepowers\\7-1-032330000000001333\\color1\\4\\color2\\4\\handicap\\100\\cg_predictItems\\1\\saber1\\single_1\\saber2\\none\\char_color_red\\255\\char_color_green\\255\\char_color_blue\\255\\ip\\127.0.0.1:1";
        game.client_connect(0, userinfo).unwrap();
        game.told.clear();
        game
    }

    /// `trap_SendServerCommand`: a command over 1022 bytes is not sent; a shorter one is.
    #[test]
    fn a_command_too_long_for_a_legacy_client_is_refused() {
        let mut game = connected();
        game.told.push(Told::One(0, vec![b'x'; 1_023]));
        game.told.push(Told::Everyone(b"print \"short\"".to_vec()));
        game.refuse_long_commands();
        assert_eq!(game.told.len(), 1);
        assert!(matches!(&game.told[0], Told::Everyone(text) if text == b"print \"short\""));
    }

    /// `ClientCommand`: a client not fully in the game yet is not listened to.
    #[test]
    fn a_client_not_yet_in_the_game_is_not_listened_to() {
        let mut game = connected();
        game.player_command(0, b"say hello", 100);
        let said = game.told.iter().any(|told| {
            matches!(told, Told::Everyone(text) | Told::One(_, text)
                if text.windows(5).any(|window| window == b"hello"))
        });
        assert!(!said);
    }
}
