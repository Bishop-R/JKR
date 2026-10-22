//! jaPRO's player log on this server (`g_playerLog`, 0; `G_AddPlayerLog`,
//! `g_account.c:211-270`): each name a person goes by — bots aside — with its host and
//! GUID, written once to `players.log` in the home directory as it connects or renames
//! itself; and `amLookup <player>` (`Cmd_Amlookup_f`, the lookup power), which lists the
//! names that player's host has used.

use super::*;
use jkr_game_jka::client_view::strip_colours;

const G_PLAYER_LOG: &[u8] = b"g_playerLog";
/// `PLAYER_LOG`.
const PLAYER_LOG: &str = "players.log";
/// jaPRO reads the log only below 80 KiB.
const LIMIT: usize = 80 * 1024;
/// The default names jaPRO does not log.
const DEFAULT_NAMES: [&[u8]; 3] = [b"padawan", b"padawan[0]", b"padawan[1]"];

/// An address's host: what comes before its port.
fn host(address: &[u8]) -> &[u8] {
    address
        .split(|byte| *byte == b':')
        .next()
        .unwrap_or_default()
}

impl NativeGame {
    /// `g_playerLog`, archived and announced, off (jaPRO's).
    pub(super) fn register_player_log(&mut self) {
        let about: &[u8] = b"jaPRO: every name a player uses is kept with its address (amLookup)";
        self.cvars
            .get_announced(G_PLAYER_LOG, b"0", crate::cvars::CVAR_ARCHIVE, Some(about));
    }

    /// `G_AddPlayerLog` for `client` under its name now: its line (`name;host;guid`, the
    /// name in lower case without colours) added unless the log has it already.
    pub(super) fn note_player(&mut self, client: usize) {
        if self.cvars.integer(G_PLAYER_LOG) == 0 || self.peer(client).is_none_or(|peer| peer.bot) {
            return;
        }
        let Some((address, guid, name)) = self.log_who(client) else {
            return;
        };
        let host = host(&address);
        if (host.is_empty() && guid == b"NOGUID")
            || DEFAULT_NAMES
                .iter()
                .any(|known| name.eq_ignore_ascii_case(known))
        {
            return;
        }
        let clean = strip_colours(&name).to_ascii_lowercase();
        let line = [&clean[..], b";", host, b";", &guid].concat();
        let known = self.config_files.read_home(PLAYER_LOG).unwrap_or_default();
        if known.len() >= LIMIT {
            println!("ERROR: Couldn't load player logfile {PLAYER_LOG}, file is too large");
            return;
        }
        if known
            .split(|byte| *byte == b'\n')
            .any(|logged| logged.eq_ignore_ascii_case(&line))
        {
            return;
        }
        let _ = self
            .config_files
            .append_record(PLAYER_LOG, &[&line[..], b"\n"].concat());
    }

    /// `amLookup <player>` from `client`: the names `player`'s host has used here, one a
    /// line. Whether `command` was it.
    pub(super) fn amlookup_command(&mut self, client: usize, text: &[u8]) -> bool {
        let words: Vec<&[u8]> = jkr_network::LegacyTokens::new(text).collect();
        if !words
            .first()
            .is_some_and(|word| word.eq_ignore_ascii_case(b"amlookup"))
        {
            return false;
        }
        if !self.admin_allowed(client, crate::admin::A_LOOKUP, "amLookup") {
            return true;
        }
        if words.len() != 2 {
            self.told.push(Told::One(
                client,
                b"print \"Usage: /amLookup <client>\n\"".to_vec(),
            ));
            return true;
        }
        let Some(target) = self.admin_target(client, words[1]) else {
            return true;
        };
        let Some(address) = self.peer(target).map(|peer| peer.address.clone()) else {
            return true;
        };
        let Some(known) = self.config_files.read_home(PLAYER_LOG) else {
            println!("ERROR: Couldn't load player logfile {PLAYER_LOG}");
            return true;
        };
        if known.len() >= LIMIT {
            println!("ERROR: Couldn't load player logfile {PLAYER_LOG}, file is too large");
            return true;
        }
        // The lines' fields one after another, as `strtok(";\n")` walks them: a field that
        // is the host names the one before it.
        let host = host(&address);
        let fields = known.split(|byte| matches!(byte, b';' | b'\n'));
        let mut names: Vec<&[u8]> = Vec::new();
        let mut last: &[u8] = &[];
        for field in fields.filter(|field| !field.is_empty()) {
            if field.eq_ignore_ascii_case(host) {
                names.push(last);
            }
            last = field;
        }
        let list = names
            .iter()
            .map(|name| [&b"^7"[..], name].concat())
            .collect::<Vec<_>>()
            .join(&b"\n  "[..]);
        self.told.push(Told::One(
            client,
            [
                &b"print \"^5 This players IP has used the following names on this server:\n  "[..],
                &list,
                b"\n\"",
            ]
            .concat(),
        ));
        true
    }
}
