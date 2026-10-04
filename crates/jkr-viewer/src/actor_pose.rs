//! Ghoul2-compatible actor pose evaluation and GPU upload.

#[path = "actor_sounds.rs"]
pub(crate) mod sounds;

use super::{ActorMesh, GpuState};
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

#[path = "actor_pose_steps.rs"]
mod steps;

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
    let frame = steps::Frame {
        world,
        local_entity,
        local_animation,
        predicted,
        snapshot,
        game_state,
        presentation_time,
    };
    for mesh in &mut gpu.actor_meshes {
        if let Err(error) = steps::prepare(mesh, &frame) {
            steps::failed(mesh, error.as_ref());
        }
    }
    gpu.actor_workers
        .evaluate(&mut gpu.actor_meshes, presentation_time);
    // Application is always in actor-vector order, never in worker completion order.
    steps::apply_all(
        &mut gpu.actor_meshes,
        &mut gpu.geometry.skinning,
        queue,
        vertex_buffer,
        presentation_time,
    );
    Ok(())
}

impl GpuState {
    pub(crate) fn update_actor_animations(
        &mut self,
        presentation_time: i64,
        audio: &mut Option<crate::GameAudio>,
    ) -> Result<(), Box<dyn Error>> {
        update(self, presentation_time)?;
        sounds::update(self, presentation_time, audio);
        Ok(())
    }

    pub(crate) fn assign_corpse_meshes(&mut self) {
        self.apply_body_commands();
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
