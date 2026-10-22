//! The weapon the player has chosen and the server not yet confirmed (`cg.weaponSelect`,
//! `cg.weaponSelectTime`), the name the HUD shows for a moment, and the pair EternalJK's
//! `weaplast` swaps between (`cg.lastWeaponSelect`, `CG_LastWeapon_f`).

use std::time::Instant;

/// `LAST_USEABLE_WEAPON` (`WP_BRYAR_OLD`): the highest weapon `weaplast` looks for.
const LAST_USEABLE_WEAPON: u8 = 16;

#[derive(Debug)]
pub(crate) struct WeaponSelect {
    /// `cg.weaponSelect`; `None` follows the weapon the player holds.
    pub(crate) selected: Option<u8>,
    /// `cg.weaponSelectTime`.
    pub(crate) selected_at: Option<Instant>,
    /// The chosen weapon's name as the HUD shows it.
    pub(crate) label: String,
    /// `cg.lastWeaponSelect`: the weapon `weaplast` left, then the one it goes back to;
    /// 0 for none yet.
    last: [u8; 2],
}

impl Default for WeaponSelect {
    fn default() -> Self {
        Self {
            selected: None,
            selected_at: None,
            label: String::with_capacity(40),
            last: [0; 2],
        }
    }
}

impl WeaponSelect {
    /// `CG_LastWeapon_f`'s choice with `select` chosen and `held` in hand: the weapon
    /// before the last swap, or, while nothing has been swapped from the weapon the
    /// player spawned with, the highest other one `selectable` allows.
    pub(crate) fn last_weapon(
        &mut self,
        select: u8,
        held: u8,
        selectable: impl Fn(u8) -> bool,
    ) -> u8 {
        for last in &mut self.last {
            if *last == 0 {
                *last = held;
            }
        }
        if self.last[1] == self.last[0]
            && let Some(other) = (1..=LAST_USEABLE_WEAPON)
                .rev()
                .find(|&weapon| weapon != select && selectable(weapon))
        {
            self.last[1] = other;
        }
        if self.last[0] != select {
            self.last = [select, self.last[0]];
        }
        self.last[1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first `weaplast` goes to the highest other weapon; after that it swaps between
    /// the last two weapons it chose.
    #[test]
    fn weaplast_swaps_between_the_last_two() {
        let mut select = WeaponSelect::default();
        let held = |weapons: &'static [u8]| move |weapon| weapons.contains(&weapon);
        let owned = held(&[3, 4, 5]);
        assert_eq!(select.last_weapon(3, 3, &owned), 5);
        assert_eq!(select.last_weapon(5, 5, &owned), 3);
        assert_eq!(select.last_weapon(3, 3, &owned), 5);
        // Another weapon chosen by hand: EternalJK records weapons only as `weaplast` runs,
        // so it goes back to the one it last left (3), not the one held before (5).
        assert_eq!(select.last_weapon(4, 4, &owned), 3);
    }
}
