//! Inline BSP mover presentation through the world material pipeline.
//!
//! `collect` mirrors `CG_CalcEntityLerpPositions` in
//! `codemp/cgame/cg_ents.c:3128-3131`: both `pos` and `apos` are evaluated at
//! the presented `cg.time`. Model selection mirrors `CG_General` at
//! `cg_ents.c:2890-2898`, where `SOLID_BMODEL` selects `inlineDrawModel`.

use super::scene_flatten::FlattenedScene;
use super::world_props::{self, Leaf};
use super::{ActorDraw, ActorInstance};
use jkr_bsp::Bsp;

use jkr_client::{LegacyMoverPresentation, legacy_present_mover};
use jkr_protocol::Snapshot;
use std::time::Instant;

#[path = "brush_scene.rs"]
mod scene;
pub(crate) use scene::append_frame;

/// One renderable partition of the map's existing mesh: an inline model,
/// or a detached world prop (`model_index` none).
pub(crate) struct Mesh {
    pub(crate) model_index: Option<usize>,
    pub(crate) draws: Vec<ActorDraw>,
}

/// Map-initialized mover meshes plus an O(1) inline-model lookup table and
/// the detached world props that share the mover draw path.
pub(crate) struct Catalog {
    pub(crate) meshes: Vec<Mesh>,
    mesh_by_model: Vec<Option<usize>>,
    props: Vec<Leaf>,
}

impl Catalog {
    /// The map's gate prop, if it has one: the first authored prop (see
    /// `menu_backdrop::routes`).
    pub(crate) fn gate(&self) -> Option<&Leaf> {
        self.props.first()
    }
}

/// An ET_MOVER evaluated at the current presentation time.
pub(crate) type Presented = LegacyMoverPresentation;

/// Build reusable draw ranges for all inline models and detach the map's
/// world props. Geometry, material indices, shader stages, and lightmaps
/// remain owned by the world pipeline.
pub(crate) fn build_catalog(bsp: &Bsp, flattened: &mut FlattenedScene) -> Catalog {
    let detached = world_props::detach(bsp, flattened);
    let mut mesh_by_model = vec![None; bsp.render().models().len()];
    let mut meshes = (1..bsp.render().models().len())
        .filter_map(|model_index| {
            let model = bsp.render().inline_model(model_index)?;
            let draws = flattened
                .draws
                .iter()
                .filter(|draw| {
                    draw.surface_index
                        .is_some_and(|surface| model.surfaces.contains(&surface))
                })
                .map(|draw| ActorDraw {
                    indices: draw.indices.clone(),
                    material: draw.material,
                })
                .collect::<Vec<_>>();
            (!draws.is_empty()).then_some(Mesh {
                model_index: Some(model_index),
                draws,
            })
        })
        .collect::<Vec<_>>();
    for (mesh_index, mesh) in meshes.iter().enumerate() {
        if let Some(model_index) = mesh.model_index {
            mesh_by_model[model_index] = Some(mesh_index);
        }
    }
    let props = detached
        .into_iter()
        .map(|detached| {
            let (mesh, leaf) = detached.placed(meshes.len());
            meshes.push(mesh);
            leaf
        })
        .collect();
    Catalog {
        meshes,
        mesh_by_model,
        props,
    }
}

/// Allocate per-mesh instance groups once during map initialization.
pub(crate) fn instance_groups(meshes: &[Mesh]) -> Vec<Vec<ActorInstance>> {
    meshes.iter().map(|_| Vec::with_capacity(4)).collect()
}

/// Collect every decoded mover into a caller-owned fixed-capacity pool.
pub(crate) fn collect(snapshot: &Snapshot, at_time: i32, output: &mut Vec<Presented>) {
    output.clear();
    for state in &snapshot.entities {
        if let Some(mover) = legacy_present_mover(state, at_time) {
            output.push(mover);
        }
    }
}

fn append_instances_with_views(
    presented: &[Presented],
    catalog: &Catalog,
    props_open: f32,
    groups: &mut [Vec<ActorInstance>],
    view_flags: impl Fn(u16) -> u32,
) {
    const IDENTITY: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
    for leaf in &catalog.props {
        groups[leaf.mesh].push(ActorInstance::new(
            leaf.offset(props_open),
            IDENTITY,
            [1.0; 3],
        ));
    }
    for mover in presented.iter().filter(|mover| mover.visible) {
        let Some(mesh_index) = catalog
            .mesh_by_model
            .get(mover.model_index)
            .copied()
            .flatten()
        else {
            continue;
        };
        let mut instance = ActorInstance::new(mover.origin, mover.rotation, [1.0; 3]);
        instance.view_flags = view_flags(mover.entity_number);
        groups[mesh_index].push(instance);
    }
}

/// Flatten preallocated mover groups into the shared instance buffer and
/// remember each mesh's draw range.
pub(crate) fn append_groups(
    instances: &mut Vec<ActorInstance>,
    groups: &[Vec<ActorInstance>],
    ranges: &mut Vec<std::ops::Range<u32>>,
) {
    for group in groups {
        let start = u32::try_from(instances.len()).unwrap_or(1_024);
        instances.extend(group.iter().copied());
        let end = u32::try_from(instances.len()).unwrap_or(1_024);
        ranges.push(start..end);
    }
}
