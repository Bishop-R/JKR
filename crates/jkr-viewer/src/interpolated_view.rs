//! Stock `CG_InterpolatePlayerState`: smooth authoritative views between snapshots.
//! Follow views interpolate angles; synchronous local views keep immediate mouse-look.

use super::GpuState;
use glam::Vec3;
use jkr_protocol::{GameState, Snapshot};

pub(crate) fn server_synchronous(game: &GameState) -> bool {
    game.config_string(1)
        .and_then(|info| jkr_protocol::info_value(info, b"g_synchronousClients"))
        .is_some_and(|value| jkr_game_jka::userinfo::atoi(value) != 0)
}

/// Do not bridge a teleport, a followed-player change, or a change of view mode.
fn fraction(previous: &Snapshot, next: &Snapshot, time: i32) -> f32 {
    if next.server_time <= previous.server_time
        || (previous.player.entity_flags() ^ next.player.entity_flags()) & 8 != 0
        || previous.player.client_num() != next.player.client_num()
        || (previous.player.movement_flags() ^ next.player.movement_flags()) & 4096 != 0
    {
        return 0.0;
    }
    ((time - previous.server_time) as f32 / (next.server_time - previous.server_time) as f32)
        .clamp(0.0, 1.0)
}

fn lerp_angle(from: f32, mut to: f32, fraction: f32) -> f32 {
    if to - from > 180.0 {
        to -= 360.0;
    }
    if to - from < -180.0 {
        to += 360.0;
    }
    from + fraction * (to - from)
}

fn sample(previous: &Snapshot, next: &Snapshot, time: i32) -> ([f32; 3], [f32; 3]) {
    let f = fraction(previous, next, time);
    let from = previous.player.origin();
    let to = next.player.origin();
    let mut eye = std::array::from_fn(|axis| from[axis] + f * (to[axis] - from[axis]));
    eye[2] += previous.player.view_height() as f32;
    let from = previous.player.view_angles();
    let to = next.player.view_angles();
    (
        eye,
        std::array::from_fn(|axis| lerp_angle(from[axis], to[axis], f)),
    )
}

pub(crate) fn present(gpu: &mut GpuState, time: i32) {
    let Some(session) = gpu.live_session.as_ref() else {
        return;
    };
    let latest = session.latest_snapshot();
    // Intermission owns its camera. A missing predictor alone is not enough.
    if matches!(latest.player.movement_type(), 7 | 8) {
        return;
    }
    let following = latest.player.movement_flags() & 4096 != 0;
    if !following && !server_synchronous(session.game_state()) {
        return;
    }
    let previous = session.snapshot_at_or_before(time);
    let next = session
        .snapshot_after(previous.server_time)
        .unwrap_or(previous);
    let (eye, angles) = sample(previous, next, time);
    gpu.camera_position = Vec3::from_array(eye);
    if following {
        gpu.camera_pitch = -angles[0].to_radians();
        gpu.camera_yaw = angles[1].to_radians();
    }
}
