//! The jetpack on a player's back and its flames (`CG_Player`,
//! `codemp/cgame/cg_players.c:8746-8807`; `CG_InitJetpackGhoul2`, `:7919-7941`). A living
//! player that carries one (`EF_JETPACK`) wears `models/weapons2/jetpack/model.glm` bolted to
//! its `*chestg`, as part of its own model (so not in its own first person). While it is lit
//! (`EF_JETPACK_ACTIVE`) the Boba jet plays at the pack's two flame bolts every cgame frame,
//! twice each while thrusting (`EF_JETPACK_FLAMING`). Its sounds are the loop adapter's
//! (`jkr_client`'s loop sounds).
use super::*;
use crate::bolt::{BoltMatrix, column, game_facing};

/// The pack's model and skeleton, and the player bolt it hangs from.
pub(crate) const MODEL: &str = "models/weapons2/jetpack/model.glm";
const SKELETON: &str = "models/weapons2/jetpack/jetpack.gla";
const CHEST_BOLT: &str = "*chestg";
/// Its flame bolts, left then right (`G2API_AddBolt` in `CG_InitJetpackGhoul2`): surfaces
/// of the pack's model, which Ghoul2 looks for before bones.
const FLAME_SURFACES: [&str; 2] = ["torso_ljet", "torso_rjet"];
/// `cgs.effects.mBobaJet`.
const JET_EFFECT: &str = "boba/jet";
/// `EF_DEAD`, `EF_JETPACK_ACTIVE`, `EF_JETPACK`, `EF_JETPACK_FLAMING`.
const EF_DEAD: u32 = 1 << 1;
const EF_JETPACK_ACTIVE: u32 = 1 << 11;
const EF_JETPACK: u32 = 1 << 29;
const EF_JETPACK_FLAMING: u32 = 1 << 30;

/// The pack's mesh and its flame bolts in its own space, posed at its skeleton's first
/// frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct JetpackModel {
    mesh: usize,
    flames: [BoltMatrix; 2],
}

impl JetpackModel {
    /// Resolved once per map from the loaded meshes and the pack's skeleton; `None` where
    /// the game's files lack either.
    pub(crate) fn build(vfs: &VirtualFileSystem, meshes: &[StaticModelMesh]) -> Option<Self> {
        let mesh = meshes.iter().position(|mesh| {
            mesh.appearance.variant.is_empty() && mesh.appearance.model == MODEL
        })?;
        let read = |path: &str| vfs.read(path).ok().flatten().map(|asset| asset.bytes);
        let glm = jkr_model::Glm::parse(&read(MODEL)?).ok()?;
        let skins = jkr_model::Gla::parse(&read(SKELETON)?)
            .ok()?
            .frame_matrices(0)
            .ok()?;
        let mut flames = [[[0.0; 4]; 3]; 2];
        for (flame, name) in flames.iter_mut().zip(FLAME_SURFACES) {
            *flame = glm.surface_bolt_matrix(name, 0, &skins).ok()??;
        }
        Some(Self { mesh, flames })
    }
}

/// The pack of the actor drawn with `mesh` at `transform`, if it carries one (`state` is its
/// entity, `None` for the local player), and its flames while it is lit.
#[allow(clippy::too_many_arguments)]
pub(super) fn submit(
    sinks: &mut Sinks<'_>,
    mesh: usize,
    transform: jkr_runtime::Transform,
    snapshot: &Snapshot,
    state: Option<&jkr_protocol::EntityState>,
    local: bool,
    draw_actor: bool,
    now: Instant,
) {
    let Some(pack) = sinks.jetpack else {
        return;
    };
    let flags = if local {
        sinks
            .predicted_local_state
            .map_or(snapshot.player.entity_flags(), |state| state.entity_flags)
    } else if let Some(state) = state {
        state.e_flags()
    } else {
        return;
    };
    if flags & EF_JETPACK == 0 || flags & EF_DEAD != 0 {
        return;
    }
    let actor = &sinks.actor_meshes[mesh];
    let Some(chest) = actor
        .preview
        .mesh
        .surface_bolt_matrix(CHEST_BOLT, 0, actor.animator.matrices())
        .ok()
        .flatten()
        .and_then(saber::attachment_from_bolt)
    else {
        return;
    };
    let rotation = weapon_view::actor_world_rotation(transform.rotation);
    let origin = Vec3::from_array(transform.translation);
    let (pack_origin, pack_rotation) = saber::world_attachment(origin, rotation, chest);
    let mut instance =
        ActorInstance::new(pack_origin.to_array(), pack_rotation.to_array(), [1.0; 3]);
    instance.view_flags = sinks.entity_view_flags | u32::from(!draw_actor);
    let parts = (&mut *sinks.object_groups, &mut *sinks.overrides);
    super::body_draw::push_part(parts, sinks.body, pack.mesh, instance);
    // Re-played on the reference 8 ms cadence, not per rendered frame (`effect_cadence.rs`).
    if flags & EF_JETPACK_ACTIVE == 0 || !sinks.effect_aux.continuous.due(now) {
        return;
    }
    let copies = if flags & EF_JETPACK_FLAMING != 0 {
        2
    } else {
        1
    };
    for (side, flame) in pack.flames.into_iter().enumerate() {
        let (point, direction) = flame_at(pack_origin, pack_rotation, flame, side);
        for copy in 0..copies {
            effect_runtime::spawn_effect(
                sinks.particles,
                sinks.effect_aux,
                sinks.effects,
                sinks.vfs,
                JET_EFFECT,
                point,
                now,
                (side as u32 * 2 + copy) | (sinks.presentation_time as u32).rotate_left(11),
                0,
                sinks.game_audio,
                combat_effects::rotation_from_direction(direction.to_array()),
            );
        }
    }
}

/// Where flame `side` burns and which way (`cg_players.c:8765-8780`): the pack's bolt as
/// `G2API_GetBoltMatrix` gives it, moved 9.5 and 13.5 units off it along its axes.
fn flame_at(origin: Vec3, rotation: Quat, bolt: BoltMatrix, side: usize) -> (Vec3, Vec3) {
    let bolt = game_facing(bolt);
    let axis = |index: usize| rotation * Vec3::from_array(column(&bolt, index));
    let point = origin + axis(3);
    let (x, y) = (axis(0), axis(1));
    if side == 0 {
        // NEGATIVE_Y by -9.5, then POSITIVE_X by -13.5; it burns along POSITIVE_X.
        (point + y * 9.5 - x * 13.5, x)
    } else {
        // POSITIVE_X by -9.5, then NEGATIVE_Y by -13.5; it burns along NEGATIVE_Y.
        (point - x * 9.5 + y * 13.5, -y)
    }
}
