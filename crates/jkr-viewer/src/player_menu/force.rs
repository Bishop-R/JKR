//! Legal Force-profile editing state backed solely by `ForceAllocation`.
//!
//! The page edits a draft. Unlike the rest of the player screen, nothing is
//! written while the player picks: Apply writes `forcepowers` once, Discard
//! returns the draft to what was last applied, and closing the screen
//! drops an unapplied draft (the next open reads the cvar again).

use crate::console::ViewerConsole;
use jkr_client::{
    ForceAllocation, ForceLegalizeRules, ForcePower, ForceSide, legalize_force_powers,
};

pub(super) const POWER_NAMES: [&str; 18] = [
    "Heal",
    "Jump",
    "Speed",
    "Push",
    "Pull",
    "Mind Trick",
    "Grip",
    "Lightning",
    "Rage",
    "Protect",
    "Absorb",
    "Team Heal",
    "Team Energize",
    "Drain",
    "Sense",
    "Saber Offense",
    "Saber Defense",
    "Saber Throw",
];

/// Stock `forcepowers` default, used when the cvar is unset.
const DEFAULT_FORCEPOWERS: &str = "7-1-032330000000001333";

/// `GT_TEAM` (`bg_public.h`): team powers are legal from here on.
const GT_TEAM: i32 = 6;

pub(super) struct ForceMenu {
    allocation: ForceAllocation,
    /// The profile `forcepowers` holds: read on open, replaced on Apply.
    applied: ForceAllocation,
    rules: ForceLegalizeRules,
    encoded: String,
}

impl ForceMenu {
    pub(super) fn new() -> Self {
        let allocation = ForceAllocation::default();
        Self {
            encoded: allocation.encode(),
            applied: allocation.clone(),
            allocation,
            rules: ForceLegalizeRules::default(),
        }
    }

    pub(super) fn open(&mut self, console: &ViewerConsole) {
        let raw = console
            .text_value("forcepowers")
            .unwrap_or(DEFAULT_FORCEPOWERS);
        let server_rank = console.integer_cvar("ui_rankChange").unwrap_or(0);
        let rules = ForceLegalizeRules {
            gametype: console.integer_cvar("g_gametype").unwrap_or(0) as i32,
            free_saber: console.integer_cvar("ui_freesaber").unwrap_or(0) != 0,
            ..ForceLegalizeRules::default()
        };
        self.load(raw, server_rank, rules);
    }

    /// Start a fresh draft from `raw` under `rules` (whose rank ceiling is
    /// replaced by the profile's own rank, or `server_rank` when positive).
    fn load(&mut self, raw: &str, server_rank: i64, mut rules: ForceLegalizeRules) {
        let mut allocation = ForceAllocation::parse(raw).unwrap_or_default();
        if server_rank > 0 {
            allocation.rank = u8::try_from(server_rank).unwrap_or(7).min(7);
        }
        rules.max_rank = allocation.rank;
        self.rules = rules;
        self.allocation = legalize_force_powers(&allocation.encode(), self.rules).allocation;
        // What was read counts as applied: opening never writes the cvar.
        self.applied.clone_from(&self.allocation);
        self.refresh_encoded();
    }

    pub(super) fn allocation(&self) -> &ForceAllocation {
        &self.allocation
    }

    /// Points remaining under the same free-saber policy as the editor's spend/refund path.
    pub(super) fn remaining_points(&self) -> u16 {
        self.allocation.remaining_points(self.rules.free_saber)
    }

    /// Whether power `index` can hold levels on the draft's side in this
    /// gametype; legalization clears it otherwise.
    pub(super) fn is_available(&self, index: usize) -> bool {
        ForcePower::ALL.get(index).is_some_and(|power| {
            power.side().is_none_or(|side| side == self.allocation.side)
                && !(power.is_team_power() && self.rules.gametype < GT_TEAM)
        })
    }

    /// Whether the draft differs from the applied profile.
    pub(super) fn is_dirty(&self) -> bool {
        self.allocation != self.applied
    }

    pub(super) fn set_side(&mut self, side: ForceSide) {
        if self.allocation.side == side {
            return;
        }
        self.allocation.side = side;
        self.allocation = legalize_force_powers(&self.allocation.encode(), self.rules).allocation;
        self.refresh_encoded();
    }

    pub(super) fn step(&mut self, index: usize, increase: bool) -> bool {
        let Some(power) = ForcePower::ALL.get(index).copied() else {
            return false;
        };
        let before = self.allocation.clone();
        let changed = if increase {
            self.allocation.spend(power, self.rules.free_saber)
        } else {
            self.allocation.refund(power, self.rules.free_saber)
        };
        if changed {
            let legalized = legalize_force_powers(&self.allocation.encode(), self.rules).allocation;
            if legalized != self.allocation {
                self.allocation = before;
                return false;
            }
            self.refresh_encoded();
        }
        changed
    }

    /// Clear every level on the draft (the free minima stay).
    pub(super) fn reset(&mut self) {
        let side = self.allocation.side;
        self.allocation = ForceAllocation {
            rank: self.rules.max_rank,
            side,
            levels: [0; 18],
        };
        self.allocation = legalize_force_powers(&self.allocation.encode(), self.rules).allocation;
        self.refresh_encoded();
    }

    /// Return the draft to the applied profile.
    pub(super) fn discard(&mut self) {
        self.allocation.clone_from(&self.applied);
        self.refresh_encoded();
    }

    /// Write the draft to `forcepowers`; nothing happens when it is unchanged.
    pub(super) fn apply(&mut self, console: &mut ViewerConsole) {
        if let Some(value) = self.commit() {
            console.set_cvar("forcepowers", value);
        }
    }

    /// Mark the draft applied and return its `forcepowers` value, or `None`
    /// when there is nothing to apply.
    fn commit(&mut self) -> Option<&str> {
        if !self.is_dirty() {
            return None;
        }
        self.applied.clone_from(&self.allocation);
        Some(&self.encoded)
    }

    fn refresh_encoded(&mut self) {
        self.encoded = self.allocation.encode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu(raw: &str) -> ForceMenu {
        let mut menu = ForceMenu::new();
        menu.load(raw, 0, ForceLegalizeRules::default());
        menu
    }

    #[test]
    fn edits_stay_in_the_draft_until_applied() {
        let mut menu = menu(DEFAULT_FORCEPOWERS);
        assert!(!menu.is_dirty());
        assert!(menu.commit().is_none());
        assert!(menu.step(2, false)); // Speed 2 -> 1
        assert!(menu.is_dirty());
        assert_eq!(menu.commit(), Some("7-1-031330000000001333"));
        assert!(!menu.is_dirty());
        assert!(menu.commit().is_none());
    }

    #[test]
    fn discard_returns_to_the_applied_profile() {
        let mut menu = menu(DEFAULT_FORCEPOWERS);
        menu.set_side(ForceSide::Dark);
        menu.reset();
        assert!(menu.is_dirty());
        menu.discard();
        assert!(!menu.is_dirty());
        assert_eq!(menu.allocation().encode(), DEFAULT_FORCEPOWERS);
    }

    #[test]
    fn stepping_back_to_the_applied_profile_is_clean() {
        let mut menu = menu(DEFAULT_FORCEPOWERS);
        assert!(menu.step(2, false));
        assert!(menu.step(2, true));
        assert!(!menu.is_dirty());
    }

    #[test]
    fn opening_legalizes_without_counting_as_a_change() {
        // Grip on the light side is stripped on open, not left pending.
        let menu = menu("7-1-030000100000001000");
        assert!(!menu.is_dirty());
        assert_eq!(menu.allocation().levels[6], 0);
    }

    #[test]
    fn availability_follows_side_and_gametype() {
        let mut menu = menu(DEFAULT_FORCEPOWERS);
        assert!(menu.is_available(0)); // Heal, light
        assert!(!menu.is_available(6)); // Grip, dark
        assert!(menu.is_available(1)); // Jump, neutral
        assert!(!menu.is_available(11)); // Team Heal outside team games
        menu.set_side(ForceSide::Dark);
        assert!(!menu.is_available(0));
        assert!(menu.is_available(6));
        menu.load(
            DEFAULT_FORCEPOWERS,
            0,
            ForceLegalizeRules {
                gametype: GT_TEAM,
                ..ForceLegalizeRules::default()
            },
        );
        assert!(menu.is_available(11));
        assert!(!menu.is_available(12)); // Team Energize is dark
    }
}
