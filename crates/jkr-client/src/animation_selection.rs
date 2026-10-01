//! BaseJKA humanoid animation selection compatibility.
//!
//! The selection path mirrors OpenJK `codemp/cgame/cg_players.c`:
//! `CG_PlayerAnimation` passes `entityState.legsAnim` and `torsoAnim` directly
//! to `CG_RunLerpFrame`; `legsFlip`/`torsoFlip` force a restart without changing
//! the clip; and `forceFrame` freezes both tracks. `CG_SetLerpFrameAnimation`
//! applies locomotion/rage speed scaling, saber transition scaling, synchronized
//! torso/legs starts, and death/flip blend rules. For the predicted local player,
//! `codemp/game/bg_misc.c::BG_PlayerStateToEntityState` copies the same animation
//! and flip fields before cgame presents it.
//!
//! `playerState.legsTimer` and `torsoTimer` are game-side guards that decide
//! when bg_pmove may select a new animation. They are not copied into
//! `entityState` and are not consulted by `CG_PlayerAnimation`; presentation
//! therefore selects the declared clips rather than deriving clips from timers.
//! View pitch is likewise a bone-angle contribution in `CG_G2PlayerAngles`, not
//! an animation-clip selection rule.

use jkr_protocol::{EntityState, PlayerState};
use jkr_runtime::{AnimationState, AnimationTrackInput, EntityId, World};

use crate::pmove::MovementState;
/// Fully decoded input to codemp's two-track player animation selector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LegacyAnimationSelection {
    lower: LegacyTrackSelection,
    upper: LegacyTrackSelection,
    forced_frame: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LegacyTrackSelection {
    clip: usize,
    revision: u64,
    speed_milli: u16,
}

impl LegacyAnimationSelection {
    /// Decode an on-wire entity exactly as `CG_PlayerAnimation` consumes it.
    pub(crate) fn from_entity(state: &EntityState) -> Self {
        let lower_clip = usize::from(state.leg_animation());
        let upper_clip = usize::from(state.torso_animation());
        Self {
            lower: LegacyTrackSelection {
                clip: lower_clip,
                revision: animation_revision(state.leg_animation(), state.leg_flip()),
                speed_milli: legacy_animation_speed(
                    lower_clip,
                    state.force_powers_active(),
                    false,
                    state.fire_flag(),
                    state.broken_limbs(),
                ),
            },
            upper: LegacyTrackSelection {
                clip: upper_clip,
                revision: animation_revision(state.torso_animation(), state.torso_flip()),
                speed_milli: legacy_animation_speed(
                    upper_clip,
                    state.force_powers_active(),
                    true,
                    state.fire_flag(),
                    state.broken_limbs(),
                ),
            },
            forced_frame: (state.force_frame() != 0).then_some(usize::from(state.force_frame())),
        }
    }

    /// Decode the local player fields copied by `BG_PlayerStateToEntityState`.
    pub(crate) fn from_player(state: &PlayerState) -> Self {
        let lower_clip = usize::from(state.leg_animation());
        let upper_clip = usize::from(state.torso_animation());
        Self {
            lower: LegacyTrackSelection {
                clip: lower_clip,
                revision: animation_revision(state.leg_animation(), state.leg_flip()),
                speed_milli: legacy_animation_speed(
                    lower_clip,
                    state.force_powers_active(),
                    false,
                    state.saber_style(),
                    state.broken_limbs(),
                ),
            },
            upper: LegacyTrackSelection {
                clip: upper_clip,
                revision: animation_revision(state.torso_animation(), state.torso_flip()),
                speed_milli: legacy_animation_speed(
                    upper_clip,
                    state.force_powers_active(),
                    true,
                    state.saber_style(),
                    state.broken_limbs(),
                ),
            },
            forced_frame: None,
        }
    }

    /// Decode the predicted fields copied by `BG_PlayerStateToEntityState`.
    fn from_movement(state: &MovementState) -> Self {
        let lower_clip = usize::from(state.legs_anim);
        let upper_clip = usize::from(state.torso_anim);
        Self {
            lower: LegacyTrackSelection {
                clip: lower_clip,
                revision: animation_revision(state.legs_anim, state.legs_flip),
                speed_milli: legacy_animation_speed(
                    lower_clip,
                    state.force_powers_active,
                    false,
                    state.saber_anim_level,
                    state.broken_limbs,
                ),
            },
            upper: LegacyTrackSelection {
                clip: upper_clip,
                revision: animation_revision(state.torso_anim, state.torso_flip),
                speed_milli: legacy_animation_speed(
                    upper_clip,
                    state.force_powers_active,
                    true,
                    state.saber_anim_level,
                    state.broken_limbs,
                ),
            },
            forced_frame: None,
        }
    }

    /// Apply both selected tracks at the snapshot's cgame presentation time.
    pub(crate) fn apply(self, world: &mut World, entity_id: EntityId, time_millis: i64) {
        let previous = world
            .entity(entity_id)
            .and_then(|entity| entity.animation());
        trace_selection(entity_id, previous, self);
        world.set_animation(
            entity_id,
            animation_track(
                self.lower,
                previous.map(|animation| animation.lower.clip),
                self.forced_frame,
                synchronized_phase(previous, self.lower.clip, false, time_millis),
            ),
            animation_track(
                self.upper,
                previous.map(|animation| animation.upper.clip),
                self.forced_frame,
                synchronized_phase(previous, self.upper.clip, true, time_millis),
            ),
            time_millis,
        );
    }
}

/// Build renderer-neutral animation tracks from `cg.predictedPlayerState`.
///
/// OpenJK copies `legsAnim`, `torsoAnim`, `legsFlip`, and `torsoFlip` in
/// `BG_PlayerStateToEntityState` (`codemp/game/bg_misc.c:2762-2830`) before
/// presenting the local entity (`codemp/cgame/cg_ents.c:3457`).
pub fn legacy_predicted_animation_inputs(
    state: &MovementState,
    previous: Option<AnimationState>,
    command_time: i64,
) -> [AnimationTrackInput; 2] {
    let selection = LegacyAnimationSelection::from_movement(state);
    [
        animation_track(
            selection.lower,
            previous.map(|animation| animation.lower.clip),
            None,
            synchronized_phase(previous, selection.lower.clip, false, command_time),
        ),
        animation_track(
            selection.upper,
            previous.map(|animation| animation.upper.clip),
            None,
            synchronized_phase(previous, selection.upper.clip, true, command_time),
        ),
    ]
}

fn animation_revision(animation: u16, flip: bool) -> u64 {
    u64::from(animation) | (u64::from(flip) << 16)
}

/// Mirror `CG_SetLerpFrameAnimation`'s synchronized lower/upper start rule.
fn synchronized_phase(
    previous: Option<AnimationState>,
    clip: usize,
    upper: bool,
    time_millis: i64,
) -> Option<i64> {
    let other = previous.map(|animation| {
        if upper {
            animation.lower
        } else {
            animation.upper
        }
    })?;
    (other.clip == clip).then(|| other.elapsed_millis(time_millis))
}

/// Mirror `CG_SetLerpFrameAnimation` death/flip blend selection.
fn animation_track(
    selection: LegacyTrackSelection,
    previous_clip: Option<usize>,
    forced_frame: Option<usize>,
    resume_phase_millis: Option<i64>,
) -> AnimationTrackInput {
    let blend_millis = legacy_blend_millis(selection.clip, previous_clip, forced_frame.is_some());
    AnimationTrackInput {
        clip: selection.clip,
        revision: selection.revision,
        speed_milli: selection.speed_milli,
        blend_millis,
        forced_frame,
        resume_phase_millis,
    }
}

pub(crate) fn legacy_blend_millis(
    clip: usize,
    previous_clip: Option<usize>,
    forced_frame: bool,
) -> u16 {
    if forced_frame {
        150
    } else if death_animation(clip) || previous_clip.is_some_and(death_animation) {
        0
    } else if flipping_animation(clip) || previous_clip.is_some_and(flipping_animation) {
        200
    } else {
        100
    }
}

/// Mirror `CG_PlayerAnimation` and `BG_SaberStartTransAnim` speed rules.
fn legacy_animation_speed(
    clip: usize,
    force_powers_active: u32,
    torso: bool,
    saber_style: u8,
    broken_limbs: u8,
) -> u16 {
    const FP_SPEED: u32 = 1 << 2;
    const FP_RAGE: u32 = 1 << 8;
    let mut speed = if torso {
        if force_powers_active & FP_RAGE != 0 {
            1_700
        } else {
            1_000
        }
    } else if !locomotion_animation(clip) {
        1_000
    } else if force_powers_active & FP_RAGE != 0 {
        1_300
    } else if force_powers_active & FP_SPEED != 0 {
        1_700
    } else {
        1_000
    };

    let saber_transition = animation_in_range(clip, "BOTH_T1_BR__R", "BOTH_T1_BL_TL")
        || animation_in_range(clip, "BOTH_T2_BR__R", "BOTH_T2_BL_TL")
        || animation_in_range(clip, "BOTH_T3_BR__R", "BOTH_T3_BL_TL");
    if saber_transition {
        speed = match saber_style {
            1 => speed * 3 / 2,
            3 => speed * 3 / 4,
            _ => speed,
        };
    }
    let saber_animation = animation_in_range(clip, "BOTH_A1_T__B_", "BOTH_H1_S1_BR");
    if saber_transition || saber_animation {
        const BROKEN_LEFT_ARM: u8 = 1 << 1;
        const BROKEN_RIGHT_ARM: u8 = 1 << 2;
        if broken_limbs & BROKEN_RIGHT_ARM != 0 {
            speed /= 2;
        } else if broken_limbs & BROKEN_LEFT_ARM != 0 {
            speed = speed * 65 / 100;
        }
    }
    speed
}

fn animation_in_range(clip: usize, first: &str, last: &str) -> bool {
    let first = crate::legacy_animation::NAMES
        .iter()
        .position(|name| *name == first);
    let last = crate::legacy_animation::NAMES
        .iter()
        .position(|name| *name == last);
    matches!((first, last), (Some(first), Some(last)) if (first..=last).contains(&clip))
}

fn locomotion_animation(clip: usize) -> bool {
    matches!(
        crate::legacy_animation_name(clip),
        Some(
            "BOTH_WALK1"
                | "BOTH_WALK2"
                | "BOTH_WALK_STAFF"
                | "BOTH_WALK_DUAL"
                | "BOTH_WALK5"
                | "BOTH_WALK6"
                | "BOTH_WALK7"
                | "BOTH_WALKBACK1"
                | "BOTH_WALKBACK2"
                | "BOTH_WALKBACK_STAFF"
                | "BOTH_WALKBACK_DUAL"
                | "BOTH_RUN1"
                | "BOTH_RUN2"
                | "BOTH_RUN_STAFF"
                | "BOTH_RUN_DUAL"
                | "BOTH_RUNBACK1"
                | "BOTH_RUNBACK2"
                | "BOTH_RUNBACK_STAFF"
                | "BOTH_RUNBACK_DUAL"
                | "BOTH_RUN1START"
                | "BOTH_RUN1STOP"
                | "BOTH_RUNSTRAFE_LEFT1"
                | "BOTH_RUNSTRAFE_RIGHT1"
                | "BOTH_RUN4"
        )
    )
}

pub(crate) fn death_animation(clip: usize) -> bool {
    let Some(name) = crate::legacy_animation_name(clip) else {
        return false;
    };
    name.starts_with("BOTH_DEATH")
        || name.starts_with("BOTH_DEAD")
        || matches!(
            name,
            "BOTH_LYINGDEATH1"
                | "BOTH_STUMBLEDEATH1"
                | "BOTH_FALLDEATH1"
                | "BOTH_FALLDEATH1INAIR"
                | "BOTH_FALLDEATH1LAND"
                | "BOTH_LYINGDEAD1"
                | "BOTH_STUMBLEDEAD1"
                | "BOTH_FALLDEAD1LAND"
                | "BOTH_DISMEMBER_HEAD1"
                | "BOTH_DISMEMBER_TORSO1"
                | "BOTH_DISMEMBER_LLEG"
                | "BOTH_DISMEMBER_RLEG"
                | "BOTH_DISMEMBER_RARM"
                | "BOTH_DISMEMBER_LARM"
        )
}

fn flipping_animation(clip: usize) -> bool {
    matches!(
        crate::legacy_animation_name(clip),
        Some(
            "BOTH_FLIP_F"
                | "BOTH_FLIP_B"
                | "BOTH_FLIP_L"
                | "BOTH_FLIP_R"
                | "BOTH_WALL_RUN_RIGHT_FLIP"
                | "BOTH_WALL_RUN_LEFT_FLIP"
                | "BOTH_WALL_FLIP_RIGHT"
                | "BOTH_WALL_FLIP_LEFT"
                | "BOTH_FLIP_BACK1"
                | "BOTH_FLIP_BACK2"
                | "BOTH_FLIP_BACK3"
                | "BOTH_WALL_FLIP_BACK1"
                | "BOTH_WALL_RUN_RIGHT"
                | "BOTH_WALL_RUN_LEFT"
                | "BOTH_WALL_RUN_RIGHT_STOP"
                | "BOTH_WALL_RUN_LEFT_STOP"
                | "BOTH_BUTTERFLY_LEFT"
                | "BOTH_BUTTERFLY_RIGHT"
                | "BOTH_BUTTERFLY_FL1"
                | "BOTH_BUTTERFLY_FR1"
                | "BOTH_ARIAL_LEFT"
                | "BOTH_ARIAL_RIGHT"
                | "BOTH_ARIAL_F1"
                | "BOTH_CARTWHEEL_LEFT"
                | "BOTH_CARTWHEEL_RIGHT"
                | "BOTH_JUMPFLIPSLASHDOWN1"
                | "BOTH_JUMPFLIPSTABDOWN"
                | "BOTH_JUMPATTACK6"
                | "BOTH_JUMPATTACK7"
                | "BOTH_FORCEWALLRUNFLIP_END"
                | "BOTH_FORCEWALLRUNFLIP_ALT"
                | "BOTH_FLIP_ATTACK7"
                | "BOTH_A7_SOULCAL"
        )
    )
}

fn trace_selection(
    entity_id: EntityId,
    previous: Option<AnimationState>,
    selection: LegacyAnimationSelection,
) {
    if std::env::var_os("JKR_TRACE_ANIMATIONS").is_none()
        || previous.is_some_and(|animation| {
            animation.lower.clip == selection.lower.clip
                && animation.upper.clip == selection.upper.clip
        })
    {
        return;
    }
    eprintln!(
        "animation entity={} legs={}({}) torso={}({})",
        entity_id.get().saturating_sub(1),
        selection.lower.clip,
        crate::legacy_animation_name(selection.lower.clip).unwrap_or("<unknown>"),
        selection.upper.clip,
        crate::legacy_animation_name(selection.upper.clip).unwrap_or("<unknown>"),
    );
}
