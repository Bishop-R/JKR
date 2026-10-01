//! Projects decal marks onto the static world.
//!
//! Mirrors `RE_AddDecalToScene` (rd-vanilla `tr_decals.cpp:160-251`): build
//! the texture axes from the impact normal and the random orientation, clip
//! the radius-sized square into the world with the engine-generic
//! `R_MarkFragments` port in `jkr-bsp`, and map every fragment point back to
//! texture space with `0.5 + dot(delta, axis) * 0.5 / radius`. The surface
//! walk (`R_MarkFragments`, `tr_marks.cpp:256-453`) feeds planar faces whole,
//! curved patches triangle by triangle behind a facing test and skips
//! triangle soups (`r_marksOnTriangleMeshes` defaults to 0).
//!
//! The world triangles come from the same tessellated batches the renderer
//! draws, so marks follow the rendered patch geometry exactly. Surfaces the
//! game forbids marks on are filtered with the caller-supplied predicate the
//! clipper exposes; the JKA flag values live in `particle_physics`.

use super::*;
use crate::particle_physics::{CONTENTS_FOG, SURF_NOIMPACT, SURF_NOMARKS};
use jkr_bsp::{MAX_VERTICES_ON_POLY, MarkFragments, MarkProjection, SurfaceKind};

/// `MAX_DECAL_POINTS` / `MAX_DECAL_FRAGMENTS` (`tr_decals.cpp:157-158`).
const MAX_DECAL_POINTS: usize = 384;
const MAX_DECAL_FRAGMENTS: usize = 128;
/// Surface list capacity handed to `R_BoxSurfaces_r` (`tr_marks.cpp:313`).
const MAX_MARK_SURFACES: usize = 64;
/// `MAX_VERTS_ON_DECAL_POLY` (`tr_decals.cpp:26`).
pub(crate) const MAX_VERTICES_ON_DECAL: usize = 10;
/// Decals project 20 units into the surface (`tr_decals.cpp:198`).
const PROJECTION_DEPTH: f32 = 20.0;
/// Curved-patch triangles must face the projection (`tr_marks.cpp:364`).
/// The reference alternates -0.1 / -0.05 per cell half; JKR applies the
/// stricter limit to every triangle.
const GRID_FACING_LIMIT: f32 = -0.1;

/// One mark to project, as handed over by `FxScheduler` (`FxScheduler.cpp:1621`).
#[derive(Clone, Debug)]
pub(crate) struct DecalRequest {
    pub(crate) origin: Vec3,
    /// Impact normal; the mark faces along it.
    pub(crate) direction: Vec3,
    /// Texture rotation about the normal in degrees.
    pub(crate) orientation: f32,
    pub(crate) color: [f32; 4],
    pub(crate) radius: f32,
    pub(crate) shader: Arc<str>,
}

/// World-space point of a clipped decal with its texture coordinate.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct DecalVertex {
    pub(crate) position: [f32; 3],
    pub(crate) st: [f32; 2],
}

/// World triangles grouped by BSP surface, captured once at load.
pub(crate) struct DecalSurfaces {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
    /// Index range per BSP surface; empty for surfaces the renderer skipped.
    ranges: Vec<Range<u32>>,
}

impl DecalSurfaces {
    /// Capture the world batches. Only draws that belong to the world model
    /// take part (`R_BoxSurfaces_r` walks `tr.world->nodes`, so marks never
    /// land on inline models).
    pub(crate) fn from_flattened(scene: &FlattenedScene, bsp: &Bsp) -> Self {
        let mut ranges = vec![0..0; bsp.render().surfaces().len()];
        for draw in &scene.draws {
            let Some(surface_index) = draw.surface_index else {
                continue;
            };
            if !draw.world_surface || surface_index >= ranges.len() {
                continue;
            }
            if ranges[surface_index].is_empty() {
                ranges[surface_index] = draw.indices.clone();
            }
        }
        Self {
            positions: scene
                .vertices
                .iter()
                .map(|vertex| vertex.position)
                .collect(),
            normals: scene.vertices.iter().map(|vertex| vertex.normal).collect(),
            indices: scene.indices.clone(),
            ranges,
        }
    }

    fn triangles(&self, surface_index: usize) -> impl Iterator<Item = [usize; 3]> + '_ {
        let range = self.ranges.get(surface_index).cloned().unwrap_or(0..0);
        self.indices[range.start as usize..range.end as usize]
            .chunks_exact(3)
            .map(|triangle| {
                [
                    triangle[0] as usize,
                    triangle[1] as usize,
                    triangle[2] as usize,
                ]
            })
    }

    /// Outward triangle normal, oriented by the stored vertex normal so the
    /// facing test does not depend on the tessellator's winding.
    fn triangle_normal(&self, triangle: [usize; 3]) -> Vec3 {
        let [a, b, c] = triangle.map(|index| Vec3::from_array(self.positions[index]));
        let normal = (b - a).cross(c - a).normalize_or_zero();
        if normal.dot(Vec3::from_array(self.normals[triangle[0]])) < 0.0 {
            -normal
        } else {
            normal
        }
    }
}

/// Fixed-capacity buffers reused by every projection.
pub(crate) struct DecalScratch {
    surfaces: Vec<usize>,
    fragments: MarkFragments,
}

impl Default for DecalScratch {
    fn default() -> Self {
        Self {
            surfaces: Vec::with_capacity(MAX_MARK_SURFACES),
            fragments: MarkFragments::with_capacity(MAX_DECAL_POINTS, MAX_DECAL_FRAGMENTS),
        }
    }
}

/// Clip one request into the world and hand each fragment (at most
/// `MAX_VERTICES_ON_DECAL` points, `tr_decals.cpp:212-214`) to `emit`.
/// Returns the number of fragments emitted. Allocation-free.
pub(crate) fn project(
    world: &DecalSurfaces,
    bsp: &Bsp,
    request: &DecalRequest,
    scratch: &mut DecalScratch,
    mut emit: impl FnMut(&[DecalVertex]),
) -> usize {
    if request.radius <= 0.0 {
        return 0;
    }
    let axis = texture_axes(request.direction, request.orientation);
    let radius = request.radius;
    let corners = [
        request.origin - radius * axis[1] - radius * axis[2],
        request.origin + radius * axis[1] - radius * axis[2],
        request.origin + radius * axis[1] + radius * axis[2],
        request.origin - radius * axis[1] + radius * axis[2],
    ]
    .map(|corner| corner.to_array());
    let projection = MarkProjection::new(&corners, (axis[0] * -PROJECTION_DEPTH).to_array());
    clip(world, bsp, &projection, scratch);
    let scale = 0.5 / radius;
    let mut vertices = [DecalVertex::default(); MAX_VERTICES_ON_POLY];
    let mut emitted = 0;
    for fragment in scratch.fragments.fragments() {
        let points = scratch.fragments.fragment_points(*fragment);
        let count = points.len().min(MAX_VERTICES_ON_DECAL);
        if count < 3 {
            continue;
        }
        for (vertex, point) in vertices.iter_mut().zip(points) {
            let delta = Vec3::from_array(*point) - request.origin;
            *vertex = DecalVertex {
                position: *point,
                // Mark shaders sample with `clampmap`; the atlas wraps, so the
                // clip epsilon must not push a coordinate past the edge.
                st: [
                    (0.5 + delta.dot(axis[1]) * scale).clamp(0.0, 1.0),
                    (0.5 + delta.dot(axis[2]) * scale).clamp(0.0, 1.0),
                ],
            };
        }
        emit(&vertices[..count]);
        emitted += 1;
    }
    emitted
}

/// Project a narrow continuous cut, following CG_CreateSaberMarks' four corners.
/// Uses the same surface filters and reusable clipping buffers as impact decals.
pub(crate) fn project_strip(
    world: &DecalSurfaces,
    bsp: &Bsp,
    start: Vec3,
    end: Vec3,
    normal: Vec3,
    width: f32,
    scratch: &mut DecalScratch,
    mut emit: impl FnMut(&[DecalVertex]),
) {
    let along = (end - start).normalize_or(Vec3::X);
    let across = along.cross(normal).normalize_or(Vec3::Y);
    let corners = [
        start - width * along - width * across,
        end + width * along - width * across,
        end + width * along + width * across,
        start - width * along + width * across,
    ]
    .map(|p| p.to_array());
    let projection = MarkProjection::new(&corners, (-normal).to_array());
    clip(world, bsp, &projection, scratch);
    let center = (start + end) * 0.5;
    let mut vertices = [DecalVertex::default(); MAX_VERTICES_ON_DECAL];
    for fragment in scratch.fragments.fragments() {
        let points = scratch.fragments.fragment_points(*fragment);
        let count = points.len().min(MAX_VERTICES_ON_DECAL);
        if count < 3 {
            continue;
        }
        for (vertex, point) in vertices.iter_mut().zip(points) {
            let delta = Vec3::from_array(*point) - center;
            *vertex = DecalVertex {
                position: *point,
                st: [
                    (0.5 + delta.dot(along) * 0.065).clamp(0., 1.),
                    (0.5 + delta.dot(across) * 0.175).clamp(0., 1.),
                ],
            };
        }
        emit(&vertices[..count]);
    }
}

fn clip(world: &DecalSurfaces, bsp: &Bsp, projection: &MarkProjection, scratch: &mut DecalScratch) {
    let direction = Vec3::from_array(projection.direction());

    scratch.surfaces.clear();
    scratch.fragments.clear();
    let shaders = bsp.shaders();
    bsp.mark_surfaces(
        projection,
        MAX_MARK_SURFACES,
        &mut scratch.surfaces,
        |_, surface| {
            let shader = &shaders[surface.shader];
            shader.surface_flags & (SURF_NOIMPACT | SURF_NOMARKS) == 0
                && shader.content_flags & CONTENTS_FOG == 0
        },
    );
    let surfaces = bsp.render().surfaces();
    'surfaces: for &surface_index in &scratch.surfaces {
        let curved = match surfaces[surface_index].kind {
            SurfaceKind::Planar => false,
            SurfaceKind::Patch => true,
            _ => continue,
        };
        for triangle in world.triangles(surface_index) {
            if curved && world.triangle_normal(triangle).dot(direction) >= GRID_FACING_LIMIT {
                continue;
            }
            let polygon = triangle.map(|index| world.positions[index]);
            scratch.fragments.push_polygon(&polygon, projection);
            if scratch.fragments.is_full() {
                break 'surfaces;
            }
        }
    }
}

/// `tr_decals.cpp:180-184`: axis 0 along the normal, axis 1 any
/// perpendicular rotated by `orientation` about the normal, axis 2 closing
/// the frame.
fn texture_axes(direction: Vec3, orientation: f32) -> [Vec3; 3] {
    let forward = direction.normalize_or(Vec3::Z);
    let seed = perpendicular_vector(forward);
    let rotated = Quat::from_axis_angle(forward, orientation.to_radians()) * seed;
    [forward, forward.cross(rotated), rotated]
}

/// `PerpendicularVector` (`q_math.c`): project the unit axis of the smallest
/// component onto the plane and normalise.
fn perpendicular_vector(source: Vec3) -> Vec3 {
    let components = source.to_array();
    let mut smallest = 0;
    for axis in 1..3 {
        if components[axis].abs() < components[smallest].abs() {
            smallest = axis;
        }
    }
    let mut seed = Vec3::ZERO;
    seed[smallest] = 1.0;
    (seed - source * seed.dot(source)).normalize_or(Vec3::Y)
}
