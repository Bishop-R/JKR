//! Bounded BaseJKA `PM_Weapon` prediction, including pistol/DEMP2/disruptor charge.
//!
//! Ordering follows `PmoveSingle` and `PM_Weapon` in
//! `codemp/game/bg_pmove.c:10503-10516,7138-7218,7242-7249,7383-7394,
//! 7608-7692`. Attack animation selection remains snapshot-driven; charge/zoom
//! and the three charged guns' fire events are predicted.

use crate::pmove_debug_melee::DebugMelee;
use jkr_protocol::UserCommand;

use crate::pmove::MovementState;
use crate::weapon_data::{LEGACY_WEAPON_COUNT, legacy_weapon_data};

#[path = "pmove_weapon_change.rs"]
mod change;

/// `PM_BeginWeaponChange`, for the saber's code.
pub(crate) fn begin_weapon_change(
    state: &mut MovementState,
    weapon: u8,
    lengths: &dyn crate::pmove_anim::AnimationLengths,
    events: &mut crate::predicted_events::PredictedEvents,
) {
    change::begin(state, weapon, Some(lengths), events);
}

/// `weaponstate_t::WEAPON_READY` (`bg_public.h:437-445`).
pub const WEAPON_READY: u8 = 0;
/// `weaponstate_t::WEAPON_RAISING` (`bg_public.h:437-445`).
pub const WEAPON_RAISING: u8 = 1;
/// `weaponstate_t::WEAPON_DROPPING` (`bg_public.h:437-445`).
pub const WEAPON_DROPPING: u8 = 2;
/// `weaponstate_t::WEAPON_FIRING` (`bg_public.h:437-445`).
pub const WEAPON_FIRING: u8 = 3;
/// `weaponstate_t::WEAPON_IDLE` (`bg_public.h:437-445`).
pub const WEAPON_IDLE: u8 = 6;

const WP_NONE: u8 = 0;
const WP_MELEE: u8 = 2;
const WP_SABER: u8 = 3;
const WP_BRYAR_PISTOL: u8 = 4;
/// `WP_BRYAR_OLD`: the siege classes' pistol, a Bryar that charges its alternate.
const WP_BRYAR_OLD: u8 = 16;
const WP_BLASTER: u8 = 5;
const WP_DISRUPTOR: u8 = 6;
const WP_BOWCASTER: u8 = 7;
const WP_REPEATER: u8 = 8;
const WP_DEMP2: u8 = 9;
const WP_FLECHETTE: u8 = 10;
const WP_ROCKET_LAUNCHER: u8 = 11;
const WP_THERMAL: u8 = 12;
const WP_TRIP_MINE: u8 = 13;
const WP_CONCUSSION: u8 = 15;
const WP_DET_PACK: u8 = 14;

const BUTTON_ATTACK: u16 = 1;
const BUTTON_USE_HOLDABLE: u16 = 4;
const BUTTON_ALT_ATTACK: u16 = 128;
const PMF_RESPAWNED: u16 = 512;
const TEAM_SPECTATOR: u8 = 3;
const PM_NOCLIP: u8 = 3;
const PM_INTERMISSION: u8 = 7;
const EF_FIRING: u32 = 1 << 9;
const EF_ALT_FIRING: u32 = 1 << 10;
const FP_RAGE: u32 = 1 << 8;
const MAX_CLIENTS: u16 = 32;
/// `BOTH_ATTACK4`, `BOTH_MELEE1`, `BOTH_MELEE2`.
const BOTH_ATTACK4: u16 = 116;
const BOTH_MELEE1: u16 = 122;
const BOTH_MELEE2: u16 = 123;
/// `WeaponAttackAnim` (`bg_misc.c:297-317`): the torso animation each weapon fires
/// with; the table lists seventeen and the two emplaced weapons read its zeros.
const WEAPON_ATTACK_ANIM: [u16; LEGACY_WEAPON_COUNT] = [
    113, 115, 115, 917, 114, 115, 115, 115, 115, 115, 115, 115, 125, 115, 115, 115, 114, 0, 0,
];

/// Whether this bounded port owns the current command's weapon result.
///
/// Other charged modes, saber specials, concussion and vehicles deliberately remain
/// authoritative until their complete branches are ported; the emplaced gun's weapon is
/// [`crate::pmove_emplaced`]'s.
/// The gates follow `bg_pmove.c:6760-6969,7009-7014,7143-7170,7221-7239`.
pub(crate) fn predicts_command(
    state: &MovementState,
    command: &UserCommand,
    has_animation_lengths: bool,
    debug_melee: DebugMelee,
) -> bool {
    if state.vehicle_entity_num != 0 {
        return false;
    }
    predicts_rider_command(state, command, has_animation_lengths, debug_melee)
}

/// [`predicts_command`] for a rider whose weapon the vehicle allows
/// ([`crate::pmove::riding`]): the same gates but the vehicle's.
pub(crate) fn predicts_rider_command(
    state: &MovementState,
    command: &UserCommand,
    has_animation_lengths: bool,
    debug_melee: DebugMelee,
) -> bool {
    // A player on an emplaced gun holding another weapon (the moment it takes the gun,
    // before the gun's think hands it the gun's) runs that weapon's code as anyone; the
    // gun's own weapon is `crate::pmove_emplaced`'s. The use key is the weapon code's
    // too (`crate::pmove_holdable`).
    if state.weapon == WP_SABER {
        // Forced-saber rules only pin the weapon; `PM_WeaponLightsaber`
        // still runs (`bg_pmove.c:6996-7013`).
        return crate::pmove_saber::can_predict(state, has_animation_lengths);
    }
    if state.forced_saber || !primary_is_predicted(state.weapon) {
        return false;
    }
    if command.buttons & BUTTON_ALT_ATTACK != 0
        && !alternate_is_predicted(state.weapon)
        && !melee_alternate_is_predicted(state, debug_melee, has_animation_lengths)
    {
        return false;
    }
    // A switch to the saber is predicted too: it ends in the saber's draw.
    state.weapon == command.weapon
        || primary_is_predicted(command.weapon)
        || command.weapon == WP_SABER
}

/// `bg_pmove.c:10518-10522`: the respawn latch cleared before `PM_Weapon` once attack and
/// use are let go, whatever the weapon.
pub(crate) fn clear_respawned(state: &mut MovementState, command: &UserCommand) {
    if state.health > 0 && command.buttons & (BUTTON_ATTACK | BUTTON_USE_HOLDABLE) == 0 {
        state.movement_flags &= !PMF_RESPAWNED;
    }
}

/// `PM_Weapon`'s spectator (`bg_pmove.c:7051`): a client on the spectators' team. An NPC's
/// `PERS_TEAM` is its NPC team, whose neutral shares the number and is no spectator.
fn spectator(state: &MovementState) -> bool {
    state.client_num < MAX_CLIENTS && state.team == TEAM_SPECTATOR
}

/// Apply the pre-`PmoveSingle` firing-flag portion of `PM_AdjustAttackStates`.
///
/// See `bg_pmove.c:8062-8070,8174-8197`. Disruptor command conversion runs
/// separately, after these flags have captured the raw input edge.
pub(crate) fn adjust_attack_flags(state: &mut MovementState, command: &UserCommand) {
    clear_respawned(state, command);
    let Some(data) = legacy_weapon_data(state.weapon) else {
        return;
    };
    let alternate = command.buttons & BUTTON_ALT_ATTACK != 0;
    let cost = if alternate {
        data.alternate_cost
    } else {
        data.primary_cost
    };
    let amount = if state.weapon == WP_DISRUPTOR && state.weapon_state == WEAPON_READY {
        if command.buttons & BUTTON_ATTACK == 0 {
            0 // PM_AdjustAttackStates :8146-8150: zooming consumes no ammo.
        } else if state.zoom_mode != 0 {
            state.ammo[data.ammo_index] - data.alternate_cost
        } else {
            state.ammo[data.ammo_index] - cost
        }
    } else {
        state.ammo[data.ammo_index] - cost
    };
    let firing = state.movement_flags & PMF_RESPAWNED == 0
        && state.movement_type != PM_INTERMISSION
        && state.movement_type != PM_NOCLIP
        && command.buttons & (BUTTON_ATTACK | BUTTON_ALT_ATTACK) != 0
        && (amount >= 0 || state.weapon == WP_SABER);
    if firing {
        state.entity_flags |= EF_FIRING;
        if alternate {
            state.entity_flags |= EF_ALT_FIRING;
        } else {
            state.entity_flags &= !EF_ALT_FIRING;
        }
    } else {
        state.entity_flags &= !(EF_FIRING | EF_ALT_FIRING);
    }
}

/// Advance the bounded `PM_Weapon` state machine by one pmove slice, with what the game
/// knows of the player (`context`) and the move's box (`bounds`, `pm->mins`/`maxs`).
/// Returns the command's buttons as the weapon's code left them (`pm->cmd.buttons`, which
/// the saber clears), for what `PmoveSingle` does after it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn advance_events_in(
    state: &mut MovementState,
    command: &UserCommand,
    millis: i32,
    animation_lengths: Option<&dyn crate::pmove_anim::AnimationLengths>,
    cancel_zoom: bool,
    events: &mut crate::predicted_events::PredictedEvents,
    collision: Option<&dyn crate::pmove::MovementCollision>,
    context: &crate::pmove::MoveContext,
    bounds: ([f32; 3], [f32; 3]),
    legacy_fixes: u32,
    debug_melee: DebugMelee,
    opponent: Option<crate::pmove_saber_lock::LockOpponent>,
    outcome: &mut crate::pmove_saber_lock::LockOutcome,
) -> u16 {
    let mut command = *command;
    advance_command(
        state,
        &mut command,
        millis,
        animation_lengths,
        cancel_zoom,
        events,
        collision,
        context,
        bounds,
        legacy_fixes,
        debug_melee,
        opponent,
        outcome,
    );
    command.buttons
}

#[allow(clippy::too_many_arguments)]
fn advance_command(
    state: &mut MovementState,
    command: &mut UserCommand,
    millis: i32,
    animation_lengths: Option<&dyn crate::pmove_anim::AnimationLengths>,
    cancel_zoom: bool,
    events: &mut crate::predicted_events::PredictedEvents,
    collision: Option<&dyn crate::pmove::MovementCollision>,
    context: &crate::pmove::MoveContext,
    bounds: ([f32; 3], [f32; 3]),
    legacy_fixes: u32,
    debug_melee: DebugMelee,
    opponent: Option<crate::pmove_saber_lock::LockOpponent>,
    outcome: &mut crate::pmove_saber_lock::LockOutcome,
) {
    // A pose the game holds the player in (`forceHandExtend`): the block comes before
    // the saber's own code and everything after.
    if crate::pmove_hand_extend::hand_extend(state, animation_lengths) {
        return;
    }
    // `bg_pmove.c:6971-6989`: a special jump, a roll (a get-up roll too) or a roll's
    // end keeps the weapon busy as long as the legs are.
    if crate::pmove_roll_anim::special_jump(state.legs_anim)
        || crate::pmove_roll::in_roll(state)
        || crate::pmove_roll::in_roll_complete(state)
    {
        if state.weapon_time < state.legs_timer {
            state.weapon_time = state.legs_timer;
        }
    }
    // A duel holds the saber (and, before it begins, the moves); a swing under way keeps
    // it (`bg_pmove.c:6991-7006`).
    if state.duel_in_progress {
        command.weapon = WP_SABER;
        state.weapon = WP_SABER;
        if state.duel_time >= command.server_time {
            (command.up_move, command.forward_move, command.right_move) = (0, 0, 0);
        }
    }
    if state.weapon == WP_SABER && state.saber_move != 1 && state.saber_move != 0 {
        command.weapon = WP_SABER;
    }
    if state.weapon == WP_SABER {
        if let Some(lengths) = animation_lengths {
            // `PM_WeaponLightsaber` (`bg_saber.c:2785-2862`): past a knockdown's or a roll's
            // wait, a saber lock comes first and is all the saber does.
            let rolling = crate::pmove_roll_anim::in_roll(state.legs_anim) && state.legs_timer > 0;
            let locked = !crate::pmove_hand_extend::in_knockdown(state.legs_anim, state.legs_timer)
                && !rolling
                && crate::pmove_saber_lock::head(
                    state,
                    command.server_time,
                    lengths,
                    events,
                    opponent,
                    outcome,
                );
            if !locked {
                let fixed_moves = legacy_fixes & 1 != 0;
                let mut saber = crate::pmove_lightsaber::Lightsaber {
                    state: &mut *state,
                    command: *command,
                    millis,
                    lengths,
                    events: &mut *events,
                    collision,
                    context,
                    bounds,
                    seed: command.server_time,
                    fixed_moves,
                };
                saber.run();
                command.buttons = saber.command.buttons;
            }
        }
        // `PM_WeaponLightsaber` does not return at once: the item code that follows
        // still runs, past the respawn, spectator and dead-player checks
        // (`bg_pmove.c:7009-7120`, `killAfterItem`).
        if state.movement_flags & PMF_RESPAWNED != 0 || spectator(state) {
            return;
        }
        if state.health <= 0 {
            state.weapon = WP_NONE;
            return;
        }
        crate::pmove_holdable::use_key(state, command, events, collision);
        return;
    }
    // `bg_pmove.c:7015-7018`: any other weapon but the emplaced gun's puts the saber away
    // as no longer held (the gun's is [`crate::pmove_emplaced`]'s).
    state.saber_holstered = 0;
    // `bg_pmove.c:7020-7041`: a thrown weapon's torso returns to its ready pose before
    // the throw's fire time is up — 200 ms before for a detonator, 700 for a mine or a
    // det pack.
    if state.vehicle_entity_num == 0
        && matches!(state.weapon, WP_THERMAL | WP_TRIP_MINE | WP_DET_PACK)
    {
        let early = if state.weapon == WP_THERMAL { 200 } else { 700 };
        if state.torso_anim == WEAPON_ATTACK_ANIM[usize::from(state.weapon)]
            && state.weapon_time - early <= 0
        {
            crate::pmove_anim::start_torso(
                state,
                crate::pmove_locomotion::weapon_ready_torso(state.weapon),
            );
        }
    }
    if state.movement_flags & PMF_RESPAWNED != 0 || spectator(state) || state.health <= 0 {
        if state.health <= 0 {
            state.weapon = WP_NONE;
        }
        return;
    }
    // The use key (`bg_pmove.c:7061-7120`).
    if crate::pmove_holdable::use_key(state, command, events, collision) {
        return;
    }

    // `bg_pmove.c:7138-7141`: retain the negative quantization remainder.
    if state.weapon_time > 0 {
        state.weapon_time -= millis;
    }
    let Some(data) = legacy_weapon_data(state.weapon) else {
        return;
    };

    // `bg_pmove.c:7172-7205`: this test runs even with no attack held.
    if state.weapon != WP_NONE
        && state.weapon == command.weapon
        && (state.weapon_time <= 0 || state.weapon_state != WEAPON_FIRING)
        && state.client_num < MAX_CLIENTS
    {
        let ammo = state.ammo[data.ammo_index];
        if ammo != -1
            && (ammo < data.primary_cost && ammo < data.alternate_cost
                || state.weapon == WP_DET_PACK && !state.has_detpack_planted && ammo < 1)
        {
            if matches!(state.weapon, WP_BRYAR_PISTOL | WP_DISRUPTOR | WP_DEMP2) {
                // bg_pmove.c:7185: preserve the predictable event sequence on dry fire.
                crate::pmove_weapon_charge::event(state, events, 25, 19 + u16::from(state.weapon));
            }
            if state.weapon_time < 500 {
                state.weapon_time += 500;
            }
            return;
        }
    }

    // `bg_pmove.c:7208-7215` permits a second request while raising.
    if (state.weapon_time <= 0 || state.weapon_state != WEAPON_FIRING)
        && state.weapon != command.weapon
    {
        change::begin(state, command.weapon, animation_lengths, events);
    }
    if state.weapon_time > 0 {
        return;
    }
    if crate::pmove_weapon_charge::blocks_fire(state, command, cancel_zoom, events) {
        return;
    }
    if state.weapon_state == WEAPON_DROPPING {
        // `PM_FinishWeaponChange`: the saber is drawn (`PM_SetSaberMove(LS_DRAW)`).
        if command.weapon == WP_SABER
            && state.weapons & (1 << WP_SABER) != 0
            && let Some(lengths) = animation_lengths
        {
            let fixed_moves = legacy_fixes & 1 != 0;
            let mut saber = crate::pmove_lightsaber::Lightsaber {
                state: &mut *state,
                command: *command,
                millis,
                lengths,
                events: &mut *events,
                collision,
                context,
                bounds,
                seed: command.server_time,
                fixed_moves,
            };
            saber.finish_weapon_change();
        } else {
            change::finish(state, command.weapon, animation_lengths);
        }
        return;
    }
    if state.weapon_state == WEAPON_RAISING {
        state.weapon_state = WEAPON_READY;
        change::raised(state, animation_lengths);
        return;
    }
    crate::pmove_locomotion::idle_torso(state, command);
    // `bg_pmove.c:7366-7381`: a weapon that is not the rocket launcher, ready, holds
    // no rocket lock (a vehicle's rider is told its by the vehicle).
    if state.weapon != WP_ROCKET_LAUNCHER && state.vehicle_entity_num == 0 {
        (
            state.rocket_lock_index,
            state.rocket_lock_time,
            state.rocket_target_time,
        ) = (jkr_protocol::ENTITY_NUMBER_NONE, 0.0, 0.0);
    }
    if crate::pmove_weapon_charge::charge(state, command, events, collision) {
        return;
    }
    if command.buttons & (BUTTON_ATTACK | BUTTON_ALT_ATTACK) == 0 {
        state.weapon_time = 0;
        state.weapon_state = WEAPON_READY;
        return;
    }
    // bg_pmove.c:7441-7453: alt cannot shoot during unlocked/binocular zoom.
    if state.weapon == WP_DISRUPTOR
        && command.buttons & BUTTON_ALT_ATTACK != 0
        && (!state.zoom_locked || state.zoom_mode == 2)
    {
        return;
    }

    let Some(data) = legacy_weapon_data(state.weapon) else {
        return;
    };
    let alternate = command.buttons & BUTTON_ALT_ATTACK != 0;
    let amount = if alternate {
        data.alternate_cost
    } else {
        data.primary_cost
    };
    // The attack's torso animation (`bg_pmove.c:7455-7606`): the scoped disruptor's,
    // melee's (on foot only: `g_debugMelee`'s grapple and kicks, else alternating punches
    // whose running timer becomes the weapon's time), or the weapon's own from
    // `WeaponAttackAnim`.
    if state.weapon == WP_DISRUPTOR && state.zoom_mode == 1 {
        crate::pmove_anim::start_torso(state, BOTH_ATTACK4);
    } else if state.weapon == WP_MELEE {
        if state.vehicle_entity_num == 0 {
            if debug_melee.melee_moves()
                && let crate::pmove_debug_melee::MeleeMove::Done = crate::pmove_debug_melee::melee(
                    state,
                    command,
                    debug_melee,
                    animation_lengths,
                    collision,
                    bounds,
                    legacy_fixes & 1 != 0,
                )
            {
                return;
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
        }
    } else {
        crate::pmove_anim::start_torso(state, WEAPON_ATTACK_ANIM[usize::from(state.weapon)]);
    }
    state.weapon_state = WEAPON_FIRING;
    if state.client_num < MAX_CLIENTS && state.ammo[data.ammo_index] != -1 {
        if state.ammo[data.ammo_index] < amount {
            if matches!(state.weapon, WP_BRYAR_PISTOL | WP_DISRUPTOR | WP_DEMP2) {
                // bg_pmove.c:7624-7627.
                crate::pmove_weapon_charge::event(state, events, 25, 19 + u16::from(state.weapon));
            }
            if state.weapon_time < 500 {
                state.weapon_time += 500;
            }
            return;
        }
        state.ammo[data.ammo_index] -= amount;
    }
    let alternate = alternate && !(state.weapon == WP_DISRUPTOR && state.zoom_mode != 1);
    // :7637-7675: `EV_FIRE_WEAPON` or `EV_ALT_FIRE` for every weapon (the game fires on
    // it), except melee on a vehicle.
    if state.weapon != WP_MELEE || state.vehicle_entity_num == 0 {
        crate::pmove_weapon_charge::event(state, events, if alternate { 28 } else { 27 }, 0);
    }
    let mut add_time = if alternate {
        data.alternate_time
    } else {
        data.primary_time
    };
    if state.force_powers_active & FP_RAGE != 0 {
        add_time = (add_time as f32 * 0.75) as i32;
    } else if state.force_rage_recovery_time > command.server_time {
        add_time = (add_time as f32 * 1.5) as i32;
    }
    state.weapon_time += add_time;
}

/// Melee's alternate attack on foot (`bg_pmove.c:7461-7581`): the punches with
/// `g_debugMelee` off, as the primary attack's; with it on, the grapple, which a client
/// does not predict past returning, or a kick, which needs the animation table.
fn melee_alternate_is_predicted(
    state: &MovementState,
    debug_melee: DebugMelee,
    has_animation_lengths: bool,
) -> bool {
    state.weapon == WP_MELEE
        && state.vehicle_entity_num == 0
        && (!debug_melee.melee_moves() || has_animation_lengths)
}

fn primary_is_predicted(weapon: u8) -> bool {
    // `WP_NONE` is a dead player's: `PM_AdjustAttackStates` still sets its firing flag.
    matches!(
        weapon,
        WP_NONE
            | 1
            | WP_MELEE
            | WP_BRYAR_PISTOL
            | WP_BLASTER
            | WP_DISRUPTOR
            | WP_BOWCASTER
            | WP_REPEATER
            | WP_DEMP2
            | WP_FLECHETTE
            | WP_ROCKET_LAUNCHER
            | WP_THERMAL
            | WP_TRIP_MINE
            | WP_DET_PACK
            | WP_CONCUSSION
            | WP_BRYAR_OLD
    )
}

fn alternate_is_predicted(weapon: u8) -> bool {
    matches!(
        weapon,
        WP_BLASTER
            | WP_REPEATER
            | WP_FLECHETTE
            | WP_BRYAR_PISTOL
            | WP_DEMP2
            | WP_DISRUPTOR
            | WP_BOWCASTER
            | WP_ROCKET_LAUNCHER
            | WP_THERMAL
            | WP_TRIP_MINE
            | WP_DET_PACK
            | WP_CONCUSSION
            | WP_BRYAR_OLD
    )
}
