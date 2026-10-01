//! World surfaces the client moves on its own.
//!
//! A prop is a static world face cut out of the world batches at load time
//! and drawn through the mover path instead, so it can be offset per frame
//! like an inline model. The first prop is the menu's gate on mp/ffa3: one
//! planar face split along its seam into two leaves that slide apart when
//! the client connects to a server. The map's own geometry is untouched — a
//! prop at rest draws exactly where the world batch drew it.

use super::movers::Mesh;
use super::scene_flatten::FlattenedScene;
use super::{ActorDraw, GpuVertex};
use jkr_bsp::Bsp;

/// One authored prop: which world face to detach (by shader and bounds),
/// where to split it, and how its leaves move as it opens.
#[derive(Debug, PartialEq)]
pub(crate) struct PropSpec {
    /// Worldspawn `message` of the map the face belongs to.
    pub(crate) map_message: &'static str,
    /// Shader of the face; together with `bounds` it identifies the face.
    pub(crate) shader: &'static str,
    /// World-space box every vertex of the face lies in (1 unit of slack).
    pub(crate) bounds: ([f32; 3], [f32; 3]),
    /// Axis (0 = x, 1 = y, 2 = z) and coordinate of the seam between the
    /// two leaves.
    pub(crate) split_axis: usize,
    pub(crate) split_at: f32,
    /// Offset both leaves take first, before they slide (the gate unseals
    /// backwards out of its frame).
    pub(crate) unseal: [f32; 3],
    /// How far each leaf slides along the split axis, away from the seam.
    pub(crate) travel: f32,
    /// The doorway the prop closes, for the world seen through it: its
    /// centre on the floor and the yaw (degrees) pointing through.
    pub(crate) doorway_origin: [f32; 3],
    pub(crate) doorway_yaw: f32,
}

/// The opening, in shares of the open fraction: the leaves first crack
/// apart a little, rest while the dust they shook loose settles, then
/// unseal and slide fully open.
const CRACK_SPAN: f32 = 0.18;
const HOLD_SPAN: f32 = 0.32;
/// How far each leaf moves along the split axis during the crack.
const CRACK_TRAVEL: f32 = 6.0;
/// Effect played where the leaves part, at each cue.
pub(crate) const DUST_EFFECT: &str = "effects/env/impact_dustonly.efx";

/// Moments of the opening worth an effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GateCue {
    /// The leaves have just started to part.
    Crack,
    /// The rest is over; the leaves are opening fully.
    Open,
}

/// The cue, if any, the open fraction passed going from `before` to
/// `after` (only while opening).
pub(crate) fn gate_cue(before: f32, after: f32) -> Option<GateCue> {
    let open_at = CRACK_SPAN + HOLD_SPAN;
    if before <= 0.0 && after > 0.0 {
        Some(GateCue::Crack)
    } else if before < open_at && after >= open_at {
        Some(GateCue::Open)
    } else {
        None
    }
}

/// One detached leaf: which mover mesh draws it and how it moves.
pub(crate) struct Leaf {
    pub(crate) mesh: usize,
    spec: &'static PropSpec,
    /// −1 for the leaf below the seam along the split axis, +1 above.
    side: f32,
}

impl PropSpec {
    /// The doorway the prop closes.
    pub(crate) fn doorway(&self) -> crate::portal::Frame {
        crate::portal::Frame {
            origin: glam::Vec3::from_array(self.doorway_origin),
            yaw: self.doorway_yaw.to_radians(),
        }
    }
}

impl Leaf {
    /// The doorway this leaf closes.
    pub(crate) fn doorway(&self) -> crate::portal::Frame {
        self.spec.doorway()
    }

    /// World-space offset of the leaf when the prop is `open` (0 = at rest,
    /// 1 = fully open).
    pub(crate) fn offset(&self, open: f32) -> [f32; 3] {
        let open = open.clamp(0.0, 1.0);
        let crack = smooth_step((open / CRACK_SPAN).min(1.0));
        let open_span = 1.0 - CRACK_SPAN - HOLD_SPAN;
        let wide = smooth_step(((open - CRACK_SPAN - HOLD_SPAN) / open_span).clamp(0.0, 1.0));
        let mut offset = self.spec.unseal.map(|axis| axis * wide);
        let slide = CRACK_TRAVEL * crack + (self.spec.travel - CRACK_TRAVEL) * wide;
        offset[self.spec.split_axis] += self.side * slide;
        offset
    }
}

impl PropSpec {
    /// Where dust falls from as the leaves part: the top and the foot of
    /// the seam, just on the near side of the face.
    pub(crate) fn dust_points(&self) -> [[f32; 3]; 2] {
        let (low, high) = self.bounds;
        let doorway = self.doorway();
        let near = doorway.origin - doorway.forward() * 6.0;
        let mut top = [near.x, near.y, high[2] - 12.0];
        let mut foot = [near.x, near.y, low[2] + 4.0];
        top[self.split_axis] = self.split_at;
        foot[self.split_axis] = self.split_at;
        [top, foot]
    }
}

fn smooth_step(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// A detached leaf's mesh and its motion, before the mesh has a slot in the
/// mover catalog.
pub(crate) struct Detached {
    mesh: Mesh,
    spec: &'static PropSpec,
    side: f32,
}

impl Detached {
    /// Split into the mesh for catalog slot `mesh` and the leaf that moves it.
    pub(crate) fn placed(self, mesh: usize) -> (Mesh, Leaf) {
        let leaf = Leaf {
            mesh,
            spec: self.spec,
            side: self.side,
        };
        (self.mesh, leaf)
    }
}

/// Cut every prop authored for this map out of the static world draws. The
/// face's triangles are reordered in place so each leaf is one contiguous
/// index range; the vertices stay where they are.
pub(crate) fn detach(bsp: &Bsp, flattened: &mut FlattenedScene) -> Vec<Detached> {
    let Some(message) = super::menu_backdrop::worldspawn_message(bsp) else {
        return Vec::new();
    };
    let specs = super::menu_backdrop::props_for(&message);
    let mut detached = Vec::new();
    let FlattenedScene {
        vertices,
        indices,
        materials,
        draws,
    } = flattened;
    for spec in specs {
        for draw in draws.iter_mut() {
            let range = draw.indices.start as usize..draw.indices.end as usize;
            if !draw.world_surface
                || materials[draw.material].shader != spec.shader
                || !within(spec.bounds, &indices[range.clone()], vertices)
            {
                continue;
            }
            draw.world_surface = false;
            let split = partition(&mut indices[range.clone()], vertices, spec);
            let seam = draw.indices.start + split as u32;
            let leaves = [
                (-1.0, draw.indices.start..seam),
                (1.0, seam..draw.indices.end),
            ];
            for (side, leaf) in leaves {
                detached.push(Detached {
                    mesh: Mesh {
                        model_index: None,
                        draws: vec![ActorDraw {
                            indices: leaf,
                            material: draw.material,
                        }],
                    },
                    spec,
                    side,
                });
            }
        }
    }
    detached
}

fn within(bounds: ([f32; 3], [f32; 3]), indices: &[u32], vertices: &[GpuVertex]) -> bool {
    indices.iter().all(|&index| {
        let position = vertices[index as usize].position;
        (0..3).all(|axis| {
            position[axis] >= bounds.0[axis] - 1.0 && position[axis] <= bounds.1[axis] + 1.0
        })
    })
}

/// Stable-partition the triangles of `indices` by which side of the seam
/// their centroid lies on (below first) and return where the upper side
/// starts.
fn partition(indices: &mut [u32], vertices: &[GpuVertex], spec: &PropSpec) -> usize {
    let below = |triangle: &[u32]| {
        let centroid: f32 = triangle
            .iter()
            .map(|&index| vertices[index as usize].position[spec.split_axis])
            .sum::<f32>()
            / 3.0;
        centroid < spec.split_at
    };
    let mut ordered = Vec::with_capacity(indices.len());
    ordered.extend(indices.chunks(3).filter(|tri| below(tri)).flatten());
    let split = ordered.len();
    ordered.extend(indices.chunks(3).filter(|tri| !below(tri)).flatten());
    indices.copy_from_slice(&ordered);
    split
}
