//! Codemp-compatible saber attachment and blade presentation.
//!
//! `codemp/cgame/cg_players.c::CG_AddSaberBlade` evaluates the player model's
//! first Ghoul2 bolt (`*r_hand`), attaches the saber weapon model there with
//! the raw renderer bolt, then reads the saber model's `*blade1` bolt through
//! `G2API_GetBoltMatrix` and takes its origin and **negative Y column** as
//! blade forward. That API applies the multiplayer 90° basis offset (see
//! [`crate::bolt`]); in raw bolt terms the blade runs along the negated X
//! column, i.e. from the tag triangle's first vertex through its origin
//! vertex. Keeping those choices here prevents renderer call sites from
//! reintroducing the historical sideways blade.

use super::PlayerPreview;
use crate::saber_rgb::{BladeColor, MATERIAL_COUNT};
use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Quat, Vec3};
use jkr_shader::ShaderCatalog;
use jkr_vfs::VirtualFileSystem;
use std::error::Error;
use std::ops::Range;

pub(crate) use crate::saber_hilts::{HiltCatalog, load_hilts};

pub(crate) const HAND_BOLT: &str = "*r_hand";
pub(crate) const LEFT_HAND_BOLT: &str = "*l_hand";

pub(crate) const MAX_BLADE_INSTANCES: usize = 1_024 * 2 * 8 * 2;

/// Model-space transform encoded by the player's right-hand Ghoul2 surface bolt.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Attachment {
    pub(crate) grip: [f32; 3],
    pub(crate) rotation: [f32; 4],
}

/// Saber-model-space blade origin and codemp forward direction.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BladeSocket {
    pub(crate) origin: [f32; 3],
    pub(crate) direction: [f32; 3],
}

/// Fully transformed blade submitted to the saber renderer.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Blade {
    pub(crate) base: [f32; 3],
    pub(crate) direction: [f32; 3],
    pub(crate) length: f32,
    pub(crate) radius: f32,
}

/// Per-entity blade extension state mirroring
/// `codemp/game/bg_saberLoad.c::BG_SI_SetLengthGradual`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Extension {
    length: f32,
    desired: f32,
    maximum: f32,
    extend_debounce: i64,
    active: bool,
    initialized: bool,
}

impl Extension {
    /// Advance one cgame presentation frame and return the visible length.
    pub(crate) fn update(&mut self, active: bool, maximum: f32, time_millis: i64) -> f32 {
        if !self.initialized {
            self.length = if active { maximum } else { 0.0 };
            self.desired = self.length;
            self.maximum = maximum;
            self.extend_debounce = time_millis;
            self.active = active;
            self.initialized = true;
            return self.length;
        }
        self.maximum = maximum;
        if active != self.active {
            self.active = active;
            self.desired = if active { maximum } else { 0.0 };
        }
        if self.length == self.desired {
            return self.length;
        }
        if self.length == self.maximum || self.length == 0.0 {
            self.extend_debounce = time_millis;
            if self.length == 0.0 {
                self.length += 1.0;
            } else {
                self.length -= 1.0;
            }
        }
        let amount = ((time_millis - self.extend_debounce) as f32 * 0.01).max(0.2);
        if self.length < self.desired {
            self.length = (self.length + amount).min(self.desired).min(self.maximum);
        } else {
            self.length = (self.length - amount).max(self.desired).max(0.0);
        }
        self.length
    }
}

/// One glow/core GPU instance. A glow carries its hilt sprite radius in colour alpha; a
/// core carries zero there.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct Instance {
    base: [f32; 3],
    length: f32,
    direction: [f32; 3],
    /// Drawn radius, after `CG_DoSaber`'s flicker and extension growth.
    radius: f32,
    color: [f32; 4],
    material: u32,
    contact: u32,
    no_light: u32,
    /// The blade's configured radius, for contacts; not a GPU attribute.
    nominal_radius: f32,
}

/// `CG_DoSaber`'s per-frame random draws (`cg_players.c:5359-5469`): `crandom()` for the
/// glow and core radii and `Q_flrand(0, 1)` for the hilt sprite (tr_surface.cpp:489).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Flicker {
    /// Glow radius draw in [-1, 1].
    pub(crate) glow: f32,
    /// Core radius draw in [-1, 1].
    pub(crate) core: f32,
    /// Hilt sprite draw in [0, 1].
    pub(crate) hilt: f32,
}

impl Flicker {
    /// Deterministic draws for one blade in one presentation millisecond; no RNG state.
    pub(crate) fn sample(key: u64, time_millis: i64) -> Self {
        let mut state = key.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ (time_millis as u64);
        let mut next = || {
            state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
        };
        Self {
            glow: next() * 2.0 - 1.0,
            core: next() * 2.0 - 1.0,
            hilt: next(),
        }
    }
}

impl Instance {
    const ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
        0 => Float32x3, 1 => Float32, 2 => Float32x3, 3 => Float32, 4 => Float32x4, 5 => Uint32];

    pub(crate) fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBUTES,
        }
    }

    /// A fully extended blade without flicker; see [`Self::pair_flickering`].
    pub(crate) fn pair(blade: Blade, color: BladeColor) -> [Self; 2] {
        Self::pair_flickering(blade, color, blade.length, Flicker::default())
    }

    /// Match `CG_DoSaber` (`cg_players.c:5359-5469`): the glow radius is
    /// `(R - 0.075R + crandom·0.075R)·radiusmult`, the core's `(R/3 + crandom·0.075R)·radiusmult`,
    /// with `radiusmult = 1 + 2/length` while the blade is shorter than `max_length`; the
    /// hilt sprite is `5.5 + flrand·0.25` (tr_surface.cpp:489). Retail textures carry their
    /// own colour (vertex colour 0xff); the neutral pair is tinted by the instance colour.
    pub(crate) fn pair_flickering(
        blade: Blade,
        color: BladeColor,
        max_length: f32,
        flicker: Flicker,
    ) -> [Self; 2] {
        let [red, green, blue] = color.tint();
        let range = blade.radius * 0.075;
        let grow = if blade.length < max_length {
            1.0 + 2.0 / blade.length.max(0.5)
        } else {
            1.0
        };
        let make = |radius: f32, hilt| Self {
            base: blade.base,
            length: blade.length,
            direction: blade.direction,
            radius: radius * grow,
            color: [red, green, blue, hilt],
            material: color.material(),
            contact: 0,
            no_light: 0,
            nominal_radius: blade.radius,
        };
        [
            make(
                blade.radius - range + flicker.glow * range,
                5.5 + flicker.hilt * 0.25,
            ),
            make(blade.radius / 3.0 + flicker.core * range, 0.0),
        ]
    }

    /// Tag render instances with a stable presentation key; GPU attributes are unchanged.
    /// Lane 2 is reserved for a thrown primary, so catching cannot bridge a cut.
    pub(crate) fn with_contact(
        mut self,
        entity: u64,
        lane: usize,
        blade: usize,
        enabled: bool,
        no_light: bool,
    ) -> Self {
        if enabled && entity > 0 && entity <= 1024 && lane < 3 && blade < 8 {
            self.contact = ((entity - 1) as usize * 24 + lane * 8 + blade + 1) as u32;
            self.no_light = u32::from(no_light);
        }
        self
    }

    /// One contact source per glow/core pair, excluding menu and suppressed blades.
    pub(crate) fn contact(self) -> Option<(usize, Blade, BladeColor, bool)> {
        if self.contact == 0 || self.color[3] <= 0.0 {
            return None;
        }
        let color = Color::ALL.get(self.material as usize).copied().map_or_else(
            || {
                BladeColor::Rgb(
                    [self.color[0], self.color[1], self.color[2]]
                        .map(|c| (c * 255.).round().clamp(0., 255.) as u8),
                )
            },
            BladeColor::Retail,
        );
        Some((
            self.contact as usize - 1,
            Blade {
                base: self.base,
                length: self.length,
                direction: self.direction,
                radius: self.nominal_radius,
            },
            color,
            self.no_light != 0,
        ))
    }

    pub(crate) const fn material(self) -> usize {
        self.material as usize
    }

    /// A world segment and radius holding everything `saber.wgsl` draws for this instance:
    /// the glow capsule (grown chain and hilt sprite) or the core line from behind the hilt.
    pub(crate) fn extent(self) -> ([f32; 3], [f32; 3], f32) {
        let direction = Vec3::from_array(self.direction);
        let base = Vec3::from_array(self.base);
        let radius = self.radius.max(0.01);
        let reach = if self.color[3] > 0.0 {
            // saber.wgsl `chain_radius` at the hilt, or the hilt sprite; quad corners reach √2.
            (radius + 0.017 * self.length / (0.65 * radius)).max(self.color[3])
                * std::f32::consts::SQRT_2
        } else {
            radius
        };
        (
            (base - direction * reach.max(1.0)).to_array(),
            (base + direction * (self.length + reach)).to_array(),
            reach,
        )
    }
}

/// Group instances by shader pair and return their draw ranges.
pub(crate) fn material_ranges(instances: &mut [Instance]) -> [Range<u32>; MATERIAL_COUNT] {
    instances.sort_unstable_by_key(|instance| instance.material());
    std::array::from_fn(|material| {
        let start = instances.partition_point(|instance| instance.material() < material);
        let end = instances.partition_point(|instance| instance.material() <= material);
        u32::try_from(start).unwrap_or(u32::MAX)..u32::try_from(end).unwrap_or(u32::MAX)
    })
}

/// The six fixed retail saber shader pairs selected by codemp client color.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Color {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
}

impl Color {
    pub(crate) const ALL: [Self; 6] = [
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Blue,
        Self::Purple,
    ];

    /// Blade tint reported by the client adapter for this stock colour
    /// (the vertex colour `CG_DoSaber` hands the retail shader pair).
    pub(crate) const fn blade_rgb(self) -> [u8; 3] {
        match self {
            Self::Red => [255, 51, 51],
            Self::Orange => [255, 128, 26],
            Self::Yellow => [255, 255, 51],
            Self::Green => [51, 255, 51],
            Self::Blue => [51, 102, 255],
            Self::Purple => [230, 51, 255],
        }
    }

    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Red => 0,
            Self::Orange => 1,
            Self::Yellow => 2,
            Self::Green => 3,
            Self::Blue => 4,
            Self::Purple => 5,
        }
    }

    pub(crate) const fn shader_stem(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Orange => "orange",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Purple => "purple",
        }
    }

    /// Vertex RGB chosen by `CG_AddSaberBlade` for the blur primitive
    /// (`codemp/cgame/cg_players.c:6371-6394`).
    pub(crate) const fn trail_rgb(self) -> [f32; 3] {
        match self {
            Self::Red => [1.0, 0.0, 0.0],
            Self::Orange => [1.0, 64.0 / 255.0, 0.0],
            Self::Yellow => [1.0, 1.0, 0.0],
            Self::Green => [0.0, 1.0, 0.0],
            Self::Blue => [0.0, 64.0 / 255.0, 1.0],
            Self::Purple => [220.0 / 255.0, 0.0, 1.0],
        }
    }
}

/// Samplers of a saber material: the glow and its integral are sampled at level 0 (the
/// capsule integrates stock's sprite chain itself), the core line with the owner's stock
/// filtering, rd-vulkan's `GL_LINEAR_MIPMAP_LINEAR` with its default
/// `r_ext_max_anisotropy 2` (`rd-vulkan/tr_init.cpp:930`, `vk_image.cpp:320-352`).
pub(crate) struct Samplers {
    glow: wgpu::Sampler,
    core: wgpu::Sampler,
}

impl Samplers {
    /// Stock core anisotropy (`r_ext_max_anisotropy` default).
    const CORE_ANISOTROPY: u16 = 2;

    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let clamped = |label, mipmaps, anisotropy_clamp| wgpu::SamplerDescriptor {
            label: Some(label),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: mipmaps,
            anisotropy_clamp,
            ..Default::default()
        };
        Self {
            glow: device.create_sampler(&clamped(
                "JKR saber glow sampler",
                wgpu::MipmapFilterMode::Nearest,
                1,
            )),
            core: device.create_sampler(&clamped(
                "JKR saber core sampler",
                wgpu::MipmapFilterMode::Linear,
                Self::CORE_ANISOTROPY,
            )),
        }
    }
}

/// Load the six real retail glow/core shader pairs used by `CG_DoSaber`,
/// followed by the engine-generated neutral pair for custom RGB blades.
pub(crate) fn create_materials(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    layout: &wgpu::BindGroupLayout,
) -> Result<Vec<wgpu::BindGroup>, Box<dyn Error>> {
    let samplers = Samplers::new(device);
    let neutral = crate::saber_rgb::create_material(device, queue, layout, &samplers);
    Color::ALL
        .into_iter()
        .map(|color| {
            let stem = color.shader_stem();
            let load = |kind: &str| {
                crate::shader_image::load_shader_image(
                    vfs,
                    shaders,
                    &format!("gfx/effects/sabers/{stem}_{kind}"),
                )
            };
            Ok(material(
                device,
                queue,
                layout,
                &samplers,
                &load("glow")?,
                &load("line")?,
            ))
        })
        .chain(std::iter::once(Ok(neutral)))
        .collect()
}

/// Bind one glow/core pair: the glow as stored, its column integral, and the core with
/// rd-vulkan's box mip chain (stored display values throughout).
pub(crate) fn material(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    layout: &wgpu::BindGroupLayout,
    samplers: &Samplers,
    glow: &image::RgbaImage,
    core: &image::RgbaImage,
) -> wgpu::BindGroup {
    let integral = crate::saber_gpu::glow_integral::upload(device, queue, glow);
    let glow = crate::gpu_texture::upload_display_image(device, queue, "JKR saber glow", glow);
    let core =
        crate::gpu_texture::upload_display_image_mipmapped(device, queue, "JKR saber core", core);
    let view = |binding, view| wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    };
    let sampler = |binding, sampler| wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::Sampler(sampler),
    };
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("JKR saber shader pair"),
        layout,
        entries: &[
            view(0, &glow),
            view(1, &core),
            sampler(2, &samplers.glow),
            view(3, &integral),
            sampler(4, &samplers.core),
        ],
    })
}

/// Evaluate both hand attachments from the same actor pose.
pub(crate) fn actor_attachments(
    preview: &PlayerPreview,
    lower_sample: (usize, usize, f32),
    upper_sample: (usize, usize, f32),
) -> [Option<Attachment>; 2] {
    let matrices = preview
        .animation
        .split_interpolated_frame_matrices(
            (lower_sample.0, lower_sample.1),
            lower_sample.2,
            (upper_sample.0, upper_sample.1),
            upper_sample.2,
            "lower_lumbar",
        )
        .ok();
    matrices.map_or([None; 2], |matrices| {
        attachments_from_matrices(preview, &matrices)
    })
}

/// Resolve the right and left hand from one already-evaluated pose.
pub(crate) fn attachments_from_matrices(
    preview: &PlayerPreview,
    matrices: &[[[f32; 4]; 3]],
) -> [Option<Attachment>; 2] {
    std::array::from_fn(|index| {
        attachment_from_matrices(preview, matrices, [HAND_BOLT, LEFT_HAND_BOLT][index])
    })
}

/// Build the right-hand attachment from an already evaluated actor pose.
pub(crate) fn attachment_from_matrices(
    preview: &PlayerPreview,
    matrices: &[[[f32; 4]; 3]],
    bolt_name: &str,
) -> Option<Attachment> {
    let matrix = preview
        .mesh
        .surface_bolt_matrix(bolt_name, 0, matrices)
        .ok()??;
    attachment_from_bolt(matrix)
}

/// Convert a Ghoul2 3x4 bolt matrix without changing its handedness.
pub(crate) fn attachment_from_bolt(matrix: [[f32; 4]; 3]) -> Option<Attachment> {
    let origin = Vec3::new(matrix[0][3], matrix[1][3], matrix[2][3]);
    let x = Vec3::new(matrix[0][0], matrix[1][0], matrix[2][0]).normalize_or_zero();
    let raw_y = Vec3::new(matrix[0][1], matrix[1][1], matrix[2][1]);
    let y = (raw_y - x * raw_y.dot(x)).normalize_or_zero();
    let z = x.cross(y).normalize_or_zero();
    let encoded_z = Vec3::new(matrix[0][2], matrix[1][2], matrix[2][2]).normalize_or_zero();
    if [x, y, z, encoded_z]
        .into_iter()
        .any(|axis| axis.length_squared() < 0.5)
        || z.dot(encoded_z) < 0.99
    {
        return None;
    }
    Some(Attachment {
        grip: origin.to_array(),
        rotation: Quat::from_mat3(&Mat3::from_cols(x, y, z)).to_array(),
    })
}

/// Extract the saber socket exactly as multiplayer `CG_AddSaberBlade` does:
/// `*blade1` through `G2API_GetBoltMatrix`, origin from `ORIGIN`, forward
/// from `NEGATIVE_Y`. `raw` is the renderer bolt from
/// [`jkr_model::Glm::surface_bolt_matrix`].
pub(crate) fn blade_socket_from_bolt(raw: crate::bolt::BoltMatrix) -> Option<BladeSocket> {
    let game = crate::bolt::game_facing(raw);
    let direction = -Vec3::from_array(crate::bolt::column(&game, 1));
    (direction.length_squared() > 0.5).then_some(BladeSocket {
        origin: crate::bolt::column(&game, 3),
        direction: direction.normalize().to_array(),
    })
}

/// Apply the actor and hand-bolt transforms once, in parent-to-child order.
pub(crate) fn world_attachment(
    actor_origin: Vec3,
    actor_rotation: Quat,
    attachment: Attachment,
) -> (Vec3, Quat) {
    (
        actor_origin + actor_rotation * Vec3::from_array(attachment.grip),
        actor_rotation * Quat::from_array(attachment.rotation),
    )
}

/// Transform a saber-local blade socket into world space.
pub(crate) fn world_blade(
    grip: Vec3,
    weapon_rotation: Quat,
    socket: BladeSocket,
    length: f32,
    radius: f32,
) -> Blade {
    Blade {
        base: (grip + weapon_rotation * Vec3::from_array(socket.origin)).to_array(),
        direction: (weapon_rotation * Vec3::from_array(socket.direction))
            .normalize_or_zero()
            .to_array(),
        length,
        radius,
    }
}
