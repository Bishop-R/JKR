//! Allocation-free entity draw ordering for the shared Q3 stage runtime.
//!
//! rd-vanilla submits MD3 surfaces in `tr_mesh.cpp:386-416` and Ghoul2
//! surfaces in `tr_ghoul2.cpp:2460-2502` through `R_AddDrawSurf`, alongside
//! world surfaces. `R_SortDrawSurfs` (`tr_main.cpp:1134-1185`) orders by the
//! shader sort key and entity. JKR keeps separate world/entity traversal for
//! batching, but preserves the required ordering: all opaque work precedes
//! blended work, then entity blends are ordered by shader sort and entity
//! distance back-to-front.

use super::{ActorDraw, ActorInstance, ActorMesh, StaticModelMesh};
use crate::world_materials::Runtime;
use glam::Vec3;
use std::ops::Range;

const MAX_ENTITY_DRAWS: usize = 16_384;

/// One entity whose original surface shader is replaced by a cgame override.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OverrideInstance {
    pub(crate) mesh: OverrideMesh,
    pub(crate) material: usize,
    pub(crate) instance: ActorInstance,
    pub(crate) no_depth: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OverrideMesh {
    Actor(usize),
    Object(usize),
}

/// Buffer range created for one override instance.
#[derive(Clone, Debug)]
pub(crate) struct OverrideRange {
    mesh: OverrideMesh,
    material: usize,
    instances: Range<u32>,
    no_depth: bool,
}

/// One surface/instance-range submission consumed by `world_materials`.
#[derive(Clone, Debug)]
pub(crate) struct Draw {
    pub(crate) indices: Range<u32>,
    pub(crate) material: usize,
    pub(crate) instances: Range<u32>,
    pub(crate) no_depth: bool,
    distance_squared: f32,
    /// Opaque, depth-tested and backed by a registered material.
    pub(crate) stage_major: bool,
}

/// Reused fixed-capacity opaque and blended entity draw lists.
pub(crate) struct Queue {
    opaque: Vec<Draw>,
    blended: Vec<Draw>,
    dropped: usize,
}

impl Queue {
    pub(crate) fn new() -> Self {
        Self {
            opaque: Vec::with_capacity(MAX_ENTITY_DRAWS),
            blended: Vec::with_capacity(MAX_ENTITY_DRAWS),
            dropped: 0,
        }
    }

    /// Rebuild draw references without cloning meshes or growing storage.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn rebuild(
        &mut self,
        runtime: &Runtime,
        actors: &[ActorMesh],
        actor_ranges: &[Range<u32>],
        objects: &[StaticModelMesh],
        object_ranges: &[Range<u32>],
        overrides: &[OverrideRange],
        instances: &[ActorInstance],
        camera: Vec3,
    ) {
        self.opaque.clear();
        self.blended.clear();
        self.dropped = 0;
        for (mesh, range) in actors.iter().zip(actor_ranges) {
            self.append_mesh(runtime, &mesh.draws, range, None, false, instances, camera);
        }
        for (mesh, range) in objects.iter().zip(object_ranges) {
            self.append_mesh(runtime, &mesh.draws, range, None, false, instances, camera);
        }
        for entry in overrides {
            let draws = match entry.mesh {
                OverrideMesh::Actor(index) => actors.get(index).map(|mesh| mesh.draws.as_slice()),
                OverrideMesh::Object(index) => objects.get(index).map(|mesh| mesh.draws.as_slice()),
            };
            let Some(draws) = draws else { continue };
            self.append_mesh(
                runtime,
                draws,
                &entry.instances,
                Some(entry.material),
                entry.no_depth,
                instances,
                camera,
            );
        }
        self.opaque.sort_unstable_by(|left, right| {
            let left_order = runtime.material_order(left.material);
            let right_order = runtime.material_order(right.material);
            left_order
                .0
                .total_cmp(&right_order.0)
                .then(left_order.1.cmp(&right_order.1))
                .then(left.material.cmp(&right.material))
        });
        self.blended.sort_unstable_by(|left, right| {
            runtime
                .material_order(left.material)
                .0
                .total_cmp(&runtime.material_order(right.material).0)
                .then(right.distance_squared.total_cmp(&left.distance_squared))
                .then(left.material.cmp(&right.material))
        });
    }

    fn append_mesh(
        &mut self,
        runtime: &Runtime,
        draws: &[ActorDraw],
        instances_range: &Range<u32>,
        override_material: Option<usize>,
        no_depth: bool,
        instances: &[ActorInstance],
        camera: Vec3,
    ) {
        if instances_range.is_empty() {
            return;
        }
        for surface in draws {
            let material = override_material.unwrap_or(surface.material);
            let blended = runtime.material_blended(material);
            let stage_major = !no_depth && blended == Some(false);
            if blended == Some(true) {
                for instance in instances_range.clone() {
                    let Some(value) = instances.get(instance as usize) else {
                        continue;
                    };
                    self.push(
                        Draw {
                            indices: surface.indices.clone(),
                            material,
                            instances: instance..instance + 1,
                            no_depth,
                            stage_major,
                            distance_squared: Vec3::from_array(value.position)
                                .distance_squared(camera),
                        },
                        true,
                    );
                }
            } else {
                self.push(
                    Draw {
                        indices: surface.indices.clone(),
                        material,
                        instances: instances_range.clone(),
                        no_depth,
                        stage_major,
                        distance_squared: 0.0,
                    },
                    false,
                );
            }
        }
    }

    fn push(&mut self, draw: Draw, blended: bool) {
        let target = if blended {
            &mut self.blended
        } else {
            &mut self.opaque
        };
        if target.len() == target.capacity() {
            self.dropped += 1;
        } else {
            target.push(draw);
        }
    }

    pub(crate) fn opaque(&self) -> &[Draw] {
        &self.opaque
    }

    pub(crate) fn blended(&self) -> &[Draw] {
        &self.blended
    }
}

/// Append cgame override instances to the shared GPU instance stream while
/// retaining the mesh/material identity needed by the draw queue.
pub(crate) fn append_override_ranges(
    instances: &mut Vec<ActorInstance>,
    overrides: &[OverrideInstance],
    ranges: &mut Vec<OverrideRange>,
) {
    ranges.clear();
    for entry in overrides {
        if instances.len() == instances.capacity() || ranges.len() == ranges.capacity() {
            break;
        }
        let start = u32::try_from(instances.len()).unwrap_or(u32::MAX);
        instances.push(entry.instance);
        ranges.push(OverrideRange {
            mesh: entry.mesh,
            material: entry.material,
            instances: start..start + 1,
            no_depth: entry.no_depth,
        });
    }
}
