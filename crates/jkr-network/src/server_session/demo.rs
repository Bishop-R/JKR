//! Server-side demos (`sv_ccmds.cpp:1527-1705`, `sv_snapshot.cpp:151-179, 710-715`): a
//! client's messages, as they are sent, copied into a demo file.
//!
//! The file is the client demo format: for each message its sequence and length (little
//! endian) and its bytes before the channel encodes them, starting with a gamestate
//! written for the demo alone and ended by `-1 -1`. A recording waits for a message
//! whose snapshot has no base; from then on no snapshot is sent against a frame older
//! than that one, so that every delta in the file has its base in the file.
//!
//! TaystJK adds two things ([`LegacyDemoExtras`]). With `sv_demoWriteMeta` every demo
//! starts with a message that is empty to any demo reader but carries, after its end, a
//! JSON object of metadata (`SV_WriteEmptyMessageWithMetadata`, `sv_ccmds.cpp:1570-1610`):
//! who wrote the demo, when it started, and what `svdemometa` set for the client. With
//! `sv_demoPreRecord` every client's recent messages are kept, a gamestate among them
//! every `sv_demoPreRecordKeyframeDistance` seconds with an unbased snapshot after it, so
//! that a demo started now begins up to `sv_demoPreRecordTime` seconds back
//! (`sv_snapshot.cpp:817-905`, `sv_ccmds.cpp:1823-1868`).
use super::{LegacyGameHost, Slot};
use jkr_protocol::{
    MAX_LEGACY_MESSAGE_BYTES, MessageWriter, ServiceCommand, write_gamestate_block,
};
use std::collections::{BTreeMap, VecDeque};

/// TaystJK's server demo settings (`sv_init.cpp:1056-1060`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyDemoExtras {
    /// `sv_demoPreRecord`: keep every client's recent messages for demos started later.
    pub pre_record: bool,
    /// `sv_demoPreRecordBots`: the bots' too.
    pub pre_record_bots: bool,
    /// `sv_demoPreRecordTime`, in milliseconds: at least this much is kept.
    pub pre_record_ms: i32,
    /// `sv_demoPreRecordKeyframeDistance`, in milliseconds: how often a demo's possible
    /// start (a gamestate and an unbased snapshot) is kept.
    pub keyframe_ms: i32,
    /// `sv_demoWriteMeta`: every demo starts with its metadata.
    pub write_meta: bool,
}

impl Default for LegacyDemoExtras {
    fn default() -> Self {
        Self {
            pre_record: false,
            pre_record_bots: false,
            pre_record_ms: 15_000,
            keyframe_ms: 5_000,
            write_meta: true,
        }
    }
}

/// What a demo reader stops at; the metadata comes after it (`postEOFMetadataMarker`).
const META_MARKER: &[u8] = b"HIDDENMETA";
/// `BIG_INFO_STRING`: longer metadata is written as an empty string (`MSG_WriteBigString`).
const BIG_INFO_STRING: usize = 8_192;
/// Who wrote the demo, the metadata's `wr`.
const WRITER: &str = "JKR_Server";

/// A slot's demo state (`client->demo`).
#[derive(Clone, Debug, Default)]
pub(super) struct DemoState {
    /// The demo's name while one is recorded.
    pub recording: Option<Vec<u8>>,
    /// `demowaiting`: no message is saved until one's snapshot has no base.
    pub waiting: bool,
    /// `minDeltaFrame`: the oldest message a snapshot may be a delta from.
    pub min_delta_frame: i32,
    /// Bytes for the file, handed to the host after the datagram or frame.
    pub pending: Vec<u8>,
    /// The file ends after `pending`.
    pub closing: bool,
    /// `svdemometa`'s keys and values for this client's demos, in key order.
    pub meta: BTreeMap<String, String>,
    /// TaystJK's kept messages.
    pub pre_record: PreRecord,
}

/// `client->demo.preRecord` and the client's `demoPreRecordBuffer`.
#[derive(Clone, Debug, Default)]
pub(super) struct PreRecord {
    /// The kept messages, oldest first.
    kept: VecDeque<Kept>,
    /// `lastKeyframeTime`; `None` makes the next message's upkeep keep one at once.
    last_keyframe: Option<i32>,
    /// `keyframeWaiting`: the next snapshot must have no base.
    keyframe_waiting: bool,
    /// `minDeltaFrame`: the oldest message a snapshot may be a delta from.
    min_delta_frame: i32,
}

/// One kept message (`bufferedMessageContainer_t`).
#[derive(Clone, Debug)]
struct Kept {
    sequence: i32,
    /// `lastClientCommand` when it was sent, which a metadata message before it repeats.
    last_client_command: i32,
    /// `sv.time` when it was sent.
    time: i32,
    /// A gamestate kept as a demo's start, not a message that was sent.
    keyframe: bool,
    bytes: Vec<u8>,
}

impl DemoState {
    /// Whether the snapshot of message `delta_message`'s request may not be a delta: a
    /// recording or a kept start waits for an unbased one, or one was sent that the
    /// client has not acknowledged.
    pub fn forbids_delta(&self, delta_message: i32, pre_record: bool) -> bool {
        (self.recording.is_some() && self.waiting)
            || self.min_delta_frame > delta_message
            || (pre_record
                && (self.pre_record.keyframe_waiting
                    || self.pre_record.min_delta_frame > delta_message))
    }

    /// After a snapshot of message `sequence`: one without a base ends the wait and is
    /// the oldest base from now on; a delta sent after a kept start's unbased snapshot was
    /// acknowledged lets go of it.
    pub fn snapshot_written(&mut self, sequence: i32, delta: bool) {
        if !delta {
            if self.waiting {
                self.min_delta_frame = sequence;
            }
            self.waiting = false;
            if self.pre_record.keyframe_waiting {
                self.pre_record.min_delta_frame = sequence;
            }
            self.pre_record.keyframe_waiting = false;
        } else if !self.pre_record.keyframe_waiting {
            self.pre_record.min_delta_frame = 0;
        }
    }

    /// `SV_SendMessageToClient`'s copy into a recording demo that is not waiting.
    pub fn message_sent(&mut self, sequence: i32, message: &[u8]) {
        if self.recording.is_some() && !self.waiting {
            self.append(sequence, message);
        }
    }

    fn append(&mut self, sequence: i32, message: &[u8]) {
        self.pending.extend_from_slice(&sequence.to_le_bytes());
        self.pending
            .extend_from_slice(&(message.len() as i32).to_le_bytes());
        self.pending.extend_from_slice(message);
    }

    /// `SV_ClearClientDemoPreRecord`: nothing kept, and a start kept with the next message.
    pub fn clear_pre_record(&mut self) {
        self.pre_record = PreRecord::default();
    }

    /// `svdemometa`: `key` set to `value`, or taken out when `value` is empty.
    pub fn set_meta(&mut self, key: &str, value: &str) {
        if value.is_empty() {
            self.meta.remove(key);
        } else {
            self.meta.insert(key.to_owned(), value.to_owned());
        }
    }

    /// Keep message `sequence`, sent at `now`; anything kept from a later message or a
    /// later time is left from before a restart and goes first.
    fn keep(&mut self, sequence: i32, last_client_command: i32, now: i32, bytes: &[u8]) {
        let kept = &mut self.pre_record.kept;
        while kept
            .front()
            .is_some_and(|old| old.sequence > sequence || old.time > now)
        {
            kept.pop_front();
        }
        kept.push_back(Kept {
            sequence,
            last_client_command,
            time: now,
            keyframe: false,
            bytes: bytes.to_vec(),
        });
    }

    /// Whether a start is due at `now`: the last is older than the distance asked for, or
    /// from a later time (a restart), or there is none yet.
    fn start_due(&self, keyframe_ms: i32, now: i32) -> bool {
        self.pre_record
            .last_keyframe
            .is_none_or(|last| last + keyframe_ms < now || last > now)
    }

    /// Keep `gamestate` as a start numbered `sequence`; the next snapshot must have no base.
    fn keep_start(
        &mut self,
        sequence: i32,
        last_client_command: i32,
        now: i32,
        gamestate: Vec<u8>,
    ) {
        let pre_record = &mut self.pre_record;
        pre_record.kept.push_back(Kept {
            sequence,
            last_client_command,
            time: now,
            keyframe: true,
            bytes: gamestate,
        });
        pre_record.min_delta_frame = 0;
        pre_record.keyframe_waiting = true;
        pre_record.last_keyframe = Some(now);
    }

    /// Keep at least `keep_ms`: everything before the newest start that is already that
    /// old goes.
    fn let_go(&mut self, keep_ms: i32, now: i32) {
        let kept = &mut self.pre_record.kept;
        if let Some(oldest) = kept
            .iter()
            .rposition(|old| old.keyframe && old.time + keep_ms < now)
        {
            kept.drain(..oldest);
        }
    }

    /// Begin the demo from the oldest kept start before `now`: its metadata (`opening`,
    /// completed with when the demo really began) numbered one before it, the start, and
    /// every message kept after it up to `outgoing` and `now`, later starts left out.
    /// Whether there was such a start.
    fn begin_from_kept(&mut self, now: i32, outgoing: i32, opening: Option<(String, i64)>) -> bool {
        let Some(start) =
            (self.pre_record.kept.iter()).position(|kept| kept.keyframe && kept.time < now)
        else {
            return false;
        };
        let kept = std::mem::take(&mut self.pre_record.kept);
        let first = &kept[start];
        if let Some((opening, unix_seconds)) = opening {
            let json = format!(
                "{opening},\"ost\":{},\"prso\":{}}}",
                unix_seconds - i64::from((now - first.time) / 1000),
                now - first.time
            );
            if let Some(meta) = metadata_message(first.last_client_command, &json) {
                self.append(first.sequence.wrapping_sub(1), &meta);
            }
        }
        for (index, message) in kept.iter().skip(start).enumerate() {
            if (!message.keyframe || index == 0)
                && message.sequence <= outgoing
                && message.time <= now
            {
                self.append(message.sequence, &message.bytes);
            }
        }
        self.pre_record.kept = kept;
        self.waiting = false;
        true
    }

    /// The metadata object's opening: the writer and every `svdemometa` key but the
    /// writer's and the start's own (`sv_ccmds.cpp:1793-1820`), each value a JSON number
    /// where it is only digits and points.
    fn meta_opening(&self) -> String {
        let mut json = format!("{{\"wr\":\"{WRITER}\"");
        for (key, value) in &self.meta {
            if matches!(key.as_str(), "wr" | "ost" | "prso") {
                continue;
            }
            json.push_str(&format!(",\"{key}\":"));
            if value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'.')
            {
                json.push_str(value);
            } else {
                json.push('"');
                for character in value.chars() {
                    if matches!(character, '"' | '\\') {
                        json.push('\\');
                    }
                    json.push(character);
                }
                json.push('"');
            }
        }
        json
    }
}

/// `SV_WriteEmptyMessageWithMetadata`: a message that ends at once, `json` hidden after
/// its end behind the marker, padded so that a reader looking for the marker finds room.
fn metadata_message(last_client_command: i32, json: &str) -> Option<Vec<u8>> {
    let mut message = MessageWriter::new(MAX_LEGACY_MESSAGE_BYTES);
    message.write_i32(last_client_command).ok()?;
    message.write_u8(ServiceCommand::End as u8).ok()?;
    // "Worst case a byte can take almost 2 bytes to save."
    let required = (message.bit_position() >> 3) + 1 + 2 * META_MARKER.len();
    for &byte in META_MARKER {
        message.write_u8(byte).ok()?;
    }
    // `MSG_WriteBigString`: the string and its end, or only the end when too long.
    let text = if json.len() < BIG_INFO_STRING {
        json.as_bytes()
    } else {
        &[]
    };
    for &byte in text.iter().chain(&[0]) {
        message.write_u8(byte).ok()?;
    }
    message.write_u8(ServiceCommand::End as u8).ok()?;
    let mut bytes = message.finish().ok()?;
    if bytes.len() < required {
        bytes.resize(required, 0);
    }
    Some(bytes)
}

impl Slot {
    /// The gamestate this client would be sent now, as a demo's start
    /// (`SV_CreateClientGameStateMessage`); nothing of the client's own state changes.
    fn demo_gamestate(&mut self, client: usize, game: &impl LegacyGameHost) -> Option<Vec<u8>> {
        let wire = &mut self.wire;
        let mut message = MessageWriter::new(MAX_LEGACY_MESSAGE_BYTES);
        let written = message
            .write_i32(wire.reliable.counters().client_sequence)
            .is_ok()
            && wire.reliable.write_pending_unsent(&mut message).is_ok()
            && write_gamestate_block(
                &mut message,
                wire.reliable.counters().server_sequence,
                game.config_strings(),
                game.baselines(),
                client as i32,
                game.checksum_feed(),
            )
            .is_ok()
            && message.write_u8(ServiceCommand::End as u8).is_ok();
        message.finish().ok().filter(|_| written)
    }

    /// `SV_RecordDemo` once the file is open. With kept messages that reach back to a
    /// start, the demo begins there and records on at once; otherwise it starts with the
    /// gamestate this client would be sent now, numbered one before its next message, and
    /// waits for a whole snapshot. Either way the metadata comes first when it is wanted.
    pub(super) fn start_demo(
        &mut self,
        client: usize,
        name: &[u8],
        game: &impl LegacyGameHost,
        (extras, now): (LegacyDemoExtras, i32),
    ) {
        self.demo.recording = Some(name.to_vec());
        let opening = extras.write_meta.then(|| self.demo.meta_opening());
        let outgoing = self.wire.channel.outgoing_sequence();
        let from_kept = opening
            .clone()
            .map(|opening| (opening, game.unix_seconds()));
        if extras.pre_record && self.demo.begin_from_kept(now, outgoing, from_kept) {
            return;
        }
        self.demo.waiting = true;
        if let Some(opening) = opening {
            let json = format!("{opening},\"ost\":{}}}", game.unix_seconds());
            let last_client_command = self.wire.reliable.counters().client_sequence;
            if let Some(meta) = metadata_message(last_client_command, &json) {
                self.demo.append(outgoing.wrapping_sub(2), &meta);
            }
        }
        // A gamestate this client could not be sent either: the demo has none.
        if let Some(gamestate) = self.demo_gamestate(client, game) {
            self.demo.append(outgoing.wrapping_sub(1), &gamestate);
        }
    }

    /// `SV_StopRecordDemo`'s end of the file.
    pub(super) fn stop_demo(&mut self) {
        self.demo.pending.extend_from_slice(&(-1_i32).to_le_bytes());
        self.demo.pending.extend_from_slice(&(-1_i32).to_le_bytes());
        self.demo.recording = None;
        self.demo.closing = true;
    }

    /// After message `sequence` went out at `now`: copied into a recording demo, and, with
    /// pre-recording on, kept; a start kept when one is due (whose next snapshot must have
    /// no base), and what lies before the newest start old enough let go
    /// (`sv_snapshot.cpp:817-905`).
    pub(super) fn demo_message_sent(
        &mut self,
        client: usize,
        game: &impl LegacyGameHost,
        (sequence, message): (i32, &[u8]),
        (extras, now): (LegacyDemoExtras, i32),
    ) {
        self.demo.message_sent(sequence, message);
        if !extras.pre_record {
            self.demo.clear_pre_record();
            return;
        }
        let bot = self.peer.address == Some(crate::LegacyPeerAddress::Bot);
        if bot && !extras.pre_record_bots {
            return;
        }
        let last_client_command = self.wire.reliable.counters().client_sequence;
        self.demo.keep(sequence, last_client_command, now, message);
        if self.demo.start_due(extras.keyframe_ms, now)
            && let Some(gamestate) = self.demo_gamestate(client, game)
        {
            self.demo
                .keep_start(sequence, last_client_command, now, gamestate);
        }
        self.demo.let_go(extras.pre_record_ms, now);
    }
}

/// Hand every slot's demo bytes to the host, and close the files that ended.
pub(super) fn flush_demos(slots: &mut [Slot], game: &mut impl LegacyGameHost) {
    for (client, slot) in slots.iter_mut().enumerate() {
        flush_demo(slot, client, game);
    }
}

/// Hand one slot's demo bytes to the host, and close its file if it ended. A new demo's
/// file must not open before the old one's end is written.
pub(super) fn flush_demo(slot: &mut Slot, client: usize, game: &mut impl LegacyGameHost) {
    if !slot.demo.pending.is_empty() {
        game.demo_data(client, &slot.demo.pending);
        slot.demo.pending.clear();
    }
    if std::mem::take(&mut slot.demo.closing) {
        game.demo_close(client);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_hides_after_an_ended_message() {
        let bytes = metadata_message(7, "{\"wr\":\"JKR_Server\",\"ost\":5}").unwrap();
        let mut reader = jkr_protocol::MessageReader::new(&bytes);
        assert_eq!(reader.read_i32().unwrap(), 7);
        assert_eq!(reader.read_u8().unwrap(), ServiceCommand::End as u8);
        let marker: Vec<u8> = (0..META_MARKER.len())
            .map(|_| reader.read_u8().unwrap())
            .collect();
        assert_eq!(marker, META_MARKER);
        let text = reader.read_c_string(BIG_INFO_STRING).unwrap();
        assert_eq!(text, b"{\"wr\":\"JKR_Server\",\"ost\":5}");
        assert_eq!(reader.read_u8().unwrap(), ServiceCommand::End as u8);
    }

    #[test]
    fn short_metadata_is_padded_for_the_marker_search() {
        let bytes = metadata_message(0, "").unwrap();
        let mut ended = MessageWriter::new(64);
        ended.write_i32(0).unwrap();
        ended.write_u8(ServiceCommand::End as u8).unwrap();
        let required = (ended.bit_position() >> 3) + 1 + 2 * META_MARKER.len();
        assert!(bytes.len() >= required);
    }

    #[test]
    fn metadata_keys_are_ordered_numbers_bare_and_text_quoted() {
        let mut demo = DemoState::default();
        demo.set_meta("runstarttime", "235254");
        demo.set_meta("course", "lift \"a\"");
        demo.set_meta("wr", "someone else");
        demo.set_meta("speed", "1.5");
        assert_eq!(
            demo.meta_opening(),
            "{\"wr\":\"JKR_Server\",\"course\":\"lift \\\"a\\\"\",\"runstarttime\":235254,\"speed\":1.5"
        );
        demo.set_meta("course", "");
        assert!(!demo.meta_opening().contains("course"));
    }

    /// Messages every 50 ms from 0 to 40 s, each kept as `M<sequence>`, a start `G<sequence>`
    /// kept whenever one is due (every 5 s), 15 s kept.
    fn forty_seconds() -> (DemoState, i32) {
        let mut demo = DemoState::default();
        let mut sequence = 0;
        for time in (0..=40_000).step_by(50) {
            sequence += 1;
            demo.keep(sequence, 3, time, format!("M{sequence}").as_bytes());
            if demo.start_due(5_000, time) {
                demo.keep_start(sequence, 3, time, format!("G{sequence}").into_bytes());
            }
            demo.let_go(15_000, time);
        }
        (demo, sequence)
    }

    /// The demo file's records: each message's number and bytes.
    fn records(bytes: &[u8]) -> Vec<(i32, Vec<u8>)> {
        let mut out = Vec::new();
        let mut at = 0;
        while at + 8 <= bytes.len() {
            let sequence = i32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
            let length = i32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
            out.push((sequence, bytes[at + 8..at + 8 + length].to_vec()));
            at += 8 + length;
        }
        out
    }

    #[test]
    fn at_least_the_time_asked_for_is_kept_from_a_start() {
        let (demo, _) = forty_seconds();
        let starts: Vec<i32> = (demo.pre_record.kept.iter())
            .filter(|kept| kept.keyframe)
            .map(|kept| kept.time)
            .collect();
        // Every 5 s and a message later (5,050 ms apart); the oldest kept is the newest
        // start already 15 s old.
        assert_eq!(starts, [20_200, 25_250, 30_300, 35_350]);
        assert!(demo.pre_record.kept.front().unwrap().keyframe);
    }

    #[test]
    fn a_demo_begins_at_the_oldest_kept_start_with_one_start_only() {
        let (mut demo, last) = forty_seconds();
        demo.recording = Some(b"test".to_vec());
        assert!(demo.begin_from_kept(40_000, last + 1, None));
        assert!(!demo.waiting);
        let records = records(&demo.pending);
        // The start at 20.2 s is message 405's; every message after it follows, the later
        // starts left out.
        assert_eq!(records[0], (405, b"G405".to_vec()));
        let numbers: Vec<i32> = records[1..].iter().map(|(sequence, _)| *sequence).collect();
        assert_eq!(numbers, (406..=last).collect::<Vec<_>>());
        assert!(records[1..].iter().all(|(_, bytes)| bytes[0] == b'M'));
    }

    #[test]
    fn a_demo_from_kept_messages_says_when_it_really_began() {
        let (mut demo, last) = forty_seconds();
        let opening = (String::from("{\"wr\":\"JKR_Server\""), 1_000_000);
        assert!(demo.begin_from_kept(40_000, last + 1, Some(opening)));
        let records = records(&demo.pending);
        assert_eq!(records[0].0, 404);
        let mut reader = jkr_protocol::MessageReader::new(&records[0].1);
        assert_eq!(reader.read_i32().unwrap(), 3);
        assert_eq!(reader.read_u8().unwrap(), ServiceCommand::End as u8);
        for _ in META_MARKER {
            reader.read_u8().unwrap();
        }
        let json = reader.read_c_string(BIG_INFO_STRING).unwrap();
        assert_eq!(
            json,
            b"{\"wr\":\"JKR_Server\",\"ost\":999981,\"prso\":19800}"
        );
        assert_eq!(records[1], (405, b"G405".to_vec()));
    }

    #[test]
    fn no_start_older_than_now_means_the_ordinary_beginning() {
        let mut demo = DemoState::default();
        demo.keep(1, 0, 1_000, b"M1");
        demo.keep_start(1, 0, 1_000, b"G1".to_vec());
        assert!(!demo.begin_from_kept(1_000, 2, None));
        assert!(demo.pending.is_empty());
        assert!(demo.begin_from_kept(1_050, 2, None));
    }

    #[test]
    fn a_kept_start_holds_back_deltas_until_an_unbased_snapshot() {
        let mut demo = DemoState::default();
        demo.pre_record.keyframe_waiting = true;
        assert!(demo.forbids_delta(10, true));
        assert!(!demo.forbids_delta(10, false));
        demo.snapshot_written(11, false);
        assert!(demo.forbids_delta(10, true));
        assert!(!demo.forbids_delta(11, true));
        demo.snapshot_written(12, true);
        assert!(!demo.forbids_delta(10, true));
    }
}
