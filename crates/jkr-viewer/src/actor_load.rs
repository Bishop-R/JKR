//! Load-time actor meshes: one deformable mesh per advertised client (plus a
//! corpse-pool twin per appearance) and the shared Kyle fallback, appended to
//! the scene the viewer uploads once. [`build_actor_mesh`] is also how the
//! in-match clientinfo refresh ([`crate::clientinfo_refresh`]) builds a
//! replacement mesh for a player who changed model.

use super::*;
use jkr_client::legacy_client_saber_names;

/// The player everyone falls back to when their model cannot be loaded.
pub(crate) fn fallback_appearance() -> Appearance {
    Appearance {
        model: "models/players/kyle".to_owned(),
        variant: "default".to_owned(),
    }
}

/// `CG_LoadClientInfo`'s stand-in for a player whose model cannot be loaded: Kyle, in a
/// team game in the team skin the player's own was given (`red`, `blue`), else in his
/// default skin. (Stock asks Kyle for the player's very skin name, and stops the game
/// when Kyle has no such skin; the team colour is what that skin names.)
pub(crate) fn fallback_for(appearance: &Appearance, team_game: bool) -> Appearance {
    let team_skin = ["red", "blue"]
        .into_iter()
        .find(|color| team_game && appearance.variant.to_ascii_lowercase().ends_with(color));
    Appearance {
        model: "models/players/kyle".to_owned(),
        variant: team_skin.unwrap_or("default").to_owned(),
    }
}

/// Whether the game is a team game (`GT_TEAM` and past), for [`fallback_for`].
pub(crate) fn team_game(game_state: Option<&GameState>) -> bool {
    game_state
        .and_then(|game_state| game_state.config_string(0))
        .and_then(|info| jkr_client::LegacyClientInfo::new(info).integer("g_gametype"))
        .is_some_and(|gametype| gametype >= 6)
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
    let (mut draws, vertex_ranges) = append_actor_mesh(scene, &preview, frame)?;
    let mouth_draw = surface_draw(&preview.mesh, &vertex_ranges, "head_eyes_mouth");
    // The caps a cut leaves, drawn after the rest and hidden until a cut shows one.
    let (caps, cap_ranges) = append_actor_caps(scene, &preview, frame)?;
    draws.extend(caps);
    let surfaces = crate::actor_surfaces::Surfaces::new(
        &preview.mesh,
        &preview.skin,
        &vertex_ranges,
        cap_ranges,
    );
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
        limb_pool: false,
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
        dissolve: Default::default(),
        fade: Default::default(),
        marks: Default::default(),
        mouth_draw,
        surfaces,
        cuts: Default::default(),
        speed_trail: Default::default(),
        trick: Default::default(),
        body: Default::default(),
    })
}

/// The draw of `mesh`'s surface `name` (the names `G2API_SetSurfaceOnOff` takes), when the
/// skin draws it. Draws and `ranges` run in step, one for each drawn surface; a range counts
/// the surfaces skinning keeps, the model's switched-off ones left out (as GPU skinning
/// reads it).
fn surface_draw(mesh: &Glm, ranges: &[PreviewVertexRange], name: &str) -> Option<usize> {
    let kept: Vec<&str> = mesh
        .hierarchy
        .iter()
        .take(mesh.lods.first()?.surfaces.len())
        .filter(|surface| surface.flags & 2 == 0)
        .map(|surface| surface.name.as_str())
        .collect();
    ranges.iter().position(|range| {
        kept.get(range.surface_index)
            .is_some_and(|surface| surface.eq_ignore_ascii_case(name))
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
            load(fallback).or_else(|_| load(&fallback_appearance()))
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
    let team_game = team_game(game_state);
    let mut cache = crate::player_assets::GlaCache::default();
    // Player entity numbers are their client slots. Build a dedicated
    // deformable mesh for every advertised client, even if that player
    // was outside the first snapshot's PVS. Otherwise a later arrival
    // can only use the shared Kyle fallback and cannot hold its own pose.
    let mut actors = BTreeMap::new();
    if let Some(game_state) = game_state {
        let mut looks = crate::clientinfo_refresh::siege_looks::SiegeLooksCache::default();
        let looks = looks.for_level(game_state, Some(vfs));
        for client_num in 0..32_u16 {
            if let Some(appearance) =
                jkr_client::legacy_client_appearance_dressed(game_state, client_num, None, looks)
            {
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
        // A failed build leaves the scene as it was, so the Kyle fallback does
        // not sit behind half-appended geometry and materials.
        let build = |scene: &mut FlattenedScene, actor: PlayerPreview, appearance: &Appearance| {
            let lengths = (
                scene.vertices.len(),
                scene.indices.len(),
                scene.materials.len(),
                scene.draws.len(),
            );
            let mut pair = Vec::with_capacity(2);
            for (assigned_entity, corpse_pool, preview) in
                [(Some(entity_id), false, actor.clone()), (None, true, actor)]
            {
                let built = build_actor_mesh(
                    scene,
                    preview,
                    assigned_entity,
                    corpse_pool,
                    appearance.clone(),
                    saber_names.clone(),
                );
                match built {
                    Ok(mesh) => pair.push(mesh),
                    Err(error) => {
                        scene.vertices.truncate(lengths.0);
                        scene.indices.truncate(lengths.1);
                        scene.materials.truncate(lengths.2);
                        scene.draws.truncate(lengths.3);
                        return Err(error);
                    }
                }
            }
            Ok::<_, Box<dyn Error>>(pair)
        };
        let stand_in = fallback_for(&appearance, team_game);
        match load_or_fallback(vfs, &appearance, &stand_in, &mut cache) {
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
                        &stand_in.model,
                        &stand_in.variant,
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

#[cfg(test)]
mod tests {
    use super::*;
    use jkr_model::{GlmLod, GlmSurface, GlmSurfaceHierarchy};

    #[test]
    fn the_mouth_is_found_among_the_drawn_surfaces() {
        let surface = |(name, flags): (&str, u32)| GlmSurfaceHierarchy {
            name: name.into(),
            flags,
            shader: String::new(),
            parent: None,
            children: Vec::new(),
        };
        let lod = |hierarchy_index| GlmSurface {
            hierarchy_index,
            vertices: Vec::new(),
            triangles: Vec::new(),
            bone_references: Vec::new(),
        };
        let mesh = Glm {
            name: "model".into(),
            animation_name: String::new(),
            bone_count: 0,
            // The model switches the cap off; the skin hides the torso.
            hierarchy: [
                ("hips", 0),
                ("head_cap_torso_off", 2),
                ("torso", 0),
                ("head_eyes_mouth", 0),
            ]
            .map(surface)
            .to_vec(),
            lods: vec![GlmLod {
                surfaces: (0..4).map(lod).collect(),
            }],
        };
        // Skinning keeps hips, torso and mouth; the torso has no shader, so it is not drawn
        // and the mouth is the second draw.
        let ranges = [0, 2].map(|surface_index| PreviewVertexRange {
            surface_index,
            vertices: 0..0,
        });
        assert_eq!(surface_draw(&mesh, &ranges, "HEAD_eyes_mouth"), Some(1));
        assert_eq!(surface_draw(&mesh, &ranges, "head_cap_torso_off"), None);
        assert_eq!(surface_draw(&mesh, &ranges, "torso"), None);
    }
}

#[cfg(test)]
mod stand_in_tests {
    use super::*;

    #[test]
    fn kyle_stands_in_wearing_the_team_skin() {
        let look = |variant: &str| Appearance {
            model: "models/players/missing".to_owned(),
            variant: variant.to_owned(),
        };
        let skin = |appearance: Appearance| appearance.variant;
        assert_eq!(skin(fallback_for(&look("red"), true)), "red");
        assert_eq!(skin(fallback_for(&look("mine_blue"), true)), "blue");
        assert_eq!(skin(fallback_for(&look("red"), false)), "default");
        assert_eq!(
            skin(fallback_for(&look("head_a1|torso_a1|lower_a1"), true)),
            "default"
        );
        assert_eq!(
            fallback_for(&look("red"), true).model,
            "models/players/kyle"
        );
    }
}
