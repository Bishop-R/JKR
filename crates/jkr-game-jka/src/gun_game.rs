//! jaPRO's gun game (`g_gunGame`; `G_GiveGunGameWeapon`, `g_client.c:3638-3805`): every
//! player climbs one ladder of weapons, a rung per kill — of a player or an NPC — less
//! its suicides ([`crate::japro_stats::Stats`]); in the team games the ladder is the
//! team's, by its score. Each rung is
//! one weapon with ammunition to spare (999) and, on some rungs, one fire mode only: the
//! other button fires it too (`forcedFireMode`, swapped in the server's `PM_Weapon`). The
//! saber rungs come last, the first of them throw-only with the throw at its third level;
//! a player on a saber rung that may swing runs 28% faster. Past the last rung the game
//! is won.

/// `WP_*` the ladder hands out.
const WP_STUN_BATON: u32 = 1;
const WP_SABER: u32 = 3;
const WP_BRYAR_PISTOL: u32 = 4;
const WP_BLASTER: u32 = 5;
const WP_DISRUPTOR: u32 = 6;
const WP_BOWCASTER: u32 = 7;
const WP_REPEATER: u32 = 8;
const WP_DEMP2: u32 = 9;
const WP_FLECHETTE: u32 = 10;
const WP_ROCKET_LAUNCHER: u32 = 11;
const WP_THERMAL: u32 = 12;
const WP_CONCUSSION: u32 = 15;
const WP_BRYAR_OLD: u32 = 16;
/// `AMMO_*`.
const AMMO_BLASTER: usize = 2;
const AMMO_POWERCELL: usize = 3;
const AMMO_METAL_BOLTS: usize = 4;
const AMMO_ROCKETS: usize = 5;
const AMMO_THERMAL: usize = 7;
/// `BUTTON_ATTACK`, `BUTTON_ALT_ATTACK`.
const BUTTON_ATTACK: u16 = 1;
const BUTTON_ALT_ATTACK: u16 = 128;

/// What a rung's weapon is given of its ammunition.
pub const AMMO: u32 = 999;
/// How a saber rung that may swing speeds its player up (`ps.speed *= 1.28f`).
pub const SABER_SPEED: f32 = 1.28;

/// The fire mode a rung allows (`forcedFireMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FireMode {
    /// Both buttons, each its own.
    #[default]
    Both,
    /// `1`: the alternate button fires the primary.
    Primary,
    /// `2`: the primary button fires the alternate.
    Alternate,
}

/// A rung of the ladder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rung {
    /// `WP_*`, the only weapon held.
    pub weapon: u32,
    /// The `AMMO_*` filled to [`AMMO`], for a weapon that uses any.
    pub ammo: Option<usize>,
    pub fire: FireMode,
    /// Saber throw known, at level 3; on every other rung it is not known.
    pub saber_throw: bool,
}

/// The rung `score` reaches (a player's kills less its suicides, or its team's score less
/// them); `None` past the last: the game is won.
pub fn rung(score: i32) -> Option<Rung> {
    use FireMode::{Alternate, Both, Primary};
    let (weapon, ammo, fire) = match score {
        ..=0 => (WP_CONCUSSION, Some(AMMO_METAL_BOLTS), Primary),
        1 => (WP_ROCKET_LAUNCHER, Some(AMMO_ROCKETS), Primary),
        2 => (WP_ROCKET_LAUNCHER, Some(AMMO_ROCKETS), Alternate),
        3 => (WP_BLASTER, Some(AMMO_BLASTER), Both),
        4 => (WP_REPEATER, Some(AMMO_METAL_BOLTS), Alternate),
        5 => (WP_REPEATER, Some(AMMO_METAL_BOLTS), Primary),
        6 => (WP_FLECHETTE, Some(AMMO_METAL_BOLTS), Both),
        7 => (WP_CONCUSSION, Some(AMMO_METAL_BOLTS), Alternate),
        8 => (WP_STUN_BATON, None, Both),
        9 => (WP_DISRUPTOR, Some(AMMO_POWERCELL), Both),
        10 => (WP_BOWCASTER, Some(AMMO_POWERCELL), Both),
        11 => (WP_DEMP2, Some(AMMO_POWERCELL), Primary),
        12 => (WP_DEMP2, Some(AMMO_POWERCELL), Alternate),
        13 => (WP_THERMAL, Some(AMMO_THERMAL), Alternate),
        14 => (WP_BRYAR_OLD, Some(AMMO_BLASTER), Both),
        15 => (WP_BRYAR_PISTOL, Some(AMMO_BLASTER), Both),
        16 => (WP_THERMAL, Some(AMMO_THERMAL), Primary),
        17 => (WP_SABER, None, Alternate),
        18 => (WP_SABER, None, Primary),
        _ => return None,
    };
    Some(Rung {
        weapon,
        ammo,
        fire,
        saber_throw: score == 17,
    })
}

impl Rung {
    /// Whether its player runs [`SABER_SPEED`] times as fast: a saber that may swing.
    pub fn fast(&self) -> bool {
        self.weapon == WP_SABER && self.fire != FireMode::Alternate
    }

    /// The rung's arms on a player: the weapon alone in hand and owned, its ammunition
    /// filled; nothing else of the state changes (`G_GiveGunGameWeapon`).
    pub fn arm(&self, state: &mut jkr_protocol::PlayerState) {
        const STAT_WEAPONS: usize = 4;
        const PS_WEAPON: usize = 47;
        state.stats[STAT_WEAPONS] = 1 << self.weapon;
        if let Some(ammo) = self.ammo {
            state.ammo[ammo] = AMMO;
        }
        state.set_raw_field(PS_WEAPON, self.weapon);
    }
}

/// `forcedFireMode` at the top of the server's `PM_Weapon`: the button a rung forbids
/// fires the one it allows.
pub fn force_fire(buttons: &mut u16, fire: FireMode) {
    match fire {
        FireMode::Alternate if *buttons & BUTTON_ATTACK != 0 => {
            *buttons = *buttons & !BUTTON_ATTACK | BUTTON_ALT_ATTACK;
        }
        FireMode::Primary if *buttons & BUTTON_ALT_ATTACK != 0 => {
            *buttons = *buttons & !BUTTON_ALT_ATTACK | BUTTON_ATTACK;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ladder_climbs_to_the_saber_and_ends() {
        assert_eq!(rung(-3), rung(0));
        assert_eq!(rung(0).map(|rung| rung.weapon), Some(WP_CONCUSSION));
        let throw_only = rung(17).unwrap();
        assert!(throw_only.saber_throw && !throw_only.fast());
        assert!(rung(18).unwrap().fast() && !rung(18).unwrap().saber_throw);
        assert_eq!(rung(19), None);
        let mut buttons = BUTTON_ATTACK | 2;
        force_fire(&mut buttons, FireMode::Alternate);
        assert_eq!(buttons, BUTTON_ALT_ATTACK | 2);
        force_fire(&mut buttons, FireMode::Primary);
        assert_eq!(buttons, BUTTON_ATTACK | 2);
        force_fire(&mut buttons, FireMode::Both);
        assert_eq!(buttons, BUTTON_ATTACK | 2);
    }

    #[test]
    fn a_rung_arms_its_weapon_alone() {
        let mut state = jkr_protocol::PlayerState::default();
        state.stats[4] = 0b11001;
        rung(9).unwrap().arm(&mut state);
        assert_eq!(state.stats[4], 1 << WP_DISRUPTOR);
        assert_eq!(state.ammo[AMMO_POWERCELL], AMMO);
        assert_eq!(state.weapon(), WP_DISRUPTOR as u8);
    }
}
