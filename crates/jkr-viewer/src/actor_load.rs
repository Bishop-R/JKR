//! Load-time actor meshes: one deformable mesh per advertised client (plus a
//! corpse-pool twin per appearance) and the shared Kyle fallback, appended to
//! the scene the viewer uploads once. [`build_actor_mesh`] is also how the
//! in-match clientinfo refresh ([`crate::clientinfo_refresh`]) builds a
//! replacement mesh for a player who changed model.

use super::*;
use jkr_client::{legacy_client_appearance, legacy_client_saber_names};

/// The player everyone falls back to when their model cannot be loaded.
pub(crate) fn fallback_appearance() -> Appearance {
    Appearance {
        model: "models/players/kyle".to_owned(),
        variant: "default".to_owned(),
    }
}

/// Saber names advertised for `client_num`, lower-cased for the hilt catalog;
/// `single_1` alone when the game state does not say.
pub(crate) fn client_saber_names(
    game_state: Option<&GameState>,
    client_num: u16,
) -> [Option<String>; 2] {
    game_state
        .map(|game_state| {
            legacy_client_saber_names(game_state, client_num)
                .map(|name| name.map(|name| name.to_ascii_lowercase()))
        })
        .unwrap_or_else(|| [Some("single_1".to_owned()), None])
}

/// Append `preview`'s bind-pose surfaces to `scene` and wrap them with the
/// pose storage one actor needs.
pub(crate) fn build_actor_mesh(
    scene: &mut FlattenedScene,
    mut preview: PlayerPreview,
    entity_id: Option<EntityId>,
    corpse_pool: bool,
    appearance: Appearance,
    saber_names: [Option<String>; 2],
) -> Result<ActorMesh, Box<dyn Error>> {
    preview.origin = [0.0; 3];
    preview.yaw = 0.0;
    let frame = preview.sequence.first_frame;
    let sample = (frame, frame, 0.0);
    let weapon_attachments = saber::actor_attachments(&preview, sample, sample);
    let (draws, vertex_ranges) = append_actor_mesh(scene, &preview, frame)?;
    let pose_vertex_capacity = vertex_ranges
        .iter()
        .map(|range| range.vertices.len())
        .max()
        .unwrap_or(0);
    let animator = actor_pose::storage(&preview.animation)?;
    // No precision/mark/dismemberment consumer exists yet. Do not buy an unused extra LOD.
    let retained_pose = actor_pose::RetainedPose::new(&preview.mesh, &preview.skin, false)?;
    Ok(ActorMesh {
        entity_id,
        corpse_pool,
        body_identity: None,
        appearance,
        angle_controller: actor_pose::angle_storage(&preview.animation),
        force_bones: actor_pose::ForceBones::new(&preview.animation),
        preview,
        draws,
        vertex_ranges,
        current_frames: (frame, frame),
        weapon_attachments,
        driver_seat: None,
        saber_names,
        render_yaw_degrees: None,
        animator,
        audio_events: Default::default(),
        pose_vertices: Vec::with_capacity(pose_vertex_capacity),
        gpu_palette: None,
        retained_pose,
    })
}

/// Load `appearance`, falling back to Kyle when it cannot be loaded.
fn load_or_fallback(
    vfs: &VirtualFileSystem,
    appearance: &Appearance,
    fallback: &Appearance,
    cache: &mut crate::player_assets::GlaCache,
) -> Result<PlayerPreview, Box<dyn Error>> {
    let mut load = |appearance: &Appearance| {
        crate::player_assets::load_player_appearance_with(
            vfs,
            &appearance.model,
            &appearance.variant,
            [0.0; 3],
            0.0,
            cache,
        )
    };
    match load(appearance) {
        Ok(actor) => Ok(actor),
        Err(error) if appearance != fallback => {
            eprintln!(
                "could not load actor appearance {}/{}: {error}; using Kyle",
                appearance.model, appearance.variant
            );
            load(fallback)
        }
        Err(error) => Err(error),
    }
}

/// Build the meshes of every client the game state advertises, every actor
/// already in `world`, and the shared fallback.
pub(crate) fn load_actor_meshes(
    vfs: &VirtualFileSystem,
    game_state: Option<&GameState>,
    world: &World,
    scene: &mut FlattenedScene,
) -> Result<Vec<ActorMesh>, Box<dyn Error>> {
    let fallback = fallback_appearance();
    let mut cache = crate::player_assets::GlaCache::default();
    // Player entity numbers are their client slots. Build a dedicated
    // deformable mesh for every advertised client, even if that player
    // was outside the first snapshot's PVS. Otherwise a later arrival
    // can only use the shared Kyle fallback and cannot hold its own pose.
    let mut actors = BTreeMap::new();
    if let Some(game_state) = game_state {
        for client_num in 0..32_u16 {
            if let Some(appearance) = legacy_client_appearance(game_state, client_num) {
                actors.insert(EntityId::new(u64::from(client_num) + 1), appearance);
            }
        }
    }
    for entity in world
        .entities()
        .filter(|entity| entity.kind == EntityKind::Actor)
    {
        actors.entry(entity.id).or_insert_with(|| {
            entity
                .appearance()
                .cloned()
                .unwrap_or_else(|| fallback.clone())
        });
    }
    let mut meshes = Vec::with_capacity(actors.len() * 2 + 1);
    for (entity_id, appearance) in actors {
        // NPC entities (numbers past the client slots) get their hilts from
        // `npc_refresh` once the first snapshot names them.
        let client_num = u16::try_from(entity_id.get().saturating_sub(1))
            .ok()
            .filter(|client_num| *client_num < 32);
        let saber_names = client_num
            .map(|client_num| client_saber_names(game_state, client_num))
            .unwrap_or_else(|| [Some("single_1".to_owned()), None]);
        let build = |scene: &mut FlattenedScene, actor: PlayerPreview, appearance: &Appearance| {
            let mut pair = Vec::with_capacity(2);
            for (assigned_entity, corpse_pool, preview) in
                [(Some(entity_id), false, actor.clone()), (None, true, actor)]
            {
                pair.push(build_actor_mesh(
                    scene,
                    preview,
                    assigned_entity,
                    corpse_pool,
                    appearance.clone(),
                    saber_names.clone(),
                )?);
            }
            Ok::<_, Box<dyn Error>>(pair)
        };
        match load_or_fallback(vfs, &appearance, &fallback, &mut cache) {
            // One player's model must not fail the map: a mesh that loads but
            // cannot be built is replaced by Kyle like one that cannot load.
            Ok(actor) => match build(scene, actor, &appearance) {
                Ok(pair) => meshes.extend(pair),
                Err(error) if appearance != fallback => {
                    eprintln!(
                        "could not build actor appearance {}/{}: {error}; using Kyle",
                        appearance.model, appearance.variant
                    );
                    let kyle = crate::player_assets::load_player_appearance_with(
                        vfs,
                        &fallback.model,
                        &fallback.variant,
                        [0.0; 3],
                        0.0,
                        &mut cache,
                    )?;
                    meshes.extend(build(scene, kyle, &appearance)?);
                }
                Err(error) => return Err(error),
            },
            Err(error) => eprintln!(
                "could not load actor appearance {}/{}: {error}",
                appearance.model, appearance.variant
            ),
        }
    }
    if let Ok(actor) = crate::player_assets::load_player_appearance_with(
        vfs,
        &fallback.model,
        &fallback.variant,
        [0.0; 3],
        0.0,
        &mut cache,
    ) {
        let names = [Some("single_1".to_owned()), None];
        meshes.push(build_actor_mesh(
            scene, actor, None, false, fallback, names,
        )?);
    }
    Ok(meshes)
}
