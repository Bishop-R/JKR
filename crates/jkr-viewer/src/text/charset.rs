//! The retail console character set, `gfx/2d/charsgrid_med`.
//!
//! The console, its notify lines and the cgame's "big" and "small" strings draw
//! with this 16x16 grid of Latin-1 cells rather than a `.fontdat` font. OpenJK
//! `codemp` `SCR_DrawSmallChar` (`cl_scrn.cpp`) and `CG_DrawChar`
//! (`cg_drawtools.c`) sample only the left half of a cell (`0.03125` by `0.0625`
//! of the texture) into a character twice as tall as it is wide
//! (`SMALLCHAR_WIDTH` 8 by `SMALLCHAR_HEIGHT` 16), and every character advances
//! by that width, so the console is monospaced. A space draws nothing. An HD
//! replacement of the image in a later PK3 keeps the same grid.

use super::{FontGlyph, GLYPH_COUNT, UiFont};
use jkr_vfs::VirtualFileSystem;
use std::error::Error;

/// Image path, without an extension, as the client registers it.
pub(crate) const PATH: &str = "gfx/2d/charsgrid_med";
/// Cells per row and per column.
const GRID: usize = 16;
/// Character height in layout units; the line is one character tall.
const HEIGHT: f32 = 16.0;
/// Character width and advance: half the height, as `SMALLCHAR_WIDTH`.
const WIDTH: f32 = HEIGHT * 0.5;

/// Layout metrics for the character set: one fixed cell per byte.
pub(crate) fn font() -> UiFont {
    let mut glyphs = [[FontGlyph::default(); GLYPH_COUNT]; 2];
    let cell = 1.0 / GRID as f32;
    for (byte, glyph) in glyphs[0].iter_mut().enumerate() {
        let u = (byte % GRID) as f32 * cell;
        let v = (byte / GRID) as f32 * cell;
        let ink = byte != usize::from(b' ');
        *glyph = FontGlyph {
            width: if ink { WIDTH } else { 0.0 },
            height: if ink { HEIGHT } else { 0.0 },
            advance: WIDTH,
            offset_x: 0.0,
            offset_y: 0.0,
            uv: [u, v, u + cell * 0.5, v + cell],
        };
    }
    UiFont {
        glyphs,
        height: HEIGHT,
        modern: false,
    }
}

/// Read the character set image from the game data. As the renderer's image
/// lookup, a TGA wins over a PNG or JPEG of the same name.
pub(crate) fn read(vfs: &VirtualFileSystem) -> Result<image::RgbaImage, Box<dyn Error>> {
    for (extension, format) in [
        ("tga", image::ImageFormat::Tga),
        ("png", image::ImageFormat::Png),
        ("jpg", image::ImageFormat::Jpeg),
    ] {
        if let Some(file) = vfs.read(&format!("{PATH}.{extension}"))? {
            return Ok(image::load_from_memory_with_format(&file.bytes, format)?.to_rgba8());
        }
    }
    Err(format!("{PATH} has no image").into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::TextFace;

    #[test]
    fn glyphs_are_the_left_half_of_their_cell() {
        let font = font();
        // 'A' (0x41): row 4, column 1.
        let glyph = font.glyph(TextFace::Regular, b'A');
        assert_eq!(glyph.uv, [0.0625, 0.25, 0.09375, 0.3125]);
        assert_eq!([glyph.width, glyph.height], [8.0, 16.0]);
        assert_eq!([glyph.offset_x, glyph.offset_y], [0.0, 0.0]);
        // The last cell ends at the texture's bottom-right quarter column.
        let last = font.glyph(TextFace::Semibold, 0xff);
        assert_eq!(last.uv, [0.9375, 0.9375, 0.96875, 1.0]);
    }

    #[test]
    fn every_character_advances_one_cell() {
        let font = font();
        assert_eq!(font.height, 16.0);
        assert!(!font.is_modern());
        for byte in [b' ', b'i', b'W', 0xac] {
            assert_eq!(font.glyph(TextFace::Regular, byte).advance, 8.0);
        }
        let width = crate::text::visible_text_width(&font, "^1ab c", 2.0);
        assert_eq!(width, 4.0 * 8.0 * 2.0);
    }

    #[test]
    fn a_space_draws_nothing() {
        let space = font().glyph(TextFace::Regular, b' ');
        assert_eq!([space.width, space.height], [0.0, 0.0]);
    }
}
