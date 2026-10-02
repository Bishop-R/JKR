//! DPI-aware text layout and cached glyph atlases for the client UI.
//!
//! Inter is rasterized once when the graphics device is created.  The render
//! loop only performs glyph lookup and appends vertices into reused buffers;
//! it never rasterizes a glyph or grows the atlas.  The legacy JKA `fontdat`
//! reader lives here as an optional compatibility path for the HUD only.

mod bounded;
pub(crate) mod style;
pub(crate) use bounded::append_bounded;
pub(crate) use style::TextStyle;

use bytemuck::{Pod, Zeroable};
use fontdue::{Font, FontSettings, Metrics};
use image::{Rgba, RgbaImage};
use jkr_vfs::VirtualFileSystem;
use std::error::Error;

pub(crate) const MAX_TEXT_VERTICES: usize = 32_768;
const GLYPH_COUNT: usize = 256;
const ATLAS_WIDTH: u32 = 4_096;
/// Glyph gutter; must stay >= 2 px at the deepest mip level so minified
/// sampling never bleeds a neighbour's coverage into a glyph edge.
const ATLAS_PADDING: u32 = 2 << (ATLAS_MIP_LEVELS - 1);
/// Modern glyphs are rasterized at 3x and minified for small UI text.
const MODERN_RASTER_SCALE: f32 = 3.0;
/// Mip levels uploaded for the modern atlas (96 px raster down to 12 px).
pub(crate) const ATLAS_MIP_LEVELS: u32 = 4;
const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");

/// Font weight available in the modern UI atlas.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TextFace {
    Regular,
    Semibold,
}

impl TextFace {
    fn index(self) -> usize {
        match self {
            Self::Regular => 0,
            Self::Semibold => 1,
        }
    }
}

/// A single cached glyph: layout metrics in font units (physical pixels at
/// scale 1) and its rectangle in normalized atlas coordinates.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FontGlyph {
    /// Ink width.
    pub(crate) width: f32,
    /// Ink height.
    pub(crate) height: f32,
    /// Pen advance.
    pub(crate) advance: f32,
    /// Ink left edge from the pen.
    pub(crate) offset_x: f32,
    /// Ink top edge below the line top.
    pub(crate) offset_y: f32,
    /// Atlas rectangle `[u0, v0, u1, v1]`, `v0` at the ink top.
    pub(crate) uv: [f32; 4],
}

/// Immutable glyph metrics used by all menu, console, HUD, and chat layout.
pub(crate) struct UiFont {
    glyphs: [[FontGlyph; GLYPH_COUNT]; 2],
    /// Baseline-to-baseline line height in physical framebuffer pixels.
    pub(crate) height: f32,
    modern: bool,
    /// Player size and spacing preference for menu text, see [`style`].
    style: TextStyle,
}

impl UiFont {
    /// Metrics and atlas rectangle of `byte` (Latin-1) in `face`, for layouts
    /// outside the 2D text path such as the ground HUD's world-space numbers.
    pub(crate) fn glyph(&self, face: TextFace, byte: u8) -> FontGlyph {
        self.glyphs[if self.modern { face.index() } else { 0 }][usize::from(byte)]
    }

    /// Whether this is the bundled vector font rather than the retail HUD font.
    pub(crate) const fn is_modern(&self) -> bool {
        self.modern
    }

    /// The player's menu text style (neutral until the cvars are synced).
    pub(crate) const fn style(&self) -> TextStyle {
        self.style
    }

    /// Apply the player's `ui_textScale` / `ui_letterSpacing` preference.
    pub(crate) fn set_style(&mut self, style: TextStyle) {
        self.style = style;
    }
}

/// CPU assets uploaded once to the GPU text texture.
pub(crate) struct FontAtlas {
    pub(crate) font: UiFont,
    pub(crate) image: RgbaImage,
}

/// Vertex consumed by `text.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct TextVertex {
    position: [f32; 2],
    texture_coordinates: [f32; 2],
    color: [f32; 4],
}

impl TextVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];

    /// WGPU layout shared by all text draw calls.
    pub(crate) fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

struct RasterizedGlyph {
    face: usize,
    byte: usize,
    metrics: Metrics,
    pixels: Vec<u8>,
}

/// Rasterize Inter Regular and SemiBold at the current monitor DPI.
///
/// Latin-1 is deliberately pre-cached: protocol-26 UI strings are byte based,
/// so this covers the complete wire character range without atlas mutation in
/// the frame loop.
pub(crate) fn load_modern(dpi_scale: f64) -> Result<FontAtlas, Box<dyn Error>> {
    let fonts = [
        Font::from_bytes(INTER_REGULAR, FontSettings::default())?,
        Font::from_bytes(INTER_SEMIBOLD, FontSettings::default())?,
    ];
    // The largest menu face is 2.7x body size.  A 3x source atlas means every
    // production text size is sampled at native resolution or downsampled,
    // never magnified from a small bitmap as the retail font was.
    let pixel_size = (32.0 * dpi_scale.clamp(1.0, 3.0) as f32 * MODERN_RASTER_SCALE).round();
    let line_metrics = fonts[0]
        .horizontal_line_metrics(pixel_size)
        .ok_or("Inter has no horizontal line metrics")?;
    let mut rasterized = Vec::with_capacity(GLYPH_COUNT * fonts.len());
    for (face, font) in fonts.iter().enumerate() {
        for byte in 0..GLYPH_COUNT {
            let character = char::from_u32(byte as u32).unwrap_or('\u{fffd}');
            let (metrics, pixels) = font.rasterize(character, pixel_size);
            rasterized.push(RasterizedGlyph {
                face,
                byte,
                metrics,
                pixels,
            });
        }
    }

    let placements = pack_glyphs(&rasterized);
    let height = placements
        .iter()
        .zip(&rasterized)
        .map(|([_, y], glyph)| y + glyph.metrics.height as u32 + ATLAS_PADDING)
        .max()
        .unwrap_or(1)
        .next_power_of_two();
    // Transparent texels retain white RGB so bilinear sampling at a glyph
    // boundary does not interpolate toward black before straight-alpha blend.
    let mut image = RgbaImage::from_pixel(ATLAS_WIDTH, height, Rgba([255, 255, 255, 0]));
    let mut glyphs = [[FontGlyph::default(); GLYPH_COUNT]; 2];
    for (glyph, [x, y]) in rasterized.iter().zip(placements) {
        for row in 0..glyph.metrics.height {
            for column in 0..glyph.metrics.width {
                let alpha = glyph.pixels[row * glyph.metrics.width + column];
                image.put_pixel(
                    x + column as u32,
                    y + row as u32,
                    Rgba([255, 255, 255, alpha]),
                );
            }
        }
        let top = (line_metrics.ascent - (glyph.metrics.ymin as f32 + glyph.metrics.height as f32))
            / MODERN_RASTER_SCALE;
        glyphs[glyph.face][glyph.byte] = FontGlyph {
            width: glyph.metrics.width as f32 / MODERN_RASTER_SCALE,
            height: glyph.metrics.height as f32 / MODERN_RASTER_SCALE,
            advance: glyph.metrics.advance_width / MODERN_RASTER_SCALE,
            offset_x: glyph.metrics.xmin as f32 / MODERN_RASTER_SCALE,
            offset_y: top,
            uv: [
                x as f32 / ATLAS_WIDTH as f32,
                y as f32 / height as f32,
                (x + glyph.metrics.width as u32) as f32 / ATLAS_WIDTH as f32,
                (y + glyph.metrics.height as u32) as f32 / height as f32,
            ],
        };
    }
    Ok(FontAtlas {
        font: UiFont {
            glyphs,
            height: line_metrics.new_line_size / MODERN_RASTER_SCALE,
            modern: true,
            style: TextStyle::NEUTRAL,
        },
        image,
    })
}

fn pack_glyphs(glyphs: &[RasterizedGlyph]) -> Vec<[u32; 2]> {
    let mut positions = Vec::with_capacity(glyphs.len());
    let mut x = ATLAS_PADDING;
    let mut y = ATLAS_PADDING;
    let mut row_height = 0;
    for glyph in glyphs {
        let width = glyph.metrics.width as u32;
        let height = glyph.metrics.height as u32;
        if x + width + ATLAS_PADDING > ATLAS_WIDTH {
            x = ATLAS_PADDING;
            y += row_height + ATLAS_PADDING;
            row_height = 0;
        }
        positions.push([x, y]);
        x += width + ATLAS_PADDING;
        row_height = row_height.max(height);
    }
    positions
}

/// Load Raven's retail bitmap font as an optional classic-HUD atlas.
pub(crate) fn load_classic(vfs: &VirtualFileSystem) -> Result<FontAtlas, Box<dyn Error>> {
    let data = vfs
        .read("fonts/arialnb.fontdat")?
        .ok_or("fonts/arialnb.fontdat is missing")?
        .bytes;
    const GLYPH_BYTES: usize = 28;
    const HEADER_BYTES: usize = 10;
    if data.len() < GLYPH_BYTES * GLYPH_COUNT + HEADER_BYTES {
        return Err(format!("unexpected arialnb.fontdat size {}", data.len()).into());
    }
    let i16_at = |offset: usize| i16::from_le_bytes([data[offset], data[offset + 1]]);
    let i32_at = |offset: usize| {
        i32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ])
    };
    let f32_at = |offset: usize| f32::from_bits(i32_at(offset) as u32);
    let header = GLYPH_BYTES * GLYPH_COUNT;
    let point_size = f32::from(i16_at(header)).max(1.0);
    let height = f32::from(i16_at(header + 2)).max(point_size);
    let mut glyphs = [[FontGlyph::default(); GLYPH_COUNT]; 2];
    for (index, glyph) in glyphs[0].iter_mut().enumerate() {
        let offset = index * GLYPH_BYTES;
        *glyph = FontGlyph {
            width: f32::from(i16_at(offset)),
            height: f32::from(i16_at(offset + 2)),
            advance: f32::from(i16_at(offset + 4)),
            offset_x: f32::from(i16_at(offset + 6)),
            offset_y: height - i32_at(offset + 8) as f32,
            uv: [
                f32_at(offset + 12),
                f32_at(offset + 16),
                f32_at(offset + 20),
                f32_at(offset + 24),
            ],
        };
    }
    let image = vfs
        .read("fonts/arialnb.tga")?
        .ok_or("fonts/arialnb.tga is missing")?;
    Ok(FontAtlas {
        font: UiFont {
            glyphs,
            height,
            modern: false,
            style: TextStyle::NEUTRAL,
        },
        image: image::load_from_memory_with_format(&image.bytes, image::ImageFormat::Tga)?
            .to_rgba8(),
    })
}

fn quake_color(index: u8) -> [f32; 4] {
    match index & 7 {
        0 => [0.0, 0.0, 0.0, 1.0],
        1 => [1.0, 0.2, 0.2, 1.0],
        2 => [0.25, 1.0, 0.25, 1.0],
        3 => [1.0, 0.82, 0.24, 1.0],
        4 => [0.25, 0.45, 1.0, 1.0],
        5 => [0.20, 0.82, 1.0, 1.0],
        6 => [1.0, 0.25, 1.0, 1.0],
        _ => [0.92, 0.95, 0.98, 1.0],
    }
}

fn push_quad(
    vertices: &mut Vec<TextVertex>,
    rectangle: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
    viewport: [f32; 2],
) {
    if vertices.len() + 6 > MAX_TEXT_VERTICES {
        return;
    }
    let [x, y, width, height] = rectangle;
    let ndc = |position: [f32; 2]| {
        [
            position[0] / viewport[0] * 2.0 - 1.0,
            1.0 - position[1] / viewport[1] * 2.0,
        ]
    };
    let points = [
        ([x, y], [uv[0], uv[1]]),
        ([x + width, y], [uv[2], uv[1]]),
        ([x + width, y + height], [uv[2], uv[3]]),
        ([x, y], [uv[0], uv[1]]),
        ([x + width, y + height], [uv[2], uv[3]]),
        ([x, y + height], [uv[0], uv[3]]),
    ];
    vertices.extend(points.map(|(position, texture_coordinates)| TextVertex {
        position: ndc(position),
        texture_coordinates,
        color,
    }));
}

/// Append anti-aliased glyph quads without allocating.
pub(crate) fn append_text(
    vertices: &mut Vec<TextVertex>,
    font: &UiFont,
    text: &str,
    origin: [f32; 2],
    scale: f32,
    viewport: [f32; 2],
) -> usize {
    append_text_face(
        vertices,
        font,
        text,
        origin,
        scale,
        viewport,
        TextFace::Regular,
    )
}

/// Append text using an explicit Inter weight.
pub(crate) fn append_text_face(
    vertices: &mut Vec<TextVertex>,
    font: &UiFont,
    text: &str,
    origin: [f32; 2],
    scale: f32,
    viewport: [f32; 2],
    face: TextFace,
) -> usize {
    append_text_style(
        vertices,
        font,
        text,
        origin,
        scale,
        viewport,
        face,
        [0.92, 0.95, 0.98, 1.0],
        0.0,
    )
}

/// Append text with an explicit weight, base colour, and tracking.
#[allow(clippy::too_many_arguments)]
pub(crate) fn append_text_style(
    vertices: &mut Vec<TextVertex>,
    font: &UiFont,
    text: &str,
    origin: [f32; 2],
    scale: f32,
    viewport: [f32; 2],
    face: TextFace,
    base_color: [f32; 4],
    letter_spacing: f32,
) -> usize {
    let mut cursor = origin;
    let mut color = base_color;
    let mut lines = 1;
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'^' && index + 1 < bytes.len() && bytes[index + 1].is_ascii_digit() {
            color = quake_color(bytes[index + 1] - b'0');
            index += 2;
            continue;
        }
        let (character, step) = glyph_byte_at(text, index);
        index += step;
        if character == b'\n' || cursor[0] + font.height * scale > viewport[0] {
            cursor[0] = origin[0];
            cursor[1] += font.height * scale * 1.18;
            lines += 1;
            if character == b'\n' {
                continue;
            }
        }
        let glyph = font.glyph(face, character);
        let rectangle = [
            cursor[0] + glyph.offset_x * scale,
            cursor[1] + glyph.offset_y * scale,
            glyph.width * scale,
            glyph.height * scale,
        ];
        let mut shadow = rectangle;
        shadow[0] += scale.max(1.0);
        shadow[1] += scale.max(1.0);
        // The shadow fades with the glyph so opacity groups hide text fully.
        push_quad(
            vertices,
            shadow,
            glyph.uv,
            [0.0, 0.0, 0.0, 0.55 * color[3]],
            viewport,
        );
        push_quad(vertices, rectangle, glyph.uv, color, viewport);
        cursor[0] += glyph.advance * scale + letter_spacing;
    }
    lines
}

/// Take the atlas index that draws the character at `index`, and its UTF-8 length.
///
/// The atlas holds 256 glyphs indexed by Latin-1 codepoint, matching how JKA's own fonts are
/// laid out. Text reaching the renderer is a Rust `str`, so any character above ASCII occupies
/// several UTF-8 bytes, and indexing the atlas with those bytes drew one character as two
/// glyphs: `ñ` became `Ã±`, and U+FFFD became `ï¿½` — the trailing `½` players kept seeing in
/// names. Characters outside Latin-1 have no glyph in a 256-entry atlas and fall back to `?`.
///
/// `index` must be a character boundary, which holds because every caller advances by whole
/// characters or by an ASCII colour escape.
pub(crate) fn glyph_byte_at(text: &str, index: usize) -> (u8, usize) {
    match text[index..].chars().next() {
        Some(character) => (
            u8::try_from(u32::from(character)).unwrap_or(b'?'),
            character.len_utf8(),
        ),
        None => (b'?', 1),
    }
}

/// Measure visible text while ignoring Quake color escapes.
pub(crate) fn visible_text_width(font: &UiFont, text: &str, scale: f32) -> f32 {
    visible_text_width_face(font, text, scale, TextFace::Regular)
}

/// Measure one face without producing vertices.
pub(crate) fn visible_text_width_face(
    font: &UiFont,
    text: &str,
    scale: f32,
    face: TextFace,
) -> f32 {
    visible_text_width_style(font, text, scale, face, 0.0)
}

/// Measure one face with additional tracking between glyph advances.
pub(crate) fn visible_text_width_style(
    font: &UiFont,
    text: &str,
    scale: f32,
    face: TextFace,
    letter_spacing: f32,
) -> f32 {
    let bytes = text.as_bytes();
    let mut width = 0.0;
    let mut maximum = 0.0_f32;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'^' && index + 1 < bytes.len() && bytes[index + 1].is_ascii_digit() {
            index += 2;
        } else if bytes[index] == b'\n' {
            maximum = maximum.max(width);
            width = 0.0;
            index += 1;
        } else {
            let (character, step) = glyph_byte_at(text, index);
            width += font.glyph(face, character).advance * scale + letter_spacing;
            index += step;
        }
    }
    maximum.max(width)
}
