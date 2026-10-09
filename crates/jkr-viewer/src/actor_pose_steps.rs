//! Per-actor preparation and upload. Failures return to the caller's actor loop.
use super::{ActorMesh, gpu_skinning, server_bone_angles};
use crate::{GpuVertex, preview_gpu_vertex};
use jkr_client::pmove::MovementState;
use jkr_protocol::{GameState, Snapshot};
use jkr_runtime::{AnimationState, EntityId, World};
use std::error::Error;

/// Immutable inputs shared by independently prepared actors in one frame.
pub(super) struct Frame<'a> {
    pub world: &'a World,
    pub local_entity: Option<EntityId>,
    pub local_animation: Option<AnimationState>,
    pub predicted: Option<&'a MovementState>,
    pub snapshot: Option<&'a Snapshot>,
    pub game_state: Option<&'a GameState>,
    pub presentation_time: i64,
    /// EternalJK's `cg_headTurn`: 0 never turns heads toward look targets, else the
    /// seconds a look holds (stock's 1).
    pub head_turn: i64,
    /// jaPRO's `cg_stylePlayer`, for its body bits.
    pub style: u32,
}

/// Prepare one actor without allowing a bad attachment or angle query to stop its peers.
pub(super) fn prepare(mesh: &mut ActorMesh, frame: &Frame<'_>) -> Result<(), Box<dyn Error>> {
    let Frame {
        world,
        local_entity,
        local_animation,
        predicted,
        snapshot,
        game_state,
        presentation_time,
        head_turn,
        style,
    } = *frame;
    let frame_millis = mesh.angle_controller.begin_frame(presentation_time);
    // A limb's copy plays on the animation its owner had when it was cut, with none of
    // the owner's turning (`CG_General` resets those bones before copying the model).
    let limb = mesh
        .cuts
        .limb
        .as_ref()
        .map(|limb| (Some(limb.animation), None));
    let requested = limb.or_else(|| {
        mesh.entity_id
            .and_then(|entity_id| world.entity(entity_id))
            .map(|entity| {
                let pose = entity.sample_pose(presentation_time);
                if mesh.entity_id == local_entity {
                    let animation = local_animation.or_else(|| entity.animation());
                    (animation, crate::local_actor_state::pose(pose, predicted))
                } else {
                    (entity.animation(), pose)
                }
            })
    });
    let Some((Some(state), pose)) = requested else {
        return Ok(());
    };
    let mut state = state;
    // `EF_DISINTEGRATION`: the pose holds the legs frame it had when the flag was first seen.
    let flags =
        crate::actor_dissolve::entity_flags(mesh.entity_id, local_entity, predicted, snapshot);
    crate::actor_dissolve::hold_pose(mesh, flags, presentation_time, &mut state);
    if mesh.corpse_pool {
        use jkr_client::style_player::{NO_BODIES, NO_FADE_EFFECTS};
        let event = crate::corpse_fade::entity_event(mesh.entity_id, snapshot);
        let no_bodies = style & NO_BODIES != 0;
        mesh.fade
            .observe(mesh.entity_id, event, presentation_time, no_bodies);
        let light_side = mesh
            .body_identity
            .as_ref()
            .is_some_and(|body| body.light_side);
        let mut seed =
            (mesh.entity_id.map_or(1, |id| id.get() as u32) ^ (presentation_time as u32)) | 1;
        let quiet = style & NO_FADE_EFFECTS != 0;
        mesh.fade
            .update(presentation_time, (light_side, quiet), move || {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                (seed >> 8) as f32 / (1 << 24) as f32
            });
    }
    // `PW_CLOAKED`'s two-second turn (`cloak_fade`).
    mesh.cloak = (!mesh.corpse_pool)
        .then(|| cloaked(mesh.entity_id, local_entity, snapshot))
        .flatten()
        .map(|cloaked| mesh.cloak_fade.advance(cloaked, presentation_time as i32));
    // `CG_IsMindTricked`: a player who mind-tricks the viewer fades out of its sight.
    if !mesh.corpse_pool && mesh.entity_id != local_entity {
        let tricked = mind_tricked(mesh.entity_id, snapshot);
        mesh.body = mesh.trick.advance(tricked, presentation_time as i32);
    }
    if mesh.corpse_pool {
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
        if let (Some(snapshot), Some(game_state), Some(id)) = (snapshot, game_state, mesh.entity_id)
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
        let mut inputs =
            jkr_client::LegacyPlayerAngleInputs::from_world(pose, origin, world, presentation_time);
        inputs.frame_millis = Some(frame_millis);
        if head_turn <= 0 {
            inputs.look_target_angles = None;
        } else if head_turn != 1 {
            inputs.look_hold_millis = Some(head_turn.saturating_mul(1_000));
        }
        if pose.angle.correct_animation_motion {
            inputs.motion_angles = mesh
                .animator
                .player_motion_angles(&mesh.preview.animation, presentation_time)?;
        }
        let angles = mesh
            .angle_controller
            .evaluate_with_inputs(pose, presentation_time, inputs);
        mesh.animator
            .set_player_angles(&mesh.preview.animation, angles)?;
        mesh.render_yaw_degrees = Some(angles.legs_yaw_degrees);
    } else {
        mesh.animator.clear_player_angles();
    }
    mesh.animator.requested = Some(state);
    Ok(())
}

/// Stage one successfully evaluated pose; the caller flushes even when another actor fails.
pub(super) fn apply(
    mesh: &mut ActorMesh,
    skinning: &mut gpu_skinning::Buffers,
    queue: &crate::frame_queue::FrameQueue,
    vertex_buffer: &wgpu::Buffer,
    presentation_time: i64,
) -> Result<(), Box<dyn Error>> {
    let Some(state) = mesh.animator.requested else {
        return Ok(());
    };
    mesh.animator.completed()?;
    let matrices = mesh.animator.matrices();
    if let Some(palette) = &mut mesh.gpu_palette {
        palette.stage(skinning, matrices)?;
        mesh.force_bones.update(&mesh.preview.animation, matrices);
        mesh.weapon_attachments = crate::saber::attachments_from_matrices(&mesh.preview, matrices);
        mesh.driver_seat = crate::vehicle_pose::driver_seat(&mesh.preview, matrices);
        mesh.current_frames = (
            state.lower.forced_frame.unwrap_or(state.lower.clip),
            state.upper.forced_frame.unwrap_or(state.upper.clip),
        );
        if !mesh.retained_pose.trace_required() {
            return Ok(());
        }
    }
    let surfaces = mesh
        .preview
        .mesh
        .skin_pose_matrices(&mesh.preview.skin, 0, matrices)?;
    mesh.retained_pose
        .update_trace_lod(&mesh.preview.mesh, matrices)?;
    mesh.force_bones.update(&mesh.preview.animation, matrices);
    mesh.weapon_attachments = crate::saber::attachments_from_matrices(&mesh.preview, matrices);
    mesh.driver_seat = crate::vehicle_pose::driver_seat(&mesh.preview, matrices);
    if mesh.gpu_palette.is_none() {
        for range in &mesh.vertex_ranges {
            let surface = surfaces
                .get(range.surface_index)
                .ok_or("animated actor surface disappeared")?;
            let scratch = (&mesh.preview, &mut mesh.pose_vertices);
            upload_range(scratch, range, surface, (queue, vertex_buffer))?;
        }
        if !mesh.surfaces.caps.is_empty() {
            let caps = mesh
                .preview
                .mesh
                .skin_switched_off(&mesh.preview.skin, 0, matrices)?;
            for (range, (_, surface)) in mesh.surfaces.caps.iter().zip(&caps) {
                let scratch = (&mesh.preview, &mut mesh.pose_vertices);
                upload_range(scratch, range, surface, (queue, vertex_buffer))?;
            }
        }
    }
    mesh.current_frames = (
        state.lower.forced_frame.unwrap_or(state.lower.clip),
        state.upper.forced_frame.unwrap_or(state.upper.clip),
    );
    if let Some(entity) = mesh.entity_id {
        mesh.retained_pose
            .publish(entity, presentation_time, surfaces);
    }
    Ok(())
}

/// Complete the upload batch even when one actor has an invalid pose.
pub(super) fn apply_all(
    meshes: &mut [ActorMesh],
    skinning: &mut gpu_skinning::Buffers,
    queue: &crate::frame_queue::FrameQueue,
    vertex_buffer: &wgpu::Buffer,
    presentation_time: i64,
) {
    for mesh in meshes {
        if mesh.animator.requested.is_none() {
            continue;
        }
        match apply(mesh, skinning, queue, vertex_buffer, presentation_time) {
            Ok(()) => mesh.animator.clear_error_report(),
            Err(error) => failed(mesh, error.as_ref()),
        }
    }
    // A bad actor must never strand healthy actors' staged palettes.
    skinning.flush(queue);
}

/// Report an actor's failure once until it successfully poses again.
pub(super) fn failed(mesh: &mut ActorMesh, error: &dyn Error) {
    mesh.retained_pose.invalidate();
    if mesh.animator.suppress_failed_frame() {
        crate::log::progress(format_args!(
            "actor {:?} ({}/{}): holding pose after animation error: {error}",
            mesh.entity_id.map(|id| id.get().saturating_sub(1)),
            mesh.appearance.model,
            mesh.appearance.variant,
        ));
    }
}

/// Whether the actor's entity mind-tricks the client whose view the snapshot holds
/// (`CG_IsMindTricked(…, cg.snap->ps.clientNum)`).
/// Whether the player `entity` is cloaked (`PW_CLOAKED`), the viewer's own from its player
/// state; `None` when the snapshot does not hold it.
fn cloaked(
    entity: Option<EntityId>,
    local: Option<EntityId>,
    snapshot: Option<&Snapshot>,
) -> Option<bool> {
    const PW_CLOAKED: usize = 11;
    let (entity, snapshot) = (entity?, snapshot?);
    if Some(entity) == local {
        return Some(snapshot.player.powerups[PW_CLOAKED] != 0);
    }
    let number = u16::try_from(entity.get().wrapping_sub(1)).ok()?;
    let index = snapshot
        .entities
        .binary_search_by_key(&number, |state| state.number())
        .ok()?;
    Some(snapshot.entities[index].powerups() & (1 << PW_CLOAKED) != 0)
}

fn mind_tricked(entity: Option<EntityId>, snapshot: Option<&Snapshot>) -> bool {
    let (Some(entity), Some(snapshot)) = (entity, snapshot) else {
        return false;
    };
    let Ok(number) = u16::try_from(entity.get().wrapping_sub(1)) else {
        return false;
    };
    snapshot
        .entities
        .binary_search_by_key(&number, |state| state.number())
        .is_ok_and(|index| snapshot.entities[index].client_bitflag(snapshot.player.client_num()))
}

/// One range's vertices skinned on the CPU, written over its share of the vertex buffer.
fn upload_range(
    (preview, scratch): (&crate::PlayerPreview, &mut Vec<GpuVertex>),
    range: &crate::player_animation::PreviewVertexRange,
    surface: &jkr_model::SkinnedSurface,
    (queue, vertex_buffer): (&crate::frame_queue::FrameQueue, &wgpu::Buffer),
) -> Result<(), Box<dyn Error>> {
    if surface.vertices.len() != range.vertices.len() {
        return Err("animated actor vertex count changed".into());
    }
    scratch.clear();
    scratch.extend(
        surface
            .vertices
            .iter()
            .map(|vertex| preview_gpu_vertex(preview, vertex)),
    );
    let byte_offset = u64::try_from(range.vertices.start)?
        .checked_mul(u64::try_from(std::mem::size_of::<GpuVertex>())?)
        .ok_or("animated actor buffer offset overflow")?;
    queue.write_buffer(vertex_buffer, byte_offset, bytemuck::cast_slice(scratch));
    Ok(())
}
