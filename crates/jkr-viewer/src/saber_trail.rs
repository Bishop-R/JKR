//! Fixed-capacity stock saber motion trails.

use crate::saber::{Blade, Extension};
use crate::saber_rgb::BladeColor;
use bytemuck::{Pod, Zeroable};
use glam::Vec3;

const MAX_ENTITIES: usize = 1_024;
const SABERS_PER_ENTITY: usize = 2;
const BLADES_PER_SABER: usize = 8;
pub(crate) const MAX_SEGMENTS: usize = 4_096;

#[path = "thrown_saber_tilt.rs"]
pub(crate) mod tilt;

/// One stock trail vertex in the order passed to `FX_AddPrimitive`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub(crate) position: [f32; 3],
    pub(crate) uv: [f32; 2],
    pub(crate) color: [f32; 4],
    pub(crate) style: u32,
    pub(crate) _padding: u32,
}

impl Vertex {
    pub(crate) fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
            0 => Float32x3,
            1 => Float32x2,
            2 => Float32x4,
            3 => Uint32
        ];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBUTES,
        }
    }
}

/// Four source vertices for one motion slice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Quad {
    pub(crate) vertices: [Vertex; 4],
    pub(crate) lifetime_millis: u16,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Trail {
    base: [f32; 3],
    tip: [f32; 3],
    last_time: i64,
    initialized: bool,
}

impl Trail {
    /// Add a slice and then retain the current endpoints exactly like
    /// `CG_AddSaberBlade` (`codemp/cgame/cg_players.c:6297-6319,6400-6470,
    /// 6504-6507`). `cg_saberTrail 2` deliberately follows mode 1 because
    /// the disabled stencil experiment is outside this renderer.
    pub(crate) fn update(
        &mut self,
        blade: Blade,
        now: i64,
        authored_duration: u16,
        style: u8,
        color: BladeColor,
        enabled: bool,
    ) -> Option<Quad> {
        if style > 1 || now <= self.last_time + 2 {
            return None;
        }
        let direction = Vec3::from_array(blade.direction);
        let base = blade.base;
        // `end` already receives +1 at cg_players.c:6110; line 6403 adds 3.
        let tip = (Vec3::from_array(base) + direction * (blade.length + 4.0)).to_array();
        let diff = now.saturating_sub(self.last_time);
        let quad = (enabled && self.initialized && now < self.last_time + 2_000)
            .then(|| {
                build_quad(
                    base,
                    tip,
                    self.base,
                    self.tip,
                    diff,
                    authored_duration,
                    style,
                    color,
                )
            })
            .flatten();
        self.base = base;
        self.tip = tip;
        self.last_time = now;
        self.initialized = true;
        quad
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BladeState {
    pub(crate) extension: Extension,
    pub(crate) trail: Trail,
}

/// O(1) per-entity/per-saber/per-blade state with one load-time allocation,
/// plus each owner's thrown-saber tilt.
pub(crate) struct StateSlab {
    states: Box<[BladeState]>,
    tilts: Box<[tilt::ThrowTilt]>,
}

impl Default for StateSlab {
    fn default() -> Self {
        Self {
            states: vec![
                BladeState::default();
                MAX_ENTITIES * SABERS_PER_ENTITY * BLADES_PER_SABER
            ]
            .into_boxed_slice(),
            tilts: vec![tilt::ThrowTilt::default(); MAX_ENTITIES].into_boxed_slice(),
        }
    }
}

impl StateSlab {
    pub(crate) fn blade_mut(
        &mut self,
        entity: u64,
        saber: usize,
        blade: usize,
    ) -> Option<&mut BladeState> {
        let entity = usize::try_from(entity.checked_sub(1)?).ok()?;
        let index = entity
            .checked_mul(SABERS_PER_ENTITY * BLADES_PER_SABER)?
            .checked_add(saber.checked_mul(BLADES_PER_SABER)?)?
            .checked_add(blade)?;
        self.states.get_mut(index)
    }

    /// The thrown-saber tilt kept on the owner's entity (`centity_t::bolt3`).
    pub(crate) fn tilt_mut(&mut self, owner_entity: u64) -> Option<&mut tilt::ThrowTilt> {
        self.tilts
            .get_mut(usize::try_from(owner_entity.checked_sub(1)?).ok()?)
    }
}

#[derive(Clone, Copy, Debug)]
struct Segment {
    quad: Quad,
    spawned_at: i64,
    active: bool,
}

impl Default for Segment {
    fn default() -> Self {
        Self {
            quad: Quad {
                vertices: [Vertex::zeroed(); 4],
                lifetime_millis: 0,
            },
            spawned_at: 0,
            active: false,
        }
    }
}

/// Bounded persistent trail slices; exhausted pools drop new slices.
pub(crate) struct SegmentPool {
    segments: Box<[Segment]>,
    cursor: usize,
    dropped: u64,
    inserted: u64,
}

impl Default for SegmentPool {
    fn default() -> Self {
        Self {
            segments: vec![Segment::default(); MAX_SEGMENTS].into_boxed_slice(),
            cursor: 0,
            dropped: 0,
            inserted: 0,
        }
    }
}

impl SegmentPool {
    pub(crate) fn insert(&mut self, quad: Quad, now: i64) -> bool {
        for offset in 0..self.segments.len() {
            let index = (self.cursor + offset) % self.segments.len();
            let segment = &mut self.segments[index];
            let expired = now >= segment.spawned_at + i64::from(segment.quad.lifetime_millis);
            if !segment.active || expired {
                *segment = Segment {
                    quad,
                    spawned_at: now,
                    active: true,
                };
                self.cursor = (index + 1) % self.segments.len();
                self.inserted = self.inserted.saturating_add(1);
                return true;
            }
        }
        self.dropped = self.dropped.saturating_add(1);
        false
    }

    pub(crate) fn append_vertices(&mut self, now: i64, output: &mut Vec<Vertex>) -> usize {
        let start = output.len();
        for segment in &mut self.segments {
            if !segment.active {
                continue;
            }
            let age = now.saturating_sub(segment.spawned_at);
            if age >= i64::from(segment.quad.lifetime_millis) {
                segment.active = false;
                continue;
            }
            // The stock trail never fades its colour: `CTrail::Update`
            // (`codemp/cgame/FxPrimitives.cpp:1771-1789`) scrolls U from `ST`
            // toward `destST = ST + 1` (clamped at 1) over the lifetime, so the
            // clamp-mapped glow texture's dark edge sweeps across the slice.
            // The shader derives that scroll from `1 - color.a`.
            let fade = 1.0 - age as f32 / f32::from(segment.quad.lifetime_millis);
            let vertices = segment.quad.vertices.map(|mut vertex| {
                vertex.color[3] = fade;
                vertex
            });
            output.extend([
                vertices[0],
                vertices[1],
                vertices[2],
                vertices[0],
                vertices[2],
                vertices[3],
            ]);
        }
        (output.len() - start) / 6
    }
}

pub(crate) fn trail_duration(authored_duration: u16) -> u16 {
    let duration = authored_duration / 5;
    if duration == 0 { 40 } else { duration }
}

fn build_quad(
    new_base: [f32; 3],
    new_tip: [f32; 3],
    old_base: [f32; 3],
    old_tip: [f32; 3],
    diff: i64,
    authored_duration: u16,
    style: u8,
    color: BladeColor,
) -> Option<Quad> {
    if diff > 10_000 {
        return None;
    }
    let lifetime_millis = trail_duration(authored_duration);
    let old_alpha = 1.0 - diff as f32 / f32::from(lifetime_millis);
    let rgb = if style == 1 {
        [32.0 / 255.0; 3]
    } else {
        color.trail_rgb()
    };
    let vertex = |position, uv| Vertex {
        position,
        uv,
        color: [rgb[0], rgb[1], rgb[2], 1.0],
        style: u32::from(style),
        _padding: 0,
    };
    Some(Quad {
        vertices: [
            vertex(new_base, [0.0, 1.0]),
            vertex(new_tip, [0.0, 0.0]),
            vertex(old_tip, [1.0 - old_alpha, 0.0]),
            vertex(old_base, [1.0 - old_alpha, 1.0]),
        ],
        lifetime_millis: if style == 1 {
            lifetime_millis.saturating_mul(2)
        } else {
            lifetime_millis
        },
    })
}
