//! Ghoul2's skin gore (`G2API_AddSkinGore`, OpenJK `codemp/rd-vanilla/G2_API.cpp:2777-2809`,
//! and `G2_GorePolys`, `G2_misc.cpp:787-1027`): a mark laid flat along a shot onto a posed
//! model. The shot is brought into model space by the inverse of the entity's world
//! matrix; at each LOD asked for, the first model is skinned and its surfaces walked as a
//! trace walks them. Each vertex of a surface gets mark coordinates on two axes across the
//! shot, turned by the mark's angle, so the mark's square reaches its size either side of
//! the hit; a surface wholly off one edge of the square is passed over, and of the rest
//! each triangle is kept unless all three corners lie off the same edge. The square has no
//! depth: whatever lies along the shot inside it is marked, the far side of a body too.
//!
//! Format behaviour only: which mark, its size, angle and time on the model are the
//! caller's.

use crate::ModelError;
use crate::g2_collision::{PosedModel, skin, walk};
use crate::g2_math::{
    add, cross, dot, inverse, normalize, scale, sub, transform, transform_and_translate,
    world_matrix,
};
use std::ops::Range;

/// A mark to lay on a model: `SSkinGoreData`'s geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GoreProjection {
    /// The hit, in the world (`hitLocation`).
    pub hit: [f32; 3],
    /// The shot's direction in the world, of unit length (`rayDirection`).
    pub direction: [f32; 3],
    /// How far the mark reaches from the hit along each of its two axes (`SSize`,
    /// `TSize`): its square is twice that across.
    pub size: [f32; 2],
    /// Its turn about the shot, in radians (`theta`).
    pub angle: f32,
    /// Whether triangles whose winding faces along the shot, and those facing back up
    /// it, are marked (`frontFaces`, `backFaces`).
    pub front_faces: bool,
    pub back_faces: bool,
}

/// One surface's share of a mark at one LOD (`GoreTextureCoordinates::tex[lod]`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GoreLod {
    /// The surface's own vertices the mark uses, in the order its triangles first use them.
    pub vertices: Vec<u32>,
    /// Each one's mark coordinates (s, t); the mark's picture spans 0 to 1.
    pub coordinates: Vec<[f32; 2]>,
    /// The marked triangles, as indices into `vertices`.
    pub triangles: Vec<[u32; 3]>,
}

/// A mark's record on one surface (`SGoreSurface`): what Ghoul2 counts as one gore mark.
#[derive(Clone, Debug, PartialEq)]
pub struct GoreSurface {
    /// The surface's index in the model.
    pub surface: usize,
    /// Its share at each LOD, indexed by LOD; `None` where the mark missed it there.
    pub lods: Vec<Option<GoreLod>>,
}

/// Reused storage, so laying a mark allocates nothing but its own records once warm.
#[derive(Default)]
pub struct GoreScratch {
    positions: Vec<[f32; 3]>,
    vertices: Vec<MarkVertex>,
}

/// `SVertexTemp`: a vertex's mark coordinates, the edges it lies beyond, and its index in
/// the surface's share once a kept triangle uses it.
#[derive(Clone, Copy)]
struct MarkVertex {
    coordinates: [f32; 2],
    outside: u8,
    share_index: u32,
}

const UNUSED: u32 = u32::MAX;
/// `GORE_MARGIN`.
const MARGIN: f32 = 0.0;

/// `G2API_AddSkinGore` on the instance's first model (`cg_g2MarksAllModels` 0): the
/// records the mark makes, one per surface it touches at any of `lods`, appended to `out`;
/// returns how many. `axes` (`AnglesToAxis`) and `origin` place the entity as
/// `G2_GenerateWorldMatrix` does. LODs the model lacks are passed over.
pub fn add_skin_gore(
    model: &PosedModel<'_>,
    origin: [f32; 3],
    axes: [[f32; 3]; 3],
    projection: &GoreProjection,
    lods: Range<usize>,
    scratch: &mut GoreScratch,
    out: &mut Vec<GoreSurface>,
) -> Result<usize, ModelError> {
    let inverse = inverse(&world_matrix(axes, origin));
    let hit = transform_and_translate(projection.hit, &inverse);
    let direction = transform(projection.direction, &inverse);
    let (s_axis, t_axis) = mark_axes(direction, projection.size, projection.angle);
    let first = out.len();
    let lod_count = model.glm.lods.len();
    for lod in lods.start..lods.end.min(lod_count) {
        walk(model, 0, &mut |surface| {
            let source = skin(model, lod, surface, &mut scratch.positions)?;
            let mut share = GoreLod::default();
            if !mark_surface(
                &scratch.positions,
                &source.triangles,
                hit,
                direction,
                [s_axis, t_axis],
                projection,
                &mut scratch.vertices,
                &mut share,
            ) {
                return Ok(true);
            }
            // `GoreTagsTemp`: one record per surface for the whole call.
            let record = match out[first..]
                .iter()
                .position(|record| record.surface == surface)
            {
                Some(index) => first + index,
                None => {
                    out.push(GoreSurface {
                        surface,
                        lods: vec![None; lod_count],
                    });
                    out.len() - 1
                }
            };
            out[record].lods[lod] = Some(share);
            Ok(true)
        })?;
    }
    Ok(out.len() - first)
}

/// The mark's two axes across a shot along `direction` (model space), each scaled so a
/// coordinate runs from 0 to 1 over the mark's size, and turned by `angle`.
fn mark_axes(direction: [f32; 3], size: [f32; 2], angle: f32) -> ([f32; 3], [f32; 3]) {
    let mut basis2 = [0.0, 0.0, 1.0];
    let mut basis1 = cross(direction, basis2);
    if dot(basis1, basis1) < 0.1 {
        basis2 = [0.0, 1.0, 0.0];
        basis1 = cross(direction, basis2);
    }
    basis2 = cross(direction, basis1);
    normalize(&mut basis1);
    normalize(&mut basis2);
    // `cos` and `sin` of the float angle, in double, kept as floats.
    let (c, s) = (f64::from(angle).cos() as f32, f64::from(angle).sin() as f32);
    let [s_size, t_size] = size;
    let t_axis = add(
        scale(basis1, 0.5 * c / t_size),
        scale(basis2, 0.5 * s / t_size),
    );
    let s_axis = add(
        scale(basis1, -0.5 * s / s_size),
        scale(basis2, 0.5 * c / s_size),
    );
    (s_axis, t_axis)
}

/// `G2_GorePolys` for one skinned surface: its share of the mark in `share`, and whether
/// there is any.
#[allow(clippy::too_many_arguments)]
fn mark_surface(
    positions: &[[f32; 3]],
    triangles: &[[u32; 3]],
    hit: [f32; 3],
    direction: [f32; 3],
    [s_axis, t_axis]: [[f32; 3]; 2],
    projection: &GoreProjection,
    vertices: &mut Vec<MarkVertex>,
    share: &mut GoreLod,
) -> bool {
    vertices.clear();
    let mut all_outside = 15_u8;
    for position in positions {
        let delta = sub(*position, hit);
        let s = dot(delta, s_axis) + 0.5;
        let t = dot(delta, t_axis) + 0.5;
        let mut inside = 0_u8;
        if s > MARGIN {
            inside |= 1;
        }
        if s < 1.0 - MARGIN {
            inside |= 2;
        }
        if t > MARGIN {
            inside |= 4;
        }
        if t < 1.0 - MARGIN {
            inside |= 8;
        }
        let outside = !inside;
        all_outside &= outside;
        vertices.push(MarkVertex {
            coordinates: [s, t],
            outside,
            share_index: UNUSED,
        });
    }
    if all_outside != 0 {
        // Completely off the mark.
        return false;
    }
    for corners in triangles {
        let [a, b, c] = corners.map(|index| index as usize);
        if a.max(b).max(c) >= vertices.len() {
            continue;
        }
        if 15 & vertices[a].outside & vertices[b].outside & vertices[c].outside != 0 {
            continue;
        }
        if !projection.front_faces || !projection.back_faces {
            let normal = cross(
                sub(positions[b], positions[a]),
                sub(positions[c], positions[a]),
            );
            let along = dot(direction, normal) > 0.0;
            if (along && !projection.front_faces) || (!along && !projection.back_faces) {
                continue;
            }
        }
        let triangle = [a, b, c].map(|index| {
            let vertex = &mut vertices[index];
            if vertex.share_index == UNUSED {
                vertex.share_index = share.vertices.len() as u32;
                share.vertices.push(index as u32);
                share.coordinates.push(vertex.coordinates);
            }
            vertex.share_index
        });
        share.triangles.push(triangle);
    }
    !share.vertices.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Glm, GlmLod, GlmSurface, GlmSurfaceHierarchy, GlmVertex, GlmWeight};

    const IDENTITY: [[f32; 4]; 3] = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
    ];
    const FORWARD: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

    /// A square in the plane x = `x`, `half` units either way, as two triangles.
    fn square(x: f32, half: f32) -> GlmSurface {
        let corner = |y: f32, z: f32| GlmVertex {
            normal: [1.0, 0.0, 0.0],
            position: [x, y, z],
            texture_coordinates: [0.0, 0.0],
            weights: vec![GlmWeight {
                bone_reference: 0,
                weight: 1.0,
            }],
        };
        GlmSurface {
            hierarchy_index: 0,
            vertices: vec![
                corner(-half, -half),
                corner(half, -half),
                corner(half, half),
                corner(-half, half),
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            bone_references: vec![0],
        }
    }

    /// Surfaces in a chain (each the child of the one before), with these flags.
    fn model(surfaces: Vec<(GlmSurface, u32)>) -> Glm {
        let count = surfaces.len();
        let hierarchy = (0..count)
            .map(|index| GlmSurfaceHierarchy {
                name: format!("s{index}"),
                flags: surfaces[index].1,
                shader: String::new(),
                parent: index.checked_sub(1),
                children: if index + 1 < count {
                    vec![index + 1]
                } else {
                    Vec::new()
                },
            })
            .collect();
        Glm {
            name: "test".into(),
            animation_name: "test".into(),
            bone_count: 1,
            hierarchy,
            lods: vec![GlmLod {
                surfaces: surfaces.into_iter().map(|(surface, _)| surface).collect(),
            }],
        }
    }

    fn posed(glm: &Glm) -> PosedModel<'_> {
        PosedModel {
            glm,
            bones: &[IDENTITY],
            placement: IDENTITY,
            overrides: &[],
            scale: [1.0; 3],
        }
    }

    fn shot(hit: [f32; 3], size: f32, angle: f32) -> GoreProjection {
        GoreProjection {
            hit,
            direction: [-1.0, 0.0, 0.0],
            size: [size, size],
            angle,
            front_faces: true,
            back_faces: true,
        }
    }

    fn lay(
        glm: &Glm,
        origin: [f32; 3],
        axes: [[f32; 3]; 3],
        projection: GoreProjection,
    ) -> Vec<GoreSurface> {
        let mut out = Vec::new();
        let added = add_skin_gore(
            &posed(glm),
            origin,
            axes,
            &projection,
            0..3,
            &mut GoreScratch::default(),
            &mut out,
        )
        .unwrap();
        assert_eq!(added, out.len());
        out
    }

    #[test]
    fn a_mark_spans_its_size_about_the_hit_on_every_surface_along_the_shot() {
        // A wall in front, another behind it along the shot, one far off to the side.
        let glm = model(vec![
            (square(0.0, 16.0), 0),
            (square(-20.0, 16.0), 0),
            (
                GlmSurface {
                    vertices: square(0.0, 4.0)
                        .vertices
                        .into_iter()
                        .map(|mut vertex| {
                            vertex.position[1] += 100.0;
                            vertex
                        })
                        .collect(),
                    ..square(0.0, 4.0)
                },
                0,
            ),
        ]);
        let marks = lay(&glm, [0.0; 3], FORWARD, shot([0.0; 3], 8.0, 0.0));
        // The far wall is marked too (no depth), the one to the side is not.
        assert_eq!(
            marks.iter().map(|mark| mark.surface).collect::<Vec<_>>(),
            [0, 1]
        );
        let front = marks[0].lods[0].as_ref().unwrap();
        assert_eq!(front.triangles.len(), 2);
        assert_eq!(front.vertices, [0, 1, 2, 3]);
        // Across a shot along -x: s runs down z, t along y, the size to each edge.
        assert_eq!(front.coordinates[0], [1.5, -0.5]);
        assert_eq!(front.coordinates[2], [-0.5, 1.5]);
        // The model has one LOD; the rest are passed over.
        assert_eq!(marks[0].lods.len(), 1);
    }

    #[test]
    fn a_triangle_wholly_off_one_edge_is_left_and_switched_off_surfaces_are_skipped() {
        // One surface, two loose triangles: one under the mark, one far along its t axis.
        let mut surface = square(0.0, 4.0);
        let far: Vec<GlmVertex> = surface.vertices[..3]
            .iter()
            .cloned()
            .map(|mut vertex| {
                vertex.position[1] += 100.0;
                vertex
            })
            .collect();
        surface.vertices.truncate(3);
        surface.vertices.extend(far);
        surface.triangles = vec![[3, 4, 5], [0, 1, 2]];
        let glm = model(vec![(surface, 0)]);
        let marks = lay(&glm, [0.0; 3], FORWARD, shot([0.0; 3], 8.0, 0.0));
        let share = marks[0].lods[0].as_ref().unwrap();
        // Only the near triangle, its corners renumbered in first-use order.
        assert_eq!(share.triangles, [[0, 1, 2]]);
        assert_eq!(share.vertices, [0, 1, 2]);
        // Switched off (flags 1), and below a no-descendants surface (0x100), nothing is marked.
        let glm = model(vec![
            (square(0.0, 16.0), 1),
            (square(-1.0, 16.0), 0x100),
            (square(-2.0, 16.0), 0),
        ]);
        let marks = lay(&glm, [0.0; 3], FORWARD, shot([0.0; 3], 8.0, 0.0));
        assert!(marks.is_empty());
    }

    #[test]
    fn the_mark_turns_with_its_angle_and_follows_the_entitys_placement() {
        let glm = model(vec![(square(0.0, 16.0), 0)]);
        let straight = lay(&glm, [0.0; 3], FORWARD, shot([0.0; 3], 8.0, 0.0));
        let turned = lay(
            &glm,
            [0.0; 3],
            FORWARD,
            shot([0.0; 3], 8.0, std::f32::consts::FRAC_PI_2),
        );
        let [s, t] = straight[0].lods[0].as_ref().unwrap().coordinates[1];
        let [s2, t2] = turned[0].lods[0].as_ref().unwrap().coordinates[1];
        // A quarter turn: the new s runs against the old t, the new t with the old s.
        assert!((s2 - (1.0 - t)).abs() < 1e-5 && (t2 - s).abs() < 1e-5);
        // The entity turned a quarter left and moved: a shot along its new facing at the
        // same spot on it marks the same coordinates.
        let left = [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        let moved = lay(
            &glm,
            [10.0, 20.0, 0.0],
            left,
            GoreProjection {
                hit: [10.0, 20.0, 0.0],
                direction: [0.0, -1.0, 0.0],
                ..shot([0.0; 3], 8.0, 0.0)
            },
        );
        let placed = &moved[0].lods[0].as_ref().unwrap().coordinates;
        let unplaced = &straight[0].lods[0].as_ref().unwrap().coordinates;
        for (a, b) in placed.iter().zip(unplaced) {
            assert!((a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5);
        }
    }
}
