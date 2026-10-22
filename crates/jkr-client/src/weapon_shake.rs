//! What the local player's own weapon does to the view: the camera shake after a heavy or
//! charged shot and the concussion rifle's kick back (`CG_FireWeapon`,
//! `cg_weapons.c:1849-1905`), the shake while a shot charges (`CG_AddPlayerWeapon`,
//! `cg_weapons.c:620-657`, only while the first-person gun is drawn) and the kick's curve
//! (`CG_OffsetFirstPersonView`, `cg_view.c:951-966`).

const WP_BRYAR_PISTOL: u8 = 4;
const WP_BOWCASTER: u8 = 7;
const WP_REPEATER: u8 = 8;
const WP_DEMP2: u8 = 9;
const WP_FLECHETTE: u8 = 10;
const WP_ROCKET_LAUNCHER: u8 = 11;
const WP_CONCUSSION: u8 = 15;
const WP_BRYAR_OLD: u8 = 16;
/// `WEAPON_CHARGING`, `WEAPON_CHARGING_ALT`.
const WEAPON_CHARGING: u8 = 4;
const WEAPON_CHARGING_ALT: u8 = 5;
/// A kick rises for 200 ms and returns over the rest of 800.
const KICK_RISE: i32 = 200;
const KICK_TIME: i32 = 800;

/// A `CGCam_Shake`: intensity and milliseconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponShake {
    pub intensity: f32,
    pub duration: i32,
}

/// What one shot does to the shooter's view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FireFeedback {
    Shake(WeaponShake),
    /// The view pitches by this many degrees (negative: up) and comes back.
    Kick(f32),
}

/// `CG_FireWeapon`'s effect on the shooter's view. `charge` is how long the shot charged
/// (`cg.time - constantLight`, ms), `random` a uniform draw in [0, 1) for `flrand`; the
/// concussion rifle's kick only shows in first person.
pub fn fire_feedback(
    weapon: u8,
    alternate: bool,
    charge: i32,
    first_person: bool,
    random: f32,
) -> Option<FireFeedback> {
    let shake = |intensity, duration| {
        Some(FireFeedback::Shake(WeaponShake {
            intensity,
            duration,
        }))
    };
    match (weapon, alternate) {
        (WP_BRYAR_PISTOL | WP_BRYAR_OLD | WP_DEMP2, true) | (WP_BOWCASTER, false) => {
            shake((charge as f32 * 0.001).clamp(0.2, 3.0) * 2.0, 250)
        }
        (WP_CONCUSSION, false) => first_person.then_some(FireFeedback::Kick(-10.0 - 5.0 * random)),
        (WP_ROCKET_LAUNCHER, _) | (WP_REPEATER | WP_FLECHETTE, true) => shake(2.0 + random, 350),
        (WP_FLECHETTE, false) => shake(1.5, 250),
        _ => None,
    }
}

/// The shake of each frame a shot charges (`weapon_state`), `charge` ms in: none before
/// it starts, growing to 0.6 at a second, then 0.2.
pub fn charge_shake(weapon: u8, weapon_state: u8, charge: i32) -> Option<WeaponShake> {
    let charging = match weapon {
        WP_BRYAR_PISTOL | WP_BRYAR_OLD | WP_DEMP2 => weapon_state == WEAPON_CHARGING_ALT,
        WP_BOWCASTER => weapon_state == WEAPON_CHARGING,
        _ => false,
    };
    let seconds = charge as f32 * 0.001;
    if !charging || seconds < 0.0 {
        return None;
    }
    let intensity = if seconds > 1.0 {
        0.2
    } else {
        seconds * seconds * 0.6
    };
    Some(WeaponShake {
        intensity,
        duration: 100,
    })
}

/// How much of a kick shows `age` ms after it: rising for 200 ms, back over 600.
pub fn kick_fraction(age: i32) -> Option<f32> {
    match age {
        0..=KICK_RISE => Some(age as f32 / KICK_RISE as f32),
        _ if (KICK_RISE..KICK_TIME).contains(&age) => {
            Some((KICK_TIME - age) as f32 / (KICK_TIME - KICK_RISE) as f32)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shots_shake_as_stock_shakes_them() {
        let shake = |intensity, duration| {
            Some(FireFeedback::Shake(WeaponShake {
                intensity,
                duration,
            }))
        };
        // A charged pistol shot: two seconds' charge doubles to 4; under 0.2 s counts as 0.2.
        assert_eq!(fire_feedback(4, true, 2000, true, 0.5), shake(4.0, 250));
        assert_eq!(fire_feedback(9, true, 50, true, 0.5), shake(0.4, 250));
        assert_eq!(fire_feedback(7, false, 9000, false, 0.5), shake(6.0, 250));
        assert_eq!(fire_feedback(4, false, 2000, true, 0.5), None);
        assert_eq!(fire_feedback(11, false, 0, true, 0.5), shake(2.5, 350));
        assert_eq!(fire_feedback(10, false, 0, true, 0.5), shake(1.5, 250));
        assert_eq!(fire_feedback(8, false, 0, true, 0.5), None);
        assert_eq!(
            fire_feedback(15, false, 0, true, 0.0),
            Some(FireFeedback::Kick(-10.0))
        );
        assert_eq!(fire_feedback(15, false, 0, false, 0.0), None);
        assert_eq!(fire_feedback(15, true, 0, true, 0.0), None);
    }

    #[test]
    fn a_charge_shakes_more_then_less() {
        let at = |charge| charge_shake(7, 4, charge).map(|shake| shake.intensity);
        assert_eq!(at(-1), None);
        assert_eq!(at(500), Some(0.15));
        assert_eq!(at(1000), Some(0.6));
        assert_eq!(at(1001), Some(0.2));
        assert_eq!(charge_shake(7, 5, 500), None);
        assert_eq!(
            charge_shake(4, 5, 500).map(|shake| shake.duration),
            Some(100)
        );
    }

    #[test]
    fn a_kick_rises_and_returns() {
        assert_eq!(kick_fraction(-1), None);
        assert_eq!(kick_fraction(100), Some(0.5));
        assert_eq!(kick_fraction(200), Some(1.0));
        assert_eq!(kick_fraction(500), Some(0.5));
        assert_eq!(kick_fraction(800), None);
    }
}
