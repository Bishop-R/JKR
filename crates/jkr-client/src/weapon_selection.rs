//! BaseJKA weapon-selection policy used by binds, HUD selection and usercmds.
//!
//! This mirrors `CG_WeaponSelectable`, `CG_NextWeapon_f`, `CG_PrevWeapon_f`,
//! and `CG_Weapon_f` in `codemp/cgame/cg_weapons.c:1054-1080,
//! 1382-1507,1514-1636`. In particular, cycling uses the non-numeric
//! Flechette -> Concussion -> Rocket and Det Pack -> Bryar Old order, requires
//! ownership and enough ammo for at least one firing mode, and preserves the
//! planted-detpack exception.

use crate::{LEGACY_WEAPON_COUNT, legacy_weapon_data};
use jkr_protocol::PlayerState;

const WEAPON_COUNT: u8 = LEGACY_WEAPON_COUNT as u8;
const WP_SABER: u8 = 3;
const WP_FLECHETTE: u8 = 10;
const WP_ROCKET: u8 = 11;
const WP_THERMAL: u8 = 12;
const WP_DET_PACK: u8 = 14;
const WP_CONCUSSION: u8 = 15;
const WP_BRYAR_OLD: u8 = 16;

/// Fixed-size inputs read by codemp's weapon selection functions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyWeaponInventory {
    /// `STAT_WEAPONS` bitset.
    pub owned: u32,
    /// `playerState_t::ammo` pools.
    pub ammo: [u32; 16],
    /// Whether a detpack exists in the world even when inventory ammo is zero.
    pub detpack_planted: bool,
    /// `PMF_FOLLOW` prevents switching.
    pub following: bool,
    /// `PM_SPECTATOR` prevents next/previous switching.
    pub spectator: bool,
    /// An emplaced gun owns selection while non-zero.
    pub emplaced: bool,
}

impl LegacyWeaponInventory {
    /// Project the relevant fields without modifying the protocol codec.
    pub fn from_player_state(player: &PlayerState) -> Self {
        Self {
            owned: player.stats[4],
            ammo: player.ammo,
            detpack_planted: player.has_detpack_planted(),
            following: player.movement_flags() & 4096 != 0,
            spectator: player.movement_type() == 4,
            emplaced: player.emplaced_index() != 0,
        }
    }
}

/// Apply `CG_WeaponSelectable` to one actual `weapon_t` value.
pub fn legacy_weapon_selectable(inventory: &LegacyWeaponInventory, weapon: u8) -> bool {
    let Some(data) = legacy_weapon_data(weapon) else {
        return false;
    };
    if weapon == 0 || inventory.owned & (1_u32 << weapon) == 0 {
        return false;
    }
    let ammo = inventory.ammo[data.ammo_index];
    if ammo < data.primary_cost as u32 && ammo < data.alternate_cost as u32 {
        return false;
    }
    weapon != WP_DET_PACK || ammo >= 1 || inventory.detpack_planted
}

/// The ammunition `weapon` draws on, as the inventory holds it
/// (`ps.ammo[weaponData[weapon].ammoIndex]`); `None` for no weapon.
pub fn legacy_weapon_ammo(inventory: &LegacyWeaponInventory, weapon: u8) -> Option<u32> {
    legacy_weapon_data(weapon).map(|data| inventory.ammo[data.ammo_index])
}

/// Select next/previous weapon using codemp's deliberately non-numeric order.
pub fn legacy_cycle_weapon(inventory: &LegacyWeaponInventory, current: u8, direction: i8) -> u8 {
    if inventory.following || inventory.spectator || inventory.emplaced || direction == 0 {
        return current;
    }
    let original = current;
    let mut selected = current;
    for _ in 0..WEAPON_COUNT {
        selected = if direction > 0 {
            match selected {
                WP_FLECHETTE => WP_CONCUSSION,
                WP_CONCUSSION => WP_ROCKET,
                WP_DET_PACK => WP_BRYAR_OLD,
                value => (value + 1) % WEAPON_COUNT,
            }
        } else {
            match selected {
                WP_ROCKET => WP_CONCUSSION,
                WP_CONCUSSION => WP_FLECHETTE,
                WP_BRYAR_OLD => WP_DET_PACK,
                0 => WEAPON_COUNT - 1,
                value => value - 1,
            }
        };
        if legacy_weapon_selectable(inventory, selected) {
            return selected;
        }
    }
    original
}

/// Apply `weapon N`'s SP-compatible slot mapping and explosive sub-cycle.
pub fn legacy_direct_weapon(
    inventory: &LegacyWeaponInventory,
    current_weapon: u8,
    slot: u8,
) -> Option<u8> {
    if inventory.following || inventory.emplaced || !(1..=WP_BRYAR_OLD).contains(&slot) {
        return None;
    }
    let selected = if slot == 1 {
        saber_or_melee(inventory)
    } else {
        slot.saturating_add(2)
    };
    direct(inventory, current_weapon, selected)
}

/// `CG_WeaponClean_f` (`cg_weapons.c`): `weaponclean` names a weapon by its own number
/// rather than its key, 1 standing for the saber (melee without one), and otherwise
/// chooses as `weapon` does.
pub fn legacy_clean_weapon(
    inventory: &LegacyWeaponInventory,
    current_weapon: u8,
    number: u8,
) -> Option<u8> {
    if inventory.following || inventory.emplaced || !(1..=WP_BRYAR_OLD).contains(&number) {
        return None;
    }
    let selected = if number == 1 {
        saber_or_melee(inventory)
    } else {
        number
    };
    direct(inventory, current_weapon, selected)
}

fn saber_or_melee(inventory: &LegacyWeaponInventory) -> u8 {
    if inventory.owned & (1 << WP_SABER) != 0 {
        WP_SABER
    } else {
        2
    }
}

/// The weapon `selected` names, the explosives cycling as `CG_Weapon_f` cycles them.
fn direct(inventory: &LegacyWeaponInventory, current_weapon: u8, mut selected: u8) -> Option<u8> {
    if selected > WP_BRYAR_OLD + 1 {
        return None;
    }
    if (WP_THERMAL..=WP_DET_PACK).contains(&selected) {
        let mut candidate = if (WP_THERMAL..=WP_DET_PACK).contains(&current_weapon) {
            current_weapon + 1
        } else {
            WP_THERMAL
        };
        for _ in 0..=4 {
            if candidate > WP_DET_PACK {
                candidate = WP_THERMAL;
            }
            if legacy_weapon_selectable(inventory, candidate) {
                selected = candidate;
                break;
            }
            candidate += 1;
        }
    }
    legacy_weapon_selectable(inventory, selected).then_some(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A weapon reads its own pool (`weaponData[].ammoIndex`): the pistol and the blaster
    /// share the blaster pack (`AMMO_BLASTER`, 2), the disruptor draws on power cells.
    #[test]
    fn a_weapon_reads_its_own_ammunition_pool() {
        let mut inventory =
            LegacyWeaponInventory::from_player_state(&jkr_protocol::PlayerState::zero());
        inventory.ammo[2] = 77;
        inventory.ammo[3] = 5;
        assert_eq!(legacy_weapon_ammo(&inventory, 4), Some(77));
        assert_eq!(legacy_weapon_ammo(&inventory, 5), Some(77));
        assert_eq!(legacy_weapon_ammo(&inventory, 6), Some(5));
    }
}
