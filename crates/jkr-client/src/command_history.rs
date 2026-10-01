//! `cl_packetdup` command redundancy.
//!
//! The stock client resends the usercmds of the previous `cl_packetdup`
//! packets in every move packet so that a dropped or reordered datagram does
//! not lose a movement step server-side (`codemp/client/cl_input.cpp:1536-1580`).
//! The server executes only the commands newer than the last one it ran
//! (`codemp/server/sv_client.cpp:1425-1440`), so duplicates cost bandwidth,
//! never movement. JKR sends one fresh command per packet, so the batch is
//! the last `packet_dup` sent commands followed by the new one.

use jkr_protocol::UserCommand;

/// Stock clamp for `cl_packetdup` (`cl_input.cpp:1539-1543`).
pub const MAX_PACKET_DUP: usize = 5;
/// Stock default (`codemp/client/cl_main.cpp:2771`).
pub const DEFAULT_PACKET_DUP: usize = 1;

/// Fixed-size history of sent commands, oldest first; no heap use.
#[derive(Debug)]
pub struct CommandHistory {
    recent: [UserCommand; MAX_PACKET_DUP + 1],
    len: usize,
    packet_dup: usize,
    awaiting_gamestate_snapshot: bool,
}

impl Default for CommandHistory {
    fn default() -> Self {
        Self::new(DEFAULT_PACKET_DUP)
    }
}

impl CommandHistory {
    /// History repeating the previous `packet_dup` commands (clamped to stock).
    pub fn new(packet_dup: usize) -> Self {
        Self {
            recent: [UserCommand::default(); MAX_PACKET_DUP + 1],
            len: 0,
            packet_dup: packet_dup.min(MAX_PACKET_DUP),
            awaiting_gamestate_snapshot: false,
        }
    }

    /// Number of earlier commands repeated per packet.
    pub fn packet_dup(&self) -> usize {
        self.packet_dup
    }

    /// Change the redundancy; takes effect from the next packet.
    pub fn set_packet_dup(&mut self, packet_dup: usize) {
        self.packet_dup = packet_dup.min(MAX_PACKET_DUP);
    }

    /// Forget the history (new gamestate: the server's time line restarted).
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Atomically invalidate old-map input when a gamestate is decoded.
    /// `CL_ParseGamestate` clears client time before installing the new server ID
    /// (codemp/client/cl_parse.cpp:514-517,605-606).
    pub fn begin_gamestate(&mut self) {
        self.clear();
        self.awaiting_gamestate_snapshot = true;
    }

    /// A snapshot of the replacement gamestate can now reach the input clock.
    pub fn received_gamestate_snapshot(&mut self) {
        self.awaiting_gamestate_snapshot = false;
    }

    /// Record `command` and return the packet's batch, oldest first: the
    /// previous `packet_dup` commands with strictly older server times, then
    /// `command`. Commands stamped from a previous time line are dropped
    /// because the server treats any command newer than the packet's last
    /// as pre-restart garbage (`sv_client.cpp:1428-1430`).
    pub fn push(&mut self, command: UserCommand) -> &[UserCommand] {
        // A receive timeout can fall between svc_gamestate and its snapshot.
        // Never pair a new server ID with a command from the departed timeline.
        let command = if self.awaiting_gamestate_snapshot {
            UserCommand::default()
        } else {
            command
        };
        if self.len == self.recent.len() {
            self.recent.copy_within(1.., 0);
            self.len -= 1;
        }
        self.recent[self.len] = command;
        self.len += 1;
        let newest = self.len - 1;
        let oldest_wanted = newest.saturating_sub(self.packet_dup);
        let mut start = newest;
        while start > oldest_wanted
            && self.recent[start - 1].server_time < self.recent[start].server_time
        {
            start -= 1;
        }
        &self.recent[start..self.len]
    }
}
