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
        let (sin, cos) = actor
            .angle_controller
            .torso_pitch_degrees()
            .to_radians()
            .sin_cos();
        let (sy, cy) = angles[1].to_radians().sin_cos();
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
            combat_effects::rotation_from_direction([cos * cy, cos * sy, -sin]),
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
