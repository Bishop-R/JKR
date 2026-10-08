//! `ClientInactivityTimer` (OpenJK `codemp/game/g_active.c:765-795`): with
//! `g_inactivity` set, a player in the game who neither moves nor attacks for that many
//! seconds is dropped, warned ten seconds before.

/// `BUTTON_ATTACK`, `BUTTON_ALT_ATTACK`: the buttons that count as activity.
const ATTACK_BUTTONS: u16 = 1 | 128;

/// A player's clock (`inactivityTime`, `inactivityWarning`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Inactivity {
    /// When the player is dropped.
    pub until: i32,
    /// The ten-second warning has been given.
    pub warned: bool,
}

/// What the clock says about one command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Stay,
    /// "Ten seconds until inactivity drop!"
    Warn,
    /// "Dropped due to inactivity".
    Drop,
}

impl Inactivity {
    /// The clock a spawn starts (`ClientSpawn`): `seconds` of `g_inactivity` from now.
    pub fn spawned(level_time: i32, seconds: i32) -> Self {
        Self {
            until: level_time + seconds.saturating_mul(1_000),
            warned: false,
        }
    }

    /// One command's check: with the setting off everyone gets a minute, so that turning
    /// it on mid-game drops nobody at once; a move or an attack winds the clock again.
    pub fn check(
        &mut self,
        seconds: i32,
        command: &jkr_protocol::UserCommand,
        level_time: i32,
    ) -> Verdict {
        let active = command.forward_move != 0
            || command.right_move != 0
            || command.up_move != 0
            || command.buttons & ATTACK_BUTTONS != 0;
        if seconds == 0 {
            *self = Self::spawned(level_time, 60);
        } else if active {
            *self = Self::spawned(level_time, seconds);
        } else if level_time > self.until {
            return Verdict::Drop;
        } else if level_time > self.until - 10_000 && !self.warned {
            self.warned = true;
            return Verdict::Warn;
        }
        Verdict::Stay
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jkr_protocol::UserCommand;

    /// Idle for 30 s of a 30 s limit: warned at 20 s once, dropped past 30 s; a move in
    /// between winds the clock back.
    #[test]
    fn an_idle_player_is_warned_then_dropped() {
        let idle = UserCommand::default();
        let mut clock = Inactivity::spawned(0, 30);
        assert_eq!(clock.check(30, &idle, 19_000), Verdict::Stay);
        assert_eq!(clock.check(30, &idle, 20_001), Verdict::Warn);
        assert_eq!(clock.check(30, &idle, 25_000), Verdict::Stay);
        assert_eq!(clock.check(30, &idle, 30_001), Verdict::Drop);
        let moving = UserCommand {
            forward_move: 127,
            ..UserCommand::default()
        };
        let mut clock = Inactivity::spawned(0, 30);
        assert_eq!(clock.check(30, &moving, 29_000), Verdict::Stay);
        assert_eq!(clock.check(30, &idle, 45_000), Verdict::Stay);
        // Off, nobody is dropped, however long idle.
        assert_eq!(clock.check(0, &idle, 500_000), Verdict::Stay);
    }
}
