//! A sentry's own turning on the client (`TurretClientRun` and `CreepToPosition`,
//! `cg_turret.c:29-260`). Stock clients unfold and turn a sentry themselves; its server sends
//! only the enemy it shoots at (`bolt2`) and whether it is about to blow (`fireflag` 2).
//!
//! Seen for the first time, a sentry plays its root's frames 0 to 11 at 0.8 and holds the
//! last. Every frame after, its hinge turns: a quarter turn at a time toward its enemy, or
//! round at 30 degrees a second while it has none, or not at all while it is about to blow.
//! The turn is absolute (`BONE_ANGLES_REPLACE`). A shot, which the server marks by changing
//! `fireflag`, flashes at the gun's muzzle.

use super::*;
use crate::saber_trail::tilt::vector_to_angles;

/// `ENTITYNUM_NONE`: no enemy.
const NO_ENEMY: u16 = 1_023;
/// The frames a sentry unfolds through when first seen, and their speed.
const UNFOLD: (i32, i32, f32) = (0, 11, 0.8);
/// `CreepToPosition`'s `max_degree_switch`.
const CREEP_STEP: f32 = 90.0;
/// The idle sweep, degrees a millisecond.
const SWEEP_SPEED: f32 = 0.03;
/// The gun's muzzle (`cg_turret.c:162`'s `torsoBolt`).
const FLASH_BOLT: &str = "*flash02";

/// A muzzle flash: where it plays and which way.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Flash {
    pub(crate) origin: [f32; 3],
    pub(crate) direction: [f32; 3],
}

/// What the client keeps for one sentry (`turAngles`, `dustTrailTime`,
/// `frame_minus1_refreshed`).
#[derive(Clone, Copy, Debug)]
pub(super) struct Sentry {
    angles: [f32; 3],
    /// When the idle sweep last turned; `None` before it first has.
    swept_at: Option<i64>,
    refreshed_at: i64,
    /// The `fireflag` last flashed for (`bolt4`), zero for none.
    flashed: u8,
}

/// One frame of `TurretClientRun` for the sentry `state` names, posed by `pose`: it stands at
/// `origin`, and its enemy, if it has one this client knows of, at `enemy` (that entity's
/// `pos.trBase`). Its muzzle flash, when it has just fired.
pub(super) fn run(
    sentry: &mut Option<Sentry>,
    pose: &mut BoneOverridePose,
    model: &PosedModel,
    state: &EntityState,
    origin: [f32; 3],
    enemy: Option<[f32; 3]>,
    time: i64,
) -> Option<Flash> {
    let animation = &model.animation;
    let sentry = sentry.get_or_insert_with(|| {
        // First sight. The zero postmultiplied turns stock also sets change nothing.
        if let Some(root) = model.root {
            let (start_frame, end_frame, speed) = UNFOLD;
            let command = BoneAnimationCommand {
                clip: usize::MAX,
                start_frame,
                end_frame,
                speed,
                time_millis: time,
                set_frame: None,
                end_behavior: OverrideEndBehavior::Freeze,
                blend: false,
                blend_millis: 0,
            };
            // A skeleton of fewer frames is left unanimated.
            let _ = pose.set_bone_animation(animation, root, command);
        }
        Sentry {
            angles: [90.0, 0.0, 0.0],
            swept_at: None,
            refreshed_at: 0,
            flashed: 0,
        }
    });
    let hinge = model.hinge?;
    // "I'm about to blow": held where it is.
    if state.fire_flag() == 2 {
        let _ = pose.set_bone_angles(animation, hinge, hinge_turn(sentry.angles));
        return None;
    }
    let flash = match state.fire_flag() {
        0 => {
            sentry.flashed = 0;
            None
        }
        flag if flag != sentry.flashed => {
            sentry.flashed = flag;
            muzzle(pose, model, origin, time)
        }
        _ => None,
    };
    if state.bolt2() != NO_ENEMY {
        if let Some(enemy) = enemy {
            let toward = Vec3::from_array(enemy) - Vec3::from_array(origin);
            let mut ideal = vector_to_angles(toward.normalize_or_zero());
            ideal[2] = 0.0;
            ideal[0] += 90.0;
            creep(ideal, &mut sentry.angles);
        }
    } else {
        if sentry.angles[1] > 360.0 {
            sentry.angles[1] -= 361.0;
        }
        let swept_at = *sentry.swept_at.get_or_insert(time);
        let turn = ((time - swept_at) as f32 * SWEEP_SPEED).min(360.0);
        sentry.swept_at = Some(time);
        creep([90.0, sentry.angles[1] + turn, 0.0], &mut sentry.angles);
    }
    if time < sentry.refreshed_at {
        sentry.refreshed_at = time;
        return flash;
    }
    sentry.refreshed_at = time;
    let _ = pose.set_bone_angles(animation, hinge, hinge_turn(sentry.angles));
    flash
}

/// The muzzle flash at the gun's `*flash02` bolt as the skeleton stands before this frame's
/// turn (`cg_turret.c:184-197`): the bolt's origin and its `NEGATIVE_X` after
/// `G2API_GetBoltMatrix`'s column swap, which is the bolt's own Y. Stock reads the bolt with
/// no angles (`vec3_origin`), so the sentry's own yaw is left out here too.
fn muzzle(
    pose: &mut BoneOverridePose,
    model: &PosedModel,
    origin: [f32; 3],
    time: i64,
) -> Option<Flash> {
    let matrices = pose.evaluate(&model.animation, time).ok()?;
    let bolt = model
        .glm
        .surface_bolt_matrix(FLASH_BOLT, 0, matrices)
        .ok()??;
    // `G2API_GetBoltMatrix` normalizes the rows before its world matrix.
    let rows = bolt.map(|row| Vec3::new(row[0], row[1], row[2]).normalize_or_zero());
    Some(Flash {
        origin: std::array::from_fn(|axis| origin[axis] + bolt[axis][3]),
        direction: std::array::from_fn(|axis| rows[axis].y),
    })
}

/// The hinge's absolute turn (`BONE_ANGLES_REPLACE` about -Y up, -Z left, -X forward).
fn hinge_turn(angles: [f32; 3]) -> BoneAngleCommand {
    BoneAngleCommand {
        angles_degrees: angles,
        mode: BoneAngleMode::Replace,
        up: BoneAxis::NegativeY,
        left: BoneAxis::NegativeZ,
        forward: BoneAxis::NegativeX,
    }
}

/// `CreepToPosition`: the yaw, then the pitch, a quarter turn toward `ideal` the shorter way
/// round (judged in whole degrees), landing on it once within a quarter turn, and wrapped by
/// 361 as stock wraps it. A negative ideal pitch counts from 360.
fn creep(mut ideal: [f32; 3], current: &mut [f32; 3]) {
    for axis in [1, 0] {
        if axis == 0 && ideal[0] < 0.0 {
            ideal[0] += 360.0;
        }
        let (wanted, now) = (ideal[axis] as i32, current[axis] as i32);
        let (negative, positive) = if wanted <= now {
            (now - wanted, (360 - now) + wanted)
        } else {
            (now + (360 - wanted), wanted - now)
        };
        if negative < positive {
            current[axis] -= CREEP_STEP;
            if current[axis] < ideal[axis] && current[axis] + CREEP_STEP * 2.0 >= ideal[axis] {
                current[axis] = ideal[axis];
            }
            if current[axis] < 0.0 {
                current[axis] += 361.0;
            }
        } else {
            current[axis] += CREEP_STEP;
            if current[axis] > ideal[axis] && current[axis] - CREEP_STEP * 2.0 <= ideal[axis] {
                current[axis] = ideal[axis];
            }
            if current[axis] > 360.0 {
                current[axis] -= 361.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::creep;

    #[test]
    fn a_sentry_creeps_a_quarter_turn_and_lands_within_one() {
        let mut current = [90.0, 0.0, 0.0];
        // Far round: a quarter turn the shorter way (negative, wrapped by 361).
        creep([90.0, 200.0, 0.0], &mut current);
        assert_eq!(current, [90.0, 271.0, 0.0]);
        // Within a quarter turn: onto it.
        creep([90.0, 200.0, 0.0], &mut current);
        assert_eq!(current, [90.0, 200.0, 0.0]);
        // A negative ideal pitch counts from 360: -30 is 330, reached the short way round
        // through 0 and the 361 wrap.
        let pitches: [f32; 3] = std::array::from_fn(|_| {
            creep([-30.0, 200.0, 0.0], &mut current);
            current[0]
        });
        assert_eq!(pitches, [0.0, 271.0, 330.0]);
        assert_eq!(current[1], 200.0);
    }
}
