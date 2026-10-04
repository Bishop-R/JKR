//! Chat presentation and addressing at the protocol-26 compatibility boundary.
//!
//! OpenJK `codemp/game/g_cmds.c:G_SayTo` appends the sender slot to `chat`
//! and `tchat`; it is metadata, not part of the quoted message. `Cmd_Tell_f`
//! accepts `tell <slot> <message>`. No transport or wire encoding lives here.

use crate::{GameState, ServerEvent, ServerEventKind};

const CS_PLAYERS: usize = 1_131;
// codemp/qcommon/q_shared.h:890, protocol-26's absolute player-slot limit.
const MAX_CLIENTS: usize = 32;
/// Conservative compose budget, in UTF-8 bytes, below codemp's MAX_SAY_TEXT.
pub const CHAT_INPUT_BYTES: usize = 149;

/// Stock reliable-command destination; private messages address a numeric slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChatDestination {
    /// Everyone the server permits to receive global chat.
    Global,
    /// The sender's team, subject to the server's game-mode policy.
    Team,
    /// One currently connected player.
    Player(u16),
}

/// A roster observation, invalidated on a name change or observed slot reuse.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChatTarget {
    slot: u16,
    generation: u64,
}

impl ChatTarget {
    /// Server-side client slot; callers must validate with the current roster.
    pub const fn slot(self) -> u16 {
        self.slot
    }
}

#[derive(Default)]
struct Player {
    // Cache wire spelling so lossy legacy-name decoding occurs only on identity changes.
    source_name: Vec<u8>,
    raw_name: String,
    name: String,
    generation: u64,
}

/// Bounded, presentation-only roster for chat actions. Refreshing an unchanged
/// gamestate borrows configstrings and does not allocate.
pub struct ChatRoster {
    players: [Player; MAX_CLIENTS],
}

impl Default for ChatRoster {
    fn default() -> Self {
        Self {
            players: std::array::from_fn(|_| Player::default()),
        }
    }
}

impl ChatRoster {
    /// Observe compact `CS_PLAYERS` name keys, including TaystJK's trailing `\`.
    pub fn update(&mut self, game: &GameState) {
        for (slot, player) in self.players.iter_mut().enumerate() {
            let name = game
                .config_string(CS_PLAYERS + slot)
                .and_then(player_name)
                .unwrap_or(b"");
            if player.source_name != name {
                player.source_name.clear();
                player.source_name.extend_from_slice(name);
                player.generation = player.generation.wrapping_add(1);
                player.raw_name.clear();
                // Latin-1, not UTF-8: see `legacy_text::decode_legacy`.
                player
                    .raw_name
                    .push_str(&crate::legacy_text::decode_legacy(name));
                player.name = chat_plain_text(&player.raw_name);
            }
        }
    }

    /// Resolve server-provided metadata. Unattributed messages stay unattributed;
    /// displayed names are never guessed into private-message destinations.
    pub fn target(&self, sender: Option<u16>) -> Option<ChatTarget> {
        let slot = sender?;
        let player = self.players.get(usize::from(slot))?;
        (!player.name.is_empty()).then_some(ChatTarget {
            slot,
            generation: player.generation,
        })
    }

    /// Current name with its colour escapes, for display only; identity and
    /// destinations keep using [`ChatRoster::name`].
    pub fn display_name(&self, target: ChatTarget) -> Option<&str> {
        let player = self.players.get(usize::from(target.slot))?;
        (player.generation == target.generation && !player.raw_name.is_empty())
            .then_some(player.raw_name.as_str())
    }

    /// Current plain name, only while this exact roster observation is valid.
    pub fn name(&self, target: ChatTarget) -> Option<&str> {
        let player = self.players.get(usize::from(target.slot))?;
        (player.generation == target.generation && !player.name.is_empty())
            .then_some(player.name.as_str())
    }

    /// Resolve a typed name to one connected player's slot; see [`crate::lookup_player`].
    pub fn lookup(&self, query: &str) -> crate::PlayerLookup {
        crate::lookup_player(
            self.players.iter().enumerate().map(|(slot, player)| {
                (slot as u16, player.name.as_str(), player.raw_name.as_str())
            }),
            query,
        )
    }
}

fn player_name(config: &[u8]) -> Option<&[u8]> {
    let info = crate::LegacyClientInfo::new(config);
    info.bytes("n").or_else(|| info.bytes("name"))
}

/// Plain modern-chat text: remove Quake colour escapes and the stock chat
/// separator (0x19), retaining Unicode and ordinary punctuation.
pub fn chat_plain_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '^' && chars.peek().is_some_and(char::is_ascii_digit) {
            chars.next();
        } else if c == '\n' || c == '\r' || c == '\t' {
            output.push(' ');
        } else if !c.is_control() {
            output.push(c);
        }
    }
    output
}

/// Display chat text: drop the stock 0x19 separators and control characters
/// but keep `^n` colour escapes, which the text renderer applies per glyph.
pub fn chat_display_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for c in value.chars() {
        if c == '\n' || c == '\r' || c == '\t' {
            output.push(' ');
        } else if !c.is_control() {
            output.push(c);
        }
    }
    output
}

/// Chars of `value` with `^n` colour escapes skipped, as `(byte index, char)`.
fn uncoloured(value: &str) -> impl Iterator<Item = (usize, char)> + '_ {
    let mut skip = false;
    value.char_indices().filter(move |(index, c)| {
        if skip {
            skip = false;
            return false;
        }
        if *c == '^'
            && value[index + 1..]
                .chars()
                .next()
                .is_some_and(|n| n.is_ascii_digit())
        {
            skip = true;
            return false;
        }
        true
    })
}

/// Byte offset just past a prefix equal to `prefix` once colour codes are
/// ignored on both sides, so a server that colours the prefix differently from
/// the configstring name still splits.
fn skip_prefix(text: &str, prefix: &str) -> Option<usize> {
    let mut wanted = uncoloured(prefix).map(|(_, c)| c);
    let mut cursor = 0;
    for (index, c) in uncoloured(text) {
        let Some(expected) = wanted.next() else {
            // Stop right after the last matched character: a colour code
            // between the prefix and the body belongs to the body.
            return Some(cursor);
        };
        if c != expected {
            return None;
        }
        cursor = index + c.len_utf8();
    }
    wanted.next().is_none().then_some(cursor)
}

/// Separate only a known stock name prefix. Unfamiliar mod formatting is

/// retained in full. The prefix does not establish sender identity.
pub fn chat_body<'a>(text: &'a str, name: &str) -> (&'a str, bool) {
    for (open, close, private) in [("", ": ", false), ("(", "): ", false), ("[", "]: ", true)] {
        if let Some(at) = skip_prefix(text, open)
            .and_then(|at| Some(at + skip_prefix(&text[at..], name)?))
            .and_then(|at| Some(at + skip_prefix(&text[at..], close)?))
        {
            return (&text[at..], private);
        }
    }
    (text, false)
}

/// Construct one quoted stock command. Quotes and controls cannot terminate
/// its payload; the byte budget always ends at a UTF-8 boundary.
pub fn chat_command(destination: ChatDestination, text: &str) -> Option<String> {
    if matches!(destination, ChatDestination::Player(slot) if usize::from(slot) >= MAX_CLIENTS) {
        return None;
    }
    let mut payload = String::with_capacity(CHAT_INPUT_BYTES);
    for c in text.chars() {
        let c = if c == '"' || c.is_control() { ' ' } else { c };
        if payload.len() + c.len_utf8() > CHAT_INPUT_BYTES {
            break;
        }
        payload.push(c);
    }
    let payload = payload.trim();
    if payload.is_empty() {
        return None;
    }
    Some(match destination {
        ChatDestination::Global => format!("say \"{payload}\""),
        ChatDestination::Team => format!("say_team \"{payload}\""),
        ChatDestination::Player(slot) => format!("tell {slot} \"{payload}\""),
    })
}

/// Interpret the already-tokenized stock chat command (`G_SayTo`).
pub(crate) fn server_chat_event(arguments: &[Vec<u8>]) -> Option<ServerEvent> {
    let command = arguments.first()?.as_slice();
    let kind = match command {
        b"chat" | b"lchat" => ServerEventKind::Chat,
        b"tchat" | b"ltchat" => ServerEventKind::TeamChat,
        _ => return None,
    };
    let localized = matches!(command, b"lchat" | b"ltchat");
    let sender_index = if localized { 5 } else { 2 };
    let sender = arguments
        .get(sender_index)
        .and_then(|arg| std::str::from_utf8(arg).ok())
        .and_then(|arg| arg.parse::<u16>().ok())
        .filter(|slot| usize::from(*slot) < MAX_CLIENTS);
    let text = if localized {
        // CG_Chat_f: location-bearing chat uses separate name/location/colour/body args.
        let [name, location, color, body] = [1, 2, 3, 4].map(|i| {
            arguments
                .get(i)
                .map(|arg| crate::legacy_text::decode_legacy(arg))
        });
        let location = location?;
        // Use the client's existing localization marker for CG_Chat_f's @key.
        let marker = if location.starts_with('@') { "@@@" } else { "" };
        let location = location.strip_prefix('@').unwrap_or(&location);
        format!("{}^7<{marker}{location}> ^{}{}", name?, color?, body?)
    } else {
        crate::legacy_text::decode_legacy(arguments.get(1)?).into_owned()
    };
    Some(ServerEvent { kind, text, sender })
}
