//! Pose-attached MP force effects, sharing actor matrices and the EFX pool.
use super::*;

#[path = "force_sprites.rs"]
mod sprites;

/// Select the activeForcePass graph (cg_players.c:9408-9510).
pub(crate) fn beam(pass: u8) -> Option<&'static str> {
    match pass {
        0 => None,
        1..=2 => Some("force/lightning"),
        3 => Some("force/lightningwide"),
        4..=5 => Some("mp/drain"),
        _ => Some("mp/drainwide"),
    }
}

/// The beam's effect frame, `AnglesToAxis(fAng)` with `fAng` = torso pitch, torso yaw and
/// no roll (`cg_players.c:9408-9510`): forward along the aim, left level, up above it.
///
/// The wide arc spreads its bolts across this frame's left axis (`origin2` spans ±384
/// there, ±2 up), so the axis must stay level. A shortest-arc turn from +X rolls the
/// frame with the aim instead, by up to half a turn when facing back along -X, and the
/// fan tilted, stood upright and flipped as the player turned.
fn beam_rotation(pitch_degrees: f32, yaw_degrees: f32) -> Quat {
    Quat::from_euler(
        glam::EulerRot::ZYX,
        yaw_degrees.to_radians(),
        pitch_degrees.to_radians(),
        0.0,
    )
}

/// Submit effects from the already evaluated hand bolts and body joints.
pub(super) fn submit(
    sinks: &mut Sinks<'_>,
    mesh: usize,
    entity: &jkr_runtime::SceneEntity,
    transform: jkr_runtime::Transform,
    snapshot: &Snapshot,
    time: i32,
    now: Instant,
) {
    let number = entity.id.get().saturating_sub(1) as u16;
    let Ok(index) = snapshot
        .entities
        .binary_search_by_key(&number, |e| e.number())
    else {
        return;
    };
    let state = &snapshot.entities[index];
    if state.npc_class() == 53 || entity.kind != EntityKind::Actor {
        return;
    }
    if state.client_bitflag(snapshot.player.client_num()) {
        return;
    }
    let actor = &sinks.actor_meshes[mesh];
    let rotation = weapon_view::actor_world_rotation(transform.rotation);
    let origin = Vec3::from_array(transform.translation);
    let left =
        actor.weapon_attachments[1].map(|bolt| saber::world_attachment(origin, rotation, bolt).0);
    let pass = state.raw_field(68).unwrap_or(0) as u8;
    // The beam is re-played on the reference 8 ms cadence (`effect_cadence.rs`).
    if let (Some(name), Some(hand)) = (beam(pass), left)
        && sinks.effects.contains_definition(name)
        && sinks.effect_aux.continuous.due(now)
    {
        let angles = entity
            .sample_pose(i64::from(time))
            .map_or(state.angular_trajectory_base(), |pose| {
                pose.view_angles_degrees
            });
        let frame = beam_rotation(actor.angle_controller.torso_pitch_degrees(), angles[1]);
        effect_runtime::spawn_effect(
            sinks.particles,
            sinks.effect_aux,
            sinks.effects,
            sinks.vfs,
            name,
            hand,
            now,
            u32::from(number) ^ time as u32,
            0,
            sinks.game_audio,
            frame,
        );
    }
    let view_left = Vec3::new(-sinks.camera_yaw.sin(), sinks.camera_yaw.cos(), 0.0);
    if state.e_flags() & (1 << 19) != 0 {
        for local in actor.force_bones.origins.iter().flatten() {
            let point = origin + rotation * (*local * Vec3::from_array(transform.scale));
            sprites::pair(
                sinks.particles,
                sinks.effects,
                point,
                view_left,
                false,
                time,
                now,
            );
        }
    }
    if state.powerups() & (1 << 9) != 0
        && let Some(hand) = left
    {
        let grip = state.force_powers_active() & (1 << 6) != 0;
        if !grip || sinks.third_person || number != snapshot.player.client_num() {
            for _ in 0..if grip { 2 } else { 1 } {
                sprites::pair(
                    sinks.particles,
                    sinks.effects,
                    hand,
                    view_left,
                    grip,
                    time,
                    now,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::beam_rotation;
    use glam::Vec3;

    /// `AngleVectors` forward for Quake pitch (positive down) and yaw, in degrees.
    fn forward(pitch: f32, yaw: f32) -> Vec3 {
        let (sp, cp) = pitch.to_radians().sin_cos();
        let (sy, cy) = yaw.to_radians().sin_cos();
        Vec3::new(cp * cy, cp * sy, -sp)
    }

    #[test]
    fn beam_frame_matches_angles_to_axis() {
        for pitch in [-60.0, -15.0, 0.0, 10.0, 45.0] {
            for yaw in [0.0, 37.0, 90.0, 179.0, 180.0, 181.0, 270.0, 359.0] {
                let frame = beam_rotation(pitch, yaw);
                let (sy, cy) = (yaw as f32).to_radians().sin_cos();
                let along = frame * Vec3::X;
                let left = frame * Vec3::Y;
                let up = frame * Vec3::Z;
                assert!(along.distance(forward(pitch, yaw)) < 1e-5);
                assert!(left.distance(Vec3::new(-sy, cy, 0.0)) < 1e-5);
                assert!(up.z > 0.0, "up {up} at pitch {pitch} yaw {yaw}");
            }
        }
    }

    /// `force/lightningwide` spreads bolts to `origin2` (500..524, -384..384, -2..2): the fan's
    /// ends must stay level with each other whichever way the player aims.
    #[test]
    fn wide_arc_stays_level_facing_any_way() {
        for pitch in [-20.0, 5.0, 30.0] {
            for yaw in (0..360).step_by(5) {
                let frame = beam_rotation(pitch, yaw as f32);
                let right_end = frame * Vec3::new(512.0, -384.0, 0.0);
                let left_end = frame * Vec3::new(512.0, 384.0, 0.0);
                assert!(
                    (right_end.z - left_end.z).abs() < 1e-2,
                    "fan tilted at pitch {pitch} yaw {yaw}: {right_end} {left_end}"
                );
            }
        }
    }
}
