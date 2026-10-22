//! A Siege round as a client follows it: `CG_ParseSiegeState` reads the server's
//! `CS_SIEGE_STATE`, and the Siege part of `CG_Draw2D` (`cg_draw.c:8168-8316`) turns it,
//! each frame, into "waiting for players", the countdown to the round (with its 3, 2, 1
//! announcer lines) and the round clock (`CG_DrawSiegeTimer`). The viewer's respawn wave
//! (`EV_SIEGESPEC`) is an event's, kept with the other timers in
//! [`crate::event_feedback`].
//!
//! The state is advanced only where `CG_Draw2D` runs: never during an intermission, which
//! the caller leaves out.

use crate::LegacyClientInfo;
use jkr_game_jka::userinfo::atoi;
use jkr_protocol::GameState;

/// `CS_SIEGE_STATE` (`CS_AMBIENT_SET + MAX_AMBIENT_SETS`): `state|time`.
pub const CS_SIEGE_STATE: usize = 37 + 256;
/// `CS_SIEGE_TIMEOVERRIDE`: the time a second round's attackers must beat.
pub const CS_SIEGE_TIMEOVERRIDE: usize = CS_SIEGE_STATE + 2;
/// `SIEGE_ROUND_BEGIN_TIME` (`bg_saga.h`).
const SIEGE_ROUND_BEGIN_TIME: i32 = 5_000;
const TEAM_RED: u8 = 1;
const TEAM_BLUE: u8 = 2;

/// What the round shows before it begins, where stock centre-prints it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundNotice {
    /// `WAITING_FOR_PLAYERS`: a side has nobody yet.
    Waiting,
    /// `ROUNDBEGINSIN`: the whole seconds left, counted from one.
    BeginsIn(i32),
}

/// One frame of the round (`CG_Draw2D`'s Siege part).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoundFrame {
    /// The notice before the round begins.
    pub notice: Option<RoundNotice>,
    /// The round has just begun: stock empties its centre print.
    pub began: bool,
    /// The countdown line to play this frame: 3, 2 or 1 (`count3Sound` … `count1Sound`).
    pub count: Option<i32>,
    /// The round clock in whole seconds, and whether the timed side is the viewer's.
    pub clock: Option<RoundClock>,
}

/// The round clock of one frame (`CG_DrawSiegeTimer`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoundClock {
    /// Whole seconds: elapsed in a first round with side switching, left otherwise.
    pub seconds: i32,
    /// The timed side is the viewer's (stock draws it red, else green).
    pub own: bool,
}

/// The rules a round clock follows, read from the server info, the map's `.siege` file
/// and `CS_SIEGE_TIMEOVERRIDE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClockRules {
    /// `team1Timed`, `team2Timed`: each side's `Timed`, in milliseconds.
    pub timed: [i32; 2],
    /// `cgs.siegeTeamSwitch` (`g_siegeTeamSwitch`).
    pub team_switch: bool,
    /// `cg_beatingSiegeTime`.
    pub beating: i32,
}

impl ClockRules {
    /// The rules on the server `game` and with the map's `timed` sides.
    pub fn read(game: &GameState, timed: [i32; 2]) -> Self {
        let switch = game
            .config_string(0)
            .and_then(|info| LegacyClientInfo::new(info).bytes("g_siegeTeamSwitch"));
        Self {
            timed,
            team_switch: switch.is_some_and(|value| atoi(value) != 0),
            beating: game.config_string(CS_SIEGE_TIMEOVERRIDE).map_or(0, atoi),
        }
    }
}

/// The client's view of the round (`cgSiegeRoundState`, `cgSiegeRoundTime`,
/// `cgSiegeRoundBeganTime`, `cgSiegeRoundCountTime`).
#[derive(Clone, Debug, Default)]
pub struct SiegeRound {
    seen: Vec<u8>,
    state: i32,
    round_time: i32,
    began_time: i32,
    count_time: i32,
}

impl SiegeRound {
    /// `CG_ConfigStringModified` for `CS_SIEGE_STATE`: a changed, non-empty value is parsed
    /// at `time`. Compares the bytes and allocates only on a change.
    pub fn observe(&mut self, game: &GameState, time: i32) {
        let value = game.config_string(CS_SIEGE_STATE).unwrap_or_default();
        if value != self.seen.as_slice() {
            self.seen.clear();
            self.seen.extend_from_slice(value);
            if !value.is_empty() {
                self.parse(value, time);
            }
        }
    }

    /// `CG_ParseSiegeState`: the state before `|`, and the time after it; without a time,
    /// the round time is now.
    fn parse(&mut self, value: &[u8], time: i32) {
        let (state, rest) = match value.iter().position(|&byte| byte == b'|') {
            Some(bar) => (&value[..bar], Some(&value[bar + 1..])),
            None => (value, None),
        };
        self.state = atoi(state);
        match rest {
            Some(rest) => {
                self.round_time = atoi(rest);
                if self.state == 0 || self.state == 2 {
                    self.began_time = self.round_time;
                }
            }
            None => self.round_time = time,
        }
    }

    /// One `CG_Draw2D` at `time` for a viewer on `team`.
    pub fn frame(&mut self, time: i32, team: u8, rules: ClockRules) -> RoundFrame {
        let mut frame = RoundFrame::default();
        if self.state != 0 {
            match self.state {
                1 => frame.notice = Some(RoundNotice::Waiting),
                2 => {
                    let left = SIEGE_ROUND_BEGIN_TIME
                        .wrapping_sub(time.wrapping_sub(self.round_time))
                        .clamp(0, SIEGE_ROUND_BEGIN_TIME);
                    let seconds = (left / 1_000 + 1).max(1);
                    if seconds <= 3 && seconds != self.count_time {
                        self.count_time = seconds;
                        frame.count = Some(seconds);
                    }
                    frame.notice = Some(RoundNotice::BeginsIn(seconds));
                }
                _ => {}
            }
        } else if self.round_time != 0 {
            frame.began = true;
            self.round_time = 0;
        } else if self.began_time != 0 {
            frame.clock = self.clock(time, team, rules);
        }
        frame
    }

    /// The clock of the timed side, team one's first (`CG_Draw2D`).
    fn clock(&self, time: i32, team: u8, rules: ClockRules) -> Option<RoundClock> {
        let (timed_team, length) = if rules.timed[0] != 0 {
            (TEAM_RED, rules.timed[0])
        } else if rules.timed[1] != 0 {
            (TEAM_BLUE, rules.timed[1])
        } else {
            return None;
        };
        let value = if rules.beating != 0 {
            rules.beating
        } else {
            length
        };
        let mut left = if rules.team_switch && rules.beating == 0 {
            // A first round with side switching counts up: the time to beat.
            time.wrapping_sub(self.began_time).max(0)
        } else {
            self.began_time.wrapping_add(value).wrapping_sub(time)
        };
        if left > value {
            left = value;
        } else if left < 0 {
            left = 0;
        }
        Some(RoundClock {
            seconds: left / 1_000,
            own: team == timed_team,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round(value: &str, time: i32) -> SiegeRound {
        let mut round = SiegeRound::default();
        round.parse(value.as_bytes(), time);
        round
    }

    const IMPERIAL_CLOCK: ClockRules = ClockRules {
        timed: [1_200_000, 0],
        team_switch: false,
        beating: 0,
    };

    #[test]
    fn the_round_waits_and_counts_down_once_a_second() {
        let mut waiting = round("1", 7_000);
        let frame = waiting.frame(7_050, 2, IMPERIAL_CLOCK);
        assert_eq!(frame.notice, Some(RoundNotice::Waiting));
        assert_eq!(frame.count, None);

        // Both sides are in: five seconds from 10 s. Stock's arithmetic reads 6 for the
        // first millisecond and holds 1 past the end until the server begins the round.
        let mut counting = round("2|10000", 0);
        let seconds: Vec<_> = [
            10_000, 10_001, 11_100, 12_100, 13_000, 13_001, 14_001, 15_500,
        ]
        .iter()
        .map(|&time| {
            let frame = counting.frame(time, 1, IMPERIAL_CLOCK);
            (frame.notice, frame.count)
        })
        .collect();
        use RoundNotice::BeginsIn;
        assert_eq!(
            seconds,
            [
                (Some(BeginsIn(6)), None),
                (Some(BeginsIn(5)), None),
                (Some(BeginsIn(4)), None),
                (Some(BeginsIn(3)), Some(3)),
                (Some(BeginsIn(3)), None),
                (Some(BeginsIn(2)), Some(2)),
                (Some(BeginsIn(1)), Some(1)),
                (Some(BeginsIn(1)), None),
            ]
        );
    }

    #[test]
    fn the_clock_starts_the_frame_after_the_round_begins() {
        let mut started = round("0|15000", 0);
        let frame = started.frame(15_050, 1, IMPERIAL_CLOCK);
        assert!(frame.began);
        assert_eq!(frame.clock, None);
        // Twenty minutes for the Imperials, counted down; red for them, green for the rest.
        let clock = started.frame(75_999, 1, IMPERIAL_CLOCK).clock;
        assert_eq!(
            clock,
            Some(RoundClock {
                seconds: 1_139,
                own: true
            })
        );
        let clock = started.frame(75_999, 2, IMPERIAL_CLOCK).clock;
        assert_eq!(clock.map(|clock| clock.own), Some(false));
        // Out of time it stays at zero; no side timed, no clock.
        assert_eq!(
            started
                .frame(2_000_000, 1, IMPERIAL_CLOCK)
                .clock
                .map(|clock| clock.seconds),
            Some(0)
        );
        let untimed = ClockRules::default();
        assert_eq!(started.frame(20_000, 1, untimed).clock, None);
    }

    #[test]
    fn a_switching_first_round_counts_up_and_the_second_counts_down_to_beat() {
        let mut first = round("0|15000", 0);
        first.frame(15_000, 2, IMPERIAL_CLOCK);
        let switching = ClockRules {
            team_switch: true,
            ..IMPERIAL_CLOCK
        };
        let clock = first.frame(41_000, 2, switching).clock;
        assert_eq!(
            clock,
            Some(RoundClock {
                seconds: 26,
                own: false
            })
        );
        // The second round must beat 5 minutes 10 seconds.
        let beating = ClockRules {
            beating: 310_000,
            ..switching
        };
        let clock = first.frame(75_000, 2, beating).clock;
        assert_eq!(clock.map(|clock| clock.seconds), Some(250));
    }

    #[test]
    fn the_state_is_parsed_when_it_changes() {
        let mut round = SiegeRound::default();
        let mut game = GameState::empty_local(0);
        game.replace_config_string(CS_SIEGE_STATE, b"1".to_vec())
            .expect("a valid config string");
        round.observe(&game, 500);
        assert_eq!((round.state, round.round_time), (1, 500));
        // The same value again keeps the first time.
        round.observe(&game, 900);
        assert_eq!(round.round_time, 500);
        game.replace_config_string(CS_SIEGE_STATE, b"3|42000".to_vec())
            .expect("a valid config string");
        round.observe(&game, 900);
        assert_eq!(
            (round.state, round.round_time, round.began_time),
            (3, 42_000, 0)
        );
        // The server's switch rule, as `atoi` reads it.
        game.replace_config_string(0, b"\\g_siegeTeamSwitch\\1".to_vec())
            .expect("a valid config string");
        game.replace_config_string(CS_SIEGE_TIMEOVERRIDE, b"310000".to_vec())
            .expect("a valid config string");
        let rules = ClockRules::read(&game, [1_200_000, 0]);
        assert!(rules.team_switch);
        assert_eq!(rules.beating, 310_000);
    }
}
