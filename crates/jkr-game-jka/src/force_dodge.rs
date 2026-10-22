//! `Jedi_DodgeEvasion` (`w_force.c:5490-5620`): whether a shot's target dodges it, and
//! with which pose. The disruptor asks it of what it strikes and the concussion beam of a
//! saber wielder; the target then takes the dodge on itself: the pose
//! (`HANDEXTEND_DODGE`, held 300 ms), a 100 ms speed burst and either the Force speed
//! sound or, with `g_forceDodge 2`, `ForceSpeed(self, 500)`.

use crate::npc_jedi_evasion::{BOTH_DODGE_FL, BOTH_DODGE_FR, BOTH_DODGE_L, BOTH_DODGE_R};
use jkr_protocol::PlayerState;

/// `FP_SEE`'s bit in `fd.forcePowersActive`.
const FP_SEE_ACTIVE: u32 = 1 << 14;
/// `HANDEXTEND_NONE`, `HANDEXTEND_DODGE`; `PW_SPEEDBURST`.
const HANDEXTEND_NONE: u32 = 0;
const HANDEXTEND_DODGE: u32 = 7;
const PW_SPEEDBURST: usize = 8;
/// The wire fields `forceHandExtend` and `forceDodgeAnim`.
const PS_FORCE_HAND_EXTEND: usize = 80;
const PS_FORCE_DODGE_ANIM: usize = 89;
/// `HL_NONE`: no hit location, as the concussion beam asks.
pub const HL_NONE: i32 = 0;
/// The sound a dodge not made by speed plays, `G_Sound(self, CHAN_BODY, ...)`, and that
/// channel.
pub const SPEED_SOUND: &[u8] = b"sound/weapons/force/speed.wav";
pub const SPEED_SOUND_CHANNEL: u32 = 6;

/// The target as the dodge reads it.
#[derive(Clone, Copy, Debug)]
pub struct Dodger {
    /// A client with health above zero.
    pub alive: bool,
    /// `fd.forcePowersActive`.
    pub active_powers: u32,
    /// `groundEntityNum != ENTITYNUM_NONE`.
    pub on_ground: bool,
    pub weapon_time: i32,
    pub hand_extend: u32,
    /// `fd.forcePowerLevel[FP_SEE]` and `[FP_SPEED]`.
    pub sight_level: i32,
    pub speed_level: i32,
    /// `WP_ForcePowerUsable(self, FP_SPEED)`, which only `g_forceDodge 2` reads.
    pub speed_usable: bool,
}

/// A dodge: its pose, and whether it is made with Force speed (`g_forceDodge 2`) rather
/// than announced with the speed sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dodge {
    pub animation: u16,
    pub by_speed: bool,
}

/// Whether `dodger` dodges a shot striking it at `hit_location` (`HL_*`). `mode` is
/// `g_forceDodge`; `irand` is `Q_irand`, drawn only where the reference draws it — even
/// for a shot with no hit location, which is never dodged.
pub fn dodge_evasion(
    dodger: &Dodger,
    mode: i32,
    hit_location: i32,
    irand: &mut dyn FnMut(i32, i32) -> i32,
) -> Option<Dodge> {
    if !dodger.alive || mode == 0 {
        return None;
    }
    if mode != 2 && dodger.active_powers & FP_SEE_ACTIVE == 0 {
        return None;
    }
    if !dodger.on_ground || dodger.weapon_time > 0 || dodger.hand_extend != HANDEXTEND_NONE {
        return None;
    }
    if mode == 2 {
        // Not while using any power; then by speed, more likely at a higher level.
        if dodger.active_powers != 0 || !dodger.speed_usable {
            return None;
        }
        if irand(1, 7) > dodger.speed_level {
            return None;
        }
    } else if dodger.sight_level < 3 {
        return None;
    }
    dodge_animation(hit_location).map(|animation| Dodge {
        animation,
        by_speed: mode == 2,
    })
}

/// The dodge on the dodger: the pose (`HANDEXTEND_DODGE`) held 300 ms
/// (`forceHandExtendTime`, in `hand_extend_time`) and a 100 ms speed burst. Its caller then
/// plays the Force speed sound or, for a dodge by speed, runs `ForceSpeed(self, 500)`.
pub fn take(dodge: Dodge, state: &mut PlayerState, hand_extend_time: &mut i32, level_time: i32) {
    state.set_raw_field(PS_FORCE_HAND_EXTEND, HANDEXTEND_DODGE);
    state.set_raw_field(PS_FORCE_DODGE_ANIM, u32::from(dodge.animation));
    *hand_extend_time = level_time + 300;
    state.powerups[PW_SPEEDBURST] = (level_time + 100) as u32;
}

/// The pose for where a shot strikes; none for the legs and feet, or without a location.
fn dodge_animation(hit_location: i32) -> Option<u16> {
    match hit_location {
        // `HL_BACK_RT`; `HL_BACK`, `HL_CHEST`, `HL_WAIST`; `HL_HEAD`.
        6 | 8 | 11 | 5 | 16 => Some(BOTH_DODGE_FL),
        // `HL_CHEST_RT`, `HL_BACK_LT`, `HL_CHEST_LT`.
        9 | 7 | 10 => Some(BOTH_DODGE_FR),
        // `HL_ARM_RT`, `HL_HAND_RT`.
        12 | 14 => Some(BOTH_DODGE_L),
        // `HL_ARM_LT`, `HL_HAND_LT`.
        13 | 15 => Some(BOTH_DODGE_R),
        _ => None,
    }
}
