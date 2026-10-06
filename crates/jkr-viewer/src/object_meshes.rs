//! Load-time collection of the rigid network models a session can draw:
//! items, held and view weapons, projectiles, effect models and the
//! server-advertised `CS_MODELS` entries, plus the map's `misc_model_static` props.

use super::*;

#[path = "posed_objects.rs"]
pub(crate) mod posed;

/// Reserve carrier instances in addition to ordinary pickups before the first frame.
pub(super) fn instance_groups(meshes: &[StaticModelMesh]) -> Vec<Vec<ActorInstance>> {
    meshes
        .iter()
        .map(|mesh| {
            let flag = actor_world_submission::flags::MODELS
                .iter()
                .flatten()
                .any(|name| *name == mesh.appearance.model);
            Vec::with_capacity(if flag {
                2 * jkr_protocol::MAX_LEGACY_ENTITIES
            } else {
                32
            })
        })
        .collect()
}

/// A rigid (non-skinned) model: its draws into the shared geometry buffers,
/// the frame-0 bounds centre and, for guns, the `tag_flash` bolt.
pub(super) struct StaticModelMesh {
    pub(super) appearance: Appearance,
    pub(super) draws: Vec<ActorDraw>,
    pub(super) center: [f32; 3],
    pub(super) flash_bolt: Option<bolt::BoltMatrix>,
    /// A Ghoul2 model (`.glm`): an entity draws it under Ghoul2's root turn
    /// ([`weapon_view::actor_world_rotation`]).
    pub(super) ghoul2: bool,
    /// For a Ghoul2 model an entity is drawn posed by, its skeleton and rigid parts: the
    /// meshes right after this one ([`posed`]).
    pub(super) posed: Option<Box<posed::PosedModel>>,
}

/// Gather every rigid model appearance the session may reference and append
/// its frame-0 mesh to `flattened`. Models that fail to load are reported and
/// skipped so one broken asset cannot take the map down.
pub(super) fn load<'a>(
    vfs: &VirtualFileSystem,
    bsp: &Bsp,
    world: &World,
    game_state: Option<&GameState>,
    effect_names: impl Iterator<Item = &'a str>,
    vehicle_models: impl Iterator<Item = &'a str>,
    flattened: &mut FlattenedScene,
) -> Vec<StaticModelMesh> {
    let mut appearances = world
        .entities()
        .filter(|entity| !matches!(entity.kind, EntityKind::Actor | EntityKind::Corpse))
        .filter_map(|entity| entity.appearance().cloned())
        .collect::<BTreeSet<_>>();
    pickups::extend_appearances(&mut appearances);
    appearances.insert(Appearance {
        model: "models/weaphits/testboom.md3".to_owned(),
        variant: String::new(),
    });
    static_models::extend_appearances(bsp, &mut appearances);
    crate::effect_aux::breakage::extend_appearances(bsp, &mut appearances);
    first_person_weapon::Catalog::extend_appearances(&mut appearances);
    appearances.extend(
        (1..=16)
            .filter_map(weapon_view::held_model)
            .chain(projectiles::model_paths())
            .map(str::to_owned)
            .chain(vehicle_models.map(str::to_owned))
            .chain(effect_assets::required_models(vfs, effect_names))
            .map(|model| Appearance {
                model,
                variant: String::new(),
            }),
    );
    if let Some(game_state) = game_state {
        // CS_MODELS has 512 slots; zero is the reserved no-model value.
        appearances.extend((1..512).filter_map(|index| legacy_model_appearance(game_state, index)));
    }
    let mut meshes = Vec::with_capacity(appearances.len());
    for appearance in appearances {
        // Brush models, vehicle names, NPC saber names and NPC bodies are
        // not rigid models (`CG_RegisterGraphics`, cg_main.c; NPC bodies
        // load through `npc_refresh` as deformable actors).
        if appearance.model.starts_with(['*', '$', '@'])
            || jkr_client::legacy_npc_body_model(&appearance.model)
        {
            continue;
        }
        if let Err(error) = load_posable(vfs, &appearance, flattened, &mut meshes) {
            eprintln!("could not load network model {}: {error}", appearance.model)
        }
    }
    // The jetpack a carrier wears: a `model.glm` like an NPC's body, but drawn rigid.
    let jetpack = Appearance {
        model: crate::actor_world_submission::jetpack::MODEL.to_owned(),
        variant: String::new(),
    };
    match load_one(vfs, &jetpack, flattened) {
        Ok(mesh) => meshes.push(mesh),
        Err(error) => eprintln!("could not load the jetpack model: {error}"),
    }
    meshes
}

/// Parse one rigid model and append its bind mesh to CPU geometry.
pub(super) fn load_one(
    vfs: &VirtualFileSystem,
    appearance: &Appearance,
    flattened: &mut FlattenedScene,
) -> Result<StaticModelMesh, Box<dyn Error>> {
    load_parsed(vfs, appearance, flattened).map(|(mesh, _)| mesh)
}

/// [`load_one`] into `meshes`, a Ghoul2 model's rigid parts after it when it can be drawn
/// posed ([`posed::split`]); the model's index.
pub(super) fn load_posable(
    vfs: &VirtualFileSystem,
    appearance: &Appearance,
    flattened: &mut FlattenedScene,
    meshes: &mut Vec<StaticModelMesh>,
) -> Result<usize, Box<dyn Error>> {
    let (mut mesh, ghoul2) = load_parsed(vfs, appearance, flattened)?;
    let index = meshes.len();
    let split = match ghoul2 {
        Some(model) => posed::split(vfs, &model, appearance, &mesh.draws, flattened),
        None => None,
    };
    match split {
        Some((posed, parts)) => {
            mesh.posed = Some(Box::new(posed));
            meshes.push(mesh);
            meshes.extend(parts);
        }
        None => meshes.push(mesh),
    }
    Ok(index)
}

/// [`load_one`], and the parsed model when it is a Ghoul2 one.
fn load_parsed(
    vfs: &VirtualFileSystem,
    appearance: &Appearance,
    flattened: &mut FlattenedScene,
) -> Result<(StaticModelMesh, Option<Glm>), Box<dyn Error>> {
    let asset = vfs
        .read(&appearance.model)?
        .ok_or("network model asset was not found")?;
    let vertex_start = flattened.vertices.len();
    let (draws, flash_bolt, ghoul2) = if appearance.model.to_ascii_lowercase().ends_with(".glm") {
        let model = Glm::parse(&asset.bytes)?;
        let identity = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ];
        let pose = vec![identity; model.bone_count];
        let flash = model.surface_bolt_matrix(muzzle_flash::FLASH_BOLT, 0, &pose)?;
        (
            append_static_glm_mesh(flattened, &model)?,
            flash,
            Some(model),
        )
    } else {
        let model = Md3::parse(&asset.bytes)?;
        (append_md3_mesh(flattened, &model)?, None, None)
    };
    let mesh = StaticModelMesh {
        appearance: appearance.clone(),
        draws,
        center: mesh_center(&flattened.vertices[vertex_start..]),
        flash_bolt,
        ghoul2: ghoul2.is_some(),
        posed: None,
    };
    Ok((mesh, ghoul2))
}
