//! Ghoul2-compatible actor pose evaluation and GPU upload.

use super::{ActorMesh, GpuState, GpuVertex, preview_gpu_vertex};
use glam::Quat;
use jkr_client::LegacyPlayerAngleController;
use jkr_model::Gla;
use jkr_runtime::{EntityId, EntityKind};
use std::error::Error;

#[path = "force_bones.rs"]
mod force_bones;
pub(crate) use force_bones::ForceBones;

#[path = "actor_pose_retained.rs"]
pub(crate) mod retained;
pub(crate) use retained::RetainedPose;

#[path = "gpu_skinning.rs"]
pub(crate) mod gpu_skinning;

#[path = "server_bone_angles.rs"]
mod server_bone_angles;

#[path = "actor_evaluation.rs"]
/// Independent presentation evaluators and persistent ownership-transferring workers.
pub(crate) mod evaluation;

/// Allocate one reusable generic skeleton evaluator for an actor model.
pub(crate) fn storage(animation: &Gla) -> Result<evaluation::Slot, Box<dyn Error>> {
    Ok(evaluation::Slot::new(animation)?)
}

/// Allocate the persistent `cent->pe`-equivalent player-angle state.
pub(crate) fn angle_storage(animation: &Gla) -> LegacyPlayerAngleController {
    let _ = animation;
    LegacyPlayerAngleController::new()
}

/// Replace the snapshot view-yaw root with cgame's evaluated legs yaw.
pub(crate) fn world_rotation(mesh: Option<&ActorMesh>, fallback: [f32; 4]) -> [f32; 4] {
    mesh.and_then(|mesh| mesh.render_yaw_degrees)
        .map_or(fallback, |yaw| {
            let roll = mesh.map_or(0.0, |mesh| mesh.angle_controller.root_roll_degrees());
            (Quat::from_rotation_z(yaw.to_radians()) * Quat::from_rotation_x(roll.to_radians()))
                .to_array()
        })
}

/// Update all live actor poses at the presented cgame time.
pub(crate) fn update(gpu: &mut GpuState, presentation_time: i64) -> Result<(), Box<dyn Error>> {
    let world = gpu
        .demo_session
        .as_ref()
        .map_or(&gpu.live_world, crate::demo_playback::Session::world);
    let local_entity = gpu
        .live_session
        .as_ref()
        .map(|session| EntityId::new(u64::from(session.latest_snapshot().player.client_num()) + 1));
    let authoritative = local_entity
        .and_then(|entity| world.entity(entity))
        .and_then(|entity| entity.animation());
    let authoritative_saber_move = gpu
        .live_session
        .as_ref()
        .map(|session| session.latest_snapshot().player.saber_move());
    let local_animation = local_entity.and_then(|entity| {
        gpu.local_actor_state.resolve(
            entity,
            gpu.local_prediction.predicted_state(),
            authoritative,
            authoritative_saber_move,
        )
    });
    let predicted = gpu.local_prediction.predicted_state();
    // The entity states and configstrings `CG_G2ServerBoneAngles` reads.
    let snapshot = crate::first_person_view::presented_snapshot(
        gpu.live_session.as_ref(),
        gpu.demo_session.as_ref(),
        presentation_time as i32,
    );
    let game_state = gpu
        .live_session
        .as_ref()
        .map(|session| session.game_state())
        .or_else(|| {
            gpu.demo_session
                .as_ref()
                .map(|session| session.game_state())
        });
    let queue = &gpu.queue;
    let vertex_buffer = &gpu.geometry.vertex_buffer;
    // Invalidate every entry first, including actors skipped below or after an upload error.
    for mesh in &mut gpu.actor_meshes {
        mesh.retained_pose.invalidate();
        mesh.animator.requested = None;
    }
    for mesh in &mut gpu.actor_meshes {
        let frame_millis = mesh.angle_controller.begin_frame(presentation_time);
        let requested = mesh
            .entity_id
            .and_then(|entity_id| world.entity(entity_id))
            .map(|entity| {
                let pose = entity.sample_pose(presentation_time);
                if mesh.entity_id == local_entity {
                    let animation = local_animation.or_else(|| entity.animation());
                    (animation, super::local_actor_state::pose(pose, predicted))
                } else {
                    (entity.animation(), pose)
                }
            });
        let Some((Some(state), pose)) = requested else {
            continue;
        };
        let mut state = state;
        if mesh.corpse_pool && !mesh.body_copied {
            state.lower.forced_frame =
                jkr_client::legacy_body_frame(&mesh.preview.config, state.lower.clip);
            state.upper.forced_frame =
                jkr_client::legacy_body_frame(&mesh.preview.config, state.upper.clip);
            state.lower.transition = None;
            state.upper.transition = None;
        }
        // Non-humanoids skip `CG_G2PlayerAngles` and face their entity yaw
        // (`cg_players.c:4274`, `:4366`).
        if !mesh.animator.humanoid() {
            mesh.render_yaw_degrees = None;
            if let (Some(snapshot), Some(game_state), Some(id)) =
                (snapshot, game_state, mesh.entity_id)
                && let Some(state) = snapshot
                    .entities
                    .iter()
                    .find(|state| u64::from(state.number()) + 1 == id.get())
            {
                server_bone_angles::apply(mesh, state, game_state);
            }
        } else if let Some(pose) = pose {
            let origin = mesh
                .entity_id
                .and_then(|id| world.entity(id))
                .map_or([0.0; 3], |entity| {
                    entity.sample(presentation_time).translation
                });
            let origin = if mesh.entity_id == local_entity {
                predicted.map_or(origin, |state| state.origin)
            } else {
                origin
            };
            let mut inputs = jkr_client::LegacyPlayerAngleInputs::from_world(
                pose,
                origin,
                world,
                presentation_time,
            );
            inputs.frame_millis = Some(frame_millis);
            if pose.angle.correct_animation_motion {
                inputs.motion_angles = mesh
                    .animator
                    .player_motion_angles(&mesh.preview.animation, presentation_time)?;
            }
            let angles =
                mesh.angle_controller
                    .evaluate_with_inputs(pose, presentation_time, inputs);
            mesh.animator
                .set_player_angles(&mesh.preview.animation, angles)?;
            mesh.render_yaw_degrees = Some(angles.legs_yaw_degrees);
        } else {
            mesh.animator.clear_player_angles();
        }
        mesh.animator.requested = Some(state);
    }
    gpu.actor_workers
        .evaluate(&mut gpu.actor_meshes, presentation_time);
    // Application is always in actor-vector order, never in worker completion order.
    for mesh in &mut gpu.actor_meshes {
        let Some(state) = mesh.animator.requested else {
            continue;
        };
        mesh.animator.completed()?;
        let matrices = mesh.animator.matrices();
        if let Some(palette) = &mut mesh.gpu_palette {
            palette.stage(&mut gpu.geometry.skinning, matrices)?;
            mesh.force_bones.update(&mesh.preview.animation, matrices);
            mesh.weapon_attachments =
                super::saber::attachments_from_matrices(&mesh.preview, matrices);
            mesh.driver_seat = crate::vehicle_pose::driver_seat(&mesh.preview, matrices);
            mesh.current_frames = (
                state.lower.forced_frame.unwrap_or(state.lower.clip),
                state.upper.forced_frame.unwrap_or(state.upper.clip),
            );
            if !mesh.retained_pose.trace_required() {
                continue;
            }
        }
        let surfaces = mesh
            .preview
            .mesh
            .skin_pose_matrices(&mesh.preview.skin, 0, matrices)?;
        mesh.retained_pose
            .update_trace_lod(&mesh.preview.mesh, matrices)?;
        mesh.force_bones.update(&mesh.preview.animation, matrices);
        mesh.weapon_attachments = super::saber::attachments_from_matrices(&mesh.preview, matrices);
        mesh.driver_seat = crate::vehicle_pose::driver_seat(&mesh.preview, matrices);
        for range in &mesh.vertex_ranges {
            if mesh.gpu_palette.is_some() {
                break;
            }
            let surface = surfaces
                .get(range.surface_index)
                .ok_or("animated actor surface disappeared")?;
            if surface.vertices.len() != range.vertices.len() {
                return Err("animated actor vertex count changed".into());
            }
            mesh.pose_vertices.clear();
            mesh.pose_vertices.extend(
                surface
                    .vertices
                    .iter()
                    .map(|vertex| preview_gpu_vertex(&mesh.preview, vertex)),
            );
            let byte_offset = u64::try_from(range.vertices.start)?
                .checked_mul(u64::try_from(std::mem::size_of::<GpuVertex>())?)
                .ok_or("animated actor buffer offset overflow")?;
            queue.write_buffer(
                vertex_buffer,
                byte_offset,
                bytemuck::cast_slice(&mesh.pose_vertices),
            );
        }
        mesh.current_frames = (
            state.lower.forced_frame.unwrap_or(state.lower.clip),
            state.upper.forced_frame.unwrap_or(state.upper.clip),
        );
        if let Some(entity) = mesh.entity_id {
            mesh.retained_pose
                .publish(entity, presentation_time, surfaces);
        }
    }
    gpu.geometry.skinning.flush(queue);
    Ok(())
}

impl GpuState {
    pub(crate) fn update_actor_animations(
        &mut self,
        presentation_time: i64,
    ) -> Result<(), Box<dyn Error>> {
        update(self, presentation_time)
    }

    pub(crate) fn assign_corpse_meshes(&mut self, presentation_time: i64) {
        self.apply_body_commands(presentation_time);
        for mesh in self
            .actor_meshes
            .iter_mut()
            .filter(|mesh| mesh.corpse_pool && mesh.body_identity.is_none())
        {
            mesh.entity_id = None;
        }
        let world = self
            .demo_session
            .as_ref()
            .map_or(&self.live_world, crate::demo_playback::Session::world);
        for corpse in world
            .entities()
            .filter(|entity| entity.kind == EntityKind::Corpse)
        {
            if self
                .actor_meshes
                .iter()
                .any(|mesh| mesh.entity_id == Some(corpse.id) && mesh.body_identity.is_some())
            {
                continue;
            }
            let Some(appearance) = corpse.appearance() else {
                continue;
            };
            if let Some(mesh) = self.actor_meshes.iter_mut().find(|mesh| {
                mesh.corpse_pool && mesh.entity_id.is_none() && &mesh.appearance == appearance
            }) {
                mesh.entity_id = Some(corpse.id);
            }
        }
    }
}
