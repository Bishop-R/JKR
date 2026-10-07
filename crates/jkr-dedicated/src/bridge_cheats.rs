//! `ClientCommand`'s table (`g_cmds.c:3393-3431`) and its gates, which every command of
//! the table passes first, in the reference's order: intermission, `sv_cheats`, alive.
//! The commands that live here are the cheats (`give`, `giveother`, `god`, `notarget`,
//! `noclip`, `killother`, `t_use`, `levelshot`, `thedestroyer`, the bot test moves) and
//! `kill` and `maplist`; the others have routes of their own (`bridge_commands.rs`).

use super::*;
use jkr_game_jka::cheat_flags::{FL_GODMODE, FL_NOTARGET};

/// `CMD_NOINTERMISSION`, `CMD_CHEAT`, `CMD_ALIVE` (`g_cmds.c:3378-3380`).
pub(super) const NO_INTERMISSION: u8 = 1;
const CHEAT: u8 = 2;
pub(super) const ALIVE: u8 = 4;

/// The reference's table: every stock command and its gates.
const COMMANDS: &[(&str, u8)] = &[
    ("addbot", 0),
    ("callteamvote", NO_INTERMISSION),
    ("callvote", NO_INTERMISSION),
    ("debugBMove_Back", CHEAT | ALIVE),
    ("debugBMove_Forward", CHEAT | ALIVE),
    ("debugBMove_Left", CHEAT | ALIVE),
    ("debugBMove_Right", CHEAT | ALIVE),
    ("debugBMove_Up", CHEAT | ALIVE),
    ("duelteam", NO_INTERMISSION),
    ("follow", NO_INTERMISSION),
    ("follownext", NO_INTERMISSION),
    ("followprev", NO_INTERMISSION),
    ("forcechanged", 0),
    ("gc", NO_INTERMISSION),
    ("give", CHEAT | ALIVE | NO_INTERMISSION),
    ("giveother", CHEAT | NO_INTERMISSION),
    ("god", CHEAT | ALIVE | NO_INTERMISSION),
    ("kill", ALIVE | NO_INTERMISSION),
    ("killother", CHEAT | NO_INTERMISSION),
    ("levelshot", CHEAT | ALIVE | NO_INTERMISSION),
    ("maplist", NO_INTERMISSION),
    ("noclip", CHEAT | ALIVE | NO_INTERMISSION),
    ("notarget", CHEAT | ALIVE | NO_INTERMISSION),
    ("npc", CHEAT | ALIVE),
    ("say", 0),
    ("say_team", 0),
    ("score", 0),
    ("setviewpos", CHEAT | NO_INTERMISSION),
    ("siegeclass", NO_INTERMISSION),
    ("team", NO_INTERMISSION),
    ("teamvote", NO_INTERMISSION),
    ("tell", 0),
    ("thedestroyer", CHEAT | ALIVE | NO_INTERMISSION),
    ("t_use", CHEAT | ALIVE),
    ("voice_cmd", NO_INTERMISSION),
    ("vote", NO_INTERMISSION),
    ("where", NO_INTERMISSION),
];

/// `Bot_SetForcedMovement`'s arguments for each test move (`g_cmds.c:3296-3370`).
const BOT_MOVES: [(&str, [i32; 3]); 5] = [
    ("debugBMove_Forward", [4000, -1, -1]),
    ("debugBMove_Back", [-4000, -1, -1]),
    ("debugBMove_Right", [-1, 4000, -1]),
    ("debugBMove_Left", [-1, -4000, -1]),
    ("debugBMove_Up", [-1, -1, 4000]),
];

impl NativeGame {
    /// `ClientCommand`'s gates for `command`: whether it is one of the table's and was
    /// refused, the client told why. Commands outside the table pass, for the routes that
    /// answer them or call them unknown.
    pub(super) fn command_refused(&mut self, client: usize, command: &[u8]) -> bool {
        stock_command(command).is_some_and(|(_, flags)| !self.passes_gates(client, command, flags))
    }

    /// The commands answered here, already past their gates: whether `text` was one.
    pub(super) fn cheat_command(&mut self, client: usize, text: &[u8], server_time: i32) -> bool {
        let words: Vec<&[u8]> = text
            .split(|byte| byte.is_ascii_whitespace())
            .filter(|word| !word.is_empty())
            .collect();
        let Some((&command, arguments)) = words.split_first() else {
            return false;
        };
        let Some((name, _)) = stock_command(command) else {
            return false;
        };
        match name {
            "give" => self.give_to(client, arguments, server_time),
            "giveother" => self.give_other(client, arguments, server_time),
            "god" => self.toggle_flag(client, FL_GODMODE, "godmode"),
            "notarget" => self.toggle_flag(client, FL_NOTARGET, "notarget"),
            "kill" => self.kill(client, server_time),
            "killother" => self.kill_other(client, arguments, server_time),
            // `pers.localClient` is only the loopback client, which a dedicated server has none of.
            "levelshot" => self.told.push(Told::One(
                client,
                b"print \"The levelshot command must be executed by a local client\n\"".to_vec(),
            )),
            "maplist" => {
                for print in self.bots.arenas.map_list(self.gametype) {
                    self.told.push(Told::One(client, print));
                }
            }
            "noclip" => {
                // `Cmd_Noclip_f`: the flag toggled and the player told; its next think
                // puts it in (or takes it out of) `PM_NOCLIP`.
                if let Some(peer) = self.peer_mut(client) {
                    let message = jkr_game_jka::noclip::toggle(&mut peer.noclip);
                    self.told.push(Told::One(client, message.to_vec()));
                }
            }
            "t_use" => {
                // `Cmd_TargetUse_f`: fires a name as the player, if given one.
                if let Some(name) = arguments.first() {
                    let name = String::from_utf8_lossy(name).into_owned();
                    self.fire_targets(&name, client, self.last_frame_time);
                }
            }
            "thedestroyer" => self.the_destroyer(client, server_time),
            _ if name.starts_with("debugBMove_") => self.bot_test_move(name, arguments),
            _ => return false,
        }
        true
    }

    /// `ClientCommand`'s gates (`g_cmds.c:3460-3480`), in its order, having told the
    /// client why when one refuses.
    pub(super) fn passes_gates(&mut self, client: usize, command: &[u8], flags: u8) -> bool {
        if flags & NO_INTERMISSION != 0 && self.refused_at_intermission(client, command) {
            return false;
        }
        if flags & CHEAT != 0 && !self.settings.cheats {
            self.told
                .push(Told::One(client, b"print \"@@@NOCHEATS\n\"".to_vec()));
            return false;
        }
        if flags & ALIVE != 0
            && self
                .peer(client)
                .is_none_or(|peer| peer.health <= 0 || !peer.playing())
        {
            self.told
                .push(Told::One(client, b"print \"@@@MUSTBEALIVE\n\"".to_vec()));
            return false;
        }
        true
    }

    /// `Cmd_God_f` / `Cmd_Notarget_f`: flip the flag and say which way it went.
    fn toggle_flag(&mut self, client: usize, flag: u32, name: &str) {
        if let Some(peer) = self.peer_mut(client) {
            let message = jkr_game_jka::cheat_flags::toggle(&mut peer.riding.flags, flag, name);
            self.told.push(Told::One(client, message));
        }
    }

    /// `G_Give` on `target`: the item's name, then its arguments.
    fn give_to(&mut self, target: usize, arguments: &[&[u8]], level_time: i32) {
        let spawns = self.item_spawns();
        let Some(peer) = self.peer_mut(target) else {
            return;
        };
        let (name, rest) = arguments
            .split_first()
            .map_or((&b""[..], &[][..]), |(name, rest)| (*name, rest));
        let powerup = give(&mut peer.state, &mut peer.health, name, rest)
            == jkr_game_jka::give::Given::Item
            && give_item(peer, name, spawns, level_time);
        // The weapons and the ammo are the movement's to spend: it restarts from
        // the state as given.
        peer.movement = peer.movement.reseeded(&peer.state);
        peer.movement.set_health(peer.health);
        // The reference's item stands at the player's feet as it is touched.
        let at = peer.state.origin();
        if powerup {
            self.deny_nearby(target, at);
        }
    }

    /// `ClientNumberFromString` as the other-player cheats use it: the player, or `None`
    /// having told the asker (`g_cmds.c:180-221`).
    fn other_player(&mut self, client: usize, word: &[u8]) -> Option<usize> {
        let views = self.client_views();
        let found = jkr_game_jka::client_view::client_number_from_string(&views, word, false)
            .map(|view| view.client);
        if found.is_none() {
            let text = [
                b"print \"User ".as_slice(),
                word,
                b" is not on the server\n\"",
            ]
            .concat();
            self.told.push(Told::One(client, text));
        }
        found
    }

    /// The other player must be alive, as `CMD_ALIVE` requires of the asker; the asker
    /// is told otherwise.
    fn other_alive(&mut self, client: usize, other: usize) -> bool {
        let alive = self
            .peer(other)
            .is_some_and(|peer| peer.health > 0 && peer.playing());
        if !alive {
            self.told
                .push(Told::One(client, b"print \"@@@MUSTBEALIVE\n\"".to_vec()));
        }
        alive
    }

    /// `Cmd_GiveOther_f` (`g_cmds.c:360-394`): `giveother <player> <give string>`.
    fn give_other(&mut self, client: usize, arguments: &[&[u8]], level_time: i32) {
        if arguments.len() < 2 {
            self.told.push(Told::One(
                client,
                b"print \"Usage: giveother <player id> <givestring>\n\"".to_vec(),
            ));
            return;
        }
        let Some(other) = self.other_player(client, arguments[0]) else {
            return;
        };
        if self.other_alive(client, other) {
            self.give_to(other, &arguments[1..], level_time);
        }
    }

    /// `Cmd_KillOther_f` (`g_cmds.c:540-569`): `killother <player>`.
    fn kill_other(&mut self, client: usize, arguments: &[&[u8]], server_time: i32) {
        let Some(&word) = arguments.first() else {
            self.told.push(Told::One(
                client,
                b"print \"Usage: killother <player id>\n\"".to_vec(),
            ));
            return;
        };
        let Some(other) = self.other_player(client, word) else {
            return;
        };
        if self.other_alive(client, other) {
            self.kill(other, server_time);
        }
    }

    /// `Cmd_TheDestroyer_f`: a holstered saber in hand is ignited (`Cmd_ToggleSaber_f`).
    fn the_destroyer(&mut self, client: usize, server_time: i32) {
        const WP_SABER: u8 = 3;
        let Some(peer) = self.peer(client) else {
            return;
        };
        if peer.state.saber_holstered() == 0 || peer.state.weapon() != WP_SABER {
            return;
        }
        let origin = peer.state.origin();
        let command = UserCommand {
            generic_command: jkr_game_jka::generic_commands::GENCMD_SABERSWITCH,
            ..Default::default()
        };
        self.generic_command(client, &command, origin, server_time);
    }

    /// `Cmd_BotMove*_f` and `Bot_SetForcedMovement`: the bot named by number (`atoi`, so
    /// none is bot 0) has each given axis switched between that push and none.
    fn bot_test_move(&mut self, name: &str, arguments: &[&[u8]]) {
        let Some(&(_, push)) = BOT_MOVES.iter().find(|(move_name, _)| *move_name == name) else {
            return;
        };
        let bot = arguments
            .first()
            .map_or(0, |word| jkr_game_jka::userinfo::atoi(word));
        let Some(moves) = usize::try_from(bot)
            .ok()
            .and_then(|bot| self.bots.forced_movement(bot))
        else {
            return;
        };
        for (axis, push) in push.into_iter().enumerate() {
            if push != -1 {
                moves[axis] = if moves[axis] != 0 { 0 } else { push };
            }
        }
    }
}

/// The reference's spelling and gates of `command`, if it is in the table.
fn stock_command(command: &[u8]) -> Option<(&'static str, u8)> {
    COMMANDS
        .iter()
        .copied()
        .find(|(name, _)| command.eq_ignore_ascii_case(name.as_bytes()))
}

/// `G_Give`'s last branch (`g_cmds.c:331-349`): the named item spawned on the player,
/// where `spawns` says the server keeps it, and touched at once (`Touch_Item`), then
/// freed. A flag's own rules (`Pickup_Team`) are not run for a given flag. Stock's
/// `EV_ITEM_PICKUP` names the freed item's entity, which no client was ever sent, so it
/// is left out. Whether a powerup was taken, whose anti-reward the caller hands out.
fn give_item(
    peer: &mut Peer,
    name: &[u8],
    spawns: impl Fn(usize) -> bool,
    level_time: i32,
) -> bool {
    use jkr_game_jka::items::{self, ITEMS, Kind};
    let Some(item) = items::find_named(name) else {
        return false;
    };
    if matches!(ITEMS[item].kind, Kind::Team | Kind::Bad) || !spawns(item) || peer.health < 1 {
        return false;
    }
    if items::wrong_side(item, u32::from(peer.state.force_side()))
        || !items::can_be_grabbed(item, &peer.state, false)
    {
        return false;
    }
    let _ = items::pick_up(
        item,
        &mut peer.state,
        &mut peer.health,
        0,
        false,
        level_time,
    );
    ITEMS[item].kind == Kind::Powerup
}
