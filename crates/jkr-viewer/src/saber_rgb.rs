//! Custom (RGB) blade colours.
//!
//! JA+/TaystJK let a player pick any blade tint (`saber_colors_t` index 6,
//! `SABER_RGB`).  Retail ships only the six coloured glow/core textures, and
//! TaystJK's own neutral `RGBglow1`/`RGBcore1` assets are not part of the
//! game data, so the engine generates a neutral pair itself — profiled from
//! the retail `blue_glow2` / `blue_line` images — and tints it per instance
//! in the shader.  The tint is applied like `CG_DoSaber` for `SABER_RGB`
//! (TaystJK `codemp/cgame/cg_players.c:6395-6460`): the glow takes the
//! colour, the hot core stays white with a coloured fringe.

use crate::saber::Color;

/// Material slot of the engine-generated neutral pair, after the six retail
/// colours.
pub(crate) const RGB_MATERIAL: u32 = Color::ALL.len() as u32;
/// Number of saber material bind groups (six retail pairs plus the neutral one).
pub(crate) const MATERIAL_COUNT: usize = RGB_MATERIAL as usize + 1;

/// Blade colour as the renderer needs it: a retail shader pair, or a tint
/// applied to the neutral pair.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum BladeColor {
    /// One of the six retail colours, drawn with its own textures.
    Retail(Color),
    /// Any other colour, drawn with the neutral textures tinted by this RGB.
    Rgb([u8; 3]),
}

impl BladeColor {
    /// Pick the retail pair for an exact stock triplet, otherwise tint.
    pub(crate) fn from_rgb(rgb: [u8; 3]) -> Self {
        Color::ALL
            .into_iter()
            .find(|color| color.blade_rgb() == rgb)
            .map_or(Self::Rgb(rgb), Self::Retail)
    }

    /// Bind-group slot of this colour's glow/core pair.
    pub(crate) fn material(self) -> u32 {
        match self {
            Self::Retail(color) => color.index() as u32,
            Self::Rgb(_) => RGB_MATERIAL,
        }
    }

    /// Per-instance multiplier; retail textures already carry their colour.
    pub(crate) fn tint(self) -> [f32; 3] {
        match self {
            Self::Retail(_) => [1.0; 3],
            Self::Rgb(rgb) => rgb.map(|channel| f32::from(channel) / 255.0),
        }
    }

    /// Vertex RGB of the blur trail: the stock table for retail colours,
    /// the player's tint for RGB (`cg_players.c:7840-7855`).
    pub(crate) fn trail_rgb(self) -> [f32; 3] {
        match self {
            Self::Retail(color) => color.trail_rgb(),
            Self::Rgb(rgb) => rgb.map(|channel| f32::from(channel) / 255.0),
        }
    }
}

/// Side length of the generated glow image (`blue_glow2.jpg` is 128×128).
const GLOW_SIZE: u32 = 128;
/// Size of the generated core image (`blue_line.jpg` is 256×64).
const CORE_WIDTH: u32 = 64;
const CORE_HEIGHT: u32 = 256;

/// Neutral glow: a radial Gaussian falloff (σ = half the half-width, peak
/// 0.55, zero at the border), matching the measured retail glow profile.
pub(crate) fn neutral_glow() -> Vec<u8> {
    let half = GLOW_SIZE as f32 / 2.0;
    let mut pixels = Vec::with_capacity((GLOW_SIZE * GLOW_SIZE * 4) as usize);
    for y in 0..GLOW_SIZE {
        for x in 0..GLOW_SIZE {
            let dx = (x as f32 + 0.5 - half) / half;
            let dy = (y as f32 + 0.5 - half) / half;
            let distance = (dx * dx + dy * dy).sqrt();
            let falloff = (-(distance / 0.5).powi(2)).exp() * 0.55;
            let edge = (1.0 - distance).clamp(0.0, 0.1) * 10.0;
            let value = (falloff * edge * 255.0).round() as u8;
            pixels.extend_from_slice(&[value, value, value, 255]);
        }
    }
    pixels
}

/// Neutral core: the red channel carries the white-hot core
/// (`exp(-(d/0.41)²)` across the width), the green channel the broader
/// coloured fringe (`exp(-(d/0.7)²)`); over the last 16 rows at either end
/// `d` becomes the distance from the end's centre, so the tips round off
/// and fade like `blue_line.jpg`.
pub(crate) fn neutral_core() -> Vec<u8> {
    const END_ROWS: f32 = 16.0;
    let half = CORE_WIDTH as f32 / 2.0;
    let mut pixels = Vec::with_capacity((CORE_WIDTH * CORE_HEIGHT * 4) as usize);
    for y in 0..CORE_HEIGHT {
        let from_end = (y as f32 + 0.5).min(CORE_HEIGHT as f32 - y as f32 - 0.5);
        let dy = (1.0 - from_end / END_ROWS).max(0.0);
        for x in 0..CORE_WIDTH {
            let dx = (x as f32 + 0.5 - half) / half;
            let distance = (dx * dx + dy * dy).sqrt();
            let core = (-(distance / 0.41).powi(2)).exp();
            let fringe = (-(distance / 0.7).powi(2)).exp();
            let channel = |value: f32| (value * 255.0).round() as u8;
            pixels.extend_from_slice(&[channel(core), channel(fringe), 0, 255]);
        }
    }
    pixels
}

/// Upload the neutral pair as the seventh saber material, as stored display values like
/// the retail pairs.
pub(crate) fn create_material(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    layout: &wgpu::BindGroupLayout,
    samplers: &crate::saber::Samplers,
) -> wgpu::BindGroup {
    let glow = image::RgbaImage::from_raw(GLOW_SIZE, GLOW_SIZE, neutral_glow())
        .expect("neutral glow has GLOW_SIZE² RGBA texels");
    let core = image::RgbaImage::from_raw(CORE_WIDTH, CORE_HEIGHT, neutral_core())
        .expect("neutral core has CORE_WIDTH×CORE_HEIGHT RGBA texels");
    crate::saber::material(device, queue, layout, samplers, &glow, &core)
}
