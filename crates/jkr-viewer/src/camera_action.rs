//! `CG_ThirdPersonActionCam` (OpenJK `codemp/cgame/cg_view.c:1345-1423`), on while
//! `cg_thirdPersonSpecialCam` is set: through a saber special the camera swings out to the
//! side of the player its first blade's base is on, up to twice the third-person range
//! away, and looks back at the player.

use super::{GpuState, movement_collision};
use glam::Vec3;
use jkr_bsp::Aabb;

/// `MASK_SOLID`.
const MASK_SOLID: u32 = 0x1001;
/// `WP_SABER`.
const WP_SABER: u8 = 3;

/// `cg_actionCamLastTime` and `cg_actionCamLastPos`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Smoothing {
    last_time: i64,
    last_position: Vec3,
}

impl Smoothing {
    /// More than 300 ms since the camera last ran: it starts over from the ordinary view.
    fn stale(&self, now: i64) -> bool {
        now - self.last_time > 300
    }

    /// One frame toward `desired`, from `seed` (the ordinary view) when starting over: a
    /// tenth of the way each frame, and not at all while within 64 units.
    fn step(&mut self, desired: Vec3, seed: Option<Vec3>, now: i64) -> Vec3 {
        if let Some(seed) = seed {
            self.last_position = seed;
        }
        self.last_time = now;
        let towards = desired - self.last_position;
        // `smoothFactor = 0.1f*timescale.value`; JKR's client runs at one speed.
        if towards.length() > 64.0 {
            self.last_position += towards * 0.1;
        }
        self.last_position
    }
}

/// The action camera's eye and target this frame, or `None` where stock keeps the ordinary
/// view: switched off, the director framing a shot, no special under way, no saber in
/// hand, or its first blade not drawn in the last 300 ms.
pub(crate) fn view(
    state: &mut GpuState,
    delta_seconds: f32,
    presentation_time: i64,
) -> Option<(Vec3, Vec3)> {
    let console = state.console.as_ref()?;
    if !console
        .bool_cvar("cg_thirdPersonSpecialCam")
        .unwrap_or(false)
        || console.director.camera_active()
    {
        return None;
    }
    let range = console.float_cvar("cg_thirdPersonRange").unwrap_or(80.0) as f32;
    let time = presentation_time as i32;
    let snapshot = crate::first_person_view::presented_snapshot(
        state.live_session.as_ref(),
        state.demo_session.as_ref(),
        time,
    )?;
    let player = &snapshot.player;
    if !jkr_game_jka::saber_rules::in_special(player.saber_move()) || player.weapon() != WP_SABER {
        return None;
    }
    let entity = u64::from(player.client_num()) + 1;
    // `cent->ghoul2`, and `ci->saber[0].blade[0].trail`.
    if !state
        .actor_meshes
        .iter()
        .any(|mesh| mesh.entity_id.is_some_and(|id| id.get() == entity))
    {
        return None;
    }
    let (base, last) = state.saber_states.blade(entity, 0, 0)?.trail.last()?;
    if presentation_time - last > 300 {
        return None;
    }
    // `cent->lerpOrigin`: the drawn body's origin, under the predicted eye.
    let origin = state.camera_position - Vec3::Z * state.local_prediction.view_height();
    let out = (Vec3::from_array(base) - origin).normalize_or_zero();
    let desired = movement_collision::brush_trace(
        &state.bsp,
        &mut state.trace_scratch,
        Some(snapshot),
        time,
        (origin, origin + out * (range * 2.0)),
        Aabb::POINT,
        MASK_SOLID,
    );
    let seed = state
        .third_person_camera
        .action
        .stale(presentation_time)
        .then(|| super::damped_third_person(state, delta_seconds, presentation_time).0);
    let eye = state
        .third_person_camera
        .action
        .step(desired, seed, presentation_time);
    let snapshot = crate::first_person_view::presented_snapshot(
        state.live_session.as_ref(),
        state.demo_session.as_ref(),
        time,
    );
    let eye = movement_collision::brush_trace(
        &state.bsp,
        &mut state.trace_scratch,
        snapshot,
        time,
        (origin, eye),
        Aabb::POINT,
        MASK_SOLID,
    );
    // `vectoangles` of no direction looks straight up.
    let target = if eye == origin {
        eye + Vec3::new(1.0e-3, 0.0, 1.0)
    } else {
        origin
    };
    Some((eye, target))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_camera_starts_from_the_ordinary_view_and_closes_a_tenth_of_the_gap_a_frame() {
        let mut smoothing = Smoothing::default();
        assert!(smoothing.stale(1_000));
        let ordinary = Vec3::new(-80.0, 0.0, 16.0);
        let desired = Vec3::new(0.0, 160.0, 0.0);
        let first = smoothing.step(desired, Some(ordinary), 1_000);
        assert!(first.distance(ordinary + (desired - ordinary) * 0.1) < 1e-4);
        assert!(!smoothing.stale(1_300));
        // Within 64 units it holds still.
        let near = first + Vec3::new(30.0, 0.0, 0.0);
        assert_eq!(smoothing.step(near, None, 1_016), first);
        assert!(smoothing.stale(1_317));
    }
}
