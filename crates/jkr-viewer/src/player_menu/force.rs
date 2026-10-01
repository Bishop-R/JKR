//! Legal Force-profile editing state backed solely by `ForceAllocation`.

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

pub(super) struct ForceMenu {
    allocation: ForceAllocation,
    rules: ForceLegalizeRules,
    encoded: String,
}

impl ForceMenu {
    pub(super) fn new() -> Self {
        let allocation = ForceAllocation::default();
        Self {
            encoded: allocation.encode(),
            allocation,
            rules: ForceLegalizeRules::default(),
        }
    }

    pub(super) fn open(&mut self, console: &ViewerConsole) {
        let raw = console
            .text_value("forcepowers")
            .unwrap_or("7-1-032330000000001333");
        let mut allocation = ForceAllocation::parse(raw).unwrap_or_default();
        let server_rank = console.integer_cvar("ui_rankChange").unwrap_or(0);
        if server_rank > 0 {
            allocation.rank = u8::try_from(server_rank).unwrap_or(7).min(7);
        }
        self.rules.gametype = console.integer_cvar("g_gametype").unwrap_or(0) as i32;
        self.rules.free_saber = console.integer_cvar("ui_freesaber").unwrap_or(0) != 0;
        self.rules.max_rank = allocation.rank;
        self.allocation = legalize_force_powers(&allocation.encode(), self.rules).allocation;
        self.refresh_encoded();
    }

    pub(super) fn allocation(&self) -> &ForceAllocation {
        &self.allocation
    }

    /// Points remaining under the same free-saber policy as the editor's spend/refund path.
    pub(super) fn remaining_points(&self) -> u16 {
        self.allocation.remaining_points(self.rules.free_saber)
    }

    pub(super) fn set_side(&mut self, side: ForceSide) {
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

    /// Write `forcepowers` to the console. Called after every change.
    pub(super) fn apply(&mut self, console: &mut ViewerConsole) {
        console.set_cvar("forcepowers", &self.encoded);
    }

    fn refresh_encoded(&mut self) {
        self.encoded = self.allocation.encode();
    }
}
