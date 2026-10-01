//! Stock local Force/holdable selection (codemp/cgame/cg_main.c:2720-2867).
use jkr_protocol::PlayerState;

/// Stock display/cycle order, codemp/game/bg_misc.c:200-220.
pub const FORCE_ORDER: [u8; 18] = [5, 0, 10, 9, 11, 1, 2, 3, 4, 14, 7, 13, 8, 6, 12, 15, 16, 17];
/// WEAPON_SELECT_TIME, codemp/cgame/cg_local.h:50.
pub const SELECT_MS: i32 = 1400;
const FORCE_MASK: u32 = ((1 << 18) - 1) & !((1 << 1) | (7 << 15));
const ITEM_MASK: u32 = ((1 << 12) - 2) & !((1 << 7) | (1 << 8) | (1 << 9));

/// Local overrides, deliberately independent of snapshot reconciliation.
#[derive(Clone, Copy, Debug, Default)]
pub struct Selection {
    /// None uses the server's selected force power.
    pub force: Option<u8>,
    /// None sends stock's -1 sentinel, leaving the server item unchanged.
    pub inventory: Option<u8>,
    force_time: Option<i32>,
    item_time: Option<i32>,
}

/// Read-only selector projection for a renderer, with no strings or allocations.
#[derive(Clone, Copy, Debug)]
pub struct SelectionView {
    /// True for the inventory selector, false for Force.
    pub inventory: bool,
    /// Available selectable power/item tags.
    pub available: u32,
    /// The highlighted tag.
    pub selected: u8,
    /// UI fade within the stock 1400 ms lifetime.
    pub alpha: f32,
}

impl Selection {
    /// Cycle locally; no snapshot, spectator and follow states cannot select.
    pub fn cycle(
        &mut self,
        player: Option<&PlayerState>,
        time: i32,
        inventory: bool,
        direction: i8,
        use_held: bool,
    ) {
        let Some(player) = player else { return };
        if player.movement_type() == 4 || player.movement_flags() & 4096 != 0 {
            return;
        }
        self.sync(player, time);
        let known = player.raw_field(51).unwrap_or(0);
        // CG_NoUseableForce checks known bits only, not energy or force levels.
        if inventory || use_held || known & FORCE_MASK == 0 {
            let current = self.inventory.unwrap_or_else(|| item_tag(player));
            let mut next = current as i32;
            // BG_CycleInven's bounded scan includes the empty initial selection.
            for _ in 0..32 {
                next += if direction > 0 { 1 } else { -1 };
                if next <= 0 {
                    next = 11;
                }
                if next >= 12 {
                    next = 1;
                }
                if next == current as i32 {
                    break;
                }
                if player.stats[2] & ITEM_MASK & (1 << next) != 0 {
                    self.inventory = Some(next as u8);
                    break;
                }
            }
            if self.inventory.is_none() && current != 0 {
                self.inventory = Some(current);
            }
            if self.inventory.is_some() {
                self.item_time = Some(time);
            }
        } else {
            let current = self.force.unwrap_or_else(|| player.selected_force_power());
            if let Some(index) = FORCE_ORDER.iter().position(|&p| p == current) {
                for step in 1..18 {
                    let offset = if direction > 0 { step } else { 18 - step };
                    let next = FORCE_ORDER[(index + offset) % 18];
                    if known & FORCE_MASK & (1 << next) != 0 {
                        self.force = Some(next);
                        break;
                    }
                }
                if known & (1 << self.force.unwrap_or(current)) != 0 {
                    self.force = Some(self.force.unwrap_or(current));
                    self.force_time = Some(time);
                }
            }
        }
    }

    /// Reset expired Force overrides and consumed items to authoritative selection.
    pub fn sync(&mut self, player: &PlayerState, time: i32) {
        if self
            .force_time
            .is_some_and(|at| time < at || time - at > SELECT_MS)
        {
            self.force = None;
            self.force_time = None;
        }
        if self
            .inventory
            .is_some_and(|tag| player.stats[2] & (1 << tag) == 0)
            || self.item_time.is_some_and(|at| time < at)
        {
            self.inventory = None;
            self.item_time = None;
        }
    }

    /// Most recently cycled selector; only visible alive and outside spectator/follow.
    pub fn view(&self, player: &PlayerState, time: i32) -> Option<SelectionView> {
        if player.health() <= 0
            || player.movement_type() == 4
            || player.movement_flags() & 4096 != 0
        {
            return None;
        }
        let inventory = self.item_time > self.force_time;
        let at = if inventory {
            self.item_time
        } else {
            self.force_time
        }?;
        let age = time - at;
        if !(0..=SELECT_MS).contains(&age) {
            return None;
        }
        let view = SelectionView {
            inventory,
            available: if inventory {
                player.stats[2] & ITEM_MASK
            } else {
                player.raw_field(51).unwrap_or(0) & FORCE_MASK
            },
            selected: if inventory {
                self.inventory.unwrap_or_else(|| item_tag(player))
            } else {
                self.force.unwrap_or_else(|| player.selected_force_power())
            },
            alpha: ((SELECT_MS - age) as f32 / 300.0).clamp(0.0, 1.0),
        };
        (view.selected < 32 && view.available & (1 << view.selected) != 0).then_some(view)
    }
}

fn item_tag(player: &PlayerState) -> u8 {
    // bg_itemlist's holdables, codemp/game/bg_misc.c:795-1003 (index = tag + 3).
    if (4..=14).contains(&player.stats[1]) {
        (player.stats[1] - 3) as u8
    } else {
        0
    }
}
