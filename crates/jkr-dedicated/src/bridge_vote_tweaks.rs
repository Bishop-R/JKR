//! jaPRO's vote tweaks on this server ([`jkr_game_jka::vote_tweaks`]): `g_tweakVote` (0,
//! stock's votes) and `g_voteTimeout` (180 seconds, which only the flood protection
//! reads), both kept from the cvar hook; the flood protection's addresses and the running
//! vote's caller; and the parts that need the clients' addresses — one ballot per address,
//! and a caller's address waiting after its votes failed.

use super::*;
use jkr_game_jka::vote_tweaks::{FLOOD_PROTECT_BY_IP, FloodProtect, KEEP_SPECTATOR_VOTES};

/// jaPRO's vote tweaks as the server keeps them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct JaproVotes {
    /// `g_tweakVote`.
    pub(super) tweak: i32,
    /// `g_voteTimeout`, in seconds.
    pub(super) timeout: i32,
    /// `voteFloodProtect`.
    flood: FloodProtect,
    /// `level.callVoteIP`: the running vote's caller's host.
    caller: Option<Vec<u8>>,
}

impl Default for JaproVotes {
    fn default() -> Self {
        Self {
            tweak: 0,
            timeout: 180,
            flood: FloodProtect::default(),
            caller: None,
        }
    }
}

impl JaproVotes {
    fn flood_protected(&self) -> bool {
        self.tweak & FLOOD_PROTECT_BY_IP != 0
    }
}

/// The host part of a client's address (`sess.IP` up to its `:`), or `None` for a bot.
fn host(peer: &crate::peer::Peer) -> Option<&[u8]> {
    if peer.bot || peer.address.is_empty() {
        return None;
    }
    peer.address.split(|byte| *byte == b':').next()
}

impl NativeGame {
    /// The two options, archived and announced, at jaPRO's defaults: no tweak is stock.
    pub(super) fn register_vote_tweaks(&mut self) {
        let options: [(&[u8], &[u8], &[u8]); 2] = [
            (
                b"g_tweakVote",
                b"0",
                b"jaPRO: vote tweaks, a bit each (see the parity notes); 0 is stock",
            ),
            (
                b"g_voteTimeout",
                b"180",
                b"jaPRO: seconds an address waits per failed vote (flood protection bit)",
            ),
        ];
        for (name, value, about) in options {
            self.cvars
                .get_announced(name, value, crate::cvars::CVAR_ARCHIVE, Some(about));
        }
    }

    /// The seconds `client`'s address must still wait before calling a vote at `now`.
    pub(super) fn vote_flood_wait(&self, client: usize, now: i32) -> Option<i32> {
        let votes = &self.japro_votes;
        if !votes.flood_protected() {
            return None;
        }
        let host = self.peer(client).and_then(host)?;
        votes.flood.wait(host, i64::from(now), votes.timeout)
    }

    /// A vote `client` called has started: its host is the vote's caller.
    pub(super) fn vote_started_by(&mut self, client: usize) {
        if !self.japro_votes.flood_protected() {
            return;
        }
        let caller = self.peer(client).and_then(host).map(<[u8]>::to_vec);
        self.japro_votes.caller = caller;
    }

    /// Whether another client from `client`'s address has voted in the running vote.
    pub(super) fn address_voted(&self, client: usize) -> bool {
        if !self.japro_votes.flood_protected() {
            return false;
        }
        let Some(mine) = self.peer(client).and_then(host) else {
            return false;
        };
        (0..self.players.places()).any(|other| {
            other != client
                && self.vote.ballot(other) != jkr_game_jka::vote::Ballot::None
                && self.peer(other).and_then(host) == Some(mine)
        })
    }

    /// `SetFailedCallVoteIP`: the running vote failed by its voters at `now`.
    pub(super) fn vote_failed(&mut self, now: i32) {
        let votes = &mut self.japro_votes;
        if let Some(caller) = &votes.caller {
            votes.flood.failed(caller, i64::from(now), votes.timeout);
        }
    }

    /// `DropVoteTimeouts`, once a frame.
    pub(super) fn forgive_failed_votes(&mut self, now: i32) {
        let votes = &mut self.japro_votes;
        votes.flood.forgive(i64::from(now), votes.timeout);
    }

    /// Whether a player going to the spectators keeps its ballot in the running vote.
    pub(super) fn spectators_keep_votes(&self) -> bool {
        self.japro_votes.tweak & KEEP_SPECTATOR_VOTES != 0
    }
}
