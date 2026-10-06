//! Raven's retail `.fontdat` bitmap-font metrics and their atlas images.
//!
//! A `.fontdat` file (OpenJK `rd-common/tr_font.cpp`, `dfontdat_t`) holds 256
//! glyph records followed by a short header. Glyph rectangles are normalized
//! texture coordinates, so a higher-resolution replacement of the matching
//! `fonts/<name>.tga` (as shipped by HD font packs) reuses the same metrics.

use super::{FontGlyph, GLYPH_COUNT, UiFont};
use jkr_vfs::VirtualFileSystem;
use std::error::Error;

/// Bytes per `glyphInfo_t`: four shorts, an int baseline and four floats.
const GLYPH_BYTES: usize = 28;
/// `mPointSize`, `mHeight`, `mAscender`, `mDescender` and `mKoreanHack`;
/// the descender is implied by the line height and not kept.
const HEADER_BYTES: usize = 10;

/// One retail glyph record, in the font's authored pixel units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct FontdatGlyph {
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) advance: f32,
    pub(crate) offset_x: f32,
    /// Ink top above the baseline.
    pub(crate) baseline: f32,
    /// Atlas rectangle `[s, t, s2, t2]`.
    pub(crate) uv: [f32; 4],
}

/// Parsed `.fontdat` contents.
pub(crate) struct Fontdat {
    pub(crate) glyphs: [FontdatGlyph; GLYPH_COUNT],
    pub(crate) point_size: f32,
    pub(crate) height: f32,
    pub(crate) ascender: f32,
}

impl Fontdat {
    /// Decode the little-endian on-disk layout.
    pub(crate) fn parse(data: &[u8]) -> Result<Self, Box<dyn Error>> {
        if data.len() < GLYPH_BYTES * GLYPH_COUNT + HEADER_BYTES {
            return Err(format!("unexpected fontdat size {}", data.len()).into());
        }
        let i16_at =
            |offset: usize| f32::from(i16::from_le_bytes([data[offset], data[offset + 1]]));
        let u32_at = |offset: usize| {
            u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ])
        };
        let f32_at = |offset: usize| f32::from_bits(u32_at(offset));
        let mut glyphs = [FontdatGlyph::default(); GLYPH_COUNT];
        for (index, glyph) in glyphs.iter_mut().enumerate() {
            let offset = index * GLYPH_BYTES;
            *glyph = FontdatGlyph {
                width: i16_at(offset),
                height: i16_at(offset + 2),
                advance: i16_at(offset + 4),
                offset_x: i16_at(offset + 6),
                baseline: u32_at(offset + 8) as i32 as f32,
                uv: [
                    f32_at(offset + 12),
                    f32_at(offset + 16),
                    f32_at(offset + 20),
                    f32_at(offset + 24),
                ],
            };
        }
        let header = GLYPH_BYTES * GLYPH_COUNT;
        Ok(Self {
            glyphs,
            point_size: i16_at(header).max(1.0),
            height: i16_at(header + 2),
            ascender: i16_at(header + 4),
        })
    }

    /// Build layout metrics with each glyph's baseline `baseline_from_top`
    /// below the line top, for a line `height` tall.
    pub(crate) fn into_font(self, height: f32, baseline_from_top: f32) -> UiFont {
        let mut glyphs = [[FontGlyph::default(); GLYPH_COUNT]; 2];
        for (target, source) in glyphs[0].iter_mut().zip(self.glyphs) {
            *target = FontGlyph {
                width: source.width,
                height: source.height,
                advance: source.advance,
                offset_x: source.offset_x,
                offset_y: baseline_from_top - source.baseline,
                uv: source.uv,
            };
        }
        UiFont {
            glyphs,
            height,
            modern: false,
        }
    }

    /// Typographic placement: the baseline sits `mAscender` below the line
    /// top and the line is `mHeight` tall, as the bundled modern font places
    /// its own ascent. Retail fonts with an empty header fall back to the
    /// point size and a bottom baseline.
    pub(crate) fn into_typographic_font(self) -> UiFont {
        let height = if self.height > 0.0 {
            self.height
        } else {
            self.point_size
        };
        let baseline = if self.ascender > 0.0 {
            self.ascender
        } else {
            height
        };
        self.into_font(height, baseline)
    }
}

/// Read `fonts/<name>.fontdat` and its atlas image from the game data.
///
/// Like the retail font registration, the atlas may be a TGA, PNG or JPEG;
/// the highest-priority mounted file wins, so an HD replacement in a later
/// PK3 supplies the texture while the retail metrics stay in effect.
pub(crate) fn read(
    vfs: &VirtualFileSystem,
    name: &str,
) -> Result<(Fontdat, image::RgbaImage), Box<dyn Error>> {
    let metrics_path = format!("fonts/{name}.fontdat");
    let metrics = vfs
        .read(&metrics_path)?
        .ok_or_else(|| format!("{metrics_path} is missing"))?;
    let fontdat = Fontdat::parse(&metrics.bytes)?;
    for (extension, format) in [
        ("tga", image::ImageFormat::Tga),
        ("png", image::ImageFormat::Png),
        ("jpg", image::ImageFormat::Jpeg),
    ] {
        if let Some(atlas) = vfs.read(&format!("fonts/{name}.{extension}"))? {
            let image = image::load_from_memory_with_format(&atlas.bytes, format)?.to_rgba8();
            return Ok((fontdat, image));
        }
    }
    Err(format!("fonts/{name} has no atlas image").into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        let mut data = vec![0_u8; GLYPH_BYTES * GLYPH_COUNT + HEADER_BYTES];
        let offset = usize::from(b'A') * GLYPH_BYTES;
        for (index, value) in [15_i16, 14, 12, -1].into_iter().enumerate() {
            data[offset + index * 2..offset + index * 2 + 2].copy_from_slice(&value.to_le_bytes());
        }
        data[offset + 8..offset + 12].copy_from_slice(&13_i32.to_le_bytes());
        for (index, value) in [0.5_f32, 0.25, 0.75, 0.5].into_iter().enumerate() {
            let start = offset + 12 + index * 4;
            data[start..start + 4].copy_from_slice(&value.to_le_bytes());
        }
        let header = GLYPH_BYTES * GLYPH_COUNT;
        for (index, value) in [20_i16, 22, 17, 5].into_iter().enumerate() {
            data[header + index * 2..header + index * 2 + 2].copy_from_slice(&value.to_le_bytes());
        }
        data
    }

    #[test]
    fn parses_glyph_records_and_header() {
        let font = Fontdat::parse(&sample()).unwrap();
        assert_eq!(
            font.glyphs[usize::from(b'A')],
            FontdatGlyph {
                width: 15.0,
                height: 14.0,
                advance: 12.0,
                offset_x: -1.0,
                baseline: 13.0,
                uv: [0.5, 0.25, 0.75, 0.5],
            }
        );
        assert_eq!(
            [font.point_size, font.height, font.ascender],
            [20.0, 22.0, 17.0]
        );
    }

    #[test]
    fn rejects_truncated_files() {
        assert!(Fontdat::parse(&sample()[..100]).is_err());
    }

    #[test]
    fn typographic_placement_puts_the_baseline_at_the_ascender() {
        let font = Fontdat::parse(&sample()).unwrap().into_typographic_font();
        let glyph = font.glyph(super::super::TextFace::Semibold, b'A');
        assert_eq!(font.height, 22.0);
        // Ink top 13 above a baseline 17 below the line top.
        assert_eq!(glyph.offset_y, 4.0);
        assert_eq!(glyph.uv, [0.5, 0.25, 0.75, 0.5]);
        assert!(!font.is_modern());
    }

    #[test]
    fn empty_header_falls_back_to_point_size() {
        let mut data = sample();
        let header = GLYPH_BYTES * GLYPH_COUNT;
        data[header + 2..header + 8].fill(0);
        let font = Fontdat::parse(&data).unwrap().into_typographic_font();
        assert_eq!(font.height, 20.0);
        assert_eq!(
            font.glyph(super::super::TextFace::Regular, b'A').offset_y,
            7.0
        );
    }
}
