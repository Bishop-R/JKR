//! Multiplayer third-person view: stock framing, collision and vehicle overrides.
//! Input angles/player prediction remain untouched; only the rendered view changes.

use super::{GpuState, movement_collision};
use glam::{Quat, Vec3};
use jkr_runtime::EntityId;

#[path = "camera_action.rs"]
pub(crate) mod action;
#[path = "camera_fall.rs"]
mod fall;
#[path = "camera_motion.rs"]
mod motion;
#[path = "camera_vehicle.rs"]
mod vehicle;
pub(crate) use motion::State;
pub(crate) use vehicle::Profile as VehicleProfile;

/// `togglecamera`: flips `cg_thirdPerson`, which a live view follows from the next frame,
/// and the view itself at once, which is all a demo's view follows.
pub(crate) fn toggle(state: &mut GpuState) {
    state.third_person = !state.third_person;
    if let Some(console) = &mut state.console {
        let next = if console.bool_cvar("cg_thirdPerson").unwrap_or(false) {
            "0"
        } else {
            "1"
        };
        console.set_cvar("cg_thirdPerson", next);
    }
}

/// `cg.renderingThirdPerson`, chosen afresh every frame of a live session.
pub(crate) fn choose_third_person(state: &mut GpuState, presentation_time: i64) {
    if let Some(third_person) = third_person_now(state, presentation_time) {
        state.third_person = third_person;
    }
}

/// The third-person view this frame: the saber-special camera where it takes over
/// ([`action`]), else the ordinary damped one.
pub(crate) fn third_person_view(
    state: &mut GpuState,
    delta_seconds: f32,
    presentation_time: i64,
) -> (Vec3, Vec3) {
    let player = crate::first_person_view::presented_snapshot(
        state.live_session.as_ref(),
        state.demo_session.as_ref(),
        presentation_time as i32,
    )
    .map(|snapshot| {
        (
            snapshot.player.falling_to_death() != 0,
            Vec3::from_array(snapshot.player.origin()),
        )
    });
    let (falling, origin) = player.unwrap_or((false, Vec3::ZERO));
    let fall = state.third_person_camera.fall.update(falling, origin);
    let view = action::view(state, delta_seconds, presentation_time)
        .unwrap_or_else(|| damped_third_person(state, delta_seconds, presentation_time));
    fall.map_or(view, |fall| self::fall::view(fall, view, origin))
}

/// Stock's camera choice for this frame of a live session ([`jkr_client::camera_mode`]):
/// `cg_thirdPerson` weighed against the presented snapshot and the predicted state.
/// `None` without a snapshot to weigh, or while the shot director frames the view.
fn third_person_now(state: &GpuState, presentation_time: i64) -> Option<bool> {
    // A shot the director frames keeps its camera.
    if state
        .console
        .as_ref()
        .is_some_and(|console| console.director.camera_active())
    {
        return None;
    }
    let snapshot = crate::first_person_view::presented_snapshot(
        state.live_session.as_ref(),
        None,
        presentation_time as i32,
    )?;
    let player = &snapshot.player;
    let mut view = jkr_client::camera_mode::ViewInputs {
        snapshot_health: player.health(),
        snapshot_team: player.team(),
        snapshot_zoom_mode: player.zoom_mode(),
        movement_type: player.movement_type(),
        weapon: player.weapon(),
        emplaced_index: player.emplaced_index(),
        torso_anim: player.torso_animation(),
        legs_anim: player.leg_animation(),
        legs_timer: player.legs_timer(),
        force_hand_extend: player.force_hand_extend(),
        falling_to_death: player.falling_to_death() != 0,
        vehicle: player.vehicle_entity_num(),
    };
    if let Some(predicted) = state.local_prediction.predicted_state() {
        view.movement_type = predicted.movement_type;
        view.weapon = predicted.weapon;
        view.emplaced_index = predicted.emplaced_index;
        view.torso_anim = predicted.torso_anim;
        view.legs_anim = predicted.legs_anim;
        view.legs_timer = predicted.legs_timer;
        view.force_hand_extend = predicted.force_hand_extend;
        view.falling_to_death = predicted.falling_to_death != 0;
        view.vehicle = predicted.vehicle_entity_num;
    }
    let cvar = |name| {
        state
            .console
            .as_ref()
            .and_then(|console| console.bool_cvar(name))
            .unwrap_or(false)
    };
    Some(jkr_client::camera_mode::rendering_third_person(
        cvar("cg_thirdPerson"),
        cvar("cg_fpls"),
        &view,
    ))
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

/// `CG_OffsetThirdPersonView` and its two collision/damping stages, from OpenJK codemp.
pub(crate) fn damped_third_person(
    state: &mut GpuState,
    _delta_seconds: f32,
    presentation_time: i64,
) -> (Vec3, Vec3) {
    let cvar = |name: &str, fallback: f32| {
        state
            .console
            .as_ref()
            .and_then(|c| c.float_cvar(name))
            .map_or(fallback, |v| v as f32)
    };
    let mut horizontal = cvar("cg_thirdPersonHorzOffset", 0.0);
    let camera_damp = cvar("cg_thirdPersonCameraDamp", 0.3);
    let target_damp = cvar("cg_thirdPersonTargetDamp", 0.5);
    let smooth = cvar("cg_smoothCamera", 1.0) != 0.0;
    let fallback = [
        cvar("cg_thirdPersonAngle", 0.0),
        cvar("cg_thirdPersonPitchOffset", 0.0),
        cvar("cg_thirdPersonRange", 80.0),
        cvar("cg_thirdPersonVertOffset", 16.0),
    ];
    let mut framing = state
        .console
        .as_mut()
        .and_then(|c| c.director.camera(fallback))
        .unwrap_or(fallback);
    framing[2] = state.scope.third_person_range(framing[2]);
    let snapshot = state
        .live_session
        .as_ref()
        .map(|s| s.latest_snapshot())
        .or_else(|| {
            state
                .demo_session
                .as_ref()
                .map(|s| s.snapshot_at_or_before(presentation_time as i32))
        });
    let riding = snapshot.map_or(0, |s| s.player.vehicle_entity_num());
    let vehicle = (riding != 0)
        .then(|| {
            let id = EntityId::new(u64::from(riding) + 1);
            state
                .actor_meshes
                .iter()
                .find(|m| m.entity_id == Some(id))
                .map(|m| &m.preview)
        })
        .flatten();
    let mut unrestrained = false;
    if let (Some(snapshot), Some(vehicle)) = (snapshot, vehicle) {
        if let Some(profile) = vehicle.vehicle_camera {
            let strafe = state
                .local_prediction
                .vehicle_camera_strafe()
                .unwrap_or_else(|| {
                    snapshot
                        .vehicle_player
                        .as_ref()
                        .and_then(|p| p.raw_field(91))
                        .unwrap_or(0) as i32
                });
            profile.apply(
                &mut framing,
                &mut horizontal,
                snapshot.player.view_angles()[0],
                -state.camera_pitch.to_degrees(),
                strafe,
            );
        }
        let game = state
            .live_session
            .as_ref()
            .map(|s| s.game_state())
            .or_else(|| state.demo_session.as_ref().map(|s| s.game_state()));
        unrestrained = vehicle.vehicle == Some(crate::vehicle_assets::VehicleKind::Fighter)
            && game
                .and_then(|g| g.config_string(1))
                .and_then(|info| jkr_protocol::info_value(info, b"bg_fighterAltControl"))
                .is_some_and(|v| jkr_game_jka::userinfo::atoi(v) != 0);
    }
    let held = held_camera_yaw(state, presentation_time);
    let (yaw, pitch) = if let Some(yaw) = held {
        framing[2] = crate::actor_world_submission::monster_hold::HELD_CAMERA_RANGE;
        (yaw.to_radians(), 0.0)
    } else if snapshot.is_some_and(|s| s.player.health() <= 0) {
        (
            snapshot.unwrap().player.stats[6] as i32 as f32 * std::f32::consts::PI / 180.0,
            state.camera_pitch,
        )
    } else {
        // Stock pitch offsets are down-positive, the renderer's pitch is up-positive.
        (
            state.camera_yaw + framing[0].to_radians(),
            state.camera_pitch - framing[1].to_radians(),
        )
    };
    let identity = snapshot.map_or((0, riding, 0), |s| {
        (s.player.client_num(), riding, s.player.entity_flags() & 8)
    });
    let hyperspace = snapshot
        .and_then(|s| s.vehicle_player.as_ref())
        .is_some_and(|p| {
            let start = p.vehicle_fields().hyperspace_time;
            start != 0 && presentation_time - i64::from(start) < 4000
        });
    // Falling to death, the camera frames the point the fall began at, at eye height.
    let focus = state
        .third_person_camera
        .fall
        .origin()
        .map_or(state.camera_position, |fall| {
            fall + Vec3::Z * snapshot.map_or(0, |s| s.player.view_height()) as f32
        });
    let frame = motion::Frame {
        focus,
        yaw,
        pitch,
        range: framing[2],
        vertical: framing[3],
        horizontal,
        camera_damp,
        target_damp,
        time: presentation_time,
        identity,
        unrestrained,
        hyperspace,
        smooth,
    };
    // Movers are evaluated at the same presentation time as their drawn geometry.
    // Packed player/vehicle bodies are excluded by stock MASK_CAMERACLIP.
    let solids = crate::first_person_view::presented_snapshot(
        state.live_session.as_ref(),
        state.demo_session.as_ref(),
        presentation_time as i32,
    );
    state.third_person_camera.update(frame, |start, end| {
        movement_collision::camera_trace(
            &state.bsp,
            &mut state.trace_scratch,
            solids,
            presentation_time as i32,
            start,
            end,
        )
    })
}

/// Held local player's facing from the snapshot currently being presented.
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
