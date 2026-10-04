//! Stock multiplayer camera damping and a defined view when collision collapses the orbit.

use glam::Vec3;

/// The camera's state is presentation-only and resets on view/vehicle/teleport changes.
#[derive(Default)]
pub(crate) struct State {
    previous: Option<Previous>,
}

struct Previous {
    position: Vec3,
    target: Vec3,
    yaw: f32,
    time: i64,
    identity: (u16, u16, u32),
}

/// Inputs after held-player and vehicle framing overrides; angles use JKR's up-positive pitch.
pub(crate) struct Frame {
    pub(crate) focus: Vec3,
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    pub(crate) range: f32,
    pub(crate) vertical: f32,
    pub(crate) horizontal: f32,
    pub(crate) camera_damp: f32,
    pub(crate) target_damp: f32,
    pub(crate) time: i64,
    pub(crate) identity: (u16, u16, u32),
    pub(crate) unrestrained: bool,
    pub(crate) hyperspace: bool,
}

/// Fraction remaining after `cg.time - cameraLastFrame`, in milliseconds.
fn remaining(damp: f32, milliseconds: f32) -> f32 {
    if damp >= 1.0 {
        0.0
    } else {
        (1.0 - damp.max(0.0)).powf(milliseconds / 50.0)
    }
}

impl State {
    /// Two four-unit hull traces, with stock pitch/yaw damping and vehicle snap policy.
    pub(crate) fn update(
        &mut self,
        f: Frame,
        mut trace: impl FnMut(Vec3, Vec3) -> Vec3,
    ) -> (Vec3, Vec3) {
        let previous = self
            .previous
            .as_ref()
            .filter(|p| p.identity == f.identity && p.time <= f.time);
        let reset = previous.is_none();
        // CG_ResetThirdPersonViewDamp clamps to 89; subsequent ordinary frames to 80.
        let limit = if reset { 89.0_f32 } else { 80.0_f32 }.to_radians();
        let pitch = if f.unrestrained && !reset {
            f.pitch
        } else {
            f.pitch.clamp(-limit, limit)
        };
        let forward = Vec3::new(
            f.yaw.cos() * pitch.cos(),
            f.yaw.sin() * pitch.cos(),
            pitch.sin(),
        );
        let ideal_target = f.focus + Vec3::Z * f.vertical;
        let ideal_position = ideal_target - forward * f.range;
        let (target, position) = if let Some(p) = previous {
            let dt = (f.time - p.time) as f32;
            let target_damp = if f.identity.1 != 0 || f.hyperspace {
                1.0
            } else {
                f.target_damp
            };
            let target = if target_damp < 0.0 {
                p.target
            } else {
                ideal_target + (p.target - ideal_target) * remaining(target_damp, dt)
            };
            let target = trace(f.focus, target);
            let yaw_delta = ((f.yaw - p.yaw).to_degrees() + 180.0).rem_euclid(360.0) - 180.0;
            // Zero elapsed server time must not turn 0/0 into an undefined view.
            let speed = if dt > 0.0 { yaw_delta.abs() / dt } else { 0.0 };
            let stiff = ((speed - 1.0) * 0.5).clamp(0.0, 0.75);
            let damp = if f.hyperspace {
                1.0
            } else if f.camera_damp == 0.0 {
                0.0
            } else {
                let base = if f.identity.1 != 0 {
                    1.0
                } else {
                    f.camera_damp
                };
                let damp = base + (1.0 - base) * (pitch.to_degrees().abs() / 115.0).powi(2);
                damp + (1.0 - damp) * stiff
            };
            let position = if damp < 0.0 {
                p.position
            } else {
                ideal_position + (p.position - ideal_position) * remaining(damp, dt)
            };
            (target, trace(target, position))
        } else {
            let target = trace(f.focus, ideal_target);
            (target, trace(target, ideal_position))
        };
        // CG_OffsetThirdPersonView falls back to camerafwd on a collapsed or axial
        // look vector. Never feed coincident eye/target into the renderer's look-at.
        let diff = target - position;
        let direction = if diff.length_squared() < 0.0001 || diff.x == 0.0 || diff.y == 0.0 {
            forward
        } else {
            diff.normalize()
        };
        let left = Vec3::new(-direction.y, direction.x, 0.0).normalize_or(Vec3::new(
            -f.yaw.sin(),
            f.yaw.cos(),
            0.0,
        ));
        let position = position + left * f.horizontal;
        self.previous = Some(Previous {
            position,
            target,
            yaw: f.yaw,
            time: f.time,
            identity: f.identity,
        });
        (position, position + direction)
    }
}
