//! Chat on this server: `say`, `say_team`, `tell` and `gc` (`jkr_game_jka::chat`), with
//! the location a teammate's message names found in the map's own PVS.

use super::{NativeGame, Told};
use crate::visibility::Eye;
use jkr_game_jka::chat::{self, ChatSettings};
use jkr_network::LegacyTokens;

impl NativeGame {
    /// A chat command from `client`; `false` for any other command.
    pub(super) fn chat_command(&mut self, client: usize, text: &[u8], server_time: i32) -> bool {
        let arguments: Vec<&[u8]> = LegacyTokens::new(text).collect();
        let Some(command) = arguments.first() else {
            return false;
        };
        if !["say", "say_team", "tell", "gc"]
            .iter()
            .any(|name| command.eq_ignore_ascii_case(name.as_bytes()))
        {
            return false;
        }
        // `gc` is `CMD_NOINTERMISSION` (`g_cmds.c:3406`); the three chat commands are not.
        if command.eq_ignore_ascii_case(b"gc") && self.refused_at_intermission(client, command) {
            return true;
        }
        let location = self.location_of(client);
        let settings = ChatSettings {
            gametype: self.gametype,
            now: server_time,
        };
        let views = self.client_views();
        let Some(spoken) =
            chat::chat_command(&settings, &views, client, &arguments, location.as_deref())
        else {
            return false;
        };
        for line in spoken.log {
            self.log(&format!("{}\n", String::from_utf8_lossy(&line)));
        }
        for line in spoken.console {
            println!("{}", String::from_utf8_lossy(&line));
        }
        for line in spoken.security {
            self.security_log(&line);
        }
        for (to, text) in spoken.told {
            if !self.ignores(to, client) {
                self.told.push(Told::One(to, text));
            }
        }
        true
    }

    /// jaPRO's `ClientIsIgnored`: `listener` ignores `speaker`'s chat.
    pub(super) fn ignores(&self, listener: usize, speaker: usize) -> bool {
        self.peer(listener)
            .is_some_and(|peer| peer.session.ignore.ignores(listener, speaker))
    }

    /// jaPRO's `ignore` (`Cmd_Ignore_f`, no gates): alone, the players `client` ignores;
    /// `-1`, everyone (or nobody, if everyone already is); a player, named as the admin
    /// commands name one, that player (or no longer). Whether `text` was the command.
    pub(super) fn ignore_command(&mut self, client: usize, text: &[u8]) -> bool {
        let words: Vec<&[u8]> = LegacyTokens::new(text).collect();
        if !words
            .first()
            .is_some_and(|word| word.eq_ignore_ascii_case(b"ignore"))
        {
            return false;
        }
        let reply = match words[..] {
            [_] => {
                let mut lines = vec![b"print \"Ignored client(s):\n\"".to_vec()];
                for other in 0..self.players.places() {
                    if let Some(peer) = self.peer(other)
                        && peer.begun
                        && self.ignores(client, other)
                    {
                        lines.push([b"print \" ".as_slice(), &peer.name, b"\n\""].concat());
                    }
                }
                lines
            }
            [_, b"-1"] => {
                let all = self
                    .peer_mut(client)
                    .is_some_and(|peer| peer.session.ignore.toggle_all());
                let text: &[u8] = if all {
                    b"print \"Ignored all\n\""
                } else {
                    b"print \"Unignored all\n\""
                };
                vec![text.to_vec()]
            }
            [_, name] => {
                let Some(target) = self.admin_target(client, name) else {
                    return true;
                };
                if target == client {
                    vec![b"print \"Cannot ignore self\n\"".to_vec()]
                } else {
                    let name = self
                        .peer(target)
                        .map(|peer| peer.name.clone())
                        .unwrap_or_default();
                    let ignored = self
                        .peer_mut(client)
                        .is_some_and(|peer| peer.session.ignore.toggle(target));
                    let verb: &[u8] = if ignored { b"Ignored " } else { b"Unignored " };
                    vec![[b"print \"".as_slice(), verb, &name, b"\n\""].concat()]
                }
            }
            _ => vec![b"print \"Invalid usage\n\"".to_vec()],
        };
        for line in reply {
            self.told.push(Told::One(client, line));
        }
        true
    }

    /// `Team_GetLocationMsg` for a client: the nearest of the map's locations its
    /// position can see, measured and seen from `r.currentOrigin` itself. Only a team game
    /// ever names one, so a free-for-all looks nothing up.
    pub(super) fn location_of(&self, client: usize) -> Option<Vec<u8>> {
        if self.gametype < jkr_game_jka::match_end::GT_TEAM {
            return None;
        }
        let map = self.map.as_ref()?;
        let origin = self.peer(client)?.state.origin();
        let eye = Eye::new(&map.bsp, &map.areas, origin);
        chat::location_message(&map.locations, origin, |point| {
            eye.sees_point(&map.bsp, point)
        })
    }

    /// `Team_GetLocation`'s `cs_index` for a client in a team game, for its team overlay:
    /// 0 for none.
    pub(super) fn location_number(&self, client: usize) -> u32 {
        if self.gametype < jkr_game_jka::match_end::GT_TEAM {
            return 0;
        }
        let (Some(map), Some(peer)) = (self.map.as_ref(), self.peer(client)) else {
            return 0;
        };
        let origin = peer.state.origin();
        let eye = Eye::new(&map.bsp, &map.areas, origin);
        chat::location_number(chat::nearest_location(&map.locations, origin, |point| {
            eye.sees_point(&map.bsp, point)
        }))
    }
}
