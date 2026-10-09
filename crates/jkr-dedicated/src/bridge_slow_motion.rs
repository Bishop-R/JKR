//! `g_slowmoDuelEnd` (`g_combat.c:2375-2382`, `g_main.c:2956-3010`): a duel's deciding
//! death played out in slow motion through `timescale`, which every client is sent and
//! runs at too ([`jkr_game_jka::match_end::slow_motion_step`]).

use super::*;

impl NativeGame {
    /// `player_die`'s part: in Duel or Power Duel with `g_slowmoDuelEnd` on, a death at a
    /// player's or an NPC's hand — the dead one's own included — starts the slow motion,
    /// unless it already runs.
    pub(super) fn begin_slow_motion(&mut self, attacker: Option<u16>, level_time: i32) {
        let duel = matches!(self.gametype, GAMETYPE_DUEL | GAMETYPE_POWERDUEL);
        if !duel || self.cvars.integer(b"g_slowmoDuelEnd") == 0 {
            return;
        }
        let by_client = attacker.is_some_and(|attacker| {
            self.peer(usize::from(attacker)).is_some() || self.npcs.roster.is_npc(attacker)
        });
        if by_client && self.match_end.slow_motion.is_none() {
            self.match_end.slow_motion = Some(level_time);
        }
    }

    /// `G_RunFrame`'s part, at the frame's start: `timescale` as the slow motion has it
    /// now, and the slow motion ended once it is over.
    pub(super) fn run_slow_motion(&mut self, level_time: i32) {
        let Some(started) = self.match_end.slow_motion else {
            return;
        };
        let restarted = self.restart_at != 0;
        let (timescale, done) = jkr_game_jka::match_end::slow_motion_step(
            started,
            level_time,
            restarted,
            self.timescale,
        );
        self.set_cvar("timescale", &timescale);
        if done {
            self.match_end.slow_motion = None;
        }
    }

    /// A new level ends a slow motion still running, and puts `timescale` back to one:
    /// the reference's restarted game forgets it with its globals and leaves the clock
    /// as slow as it was.
    pub(super) fn end_slow_motion_with_level(&mut self) {
        if self.match_end.slow_motion.take().is_some() {
            self.set_cvar("timescale", "1");
        }
    }
}
