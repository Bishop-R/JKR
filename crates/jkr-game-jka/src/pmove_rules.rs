//! System/server info adapters for the shared on-foot predictor.

use crate::pmove::MovementConfig;
use jkr_protocol::{GameState, InfoString};

/// `CS_LEGACY_FIXES` (`bg_public.h:123`): which animation fixes the server runs.
pub const CS_LEGACY_FIXES: usize = 36;

/// The movement variables a server reads before every move it makes (`ClientThink_real`,
/// `g_active.c:2031-2041,2837-2891`): `pmove_fixed` with `pmove_msec`, `pmove_float`,
/// `g_stepSlideFix` and `g_noSpecMove`. A client reads the same from the systeminfo and
/// the serverinfo ([`MovementConfig::refresh_game_state`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoveCvars {
    /// `pmove_msec` while `pmove_fixed` is on: no move longer, and every command's time
    /// a whole number of them.
    pub fixed_millis: Option<i32>,
    /// `pmove_float` off: the velocity snapped to whole units after a move.
    pub snap_velocity: bool,
    pub step_slide_fix: bool,
    pub no_spectator_move: bool,
}

impl Default for MoveCvars {
    /// The reference's defaults: no fixed step, a snapped velocity, the step-slide fix.
    fn default() -> Self {
        Self {
            fixed_millis: None,
            snap_velocity: true,
            step_slide_fix: true,
            no_spectator_move: false,
        }
    }
}

impl MoveCvars {
    /// From the variables' integers (`pmove_msec` held within 8 to 33, as
    /// `ClientThink_real` holds it).
    pub fn new(fixed: i32, msec: i32, float: i32, step_slide_fix: i32, no_spec_move: i32) -> Self {
        Self {
            fixed_millis: (fixed != 0).then_some(msec.clamp(8, 33)),
            snap_velocity: float == 0,
            step_slide_fix: step_slide_fix != 0,
            no_spectator_move: no_spec_move != 0,
        }
    }

    /// `SpectatorThink`'s cleared `pmove_t`, which reads `g_noSpecMove` alone.
    pub fn spectator(self) -> Self {
        Self {
            fixed_millis: None,
            snap_velocity: true,
            step_slide_fix: false,
            no_spectator_move: self.no_spectator_move,
        }
    }

    /// `ClientThink_real` under `pmove_fixed`: a command's time rounded up to a whole
    /// `pmove_msec`, for everything the think does with it.
    pub fn command_time(self, time: i32) -> i32 {
        self.fixed_millis
            .map_or(time, |msec| time.wrapping_add(msec - 1) / msec * msec)
    }
}

impl MovementConfig {
    /// A server's own moves under `cvars` (`pmove.pmove_fixed` and the rest).
    pub fn apply_move_cvars(&mut self, cvars: MoveCvars) {
        self.fixed_millis = cvars.fixed_millis;
        self.snap_velocity = cvars.snap_velocity;
        self.step_slide_fix = cvars.step_slide_fix;
        self.no_spectator_move = cvars.no_spectator_move;
    }

    /// Server side: the configstring that tells clients which legacy fixes this
    /// simulation runs, as `UpdateLegacyFixesConfigstring` writes it (`g_cvar.c:45`).
    /// Clients predict animations from it, so it has to be what the server simulates.
    pub fn legacy_fixes_config_string(&self) -> (usize, Vec<u8>) {
        (CS_LEGACY_FIXES, self.legacy_fixes.to_string().into_bytes())
    }

    /// Read CS_SYSTEMINFO movement cvars and CS_SERVERINFO g_stepSlideFix.
    /// cg_predict.c:1059-1068,1247; cg_servercmds.c:138; cg_xcvar.h:157-159.
    pub fn from_game_state(game: &GameState) -> Self {
        let mut config = Self::default();
        config.refresh_game_state(game);
        config
    }

    /// Refresh advertised policy, retaining already-loaded saber restrictions/scales.
    pub fn refresh_game_state(&mut self, game: &GameState) {
        let integer = |index, key, default| {
            game.config_string(index)
                .and_then(|bytes| std::str::from_utf8(bytes).ok())
                .and_then(|text| InfoString::parse(text).ok())
                .and_then(|info| info.get_i32(key))
                .unwrap_or(default)
        };
        self.fixed_millis =
            (integer(1, "pmove_fixed", 0) != 0).then(|| integer(1, "pmove_msec", 8).clamp(8, 33));
        self.snap_velocity = integer(1, "pmove_float", 0) == 0;
        // atoi(Info_ValueForKey(...)) is zero when the server omits this key.
        self.step_slide_fix = integer(0, "g_stepSlideFix", 0) != 0;
        // cg_servercmds.c:140; cg_predict.c:1248.
        self.no_spectator_move = integer(0, "g_noSpecMove", 0) != 0;
        // cg_servercmds.c:137; cg_predict.c:1246.
        self.debug_melee = integer(0, "g_debugMelee", 0) != 0;
        // `CS_LEGACY_FIXES`, configstring 36, a decimal bit mask (`g_cvar.c:45`); a server
        // that predates it publishes nothing and runs none of the fixes
        // (`bg_pmove.c:5168-5170`, `strtoul` of an empty string).
        self.legacy_fixes = game
            .config_string(CS_LEGACY_FIXES)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .map(|text| text.trim_start())
            .map(|text| {
                &text[..text
                    .find(|digit: char| !digit.is_ascii_digit())
                    .unwrap_or(text.len())]
            })
            .and_then(|digits| digits.parse().ok())
            .unwrap_or(0);
        let no_rolls = self.roll_rules.saber_forbids_rolls;
        self.roll_rules = crate::RollRules::from_game_state(game);
        self.roll_rules.saber_forbids_rolls = no_rolls;
    }

    /// CG_PredictPlayerState's fixed-step timestamp ceiling (:1239-1241).
    /// This is prediction-only: the original user command remains untouched.
    pub fn command_time(&self, time: i32) -> i32 {
        self.fixed_millis.map_or(time, |msec| {
            let msec = i64::from(msec.clamp(8, 33));
            (((i64::from(time) + msec - 1) / msec) * msec) as i32
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `((serverTime + pmove_msec - 1) / pmove_msec) * pmove_msec`, with `pmove_msec` held
    /// within 8 to 33; nothing without `pmove_fixed`.
    #[test]
    fn a_fixed_step_rounds_command_times_up() {
        let fixed = MoveCvars::new(1, 8, 0, 1, 0);
        assert_eq!(fixed.fixed_millis, Some(8));
        assert_eq!(fixed.command_time(1_000), 1_000);
        assert_eq!(fixed.command_time(1_001), 1_008);
        assert_eq!(fixed.command_time(1_007), 1_008);
        assert_eq!(MoveCvars::new(1, 2, 0, 1, 0).fixed_millis, Some(8));
        assert_eq!(MoveCvars::new(1, 50, 0, 1, 0).command_time(51), 66);
        assert_eq!(MoveCvars::new(0, 8, 0, 1, 0).command_time(1_001), 1_001);
    }

    /// A spectator's move reads only `g_noSpecMove`; the defaults are the reference's.
    #[test]
    fn a_spectator_moves_with_a_cleared_pmove() {
        let cvars = MoveCvars::new(1, 16, 1, 1, 1);
        assert!(!cvars.snap_velocity);
        let spectator = cvars.spectator();
        assert_eq!(spectator.fixed_millis, None);
        assert!(spectator.snap_velocity && !spectator.step_slide_fix);
        assert!(spectator.no_spectator_move);
        assert_eq!(MoveCvars::default(), MoveCvars::new(0, 8, 0, 1, 0));
    }
}
