//! BaseJKA third-person camera geometry.
//!
//! OpenJK `codemp/cgame/cg_view.c` keeps `cameraIdealTarget` attached to the
//! predicted player viewpoint and computes `cameraIdealLoc` by moving backward
//! along the fully pitched `camerafwd`. Consequently, looking upward lowers the
//! camera rather than lifting the target/player in frame. Player root yaw is
//! independent: `codemp/cgame/cg_ents.c` rebuilds the predicted entity from
//! player state and `CG_G2PlayerAngles` distributes pitch through bones instead
//! of rotating the complete model toward the camera.
//!
//! `CG_OffsetThirdPersonView` keeps the camera out of the player and the level:
//! the focus pitch is capped at 80 degrees, so looking up or down never drops
//! the camera straight under or over the player; fast yaw turns stiffen the
//! damping, so the eased camera does not cut across the player; and both the
//! target and the camera sweep an 8-unit cube against `MASK_CAMERACLIP`, which
//! covers the world and solid brush entities such as lifts and doors.

use super::{GpuState, movement_collision};
use crate::local_prediction::movers::Collider;
use glam::{Quat, Vec3};
use jkr_bsp::Aabb;
use jkr_protocol::Snapshot;

const BASE_RANGE: f32 = 80.0;
const BASE_VERTICAL_OFFSET: f32 = 16.0;
/// `CG_OffsetThirdPersonView` caps the focus pitch, offset included, at 80 degrees.
const FOCUS_PITCH_LIMIT_DEGREES: f32 = 80.0;
/// `MASK_CAMERACLIP` (`cg_view.c`): `MASK_SOLID` (solid and terrain) plus
/// player clip. Bodies are not in it, so players never push the camera.
const MASK_CAMERACLIP: u32 = 0x0000_0001 | 0x0000_1000 | 0x0000_0010;
/// `CAMERA_SIZE`: the half-extent of the cube the camera sweeps.
const CAMERA_SIZE: f32 = 4.0;
/// Below this separation the look direction falls back to `camerafwd`.
const MIN_LOOK_DISTANCE: f32 = 0.01;

/// Unoccluded BaseJKA camera target and location before temporal damping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ThirdPersonOrbit {
    pub(crate) target: Vec3,
    pub(crate) position: Vec3,
    /// `camerafwd`: the capped focus direction the camera backs away along.
    pub(crate) forward: Vec3,
    /// The capped focus pitch in degrees, positive upward.
    pub(crate) pitch_degrees: f32,
}

/// `CG_OffsetThirdPersonView` (`cgame/cg_view.c:340-420`) with the archived
/// camera cvars: range, vertical offset, yaw angle and pitch offset. The
/// focus pitch, offset included, is capped at 80 degrees either way.
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
    let pitch_degrees = (pitch_radians.to_degrees() + pitch_offset_degrees)
        .clamp(-FOCUS_PITCH_LIMIT_DEGREES, FOCUS_PITCH_LIMIT_DEGREES);
    let pitch_radians = pitch_degrees.to_radians();
    let forward = Vec3::new(
        yaw_radians.cos() * pitch_radians.cos(),
        yaw_radians.sin() * pitch_radians.cos(),
        pitch_radians.sin(),
    );
    let target = first_person_view + Vec3::Z * vertical_offset;
    ThirdPersonOrbit {
        target,
        position: target - forward * range.max(0.0),
        forward,
        pitch_degrees,
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

/// The damped camera carried between frames (the `cam` state of `cg_view.c`).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ThirdPersonCamera {
    position: Option<Vec3>,
    target: Option<Vec3>,
    /// Focus yaw of the previous frame in degrees, for the stiff factor.
    yaw_degrees: Option<f32>,
}

impl ThirdPersonCamera {
    /// Forget the damped state; the next third-person frame starts at the ideal.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
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

/// `cameraStiffFactor` (`CG_OffsetThirdPersonView`): how much of the camera
/// damping a fast yaw turn shaves off. Below 1 degree per millisecond there is
/// none; it then rises by half the excess and holds at 0.75 above 2.5.
fn stiff_factor(previous_yaw_degrees: f32, yaw_degrees: f32, frame_millis: f32) -> f32 {
    if frame_millis <= 0.0 {
        return 0.0;
    }
    let turn = (yaw_degrees - previous_yaw_degrees).rem_euclid(360.0);
    let rate = turn.min(360.0 - turn) / frame_millis;
    if rate < 1.0 {
        0.0
    } else if rate > 2.5 {
        0.75
    } else {
        (rate - 1.0) * 0.5
    }
}

/// `CG_UpdateThirdPersonCameraDamp`'s factor: the higher the focus pitch,
/// the less the camera damps, and the stiff factor shaves off part of the rest.
fn camera_damp_factor(damp: f32, pitch_degrees: f32, stiff: f32) -> f32 {
    let pitch = pitch_degrees.abs() / 115.0;
    let damp = damp + (1.0 - damp) * pitch * pitch;
    if stiff > 0.0 {
        damp + (1.0 - damp) * stiff
    } else {
        damp
    }
}

/// The view target, or the focus direction when the camera sits on it
/// (`CG_OffsetThirdPersonView`'s "must be hitting something" fallback).
fn look_target(position: Vec3, target: Vec3, forward: Vec3) -> Vec3 {
    if position.distance_squared(target) < MIN_LOOK_DISTANCE * MIN_LOOK_DISTANCE {
        position + forward
    } else {
        target
    }
}

/// A solid brush entity of `snapshot` the camera clips against, placed and
/// turned as drawn at `time`. Packed player boxes are `CONTENTS_BODY`, which
/// `MASK_CAMERACLIP` leaves out, so they are skipped before any trace.
fn camera_solids(snapshot: Option<&Snapshot>, time: i32) -> impl Iterator<Item = Collider> + '_ {
    snapshot
        .into_iter()
        .flat_map(|snapshot| snapshot.entities.iter())
        .filter_map(move |entity| Collider::from_entity(entity, time, time, false))
        .filter(|solid| solid.bounds.is_none())
}

/// The damped, occlusion-traced third-person camera for this frame: the
/// ideal orbit chases the player, then both target and position are eased
/// and pulled in front of world geometry and solid brush entities.
///
/// `focus_offset` is the decaying prediction error. Stock adds it to the view
/// origin before `CG_OffsetThirdPersonView`, so it moves the focus and is
/// traced with it rather than shifting the finished camera into a wall.
pub(crate) fn damped_third_person(
    state: &mut GpuState,
    focus_offset: Vec3,
    delta_seconds: f32,
    presentation_time: i64,
) -> (Vec3, Vec3) {
    let camera_bounds =
        Aabb::new([-CAMERA_SIZE; 3], [CAMERA_SIZE; 3]).expect("constant camera bounds are valid");
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
    let camera_damp = cvar("cg_thirdPersonCameraDamp", STOCK_CAMERA_DAMP);
    let target_damp = state
        .console
        .as_ref()
        .and_then(|c| c.float_cvar("cg_thirdPersonTargetDamp"));
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
    let focus = state.camera_position + focus_offset;
    let orbit = orbit_with(
        focus, yaw, pitch, framing[2], framing[3], framing[0], framing[1],
    );
    let focus_yaw_degrees = (yaw + framing[0].to_radians()).to_degrees();
    let time = presentation_time as i32;
    let snapshot = crate::first_person_view::presented_snapshot(
        state.live_session.as_ref(),
        state.demo_session.as_ref(),
        time,
    );
    let bsp = &state.bsp;
    let scratch = &mut state.trace_scratch;
    let mut sweep = |start: Vec3, end: Vec3| {
        movement_collision::trace_end_through_solids(
            bsp,
            scratch,
            start,
            end,
            camera_bounds,
            MASK_CAMERACLIP,
            camera_solids(snapshot, time),
        )
    };
    let camera = &mut state.third_person_camera;

    let ideal_target = orbit.target;
    let target_remaining = remaining_fraction(target_damp, STOCK_TARGET_DAMP, delta_seconds);
    let damped_target = camera
        .target
        .filter(|target| target.distance_squared(ideal_target) < 256.0_f32.powi(2))
        .map_or(ideal_target, |target| {
            ideal_target + (target - ideal_target) * target_remaining
        });
    let target = sweep(focus, damped_target);
    camera.target = Some(target);

    let ideal_position = orbit.position;
    let stiff = camera.yaw_degrees.map_or(0.0, |previous| {
        stiff_factor(previous, focus_yaw_degrees, delta_seconds * 1000.0)
    });
    camera.yaw_degrees = Some(focus_yaw_degrees);
    let camera_remaining = remaining_fraction(
        Some(f64::from(camera_damp_factor(
            camera_damp,
            orbit.pitch_degrees,
            stiff,
        ))),
        0.0,
        delta_seconds,
    );
    let damped_position = camera
        .position
        .filter(|position| position.distance_squared(ideal_position) < 256.0_f32.powi(2))
        .map_or(ideal_position, |position| {
            ideal_position + (position - ideal_position) * camera_remaining
        });
    let position = sweep(target, damped_position);
    camera.position = Some(position);
    (position, look_target(position, target, orbit.forward))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn orbit(pitch_degrees: f32, pitch_offset_degrees: f32) -> ThirdPersonOrbit {
        orbit_with(
            Vec3::ZERO,
            0.0,
            pitch_degrees.to_radians(),
            BASE_RANGE,
            BASE_VERTICAL_OFFSET,
            0.0,
            pitch_offset_degrees,
        )
    }

    #[test]
    fn steep_pitch_keeps_the_camera_behind_the_player() {
        // The input allows about 88 degrees; stock frames the camera at 80.
        for pitch in [88.0, -88.0] {
            let orbit = orbit(pitch, 0.0);
            assert_eq!(orbit.pitch_degrees, pitch.signum() * 80.0);
            let behind = -orbit.position.x;
            let expected = BASE_RANGE * 80.0_f32.to_radians().cos();
            assert!((behind - expected).abs() < 1e-3, "{behind} vs {expected}");
        }
    }

    #[test]
    fn pitch_offset_is_capped_with_the_view_pitch() {
        assert_eq!(orbit(70.0, 20.0).pitch_degrees, 80.0);
        assert!((orbit(30.0, 10.0).pitch_degrees - 40.0).abs() < 1e-4);
    }

    #[test]
    fn stiff_factor_follows_the_yaw_rate() {
        assert_eq!(stiff_factor(0.0, 7.0, 8.0), 0.0);
        assert!((stiff_factor(0.0, 12.0, 8.0) - 0.25).abs() < 1e-6);
        assert_eq!(stiff_factor(0.0, 40.0, 8.0), 0.75);
        // The shorter way round, across the wrap and across whole turns.
        assert!((stiff_factor(355.0, 7.0, 8.0) - 0.25).abs() < 1e-5);
        assert!((stiff_factor(-725.0, 7.0, 8.0) - 0.25).abs() < 1e-5);
        assert_eq!(stiff_factor(0.0, 90.0, 0.0), 0.0);
    }

    #[test]
    fn stiffness_and_pitch_reduce_camera_damping() {
        assert!((camera_damp_factor(0.3, 0.0, 0.0) - 0.3).abs() < 1e-6);
        assert!((camera_damp_factor(0.3, 0.0, 0.5) - 0.65).abs() < 1e-6);
        let steep = camera_damp_factor(0.3, 80.0, 0.0);
        assert!(steep > 0.3 && steep < 1.0);
        assert!(camera_damp_factor(0.3, 80.0, 0.75) > steep);
    }

    #[test]
    fn coincident_camera_and_target_look_along_the_focus() {
        let forward = Vec3::X;
        let position = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(look_target(position, position, forward), position + forward);
        let target = position + Vec3::Y * 10.0;
        assert_eq!(look_target(position, target, forward), target);
    }
}
