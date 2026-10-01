//! Keep the player's Force profile legal for the current server.
//!
//! The user's preferred `forcepowers` is a client setting; what goes on the
//! wire is a per-server legalized copy. `WP_InitForcePowers`
//! (`codemp/game/w_force.c:240-251`) strips disabled or over-budget powers,
//! and (`w_force.c:368-392`) parks any client whose profile needed stripping
//! — or whose session has not yet confirmed its powers (`!sess.setForce`) —
//! in spectator mode with `spc` + `nfr <rank> 1 <team>`: the stock "Force menu
//! on first join". [`enter_play`] sends the legalized value and then the
//! stock Force menu's own accept command, `forcechanged "<TEAM>"`, while the
//! client is still the spectator it connects as; `Cmd_ForceChanged_f`
//! (`codemp/game/g_cmds.c:1276-1313`) runs `WP_InitForcePowers` immediately,
//! marks the session and joins the team, so play starts without a bounce
//! (verified against JA+ 2.4 on the EFF server,
//! `target/parity-reports/playtest-japlus5`).
//!
//! When a server still sends `nfr <rank> 1 <team>`, stock JKA opens the Force
//! menu; closing it flushes the legalized userinfo, sends `forcechanged`, and
//! the player clicks Join again (`assets1.pk3:ui/jamp/ingame_playerforce.menu:
//! 1556-1562`). [`ForceProfileNegotiator`] performs those steps itself. Because
//! `Cmd_Team_f` refuses a second team change within five seconds of the first
//! (`codemp/game/g_cmds.c:998-1001`, `1031`), the join is retried a bounded
//! number of times.

use crate::force_profile::legalize_force_powers;
use crate::force_rank_reply::{ForceRankReply, force_rank_reply, force_rules_from_serverinfo};
use crate::team_commands::{LegacyTeamChoice, legacy_team_command};
use crate::{ClientError, ClientSession, UserinfoUpdateStatus};
use jkr_network::LegacyUserInfo;
use jkr_protocol::{GameState, InfoString};
use std::time::{Duration, Instant};

/// `switchTeamTime` window (`g_cmds.c:1031`) plus transport slack.
const REJOIN_INTERVAL: Duration = Duration::from_millis(5_500);
/// Bounded so a server that keeps refusing never produces a join/spectate loop.
const REJOIN_ATTEMPTS: u8 = 3;

/// Legalize `preferred` against the rules the server advertises in
/// `CS_SERVERINFO`, using its `g_maxForceRank` as the rank ceiling.
pub fn server_legal_forcepowers(game_state: &GameState, preferred: &str) -> String {
    let max_rank = serverinfo_i32(game_state, "g_maxForceRank");
    let rules = force_rules_from_serverinfo(game_state, max_rank, 0);
    legalize_force_powers(preferred, rules).allocation.encode()
}

/// `gametype_t` values (`bg_public.h:235-244`) for which `Cmd_ForceChanged_f`
/// ignores its team argument (`g_cmds.c:1297-1300`).
const GT_DUEL: i32 = 3;
const GT_POWERDUEL: i32 = 4;
const GT_SIEGE: i32 = 7;
/// Ordinary `team` → first snapshot in play on a loaded server.
/// How long to wait for the server to confirm the Force profile and spawn
/// us. Reaching it is no longer a failure -- the caller enters the game
/// anyway -- so this only decides how long a join sits still on a server
/// that never answers `nfr`, which a modified game module need not do. Ten
/// seconds of that was the "takes longer to join" the owner reported.
const JOIN_TIMEOUT: Duration = Duration::from_secs(3);
/// Upper bound on the userinfo coalescing wait before joining regardless.
const USERINFO_TIMEOUT: Duration = Duration::from_secs(2);

/// How the client entered play.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnterPlayOutcome {
    /// The `forcepowers` value now on the wire (also written to `userinfo`).
    pub forcepowers: String,
    /// Whether the server confirmed the profile and a snapshot in play
    /// arrived before the timeout.
    ///
    /// This is not a verdict on the connection. Plenty of servers hold a
    /// fresh client in spectator until it picks a team, and a mod's game
    /// module need not answer with `nfr` at all; a stock client is simply
    /// connected, watching, with the join menu up. Treat a false here as
    /// "still spectating", never as a failed join.
    pub active: bool,
    /// The server answered the force profile (`nfr`), or the gametype never
    /// sends one.
    pub confirmed: bool,
    /// The newest snapshot still shows this client spectating.
    pub spectating: bool,
}

/// Join `team` with a server-legal Force profile, without the stock
/// spectator bounce.
///
/// Legalizes `userinfo.forcepowers` against `CS_SERVERINFO`, pushes it when
/// it changed, then sends `forcechanged "<TEAM>"`: as a spectator that runs
/// `WP_InitForcePowers` immediately (marking the session so the join does not
/// bounce) and hands the team to `Cmd_Team_f` (`g_cmds.c:1276-1313`). Duel
/// gametypes ignore that argument, so a plain `team` command follows there.
/// The `nfr` notices the exchange provokes are consumed. Must be called
/// while the client is still the spectator it connects as.
pub fn enter_play(
    session: &mut ClientSession,
    userinfo: &mut LegacyUserInfo,
    team: LegacyTeamChoice,
) -> Result<EnterPlayOutcome, ClientError> {
    let forcepowers = server_legal_forcepowers(session.game_state(), &userinfo.forcepowers);
    if forcepowers != userinfo.forcepowers {
        userinfo.forcepowers = forcepowers.clone();
        // The tracker coalesces updates to one per second from the connect
        // userinfo on; wait that out so the legal value precedes the
        // `forcechanged` instead of being skipped.
        let deadline = Instant::now() + USERINFO_TIMEOUT;
        while session.update_userinfo(userinfo, Instant::now())?
            == UserinfoUpdateStatus::RateLimited
            && Instant::now() < deadline
        {
            receive_quietly(session)?;
        }
    }
    let rules = force_rules_from_serverinfo(session.game_state(), 0, 0);
    let reply = force_rank_reply(&forcepowers, rules, Some(team));
    session.send_reliable_command(&reply.command)?;
    if matches!(
        serverinfo_i32(session.game_state(), "g_gametype"),
        GT_DUEL | GT_POWERDUEL
    ) {
        session.send_reliable_command(legacy_team_command(team))?;
    }
    // The first snapshots can still show the pre-park in-game state, so wait
    // for the server's own confirmation: `WP_InitForcePowers` answers a
    // legal, confirmed profile with `nfr <rank> 0 <team>`
    // (`w_force.c:394`). Siege never sends it (`scl` instead).
    let siege = serverinfo_i32(session.game_state(), "g_gametype") == GT_SIEGE;
    let deadline = Instant::now() + JOIN_TIMEOUT;
    let mut confirmed = siege;
    let mut active = false;
    while Instant::now() < deadline {
        receive_quietly(session)?;
        while let Some(update) = session.take_force_rank_event() {
            confirmed |= !update.open_profile;
        }
        active = confirmed && !session.latest_snapshot().player.is_spectator();
        if active {
            break;
        }
    }
    let spectating = session.latest_snapshot().player.is_spectator();
    Ok(EnterPlayOutcome {
        forcepowers,
        active,
        confirmed,
        spectating,
    })
}

/// One receive step that also releases paced commands; timeouts are normal.
fn receive_quietly(session: &mut ClientSession) -> Result<(), ClientError> {
    match session.receive_snapshot(Duration::from_millis(100)) {
        Ok(_) => Ok(()),
        Err(error) if error.is_timeout() => Ok(()),
        Err(error) => Err(error),
    }
}

fn serverinfo_i32(game_state: &GameState, key: &str) -> i32 {
    game_state
        .config_string(0)
        .and_then(|raw| std::str::from_utf8(raw).ok())
        .and_then(|text| InfoString::parse(text).ok())
        .and_then(|info| info.get_i32(key))
        .unwrap_or(0)
}

/// Reliable commands the shell must send this frame, in order.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RejoinOutput {
    /// Byte-exact reliable commands, in send order.
    pub commands: Vec<Vec<u8>>,
    /// Console notice once the bounded retries are exhausted.
    pub notice: Option<String>,
}

#[derive(Debug)]
struct Retry {
    due: Instant,
    attempts_left: u8,
}

/// Per-connection Force-profile state: the server-legal userinfo override and
/// the `nfr` reply / rejoin machine.
#[derive(Debug)]
pub struct ForceProfileNegotiator {
    server_forcepowers: Option<String>,
    desired_team: LegacyTeamChoice,
    pending: Option<Vec<u8>>,
    retry: Option<Retry>,
}

impl Default for ForceProfileNegotiator {
    fn default() -> Self {
        Self {
            server_forcepowers: None,
            desired_team: LegacyTeamChoice::Free,
            pending: None,
            retry: None,
        }
    }
}

impl ForceProfileNegotiator {
    /// The `forcepowers` value the server accepted, if it differs from the
    /// player's preference.
    pub fn server_forcepowers(&self) -> Option<&str> {
        self.server_forcepowers.as_deref()
    }

    /// Record the value negotiated for this server (`None` clears it).
    pub fn set_server_forcepowers(&mut self, value: Option<String>) {
        self.server_forcepowers = value;
    }

    /// Replace the preferred profile in `userinfo` with the negotiated value.
    pub fn apply_to_userinfo(&self, userinfo: &mut LegacyUserInfo) {
        if let Some(value) = &self.server_forcepowers {
            userinfo.forcepowers.clone_from(value);
        }
    }

    /// Remember the team the player last asked for; spectator disables rejoin.
    pub fn note_team_choice(&mut self, choice: LegacyTeamChoice) {
        self.desired_team = choice;
        if choice == LegacyTeamChoice::Spectator {
            self.pending = None;
            self.retry = None;
        }
    }

    /// The team a `forcechanged` reply should return the player to.
    pub fn rejoin_team(&self) -> Option<LegacyTeamChoice> {
        (self.desired_team != LegacyTeamChoice::Spectator).then_some(self.desired_team)
    }

    /// Queue the reply; it is sent once the userinfo has reached the server.
    pub fn queue(&mut self, reply: &ForceRankReply, now: Instant) {
        if reply.changed {
            self.server_forcepowers = Some(reply.forcepowers.clone());
        }
        self.pending = Some(reply.command.clone());
        self.retry = self.rejoin_team().map(|_| Retry {
            due: now + REJOIN_INTERVAL,
            attempts_left: REJOIN_ATTEMPTS,
        });
    }

    /// Drop all per-connection state, e.g. on disconnect or a new server.
    pub fn reset(&mut self) {
        self.server_forcepowers = None;
        self.pending = None;
        self.retry = None;
    }

    /// Advance the state machine for one frame.
    ///
    /// `userinfo_settled` is true when no userinfo change is still waiting to be
    /// sent; `spectator` reflects the latest snapshot.
    pub fn poll(&mut self, now: Instant, userinfo_settled: bool, spectator: bool) -> RejoinOutput {
        let mut output = RejoinOutput::default();
        if userinfo_settled {
            if let Some(command) = self.pending.take() {
                output.commands.push(command);
            }
        }
        let Some(retry) = &mut self.retry else {
            return output;
        };
        if self.pending.is_some() || now < retry.due {
            return output;
        }
        if !spectator {
            self.retry = None;
            return output;
        }
        if retry.attempts_left == 0 {
            self.retry = None;
            output.notice = Some(
                "^3Server kept you in spectator mode after the Force profile update; \
                 use the join menu."
                    .to_owned(),
            );
            return output;
        }
        retry.attempts_left -= 1;
        retry.due = now + REJOIN_INTERVAL;
        output
            .commands
            .push(legacy_team_command(self.desired_team).to_vec());
        output
    }
}
