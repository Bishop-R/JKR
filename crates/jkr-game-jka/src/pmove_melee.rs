//! Melee's attack animation (`bg_pmove.c:7458-7600`): nothing at all on a vehicle; under
//! `g_debugMelee`, both attack buttons grab (the server's `TryGrapple`, `g_cmds.c:3230-3273`,
//! which a client does not predict) and the alternate one kicks the way the player moves;
//! else the punches alternate, the punch's running time the weapon's. The grab's hold is
//! the game's ([`crate::debug_melee`]).

use crate::pmove::{MovementCollision, MovementConfig, MovementState};
use crate::pmove_anim::{
    AnimationLengths, SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, set_animation,
};
use crate::saber_move_data::movement::{
    LS_KICK_B, LS_KICK_B_AIR, LS_KICK_F, LS_KICK_F_AIR, LS_KICK_L, LS_KICK_L_AIR, LS_KICK_R,
    LS_KICK_R_AIR,
};
use crate::saber_rules as rules;
use jkr_protocol::{ENTITY_NUMBER_NONE, UserCommand};

const BUTTON_ATTACK: u16 = 1;
const BUTTON_ALT_ATTACK: u16 = 128;
/// `BOTH_MELEE1`, `BOTH_MELEE2`: the punches.
const BOTH_MELEE1: u16 = 122;
const BOTH_MELEE2: u16 = 123;
/// `HANDEXTEND_NONE`.
const HANDEXTEND_NONE: u8 = 0;

/// What a mover's `Pmove` knows of `g_debugMelee`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeleeRules {
    /// `pm->debugMelee`.
    pub debug_melee: bool,
    /// The game's `grappleState`: already in a grapple.
    pub grappling: bool,
    /// The server's own move, which tries the grab (`#ifdef _GAME`); a client's returns.
    pub authoritative: bool,
}

impl MeleeRules {
    /// The rules a movement's configuration carries.
    pub fn of(config: &MovementConfig) -> Self {
        Self {
            debug_melee: config.debug_melee,
            grappling: config.grappling,
            authoritative: config.authoritative,
        }
    }
}

/// Melee's attack animation. Returns whether `PM_Weapon` returns there; otherwise it
/// fires the weapon on. `bounds` is the move's box, `fixed_moves` `BG_FixSaberMoveData`'s
/// fix.
pub(crate) fn attack(
    state: &mut MovementState,
    command: &mut UserCommand,
    rules: MeleeRules,
    lengths: Option<&dyn AnimationLengths>,
    collision: Option<&dyn MovementCollision>,
    bounds: ([f32; 3], [f32; 3]),
    fixed_moves: bool,
) -> bool {
    // "if riding a vehicle don't do this stuff at all".
    if state.vehicle_entity_num != 0 {
        return false;
    }
    let alternate = command.buttons & BUTTON_ALT_ATTACK != 0;
    if rules.debug_melee && alternate && command.buttons & BUTTON_ATTACK != 0 {
        // "ok, grapple time": a client predicts nothing of it; a grab the server cannot
        // begin fires the weapon without an animation.
        return !rules.authoritative || try_grapple(state, rules.grappling, lengths);
    }
    if rules.debug_melee && alternate {
        kick(state, command, lengths, collision, bounds, fixed_moves);
        return true;
    }
    let punch = if state.torso_anim == BOTH_MELEE1 {
        BOTH_MELEE2
    } else {
        BOTH_MELEE1
    };
    crate::pmove_anim::start_torso(state, punch);
    if state.torso_anim == punch {
        state.weapon_time = state.torso_timer;
    }
    false
}

/// `TryGrapple` from melee's attack: the hand out to grab (`BOTH_KYLE_GRAB`, half a second
/// longer than it runs), the weapon busy as long; not with the weapon busy, a hand pose
/// held (`forceHandExtend`) or a grapple already on. Its saber half — a lit blade put
/// away first — is never reached from here.
fn try_grapple(
    state: &mut MovementState,
    grappling: bool,
    lengths: Option<&dyn AnimationLengths>,
) -> bool {
    if state.weapon_time > 0 || state.force_hand_extend != HANDEXTEND_NONE || grappling {
        return false;
    }
    let Some(lengths) = lengths else {
        return false;
    };
    set_animation(
        state,
        SETANIM_BOTH,
        crate::debug_melee::BOTH_KYLE_GRAB,
        SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        lengths,
    );
    if state.torso_anim != crate::debug_melee::BOTH_KYLE_GRAB {
        return false;
    }
    state.torso_timer += 500;
    if state.legs_anim == state.torso_anim {
        state.legs_timer = state.torso_timer;
    }
    state.weapon_time = state.torso_timer;
    state.grab_started = true;
    true
}

/// `g_debugMelee`'s kick (`bg_pmove.c:7512-7570`): not while kicking, the way the player
/// moves (`PM_KickMoveForConditions`, the move spent on it), in the air only well above
/// the ground — more than 64 units and more than its fall speed less 64, not mid-flip —
/// and then as the air kick. No kick puts the torso on the legs' animation; either way
/// the weapon is free when it ends.
fn kick(
    state: &mut MovementState,
    command: &mut UserCommand,
    lengths: Option<&dyn AnimationLengths>,
    collision: Option<&dyn MovementCollision>,
    bounds: ([f32; 3], [f32; 3]),
    fixed_moves: bool,
) {
    if !rules::kicking(state.torso_anim) && !rules::kicking(state.legs_anim) {
        let mut kick = crate::pmove_saber_attack::kick_for_conditions(command);
        if let Some(found) = kick
            && state.ground_entity_number == ENTITY_NUMBER_NONE
        {
            let ground =
                crate::pmove_saber_attack::ground_distance_below(state.origin, bounds, collision);
            kick = ((!rules::flipping(state.legs_anim) || state.legs_timer <= 0)
                && ground > 64.0
                && ground > -state.velocity[2] - 64.0)
                .then(|| air_kick(found))
                .flatten();
        }
        if let (Some(kick), Some(lengths)) = (kick, lengths) {
            let animation = crate::saber_move_data::move_animation(kick, fixed_moves);
            set_animation(
                state,
                SETANIM_BOTH,
                animation,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                lengths,
            );
            if state.legs_anim == animation {
                state.weapon_time = state.legs_timer;
                return;
            }
        }
    }
    if state.torso_anim != state.legs_anim
        && let Some(lengths) = lengths
    {
        let legs = state.legs_anim;
        set_animation(
            state,
            SETANIM_BOTH,
            legs,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            lengths,
        );
    }
    state.weapon_time = 0;
}

/// The air kick of a ground kick; none for any other move.
fn air_kick(kick: u16) -> Option<u16> {
    match kick {
        LS_KICK_F => Some(LS_KICK_F_AIR),
        LS_KICK_B => Some(LS_KICK_B_AIR),
        LS_KICK_R => Some(LS_KICK_R_AIR),
        LS_KICK_L => Some(LS_KICK_L_AIR),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pmove_anim::{AnimationLengthTable, AnimationTiming};

    const WP_MELEE: u8 = 2;

    fn lengths() -> AnimationLengthTable {
        AnimationLengthTable::new((0..1_600).map(|_| AnimationTiming {
            frame_count: 10,
            frame_lerp_ms: 50,
        }))
    }

    fn fists() -> MovementState {
        MovementState {
            weapon: WP_MELEE,
            ground_entity_number: 0,
            ..MovementState::default()
        }
    }

    fn rules(authoritative: bool) -> MeleeRules {
        MeleeRules {
            debug_melee: true,
            grappling: false,
            authoritative,
        }
    }

    fn command(buttons: u16, forward_move: i8) -> UserCommand {
        UserCommand {
            buttons,
            forward_move,
            ..UserCommand::default()
        }
    }

    #[test]
    fn both_buttons_grab_on_the_server_only() {
        let lengths = lengths();
        let both = BUTTON_ATTACK | BUTTON_ALT_ATTACK;
        // A client predicts nothing and returns.
        let mut state = fists();
        assert!(attack(
            &mut state,
            &mut command(both, 0),
            rules(false),
            Some(&lengths),
            None,
            ([0.0; 3], [0.0; 3]),
            true,
        ));
        assert_eq!(state.torso_anim, 0);
        // The server reaches out, half a second past the animation's 500 ms.
        assert!(attack(
            &mut state,
            &mut command(both, 0),
            rules(true),
            Some(&lengths),
            None,
            ([0.0; 3], [0.0; 3]),
            true,
        ));
        assert_eq!(state.torso_anim, crate::debug_melee::BOTH_KYLE_GRAB);
        assert_eq!((state.torso_timer, state.legs_timer), (1_000, 1_000));
        assert_eq!(state.weapon_time, 1_000);
        assert!(state.grab_started);
        // Already grappling: no grab, and the weapon fires on.
        let mut state = fists();
        let grappling = MeleeRules {
            grappling: true,
            ..rules(true)
        };
        assert!(!attack(
            &mut state,
            &mut command(both, 0),
            grappling,
            Some(&lengths),
            None,
            ([0.0; 3], [0.0; 3]),
            true,
        ));
        assert_eq!(state.torso_anim, 0);
    }

    #[test]
    fn the_alternate_button_kicks_the_way_the_player_moves() {
        let lengths = lengths();
        let mut state = fists();
        let mut forward = command(BUTTON_ALT_ATTACK, 127);
        assert!(attack(
            &mut state,
            &mut forward,
            rules(true),
            Some(&lengths),
            None,
            ([0.0; 3], [0.0; 3]),
            true,
        ));
        let kick = crate::saber_move_data::move_animation(LS_KICK_F, true);
        assert_eq!((state.legs_anim, state.torso_anim), (kick, kick));
        assert_eq!(state.weapon_time, state.legs_timer);
        assert_eq!(forward.forward_move, 0);
        // Standing still: no kick, the weapon free.
        let mut state = fists();
        state.weapon_time = 30;
        assert!(attack(
            &mut state,
            &mut command(BUTTON_ALT_ATTACK, 0),
            rules(true),
            Some(&lengths),
            None,
            ([0.0; 3], [0.0; 3]),
            true,
        ));
        assert_eq!(state.weapon_time, 0);
        // In the air without a world below: far enough for the air kick.
        let mut state = MovementState {
            ground_entity_number: ENTITY_NUMBER_NONE,
            ..fists()
        };
        assert!(attack(
            &mut state,
            &mut command(BUTTON_ALT_ATTACK, -127),
            rules(true),
            Some(&lengths),
            None,
            ([0.0; 3], [0.0; 3]),
            true,
        ));
        assert_eq!(
            state.legs_anim,
            crate::saber_move_data::move_animation(LS_KICK_B_AIR, true)
        );
    }

    #[test]
    fn without_the_cvar_the_fists_punch_and_on_a_vehicle_nothing() {
        let lengths = lengths();
        let mut state = fists();
        let stock = MeleeRules::default();
        assert!(!attack(
            &mut state,
            &mut command(BUTTON_ALT_ATTACK, 127),
            stock,
            Some(&lengths),
            None,
            ([0.0; 3], [0.0; 3]),
            true,
        ));
        assert_eq!(state.torso_anim, BOTH_MELEE1);
        let mut riding = MovementState {
            vehicle_entity_num: 40,
            ..fists()
        };
        assert!(!attack(
            &mut riding,
            &mut command(BUTTON_ATTACK, 0),
            stock,
            Some(&lengths),
            None,
            ([0.0; 3], [0.0; 3]),
            true,
        ));
        assert_eq!(riding.torso_anim, 0);
    }
}
