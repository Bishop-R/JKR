//! jaPRO's vote tweaks (`g_tweakVote`, the `TV_*` bits of `g_local.h`, read by
//! `Cmd_CallVote_f`, `Cmd_Vote_f`, `SetTeam` and `CheckVote`) and its flood protection by
//! address (`g_voteTimeout`, `voteFloodProtect`, `SetFailedCallVoteIP`,
//! `DropVoteTimeouts`). With no bit set every vote rule is stock's.
//!
//! One difference from jaPRO: its spectators may call a vote in no game type unless a bit
//! allows it, where stock lets them call one in the two duel game types. Here stock's rule
//! is the base and the bits only add to it, so a server with no tweaks keeps stock's
//! votes.

use crate::match_end::{GT_DUEL, GT_FFA, GT_POWERDUEL, GT_SIEGE};

/// `TV_ALLOW_SIEGESPECVOTE`: spectators may call votes in siege.
pub const SIEGE_SPECTATORS_CALL: i32 = 1 << 0;
/// `TV_ALLOW_CTFTFFASPECVOTE`: spectators may call votes in the team game types.
pub const TEAM_SPECTATORS_CALL: i32 = 1 << 1;
/// `TV_CLEAR_SPEC_VOTES`, named for what it does: a player going to the spectators keeps
/// its ballot in the running vote.
pub const KEEP_SPECTATOR_VOTES: i32 = 1 << 2;
/// `TV_MAPLOADTIMEOUT`: no vote in a level's first 30 seconds.
pub const MAP_LOAD_TIMEOUT: i32 = 1 << 3;
/// `TV_FLOODPROTECTBYIP`: one ballot per address, and an address whose votes failed waits
/// before calling another ([`FloodProtect`]).
pub const FLOOD_PROTECT_BY_IP: i32 = 1 << 4;
/// `TV_MAPCHANGELOCKOUT`: no map vote in a free-for-all level's first ten minutes.
pub const MAP_CHANGE_LOCKOUT: i32 = 1 << 5;
/// `TV_MAPCHANGEVOTEDELAY`: only a map vote waits `g_voteDelay` once passed; the other
/// delayed votes wait three seconds.
pub const MAP_CHANGE_VOTE_DELAY: i32 = 1 << 6;
/// `TV_ALLOW_SPECVOTE`: spectators may call votes and vote in every game type.
pub const SPECTATORS_VOTE: i32 = 1 << 7;
/// `TV_SHOW_VOTES`: everyone is told who voted which way.
pub const SHOW_VOTES: i32 = 1 << 8;
/// `TV_ONLY_COUNT_VOTERS`: the majority is of everyone connected (spectators included,
/// bots not), and a vote that runs out of time passes if more voted yes than no.
pub const ONLY_COUNT_VOTERS: i32 = 1 << 9;
/// `TV_FIX_GAMETYPEMAP`: a passed game-type vote keeps the map; the new game type starts
/// with the next one.
pub const FIX_GAMETYPE_MAP: i32 = 1 << 10;
/// `TV_IGNOREMAPARENAS`: a map vote does not ask the arena files whether the map has the
/// game type.
pub const IGNORE_MAP_ARENAS: i32 = 1 << 11;

/// `GT_TEAM`.
const GT_TEAM: i32 = 6;

/// Whether a spectator may call a vote: in the duel game types (stock), and wherever a
/// bit allows it.
pub fn spectators_call(tweak: i32, gametype: i32) -> bool {
    matches!(gametype, GT_DUEL | GT_POWERDUEL)
        || (gametype == GT_SIEGE && tweak & SIEGE_SPECTATORS_CALL != 0)
        || (gametype >= GT_TEAM && tweak & TEAM_SPECTATORS_CALL != 0)
        || tweak & SPECTATORS_VOTE != 0
}

/// Whether a spectator may vote: in the duel game types (stock), and everywhere with
/// [`SPECTATORS_VOTE`].
pub fn spectators_vote(tweak: i32, gametype: i32) -> bool {
    matches!(gametype, GT_DUEL | GT_POWERDUEL) || tweak & SPECTATORS_VOTE != 0
}

/// `TimeSecToString`: seconds as `h:mm:ss` past an hour, `m:ss` past a minute, else the
/// bare number.
pub fn seconds_text(seconds: i32) -> String {
    if seconds > 3_600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3_600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else if seconds > 60 {
        format!("{}:{:02}", seconds / 60 % 60, seconds % 60)
    } else {
        seconds.to_string()
    }
}

/// [`MAP_LOAD_TIMEOUT`]'s refusal for a vote called at `now` in a level begun at
/// `level_start`.
pub fn map_load_refusal(tweak: i32, level_start: i32, now: i32) -> Option<Vec<u8>> {
    (tweak & MAP_LOAD_TIMEOUT != 0 && level_start > now - 30_000).then(|| {
        b"print \"You are not allowed to callvote within 30 seconds of map load.\n\"".to_vec()
    })
}

/// [`MAP_CHANGE_LOCKOUT`]'s refusal for the vote named `name` (as the caller typed it) in
/// a free-for-all level begun at `level_start`, with the wait left.
pub fn map_change_refusal(
    tweak: i32,
    name: &[u8],
    gametype: i32,
    level_start: i32,
    now: i32,
) -> Option<Vec<u8>> {
    const LOCKOUT: i32 = 600_000;
    if tweak & MAP_CHANGE_LOCKOUT == 0
        || !name.eq_ignore_ascii_case(b"map")
        || gametype != GT_FFA
        || level_start <= now - LOCKOUT
    {
        return None;
    }
    // `(LOCKOUT - (level.time - level.startTime)) * 0.001f`, truncated.
    let left = ((LOCKOUT - (now - level_start)) as f32 * 0.001) as i32;
    Some(
        format!(
            "print \"The server just changed to this map, please wait {} before calling a map vote.\n\"",
            seconds_text(left)
        )
        .into_bytes(),
    )
}

/// How long a passed vote named `name` waits before it is carried out: `g_voteDelay`
/// (`delay`) for a vote that waits at all, or with [`MAP_CHANGE_VOTE_DELAY`] that only
/// for a map vote and three seconds for the others.
pub fn execute_delay(tweak: i32, name: &[u8], delayed: bool, delay: i32) -> i32 {
    match delayed {
        false => 0,
        true if tweak & MAP_CHANGE_VOTE_DELAY != 0 && !name.eq_ignore_ascii_case(b"map") => 3_000,
        true => delay,
    }
}

/// `voteFloodProtect`: the addresses (hosts, without a port) whose votes failed, how
/// often, and until when each waits before calling another vote. Its times are the
/// server's milliseconds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FloodProtect {
    entries: Vec<FloodEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FloodEntry {
    address: Vec<u8>,
    fails: i32,
    wait_until: i64,
    next_drop: i64,
}

impl FloodProtect {
    /// Seconds `address` must still wait at `now` before calling a vote (`g_voteTimeout`
    /// `timeout`, 0 for none), as `Cmd_CallVote_f` prints them; `None` when it need not.
    pub fn wait(&self, address: &[u8], now: i64, timeout: i32) -> Option<i32> {
        if timeout == 0 {
            return None;
        }
        let entry = self.entries.iter().find(|entry| entry.address == address)?;
        // `(voteTimeoutUntil - time) * 0.001f`, truncated.
        (entry.wait_until != 0 && entry.wait_until > now)
            .then(|| ((entry.wait_until - now) as f32 * 0.001) as i32)
    }

    /// `SetFailedCallVoteIP`: a vote `address` called failed at `now`; it waits one
    /// `timeout` more for every failure, and a failure is forgiven every five.
    pub fn failed(&mut self, address: &[u8], now: i64, timeout: i32) {
        if timeout == 0 {
            return;
        }
        let at = match self
            .entries
            .iter()
            .position(|entry| entry.address == address)
        {
            Some(at) => at,
            None => {
                self.entries.push(FloodEntry {
                    address: address.to_vec(),
                    fails: 0,
                    wait_until: 0,
                    next_drop: 0,
                });
                self.entries.len() - 1
            }
        };
        let entry = &mut self.entries[at];
        let timeout = i64::from(timeout) * 1_000;
        entry.fails += 1;
        entry.wait_until = now + i64::from(entry.fails) * timeout;
        entry.next_drop = now + timeout * 5;
    }

    /// `DropVoteTimeouts`, once a frame: one failure forgiven for each address whose
    /// forgiving time has come.
    pub fn forgive(&mut self, now: i64, timeout: i32) {
        for entry in &mut self.entries {
            if entry.fails > 0 && entry.next_drop < now {
                entry.fails -= 1;
                entry.next_drop = now + i64::from(timeout) * 5_000;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spectators_keep_stock_rights_and_gain_the_bits() {
        use crate::match_end::GT_CTF;
        assert!(!spectators_call(0, GT_FFA) && spectators_call(0, GT_DUEL));
        assert!(spectators_call(SIEGE_SPECTATORS_CALL, GT_SIEGE));
        assert!(spectators_call(TEAM_SPECTATORS_CALL, GT_CTF));
        assert!(!spectators_call(TEAM_SPECTATORS_CALL, GT_FFA));
        assert!(spectators_call(SPECTATORS_VOTE, GT_FFA));
        assert!(!spectators_vote(TEAM_SPECTATORS_CALL, GT_CTF));
        assert!(spectators_vote(SPECTATORS_VOTE, GT_CTF));
    }

    #[test]
    fn waits_are_told_and_counted_as_japro_does() {
        assert_eq!(seconds_text(42), "42");
        assert_eq!(seconds_text(61), "1:01");
        assert_eq!(seconds_text(3_725), "1:02:05");
        assert!(map_load_refusal(MAP_LOAD_TIMEOUT, 10_000, 39_999).is_some());
        assert!(map_load_refusal(MAP_LOAD_TIMEOUT, 10_000, 40_000).is_none());
        assert!(map_load_refusal(0, 10_000, 11_000).is_none());
        let refusal = map_change_refusal(MAP_CHANGE_LOCKOUT, b"MAP", GT_FFA, 0, 60_000).unwrap();
        assert!(
            String::from_utf8(refusal)
                .unwrap()
                .contains("wait 9:00 before")
        );
        assert!(map_change_refusal(MAP_CHANGE_LOCKOUT, b"map", GT_DUEL, 0, 60_000).is_none());
        assert_eq!(execute_delay(0, b"kick", true, 5_000), 5_000);
        assert_eq!(
            execute_delay(MAP_CHANGE_VOTE_DELAY, b"kick", true, 5_000),
            3_000
        );
        assert_eq!(
            execute_delay(MAP_CHANGE_VOTE_DELAY, b"map", true, 5_000),
            5_000
        );
        assert_eq!(
            execute_delay(MAP_CHANGE_VOTE_DELAY, b"map", false, 5_000),
            0
        );
    }

    #[test]
    fn failed_votes_make_an_address_wait_longer_until_forgiven() {
        let address: &[u8] = b"10.0.0.7";
        let mut flood = FloodProtect::default();
        assert_eq!(flood.wait(address, 0, 180), None);
        flood.failed(address, 1_000, 180);
        assert_eq!(flood.wait(address, 1_000, 180), Some(180));
        flood.failed(address, 2_000, 180);
        assert_eq!(flood.wait(address, 2_000, 180), Some(360));
        assert_eq!(flood.wait(address, 2_000, 0), None);
        // Five timeouts on, one failure is forgiven.
        flood.forgive(2_000 + 900_001, 180);
        flood.failed(address, 902_001, 180);
        assert_eq!(flood.wait(address, 902_001, 180), Some(360));
    }

    /// The rules with the tweaks on: who may call and how a ballot is told, what a
    /// running vote waits for, and when it counts against its caller.
    #[test]
    fn the_vote_rules_read_the_tweaks() {
        use crate::vote::{
            self, Connection, NoMaps, Said, Vote, VoteSettings, Voter, call_vote, cast_vote,
        };
        let voter = |client: usize, team: i32| Voter {
            client,
            connection: Connection::Connected,
            team,
            bot: false,
            name: b"Kyle".to_vec(),
            temp_spectate: 0,
        };
        let voters = [voter(0, 0), voter(1, 0), voter(2, 3)];
        let call: [&[u8]; 3] = [b"callvote", b"fraglimit", b"10"];
        let settings = |tweak: i32| VoteSettings {
            tweak,
            level_start: 1_000,
            ..VoteSettings::default()
        };
        let caller_said = |said: &[Said]| -> Vec<String> {
            said.iter()
                .filter_map(|said| match said {
                    Said::Caller(text) | Said::Everyone(text) => {
                        Some(String::from_utf8_lossy(text).into_owned())
                    }
                    _ => None,
                })
                .collect()
        };
        // A spectator calls only with a bit; the first 30 seconds are closed with another.
        let mut running = Vote::default();
        let said = call_vote(
            &settings(0),
            &mut running,
            &voters,
            2,
            &call,
            50_000,
            &NoMaps,
            None,
        );
        assert!(caller_said(&said)[0].contains("NOSPECVOTE"));
        let said = call_vote(
            &settings(MAP_LOAD_TIMEOUT),
            &mut running,
            &voters,
            0,
            &call,
            20_000,
            &NoMaps,
            None,
        );
        assert!(caller_said(&said)[0].contains("within 30 seconds"));
        let said = call_vote(
            &settings(0),
            &mut running,
            &voters,
            0,
            &call,
            50_000,
            &NoMaps,
            Some(42),
        );
        assert_eq!(
            caller_said(&said),
            ["print \"Please wait 42 before calling a new vote.\n\""]
        );
        let said = call_vote(
            &settings(SPECTATORS_VOTE),
            &mut running,
            &voters,
            2,
            &call,
            50_000,
            &NoMaps,
            None,
        );
        assert!(running.time == 50_000 && running.yes == 1, "{said:?}");
        // Ballots told to everyone; a second ballot from the same address refused.
        let said = cast_vote(
            &settings(SHOW_VOTES),
            &mut running,
            &voters[0],
            b"no",
            false,
        );
        assert_eq!(caller_said(&said), ["print \"Kyle^7 voted no\n\""]);
        let said = cast_vote(&settings(0), &mut running, &voters[1], b"y", true);
        assert!(caller_said(&said)[0].contains("VOTEALREADY"));
        // Run out at one each: with only the voters counted it fails against the caller.
        let said = vote::check_vote(&mut running, 3, 80_000, 0, ONLY_COUNT_VOTERS, &NoMaps);
        assert!(said.contains(&Said::Failed));
        // A passed vote waiting to be carried out refuses a new call under any tweak.
        let mut waiting = Vote::default();
        waiting.execute_time = 90_000;
        let said = call_vote(
            &settings(SHOW_VOTES),
            &mut waiting,
            &voters,
            0,
            &call,
            85_000,
            &NoMaps,
            None,
        );
        assert!(caller_said(&said)[0].contains("not allowed to call a new vote"));
        // With only the voters counted, more yes than no passes when time runs out.
        let mut ahead = Vote::default();
        let _ = call_vote(
            &settings(0),
            &mut ahead,
            &voters,
            0,
            &call,
            50_000,
            &NoMaps,
            None,
        );
        let said = vote::check_vote(&mut ahead, 3, 80_000, 0, ONLY_COUNT_VOTERS, &NoMaps);
        assert!(caller_said(&said)[0].contains("VOTEPASSED"));
    }
}
