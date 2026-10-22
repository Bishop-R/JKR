//! The health and ammunition dispensers Siege's techs and suppliers carry, used on a
//! teammate with the use key (`TryUse`, `g_utils.c:1752-1777`): `G_CanUseDispOn` asks
//! whether the teammate needs either, `G_UseDispenserOn` (`:1450-1482`) gives it four health
//! a use, and ammunition for the weapon it holds at that weapon's own pace — and marks it
//! as being served, which `G_SetClientSound` (`g_active.c:456-485`) loops a sound for at the
//! end of its frame ([`loop_sound`]), as it does for a player hacking or standing in lava
//! or slime.

use crate::npc_combat::AMMO_MAXIMA;
use crate::weapon_data::legacy_weapon_data;
use crate::world_effects::{CONTENTS_LAVA, CONTENTS_SLIME};
use jkr_protocol::PlayerState;

/// Health a use of the health dispenser gives.
const HEAL_PER_USE: i32 = 4;
/// How long a use marks its target as being served (`isMedHealed`, `isMedSupplied`).
const SERVED_MS: i32 = 500;
/// `STAT_HEALTH`, `STAT_MAX_HEALTH`.
const STAT_HEALTH: usize = 0;
const STAT_MAX_HEALTH: usize = 8;
/// `LAST_USEABLE_WEAPON` (`WP_BRYAR_OLD`).
const LAST_USEABLE_WEAPON: u8 = 16;
/// The sounds `G_InitGame` registers first (`g_main.c:223-227`, [`crate::registries::init_game`]),
/// so the same four indices on every level: `snd_fry`, `snd_hack`, `snd_medHealed`,
/// `snd_medSupplied`.
pub const SOUND_FRY: u16 = 1;
pub const SOUND_HACK: u16 = 2;
pub const SOUND_HEALED: u16 = 3;
pub const SOUND_SUPPLIED: u16 = 4;

/// What a client keeps of the dispensers: until when it is being healed and supplied
/// (`isMedHealed`, `isMedSupplied`), and — as a user — when it may next give ammunition
/// (`medSupplyDebounce`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Service {
    pub healed_until: i32,
    pub supplied_until: i32,
    pub supply_debounce: i32,
}

/// `G_CanUseDispOn`'s life check: in use, alive by both counts.
fn alive(state: &PlayerState, health: i32) -> bool {
    health >= 1 && (state.stats[STAT_HEALTH] as i32) >= 1
}

/// `G_CanUseDispOn(target, HI_HEALTHDISP)`: alive and hurt.
pub fn needs_health(state: &PlayerState, health: i32) -> bool {
    alive(state, health) && (state.stats[STAT_HEALTH] as i32) < state.stats[STAT_MAX_HEALTH] as i32
}

/// `G_CanUseDispOn(target, HI_AMMODISP)`: alive, holding a weapon a player uses, and short
/// of its ammunition.
pub fn needs_ammo(state: &PlayerState, health: i32) -> bool {
    let weapon = state.weapon();
    if !alive(state, health) || weapon == 0 || weapon > LAST_USEABLE_WEAPON {
        return false;
    }
    legacy_weapon_data(weapon).is_some_and(|data| {
        let maximum = AMMO_MAXIMA.get(data.ammo_index).copied().unwrap_or(0);
        state.ammo_value(data.ammo_index).unwrap_or(0) < maximum
    })
}

/// `G_UseDispenserOn(user, HI_HEALTHDISP, target)`: four health, held to the maximum, and
/// half a second of being healed.
pub fn heal(state: &mut PlayerState, health: &mut i32, served: &mut Service, level_time: i32) {
    let maximum = state.stats[STAT_MAX_HEALTH] as i32;
    let healed = (state.stats[STAT_HEALTH] as i32 + HEAL_PER_USE).min(maximum);
    state.stats[STAT_HEALTH] = healed as u32;
    *health = healed;
    served.healed_until = level_time + SERVED_MS;
}

/// `G_UseDispenserOn(user, HI_AMMODISP, target)`: when the user's pace allows, a shot's
/// worth of ammunition for the weapon the target holds, held to its maximum, and the next
/// gift a shot's time later; either way half a second of being supplied.
pub fn supply(
    state: &mut PlayerState,
    served: &mut Service,
    user_debounce: &mut i32,
    level_time: i32,
) {
    if *user_debounce < level_time
        && let Some(data) = legacy_weapon_data(state.weapon())
    {
        let maximum = AMMO_MAXIMA.get(data.ammo_index).copied().unwrap_or(0);
        if let Some(ammo) = state.ammo.get_mut(data.ammo_index) {
            *ammo = (*ammo as i32 + data.primary_cost).min(maximum) as u32;
        }
        *user_debounce = level_time + data.primary_time;
    }
    served.supplied_until = level_time + SERVED_MS;
}

/// `G_SetClientSound`: the loop a client's frame ends with — hacking first, then being
/// healed, then being supplied, then frying in lava or slime it is in; none otherwise.
pub fn loop_sound(
    hacking: bool,
    served: &Service,
    water_level: u8,
    water_type: u32,
    level_time: i32,
) -> u16 {
    if hacking {
        SOUND_HACK
    } else if served.healed_until > level_time {
        SOUND_HEALED
    } else if served.supplied_until > level_time {
        SOUND_SUPPLIED
    } else if water_level != 0 && water_type & (CONTENTS_LAVA | CONTENTS_SLIME) != 0 {
        SOUND_FRY
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A teammate at `health` of 100 holding the blaster with `ammo` of 300.
    fn mate(health: u32, ammo: u32) -> PlayerState {
        let mut state = PlayerState::zero();
        state.stats[STAT_HEALTH] = health;
        state.stats[STAT_MAX_HEALTH] = 100;
        state.set_raw_field(47, 5);
        state.ammo[2] = ammo;
        state
    }

    #[test]
    fn the_loop_sounds_are_the_games_first_registered() {
        let mut names = Vec::new();
        let _ = crate::registries::init_game(&mut |index, name: &[u8]| {
            names.push((index, name.to_vec()))
        });
        let index = |name: &str| {
            names
                .iter()
                .find(|(_, known)| known.as_slice() == name.as_bytes())
                .map(|(index, _)| (*index - names[0].0 + 1) as u16)
        };
        assert_eq!(index("sound/player/fry.wav"), Some(SOUND_FRY));
        assert_eq!(index("sound/player/hacking.wav"), Some(SOUND_HACK));
        assert_eq!(index("sound/player/supp_healed.wav"), Some(SOUND_HEALED));
        assert_eq!(
            index("sound/player/supp_supplied.wav"),
            Some(SOUND_SUPPLIED)
        );
    }

    #[test]
    fn a_use_heals_four_and_supplies_at_the_weapons_pace() {
        let mut state = mate(98, 299);
        let (mut health, mut served, mut debounce) = (98, Service::default(), 0);
        assert!(needs_health(&state, health) && needs_ammo(&state, health));
        heal(&mut state, &mut health, &mut served, 1_000);
        assert_eq!((health, state.stats[STAT_HEALTH]), (100, 100));
        assert!(!needs_health(&state, health));
        supply(&mut state, &mut served, &mut debounce, 1_000);
        // The blaster's shot costs 2 and takes 350 ms: held to the maximum, then waited for.
        assert_eq!((state.ammo[2], debounce), (300, 1_350));
        state.ammo[2] = 10;
        supply(&mut state, &mut served, &mut debounce, 1_300);
        assert_eq!(state.ammo[2], 10);
        assert_eq!(
            served,
            Service {
                healed_until: 1_500,
                supplied_until: 1_800,
                supply_debounce: 0
            }
        );
        assert_eq!(loop_sound(false, &served, 0, 0, 1_400), SOUND_HEALED);
        assert_eq!(loop_sound(false, &served, 0, 0, 1_600), SOUND_SUPPLIED);
        assert_eq!(loop_sound(true, &served, 0, 0, 1_600), SOUND_HACK);
        assert_eq!(
            loop_sound(false, &served, 1, CONTENTS_LAVA, 1_900),
            SOUND_FRY
        );
        assert_eq!(loop_sound(false, &served, 0, CONTENTS_LAVA, 1_900), 0);
        // The dead and the bare-handed need nothing.
        assert!(!needs_health(&mate(0, 0), 0));
        let mut fists = mate(50, 0);
        fists.set_raw_field(47, 2);
        assert!(!needs_ammo(&fists, 50));
    }
}
