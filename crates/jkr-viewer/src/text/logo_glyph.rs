//! The retail "WSI fonts" logo that Jedi Academy draws for `¬`.
//!
//! Byte 0xAC of the retail `ergoec` and `ocr_a` fonts is not a not-sign but
//! the boxed logo of the fonts' foundry, which players have long put in their
//! names. Inter draws a plain `¬` there, so the modern atlas takes this glyph
//! from the player's game data instead: [`LogoGlyph::read`] crops it from the
//! mounted font (an HD replacement atlas in a later PK3 included) and
//! [`LogoGlyph::rasterize`] scales it to Inter's cap height when the atlas is
//! built. Nothing is bundled; without the retail fonts `¬` stays Inter's.

use jkr_vfs::VirtualFileSystem;

/// The byte (Latin-1 `¬`) the retail fonts draw as the logo.
pub(crate) const BYTE: u8 = 0xAC;
/// Retail fonts that carry the logo, in preference order: the menu font
/// (`FONT_MEDIUM`), then the chat font (`FONT_SMALL`).
const FONTS: [&str; 2] = ["ergoec", "ocr_a"];
/// Bytes per `glyphInfo_t` in a `.fontdat` (OpenJK `rd-common/tr_font.cpp`):
/// four shorts, an int baseline and four floats.
const GLYPH_BYTES: usize = 28;
/// Glyph records in a `.fontdat`.
const GLYPH_COUNT: usize = 256;
/// The font's own capital, whose height sets the logo's scale.
const CAP: u8 = b'H';

/// The logo's coverage and metrics, in the retail font's authored pixels.
#[derive(Clone, Debug)]
pub(crate) struct LogoGlyph {
    /// Ink width.
    width: f32,
    /// Ink height.
    height: f32,
    /// Pen advance.
    advance: f32,
    /// Ink left edge from the pen.
    offset_x: f32,
    /// Ink top above the baseline.
    baseline: f32,
    /// Height of the font's `H`, the reference the logo is scaled against.
    cap_height: f32,
    /// Alpha coverage cropped from the atlas, at the atlas's own resolution.
    coverage: Vec<u8>,
    coverage_width: usize,
    coverage_height: usize,
}

/// The logo scaled for one rasterized face: fontdue-style metrics in raster
/// pixels plus coverage rows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScaledLogo {
    pub(crate) width: usize,
    pub(crate) height: usize,
    /// Ink left edge from the pen.
    pub(crate) xmin: i32,
    /// Ink bottom above the baseline (negative below), as fontdue's `ymin`.
    pub(crate) ymin: i32,
    pub(crate) advance: f32,
    pub(crate) pixels: Vec<u8>,
}

impl LogoGlyph {
    /// Crop the logo from the first retail font in the game data that has it.
    pub(crate) fn read(vfs: &VirtualFileSystem) -> Option<Self> {
        FONTS.into_iter().find_map(|name| {
            let metrics = vfs.read(&format!("fonts/{name}.fontdat")).ok()??;
            // Like retail font registration, the atlas may be a TGA, PNG or JPEG.
            let atlas = [
                ("tga", image::ImageFormat::Tga),
                ("png", image::ImageFormat::Png),
                ("jpg", image::ImageFormat::Jpeg),
            ]
            .into_iter()
            .find_map(|(extension, format)| {
                let file = vfs.read(&format!("fonts/{name}.{extension}")).ok()??;
                image::load_from_memory_with_format(&file.bytes, format).ok()
            })?;
            Self::from_font(&metrics.bytes, &atlas.to_rgba8())
        })
    }

    /// The logo from a `.fontdat` and its atlas, or `None` when the font has
    /// no ink at 0xAC or no usable capital to scale against.
    pub(crate) fn from_font(fontdat: &[u8], atlas: &image::RgbaImage) -> Option<Self> {
        if fontdat.len() < GLYPH_BYTES * GLYPH_COUNT {
            return None;
        }
        let record = |byte: u8| {
            let offset = usize::from(byte) * GLYPH_BYTES;
            let short = |at: usize| {
                f32::from(i16::from_le_bytes([
                    fontdat[offset + at],
                    fontdat[offset + at + 1],
                ]))
            };
            let word = |at: usize| {
                let bytes = [
                    fontdat[offset + at],
                    fontdat[offset + at + 1],
                    fontdat[offset + at + 2],
                    fontdat[offset + at + 3],
                ];
                (i32::from_le_bytes(bytes), f32::from_le_bytes(bytes))
            };
            (
                [short(0), short(2), short(4), short(6)],
                word(8).0 as f32,
                [word(12).1, word(16).1, word(20).1, word(24).1],
            )
        };
        let ([width, height, advance, offset_x], baseline, uv) = record(BYTE);
        let ([_, cap_height, _, _], _, _) = record(CAP);
        if width <= 0.0 || height <= 0.0 || cap_height <= 0.0 {
            return None;
        }
        // Normalized rectangles let an HD replacement atlas supply the pixels.
        let (atlas_width, atlas_height) = (atlas.width() as f32, atlas.height() as f32);
        let left = (uv[0] * atlas_width).round().max(0.0) as u32;
        let top = (uv[1] * atlas_height).round().max(0.0) as u32;
        let right = ((uv[2] * atlas_width).round() as u32).min(atlas.width());
        let bottom = ((uv[3] * atlas_height).round() as u32).min(atlas.height());
        if right <= left || bottom <= top {
            return None;
        }
        let coverage = (top..bottom)
            .flat_map(|y| (left..right).map(move |x| atlas.get_pixel(x, y)[3]))
            .collect();
        Some(Self {
            width,
            height,
            advance,
            offset_x,
            baseline,
            cap_height,
            coverage,
            coverage_width: (right - left) as usize,
            coverage_height: (bottom - top) as usize,
        })
    }

    /// The logo for a face whose `H` is `cap_height` raster pixels tall, so it
    /// keeps the size it has beside the retail font's letters.
    pub(crate) fn rasterize(&self, cap_height: f32) -> ScaledLogo {
        let scale = cap_height / self.cap_height;
        let width = (self.width * scale).round().max(1.0) as usize;
        let height = (self.height * scale).round().max(1.0) as usize;
        let top = (self.baseline * scale).round() as i32;
        // Supersample enough to average every source texel when shrinking an
        // HD crop, and to interpolate smoothly when enlarging a retail one.
        let steps_x = (self.coverage_width as f32 / width as f32).ceil().max(2.0) as usize;
        let steps_y = (self.coverage_height as f32 / height as f32)
            .ceil()
            .max(2.0) as usize;
        let mut pixels = Vec::with_capacity(width * height);
        for row in 0..height {
            for column in 0..width {
                let mut sum = 0.0;
                for sub_y in 0..steps_y {
                    for sub_x in 0..steps_x {
                        let u =
                            (column as f32 + (sub_x as f32 + 0.5) / steps_x as f32) / width as f32;
                        let v =
                            (row as f32 + (sub_y as f32 + 0.5) / steps_y as f32) / height as f32;
                        sum += self.sample(u, v);
                    }
                }
                pixels.push((sum / (steps_x * steps_y) as f32).round() as u8);
            }
        }
        ScaledLogo {
            width,
            height,
            xmin: (self.offset_x * scale).round() as i32,
            ymin: top - height as i32,
            advance: self.advance * scale,
            pixels,
        }
    }

    /// Bilinear coverage at normalized `(u, v)` inside the crop, clamped to
    /// its edges so no neighbouring atlas glyph bleeds in.
    fn sample(&self, u: f32, v: f32) -> f32 {
        let x = (u * self.coverage_width as f32 - 0.5).clamp(0.0, (self.coverage_width - 1) as f32);
        let y =
            (v * self.coverage_height as f32 - 0.5).clamp(0.0, (self.coverage_height - 1) as f32);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = (
            (x0 + 1).min(self.coverage_width - 1),
            (y0 + 1).min(self.coverage_height - 1),
        );
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let at = |x: usize, y: usize| f32::from(self.coverage[y * self.coverage_width + x]);
        let upper = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
        let lower = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
        upper * (1.0 - fy) + lower * fy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    /// A fontdat whose 0xAC is a 4x2 glyph in the atlas's left half, with
    /// ink 2 above the baseline, and whose `H` is 2 tall.
    fn font() -> Vec<u8> {
        let mut data = vec![0_u8; GLYPH_BYTES * GLYPH_COUNT + 10];
        let mut put = |byte: u8, shorts: [i16; 4], baseline: i32, uv: [f32; 4]| {
            let offset = usize::from(byte) * GLYPH_BYTES;
            for (index, value) in shorts.into_iter().enumerate() {
                data[offset + index * 2..offset + index * 2 + 2]
                    .copy_from_slice(&value.to_le_bytes());
            }
            data[offset + 8..offset + 12].copy_from_slice(&baseline.to_le_bytes());
            for (index, value) in uv.into_iter().enumerate() {
                let start = offset + 12 + index * 4;
                data[start..start + 4].copy_from_slice(&value.to_le_bytes());
            }
        };
        put(BYTE, [4, 2, 5, 1], 2, [0.0, 0.0, 0.5, 1.0]);
        put(CAP, [2, 2, 3, 0], 2, [0.5, 0.0, 1.0, 1.0]);
        data
    }

    /// An opaque 8x2 atlas (logo on the left, `H` on the right), `scale` times larger.
    fn atlas(scale: u32) -> RgbaImage {
        RgbaImage::from_pixel(8 * scale, 2 * scale, Rgba([255, 255, 255, 255]))
    }

    #[test]
    fn reads_the_logo_record_and_crop() {
        let logo = LogoGlyph::from_font(&font(), &atlas(1)).unwrap();
        assert_eq!(
            [logo.width, logo.height, logo.advance, logo.offset_x],
            [4.0, 2.0, 5.0, 1.0]
        );
        assert_eq!([logo.baseline, logo.cap_height], [2.0, 2.0]);
        assert_eq!([logo.coverage_width, logo.coverage_height], [4, 2]);
    }

    #[test]
    fn hd_atlas_supplies_a_larger_crop_with_the_same_metrics() {
        let logo = LogoGlyph::from_font(&font(), &atlas(8)).unwrap();
        assert_eq!([logo.coverage_width, logo.coverage_height], [32, 16]);
        assert_eq!([logo.width, logo.height], [4.0, 2.0]);
    }

    #[test]
    fn scales_to_the_face_cap_height() {
        let logo = LogoGlyph::from_font(&font(), &atlas(1)).unwrap();
        // The retail H is 2 tall; a 10 px cap height is a 5x scale.
        let scaled = logo.rasterize(10.0);
        assert_eq!([scaled.width, scaled.height], [20, 10]);
        assert_eq!([scaled.xmin, scaled.ymin], [5, 0]);
        assert_eq!(scaled.advance, 25.0);
        assert_eq!(scaled.pixels.len(), 200);
        assert!(scaled.pixels.iter().all(|&alpha| alpha == 255));
    }

    #[test]
    fn coverage_does_not_bleed_from_the_neighbouring_glyph() {
        let mut image = atlas(1);
        for x in 4..8 {
            for y in 0..2 {
                image.put_pixel(x, y, Rgba([255, 255, 255, 0]));
            }
        }
        let logo = LogoGlyph::from_font(&font(), &image).unwrap();
        let scaled = logo.rasterize(8.0);
        assert!(scaled.pixels.iter().all(|&alpha| alpha == 255));
    }

    #[test]
    fn a_font_without_the_logo_gives_none() {
        let mut data = font();
        data[usize::from(BYTE) * GLYPH_BYTES..usize::from(BYTE) * GLYPH_BYTES + 4].fill(0);
        assert!(LogoGlyph::from_font(&data, &atlas(1)).is_none());
        assert!(LogoGlyph::from_font(&data[..100], &atlas(1)).is_none());
    }

    #[test]
    fn modern_atlas_draws_the_logo_in_both_faces() {
        use super::super::{TextFace, load_modern};
        let logo = LogoGlyph::from_font(&font(), &atlas(1)).unwrap();
        let plain = load_modern(1.0, None).unwrap().font;
        let spliced = load_modern(1.0, Some(&logo)).unwrap().font;
        for face in [TextFace::Regular, TextFace::Semibold] {
            let cap = spliced.glyph(face, CAP);
            let glyph = spliced.glyph(face, BYTE);
            // 4x2 retail units against a 2-unit H: twice as wide as the cap is tall.
            assert!((glyph.width - 2.0 * cap.height).abs() <= 1.0, "{face:?}");
            assert!((glyph.height - cap.height).abs() <= 1.0, "{face:?}");
            // Ink sits on the baseline, its top level with the capital's.
            assert!((glyph.offset_y - cap.offset_y).abs() <= 1.0, "{face:?}");
            assert_ne!(glyph.advance, plain.glyph(face, BYTE).advance);
            // Other glyphs are untouched.
            assert_eq!(cap.advance, plain.glyph(face, CAP).advance);
        }
    }
}
