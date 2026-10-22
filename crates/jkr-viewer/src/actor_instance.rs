//! Per-instance GPU record shared by skinned actors, rigid MD3 objects and
//! instanced inline models.
//!
//! Besides the transform and entity colour, every instance carries the
//! entity light computed on the CPU (`entity_lighting`): rd-vanilla evaluates
//! `R_SetupEntityLighting` once per refEntity (`tr_light.cpp:304-412`) and
//! `RB_CalcDiffuseColor` (`tr_shade_calc.cpp:1141-1191`) then shades each
//! vertex from those three vectors. They remain the legacy diffuse result and
//! missing-grid fallback; optional spatial diffuse samples the map at each fragment.

use bytemuck::{Pod, Zeroable};

/// Entity light on the 0..=1 colour scale, ready for the vertex stage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EntityLight {
    pub(crate) ambient: [f32; 3],
    pub(crate) directed: [f32; 3],
    /// Unit vector pointing towards the light.
    pub(crate) direction: [f32; 3],
}

impl EntityLight {
    /// Light used before the world grid is sampled: the reference's grid-less
    /// fallback (`tr_light.cpp:335-349`, 150 ambient + 32 minimum add, 150
    /// directed) lit from the default sun direction (`tr_bsp.cpp:2024-2028`).
    pub(crate) const FALLBACK: Self = Self {
        ambient: [182.0 / 255.0; 3],
        directed: [150.0 / 255.0; 3],
        direction: [0.428_57, 0.285_71, 0.857_14],
    };
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct ActorInstance {
    pub(crate) position: [f32; 3],
    /// 1.0 requests `RF_DEPTHHACK` (`tr_backend.cpp:906`: depth range 0..0.3
    /// so first-person models never clip into walls); 0.0 otherwise.
    pub(crate) depth_hack: f32,
    pub(crate) rotation: [f32; 4],
    pub(crate) scale: [f32; 3],
    /// One-based fog table slot; zero disables fog.
    pub(crate) fog_index: u32,
    pub(crate) entity_color: [f32; 4],
    pub(crate) shader_tex_coord: [f32; 2],
    pub(crate) entity_control: [f32; 2],
    pub(crate) light_ambient: [f32; 3],
    /// Bit 0: mirror/camera-portal only (RF_THIRD_PERSON). Bit 1: also
    /// submitted to the sky-portal scene (isPortalEnt).
    pub(crate) view_flags: u32,
    pub(crate) light_directed: [f32; 3],
    /// A disintegration's burn radius (`RF_DISINTEGRATE1`/`2`, [`Self::with_dissolve`]).
    dissolve_radius: f32,
    pub(crate) light_direction: [f32; 3],
    /// The burn's centre in the model's scaled space, packed by [`pack_dissolve_center`].
    dissolve_center: u32,
}

impl ActorInstance {
    /// `view_flags` bit of world geometry (the identity instance, movers): the stage
    /// program gives such an instance no entity light.
    pub(crate) const WORLD: u32 = 4;
    const ATTRIBUTES: [wgpu::VertexAttribute; 11] = [
        attribute(wgpu::VertexFormat::Float32x4, 0, 5),
        attribute(wgpu::VertexFormat::Float32x4, 16, 6),
        attribute(wgpu::VertexFormat::Float32x3, 32, 7),
        attribute(wgpu::VertexFormat::Float32x4, 48, 8),
        attribute(wgpu::VertexFormat::Float32x2, 64, 9),
        attribute(wgpu::VertexFormat::Float32x2, 72, 10),
        attribute(wgpu::VertexFormat::Float32x3, 80, 11),
        // The fourth components carry a disintegration's radius and packed centre.
        attribute(wgpu::VertexFormat::Float32x4, 96, 12),
        attribute(wgpu::VertexFormat::Float32x4, 112, 13),
        attribute(wgpu::VertexFormat::Uint32, 44, 14),
        attribute(wgpu::VertexFormat::Uint32, 92, 15),
    ];

    pub(crate) fn new(position: [f32; 3], rotation: [f32; 4], scale: [f32; 3]) -> Self {
        Self {
            position,
            depth_hack: 0.0,
            rotation,
            scale,
            fog_index: 0,
            entity_color: [1.0; 4],
            shader_tex_coord: [0.0; 2],
            entity_control: [0.0; 2],
            light_ambient: EntityLight::FALLBACK.ambient,
            view_flags: 0,
            light_directed: EntityLight::FALLBACK.directed,
            dissolve_radius: 0.0,
            light_direction: EntityLight::FALLBACK.direction,
            dissolve_center: 0,
        }
    }

    /// `view_flags` bits of a disintegration pass: 1 the body (`RF_DISINTEGRATE1`), 2 the
    /// glow (`RF_DISINTEGRATE2`).
    pub(crate) const DISSOLVE_SHIFT: u32 = 4;

    /// `view_flags` bit of `RF_VOLUMETRIC` ([`Self::volumetric`]).
    pub(crate) const VOLUMETRIC: u32 = 1 << 6;

    /// This instance drawn as `RF_VOLUMETRIC` (`tr_shade.cpp:1161-1188`): every stage's
    /// colour is the entity colour's red, dimmed where the model's surface faces the view.
    pub(crate) fn volumetric(mut self) -> Self {
        self.view_flags |= Self::VOLUMETRIC;
        self
    }

    /// This instance drawn as a disintegration pass `mode` (1 the body, 2 the glow) burning
    /// `radius` units out from `center`, a point in the model's own space with its scale
    /// applied (`RB_CalcDisintegrateColors`, `tr_shade_calc.cpp:1262-1340`).
    pub(crate) fn with_dissolve(mut self, mode: u32, center: [f32; 3], radius: f32) -> Self {
        self.view_flags =
            (self.view_flags & !(3 << Self::DISSOLVE_SHIFT)) | ((mode & 3) << Self::DISSOLVE_SHIFT);
        self.dissolve_radius = radius;
        self.dissolve_center = pack_dissolve_center(center);
        self
    }

    /// Replace the entity light consumed by `rgbGen lightingDiffuse`.
    /// The identity instance world statics bind: flagged as world geometry.
    pub(crate) fn world_identity() -> Self {
        Self {
            // Static BSP geometry participates in sky views as well as the main view.
            view_flags: Self::WORLD | 2,
            ..Self::new([0.; 3], [0., 0., 0., 1.], [1.; 3])
        }
    }

    pub(crate) fn set_light(&mut self, light: EntityLight) {
        self.light_ambient = light.ambient;
        self.light_directed = light.directed;
        self.light_direction = light.direction;
    }

    pub(crate) fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// A disintegration's centre in a word: each axis a signed ten-bit count of quarter units
/// (±128 units, well past a body's reach; the burn's bands are seven units and more wide),
/// x in the low bits. `unpack_dissolve_center` in `vertex_transform.wgsl` reads it back.
pub(crate) fn pack_dissolve_center(center: [f32; 3]) -> u32 {
    center.iter().enumerate().fold(0, |packed, (axis, value)| {
        let quarters = (value * 4.0).round().clamp(-511.0, 511.0) as i32;
        packed | (((quarters as u32) & 0x3ff) << (10 * axis))
    })
}

/// Rendering metadata only; snapshot state takes precedence over baselines.
pub(crate) fn scene_flags(
    game: Option<&jkr_protocol::GameState>,
    snapshot: Option<&jkr_protocol::Snapshot>,
    number: u16,
) -> u32 {
    snapshot
        .and_then(|s| {
            s.entities
                .binary_search_by_key(&number, |e| e.number())
                .ok()
                .map(|i| &s.entities[i])
        })
        .or_else(|| game.and_then(|g| g.baseline(usize::from(number))))
        .map_or(0, legacy_render_flags)
}

/// Stock `EF_SHADER_ANIM` / `RF_SETANIMINDEX`: retain the per-entity image frame
/// alongside view flags without enlarging the GPU instance or allocating per frame.
pub(crate) fn legacy_render_flags(state: &jkr_protocol::EntityState) -> u32 {
    let portal = if state.is_portal_entity() { 2 } else { 0 };
    portal
        | if state.e_flags() & 16 != 0 {
            8 | ((state.raw_field(83).unwrap_or(0) & 0xffff) << 8)
        } else {
            0
        }
}

const fn attribute(
    format: wgpu::VertexFormat,
    offset: u64,
    shader_location: u32,
) -> wgpu::VertexAttribute {
    wgpu::VertexAttribute {
        format,
        offset,
        shader_location,
    }
}

#[cfg(test)]
mod dissolve_tests {
    use super::*;

    #[test]
    fn a_burn_centre_packs_to_quarter_units() {
        let packed = pack_dissolve_center([1.25, -3.5, 40.0]);
        let field = |axis: u32| {
            let raw = ((packed >> (10 * axis)) & 0x3ff) as i32;
            (if raw >= 512 { raw - 1024 } else { raw }) as f32 / 4.0
        };
        assert_eq!([field(0), field(1), field(2)], [1.25, -3.5, 40.0]);
        // Out of reach clamps rather than wraps.
        assert_eq!(pack_dissolve_center([500.0, 0.0, 0.0]) & 0x3ff, 511);
        let instance = ActorInstance::new([0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0; 3])
            .with_dissolve(2, [0.0; 3], 9.0);
        assert_eq!(
            (instance.view_flags >> ActorInstance::DISSOLVE_SHIFT) & 3,
            2
        );
    }
}
