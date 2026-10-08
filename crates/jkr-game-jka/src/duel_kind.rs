//! jaPRO's kinds of private duel (`dueltypes[]`, `Cmd_ForceDuel_f`, `Cmd_GunDuel_f` and
//! `Cmd_EngageDuel_f`, `g_cmds.c:4147-4510` of jaPRO): stock's saber duel (`engage_duel`),
//! the Force duel (`engage_fullforceduel`), in which the two may use every power on each
//! other and hurt each other with anything but mines, and gun duels
//! (`engage_gunduel [weapon]`, with `g_allowGunDuel`), fought with one weapon, or with all
//! of them. A challenge is accepted only with the same kind; a gun duel is fought with the
//! challenger's weapon.

use crate::weapon_data::LEGACY_WEAPON_DATA;

const WP_STUN_BATON: u32 = 1;
const WP_MELEE: u32 = 2;
const WP_SABER: u32 = 3;
const WP_BRYAR_PISTOL: u32 = 4;
const WP_ROCKET_LAUNCHER: u32 = 11;
const WP_CONCUSSION: u32 = 15;
/// `LAST_USEABLE_WEAPON` (`WP_BRYAR_OLD`).
const LAST_USEABLE_WEAPON: u32 = 16;
/// jaPRO's numbers past the weapons: the stun baton by name, and every weapon.
const STUN_BY_NAME: u8 = 19;
const ALL_WEAPONS: u8 = 20;
/// `AMMO_BLASTER` .. `AMMO_DETPACK`, as "all weapons" fills them.
const ALL_WEAPONS_AMMO: [(usize, u32); 7] = [
    (2, 300),
    (3, 300),
    (4, 300),
    (5, 10),
    (7, 10),
    (8, 10),
    (9, 10),
];

/// What a private duel is fought with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DuelKind {
    /// Stock's: sabers, no Force powers but jumping and the saber's own.
    #[default]
    Saber,
    /// jaPRO's Force duel.
    Force,
    /// jaPRO's gun duel, by its number: a weapon's plus two, [`STUN_BY_NAME`] for the stun
    /// baton asked for by name, [`ALL_WEAPONS`] for every weapon.
    Gun(u8),
}

impl DuelKind {
    /// `Cmd_GunDuel_f`'s weapon: the one named (`melee`, `pistol`, `e11`, `rocket`, `all`
    /// ...), or without a name the one in hand; the pistol for anything else, the saber
    /// included.
    pub fn gun(name: Option<&[u8]>, in_hand: u32) -> Self {
        let weapon = match name {
            None => {
                if in_hand > LAST_USEABLE_WEAPON {
                    0
                } else {
                    in_hand
                }
            }
            Some(name) => gun_by_name(&name.to_ascii_lowercase()),
        };
        let weapon = if weapon == 0 || weapon == WP_SABER || weapon > LAST_USEABLE_WEAPON + 2 {
            WP_BRYAR_PISTOL
        } else {
            weapon
        };
        Self::Gun(weapon as u8 + 2)
    }

    /// jaPRO's number for it (`dueltypes[n]`).
    pub fn number(self) -> u32 {
        match self {
            Self::Saber => 0,
            Self::Force => 1,
            Self::Gun(number) => u32::from(number),
        }
    }

    /// The `EV_PRIVATE_DUEL` an accepted duel raises on both: stock's 1 for a saber duel,
    /// 2 for a Force duel (which a stock client takes for the duel's start), a gun
    /// duel's number for one.
    pub fn accept_event(self) -> u32 {
        match self {
            Self::Saber => 1,
            Self::Force => 2,
            Self::Gun(number) => u32::from(number),
        }
    }

    /// Whether its duellists must hold their sabers to challenge and be challenged.
    pub fn needs_saber(self) -> bool {
        matches!(self, Self::Saber | Self::Force)
    }

    /// Whether a challenge of kind `challenge` is accepted by answering with this kind:
    /// the same saber or Force duel, or a gun duel for a gun duel (fought with the
    /// challenger's weapon).
    pub fn accepts(self, challenge: Self) -> bool {
        match (self, challenge) {
            (Self::Gun(_), Self::Gun(_)) => true,
            _ => self == challenge && self.needs_saber(),
        }
    }

    /// The weapon its duellists are held to (`bg_pmove.c:10611-10645` of jaPRO): the saber,
    /// the gun duel's, or none for a duel of every weapon.
    pub fn held_weapon(self) -> Option<u32> {
        match self {
            Self::Saber | Self::Force => Some(WP_SABER),
            Self::Gun(ALL_WEAPONS) => None,
            Self::Gun(STUN_BY_NAME) => Some(WP_STUN_BATON),
            Self::Gun(number) => Some(u32::from(number) - 2),
        }
    }

    /// The duel's name in the challenge's centre print (`^2(Saber Duel)`, `^4(Force Duel)`,
    /// `^4(Rocket Duel)`); `None` for stock's saber duel, whose print has none.
    pub fn challenge_label(self) -> Option<String> {
        match self {
            Self::Saber => None,
            Self::Force => Some("^4(Force Duel)".to_owned()),
            Self::Gun(number) => Some(format!("^4({} Duel)", gun_label(u32::from(number) - 2))),
        }
    }

    /// The word the duel's start and end prints close with: `(Force)` or `(Gun)`; `None`
    /// for stock's saber duel.
    pub fn result_label(self) -> Option<&'static str> {
        match self {
            Self::Saber => None,
            Self::Force => Some("(Force)"),
            Self::Gun(_) => Some("(Gun)"),
        }
    }

    /// The health a gun duel's duellists start with under `g_duelStartHealth`: twice it for
    /// rockets and the concussion rifle, three times for every weapon.
    pub fn start_health(self, start_health: i32) -> i32 {
        match self {
            Self::Gun(number)
                if u32::from(number) == WP_ROCKET_LAUNCHER + 2
                    || u32::from(number) == WP_CONCUSSION + 2 =>
            {
                start_health * 2
            }
            Self::Gun(18) => start_health * 3,
            _ => start_health,
        }
    }

    /// What an accepted gun duel arms both duellists with: every weapon and set ammunition
    /// for a duel of every weapon, else 999 rounds of the weapon's ammunition (none for the
    /// stun baton, the fists and the pistol, which need none). Each entry is an ammunition
    /// index and its count; the flag says whether every weapon is given.
    pub fn arms(self) -> (bool, Vec<(usize, u32)>) {
        let Self::Gun(number) = self else {
            return (false, Vec::new());
        };
        let weapon = u32::from(number) - 2;
        if number == ALL_WEAPONS {
            return (true, ALL_WEAPONS_AMMO.to_vec());
        }
        if matches!(weapon, WP_STUN_BATON | WP_MELEE | WP_BRYAR_PISTOL) {
            return (false, Vec::new());
        }
        let ammo = LEGACY_WEAPON_DATA
            .get(weapon as usize)
            .map(|data| data.ammo_index);
        (
            false,
            ammo.map(|index| vec![(index, 999)]).unwrap_or_default(),
        )
    }

    /// The ammunition a gun duel's winner is left with (`g_active.c:4476-4488` of jaPRO):
    /// three rockets, or half the most of the weapon's ammunition; `None` when the duel was
    /// of every weapon (whose winner and loser are given every weapon again) or of a weapon
    /// outside the blaster .. Bryar range.
    pub fn winner_ammo(self) -> Option<(usize, u32)> {
        let Self::Gun(number) = self else {
            return None;
        };
        let weapon = u32::from(number) - 2;
        if !(5..=LAST_USEABLE_WEAPON).contains(&weapon) {
            return None;
        }
        let index = LEGACY_WEAPON_DATA.get(weapon as usize)?.ammo_index;
        let count = if weapon == WP_ROCKET_LAUNCHER {
            3
        } else {
            (crate::npc_combat::AMMO_MAXIMA
                .get(index)
                .copied()
                .unwrap_or(0) as f32
                * 0.5) as u32
        };
        Some((index, count))
    }

    /// Whether the duel was of every weapon: both are given every weapon again at its end.
    pub fn every_weapon(self) -> bool {
        self == Self::Gun(ALL_WEAPONS)
    }
}

/// `Cmd_GunDuel_f`'s names (lower case); 0 for none of them.
fn gun_by_name(name: &[u8]) -> u32 {
    match name {
        b"melee" | b"fists" | b"fisticuffs" => WP_MELEE,
        b"stun" | b"stunbaton" => u32::from(STUN_BY_NAME) - 2,
        b"pistol" | b"dl44" => WP_BRYAR_PISTOL,
        b"bryar" | b"bryarpistol" => 16,
        b"blaster" | b"e11" => 5,
        b"sniper" | b"disruptor" => 6,
        b"bowcaster" => 7,
        b"repeater" => 8,
        b"demp2" | b"demp" => 9,
        b"flechette" | b"shotgun" => 10,
        b"concussion" | b"conc" => WP_CONCUSSION,
        b"rocket" | b"rockets" | b"rocketlauncher" => WP_ROCKET_LAUNCHER,
        b"detpack" | b"detpacks" => 14,
        b"thermal" | b"thermals" | b"grenade" | b"grenades" | b"dets" => 12,
        b"tripmine" | b"trip" | b"trips" => 13,
        b"all" => u32::from(ALL_WEAPONS) - 2,
        _ => 0,
    }
}

/// The weapon's name in a gun duel's challenge.
fn gun_label(weapon: u32) -> &'static str {
    match weapon {
        1 | 17 => "Stun baton",
        2 => "Melee",
        4 => "Pistol",
        5 => "E11",
        6 => "Sniper",
        7 => "Bowcaster",
        8 => "Repeater",
        9 => "Demp2",
        10 => "Flechette",
        11 => "Rocket",
        12 => "Thermal",
        13 => "Tripmine",
        14 => "Detpack",
        15 => "Concussion rifle",
        16 => "Bryar",
        _ => "Gun",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gun_duels_take_the_named_or_held_weapon() {
        assert_eq!(DuelKind::gun(Some(b"Rocket"), 3), DuelKind::Gun(13));
        assert_eq!(DuelKind::gun(Some(b"stun"), 3), DuelKind::Gun(19));
        assert_eq!(DuelKind::gun(Some(b"all"), 3), DuelKind::Gun(20));
        assert_eq!(DuelKind::gun(Some(b"spoon"), 3), DuelKind::Gun(6));
        // The weapon in hand; the saber or nothing make it the pistol.
        assert_eq!(DuelKind::gun(None, 5), DuelKind::Gun(7));
        assert_eq!(DuelKind::gun(None, 3), DuelKind::Gun(6));
        assert_eq!(DuelKind::gun(None, 17), DuelKind::Gun(6));
        assert_eq!(DuelKind::Gun(19).held_weapon(), Some(1));
        assert_eq!(DuelKind::Gun(20).held_weapon(), None);
        assert_eq!(DuelKind::Gun(13).held_weapon(), Some(11));
        assert_eq!(DuelKind::Force.held_weapon(), Some(3));
    }

    #[test]
    fn a_challenge_is_accepted_only_in_kind() {
        assert!(DuelKind::Force.accepts(DuelKind::Force));
        assert!(!DuelKind::Saber.accepts(DuelKind::Force));
        assert!(!DuelKind::Force.accepts(DuelKind::Saber));
        assert!(DuelKind::Gun(6).accepts(DuelKind::Gun(13)));
        assert!(!DuelKind::Gun(6).accepts(DuelKind::Saber));
        assert_eq!(
            DuelKind::Gun(13).challenge_label().as_deref(),
            Some("^4(Rocket Duel)")
        );
        assert_eq!(DuelKind::Gun(13).winner_ammo(), Some((5, 3)));
        assert_eq!(DuelKind::Gun(7).winner_ammo(), Some((2, 150)));
        assert_eq!(DuelKind::Gun(13).arms(), (false, vec![(5, 999)]));
        assert_eq!(DuelKind::Gun(6).arms(), (false, Vec::new()));
    }
}
