//! BaseJKA third-person camera geometry.
//!
//! OpenJK `codemp/cgame/cg_view.c` keeps `cameraIdealTarget` attached to the
//! predicted player viewpoint and computes `cameraIdealLoc` by moving backward
//! along the fully pitched `camerafwd`. Consequently, looking upward lowers the
//! camera rather than lifting the target/player in frame. Player root yaw is
//! independent: `codemp/cgame/cg_ents.c` rebuilds the predicted entity from
//! player state and `CG_G2PlayerAngles` distributes pitch through bones instead
//! of rotating the complete model toward the camera.

use super::{GpuState, movement_collision};
use glam::{Quat, Vec3};
use jkr_bsp::Aabb;

const BASE_RANGE: f32 = 80.0;
const BASE_VERTICAL_OFFSET: f32 = 16.0;

/// Unoccluded BaseJKA camera target and location before temporal damping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ThirdPersonOrbit {
    pub(crate) target: Vec3,
    pub(crate) position: Vec3,
}

/// `CG_OffsetThirdPersonView` (`cgame/cg_view.c:340-420`) with the archived
/// camera cvars: range, vertical offset, yaw angle and pitch offset.
#[allow(clippy::too_many_arguments)]
pub(crate) fn orbit_with(
    first_person_view: Vec3,
    yaw_radians: f32,
    pitch_radians: f32,
    range: f32,
    vertical_offset: f32,
    angle_degrees: f32,
    pitch_offset_degrees: f32,
) -> ThirdPersonOrbit {
    let yaw_radians = yaw_radians + angle_degrees.to_radians();
    let pitch_radians = pitch_radians + pitch_offset_degrees.to_radians();
    let forward = Vec3::new(
        yaw_radians.cos() * pitch_radians.cos(),
        yaw_radians.sin() * pitch_radians.cos(),
        pitch_radians.sin(),
    );
    let target = first_person_view + Vec3::Z * vertical_offset;
    ThirdPersonOrbit {
        target,
        position: target - forward * range.max(0.0),
    }
}

/// Keep the local actor at its predicted root with yaw-only orientation.
pub(crate) fn local_actor_root(
    first_person_view: Vec3,
    view_height: f32,
    yaw_radians: f32,
) -> ([f32; 3], [f32; 4]) {
    (
        (first_person_view - Vec3::Z * view_height).to_array(),
        Quat::from_rotation_z(yaw_radians).to_array(),
    )
}

/// `cg_thirdPersonCameraDamp` / `cg_thirdPersonTargetDamp` defaults (`cg_main.c`).
const STOCK_CAMERA_DAMP: f32 = 0.3;
const STOCK_TARGET_DAMP: f32 = 0.5;
/// `CAMERA_DAMP_INTERVAL`: the damp factor is the fraction bled off per 50 ms.
const DAMP_INTERVAL_SECONDS: f32 = 0.05;

/// Fraction of the ideal-to-current difference that survives this frame
/// (`cg_view.c:449-466`, `(1 - damp)^(dt / 50 ms)`); a damp of 1 or more
/// snaps to the ideal, as the stock code copies it outright.
fn remaining_fraction(damp: Option<f64>, fallback: f32, delta_seconds: f32) -> f32 {
    let damp = damp.map_or(fallback, |value| value as f32);
    if damp >= 1.0 {
        return 0.0;
    }
    (1.0 - damp.max(0.0)).powf(delta_seconds / DAMP_INTERVAL_SECONDS)
}

/// The damped, occlusion-traced third-person camera for this frame: the
/// ideal orbit chases the player, then both target and position are eased
/// and pulled in front of world geometry.
pub(crate) fn damped_third_person(
    state: &mut GpuState,
    delta_seconds: f32,
    presentation_time: i64,
) -> (Vec3, Vec3) {
    const CAMERA_MASK: u32 = 0x0000_0001 | 0x0000_0010 | 0x0000_0100 | 0x0000_1000;
    let camera_bounds = Aabb::new([-4.0; 3], [4.0; 3]).expect("constant camera bounds are valid");
    let cvar = |name: &str, fallback: f32| {
        state
            .console
            .as_ref()
            .and_then(|console| console.float_cvar(name))
            .map_or(fallback, |value| value as f32)
    };
    let fallback = [
        cvar("cg_thirdPersonAngle", 0.0),
        cvar("cg_thirdPersonPitchOffset", 0.0),
        cvar("cg_thirdPersonRange", BASE_RANGE),
        cvar("cg_thirdPersonVertOffset", BASE_VERTICAL_OFFSET),
    ];
    let framing = state
        .console
        .as_mut()
        .and_then(|c| c.director.camera(fallback))
        .unwrap_or(fallback);
    // A player a rancor holds is watched from 120 units along the rancor's facing turned
    // about, level, without the angle cvars (`cg_view.c:371-378`, `652-664`).
    let held = held_camera_yaw(state, presentation_time);
    let (yaw, pitch, framing) = match held {
        Some(yaw) => (
            yaw.to_radians(),
            0.0,
            [
                0.0,
                0.0,
                crate::actor_world_submission::monster_hold::HELD_CAMERA_RANGE,
                framing[3],
            ],
        ),
        None => (state.camera_yaw, state.camera_pitch, framing),
    };
    let orbit = orbit_with(
        state.camera_position,
        yaw,
        pitch,
        framing[2],
        framing[3],
        framing[0],
        framing[1],
    );
    let ideal_target = orbit.target;
    let target_remaining = remaining_fraction(
        state
            .console
            .as_ref()
            .and_then(|c| c.float_cvar("cg_thirdPersonTargetDamp")),
        STOCK_TARGET_DAMP,
        delta_seconds,
    );
    let damped_target = state
        .third_person_camera_target
        .filter(|target| target.distance_squared(ideal_target) < 256.0_f32.powi(2))
        .map_or(ideal_target, |target| {
            ideal_target + (target - ideal_target) * target_remaining
        });
    let target = movement_collision::trace_end(
        &state.bsp,
        &mut state.trace_scratch,
        state.camera_position,
        damped_target,
        camera_bounds,
        CAMERA_MASK,
    );
    state.third_person_camera_target = Some(target);
    let ideal_position = orbit.position;
    // cg_view.c:507-529: the higher the pitch, the less the camera damps.
    let pitch = pitch.abs().to_degrees();
    let camera_damp = state
        .console
        .as_ref()
        .and_then(|c| c.float_cvar("cg_thirdPersonCameraDamp"))
        .map_or(STOCK_CAMERA_DAMP, |value| value as f32);
    let camera_damp = camera_damp + (1.0 - camera_damp) * (pitch / 115.0).powi(2);
    let camera_remaining = remaining_fraction(Some(f64::from(camera_damp)), 0.0, delta_seconds);
    let damped_position = state
        .third_person_camera_position
        .filter(|position| position.distance_squared(ideal_position) < 256.0_f32.powi(2))
        .map_or(ideal_position, |position| {
            ideal_position + (position - ideal_position) * camera_remaining
        });
    let position = movement_collision::trace_end(
        &state.bsp,
        &mut state.trace_scratch,
        target,
        damped_position,
        camera_bounds,
        CAMERA_MASK,
    );
    state.third_person_camera_position = Some(position);
    (position, target)
}

/// The held local player's camera yaw in degrees ([`crate::actor_world_submission::monster_hold::camera_yaw`]),
/// from the snapshot presented at `presentation_time`.
fn held_camera_yaw(state: &GpuState, presentation_time: i64) -> Option<f32> {
    let snapshot = crate::first_person_view::presented_snapshot(
        state.live_session.as_ref(),
        state.demo_session.as_ref(),
        presentation_time as i32,
    )?;
    let world = state
        .demo_session
        .as_ref()
        .map_or(&state.live_world, crate::demo_playback::Session::world);
    crate::actor_world_submission::monster_hold::camera_yaw(world, snapshot, presentation_time)
}
