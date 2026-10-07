//! Whether stock's client draws the view from behind the player (`CG_DrawActiveFrame`,
//! OpenJK `codemp/cgame/cg_view.c:2571-2611`): when `cg_thirdPerson` asks or the player is
//! dead; whatever the cvar says, while alive at an emplaced gun or an E-Web, holding a saber
//! or bare fists, in a grapple, knocked down, falling to its death or on a vehicle; never
//! while zoomed (and none of those), nor for a spectator.

use jkr_game_jka::pmove_hand_extend::in_knockdown;

/// `WP_MELEE`, `WP_SABER`, `WP_EMPLACED_GUN`.
const WP_MELEE: u8 = 2;
const WP_SABER: u8 = 3;
const WP_EMPLACED_GUN: u8 = 17;
/// `HANDEXTEND_KNOCKDOWN`, `PM_SPECTATOR`, `TEAM_SPECTATOR`.
const HANDEXTEND_KNOCKDOWN: u8 = 8;
const PM_SPECTATOR: u8 = 4;
const TEAM_SPECTATOR: u8 = 3;
/// `BG_InGrappleMove`'s animations (`bg_panimate.c:691-709`): grabbing, beating and
/// being beaten.
const BOTH_KYLE_GRAB: u16 = 1_282;
const BOTH_KYLE_MISS: u16 = 1_283;
const BOTH_KYLE_PA_1: u16 = 1_284;
const BOTH_PLAYER_PA_1: u16 = 1_285;
const BOTH_KYLE_PA_2: u16 = 1_286;
const BOTH_PLAYER_PA_2: u16 = 1_287;
const BOTH_PLAYER_PA_FLY: u16 = 1_288;

/// What the choice reads: health, team and zoom from the snapshot (`cg.snap->ps`), the
/// rest from the predicted state (`cg.predictedPlayerState`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewInputs {
    pub snapshot_health: i32,
    pub snapshot_team: u8,
    pub snapshot_zoom_mode: u8,
    pub movement_type: u8,
    pub weapon: u8,
    pub emplaced_index: u16,
    pub torso_anim: u16,
    pub legs_anim: u16,
    pub legs_timer: i32,
    pub force_hand_extend: u8,
    pub falling_to_death: bool,
    pub vehicle: u16,
}

/// `cg.renderingThirdPerson` for `cg_thirdPerson` and the player as `view` holds it.
pub fn rendering_third_person(cvar: bool, view: &ViewInputs) -> bool {
    let mut third = cvar || view.snapshot_health <= 0;
    if view.snapshot_health > 0 {
        if view.weapon == WP_EMPLACED_GUN && view.emplaced_index != 0 {
            third = true;
        } else if view.weapon == WP_SABER
            || view.weapon == WP_MELEE
            || in_grapple(view.torso_anim)
            || in_grapple(view.legs_anim)
            || view.force_hand_extend == HANDEXTEND_KNOCKDOWN
            || view.falling_to_death
            || view.vehicle != 0
            || in_knockdown(view.legs_anim, view.legs_timer)
        {
            third = true;
        } else if view.snapshot_zoom_mode != 0 {
            third = false;
        }
    }
    if view.movement_type == PM_SPECTATOR || view.snapshot_team == TEAM_SPECTATOR {
        third = false;
    }
    third
}

/// `BG_InGrappleMove`.
fn in_grapple(anim: u16) -> bool {
    matches!(
        anim,
        BOTH_KYLE_GRAB
            | BOTH_KYLE_MISS
            | BOTH_KYLE_PA_1
            | BOTH_KYLE_PA_2
            | BOTH_PLAYER_PA_1
            | BOTH_PLAYER_PA_2
            | BOTH_PLAYER_PA_FLY
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alive(weapon: u8) -> ViewInputs {
        ViewInputs {
            snapshot_health: 100,
            snapshot_team: 0,
            weapon,
            ..ViewInputs::default()
        }
    }

    #[test]
    fn the_cvar_chooses_unless_the_player_is_dead_armed_with_a_blade_or_spectating() {
        let gun = alive(5);
        assert!(!rendering_third_person(false, &gun));
        assert!(rendering_third_person(true, &gun));
        // A saber, fists or an emplaced gun force it; a zoomed gun forbids it.
        assert!(rendering_third_person(false, &alive(WP_SABER)));
        assert!(rendering_third_person(false, &alive(WP_MELEE)));
        let emplaced = ViewInputs {
            emplaced_index: 40,
            ..alive(WP_EMPLACED_GUN)
        };
        assert!(rendering_third_person(false, &emplaced));
        let zoomed = ViewInputs {
            snapshot_zoom_mode: 1,
            ..alive(6)
        };
        assert!(!rendering_third_person(true, &zoomed));
        // Dead: behind the body; spectating: never.
        let dead = ViewInputs {
            snapshot_health: 0,
            ..gun
        };
        assert!(rendering_third_person(false, &dead));
        let spectator = ViewInputs {
            movement_type: PM_SPECTATOR,
            ..alive(WP_SABER)
        };
        assert!(!rendering_third_person(true, &spectator));
        // Grappled, or lying in a knockdown.
        let grappled = ViewInputs {
            torso_anim: BOTH_PLAYER_PA_1,
            ..gun
        };
        assert!(rendering_third_person(false, &grappled));
        let down = ViewInputs {
            legs_anim: 1_219,
            ..gun
        };
        assert!(rendering_third_person(false, &down));
    }

    #[test]
    fn the_grapple_numbers_are_the_animation_tables() {
        for (number, name) in [
            (BOTH_KYLE_GRAB, "BOTH_KYLE_GRAB"),
            (BOTH_KYLE_MISS, "BOTH_KYLE_MISS"),
            (BOTH_KYLE_PA_1, "BOTH_KYLE_PA_1"),
            (BOTH_PLAYER_PA_1, "BOTH_PLAYER_PA_1"),
            (BOTH_KYLE_PA_2, "BOTH_KYLE_PA_2"),
            (BOTH_PLAYER_PA_2, "BOTH_PLAYER_PA_2"),
            (BOTH_PLAYER_PA_FLY, "BOTH_PLAYER_PA_FLY"),
        ] {
            assert_eq!(
                jkr_game_jka::legacy_animation_name(usize::from(number)),
                Some(name)
            );
        }
    }
}
