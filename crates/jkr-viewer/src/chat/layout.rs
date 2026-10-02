//! Bounded wrapping and viewport-relative conversation geometry.

use crate::text::{self, TextFace, UiFont};
use std::ops::Range;

pub(super) const WRAP_LINES: usize = 4;

pub(super) struct Geometry {
    pub(super) scale: f32,
    pub(super) left: f32,
    pub(super) top: f32,
    pub(super) bottom: f32,
    pub(super) width: f32,
    pub(super) row: f32,
    pub(super) font: f32,
}

impl Geometry {
    pub(super) fn new(viewport: [f32; 2]) -> Self {
        let scale = (viewport[1] / 1_080.0).clamp(0.6, 2.5);
        let left = 40.0 * scale;
        Self {
            scale,
            left,
            top: 150.0 * scale,
            bottom: (viewport[1] * 0.64).min(viewport[1] - 210.0 * scale),
            width: (620.0 * scale).min(viewport[0] - left * 2.0).max(1.0),
            row: 27.0 * scale,
            font: 18.0 * scale,
        }
    }
}

pub(super) struct Wrapped {
    pub(super) rows: [Range<usize>; WRAP_LINES],
    pub(super) len: usize,
    width: f32,
    size: f32,
    font_height: f32,
    font_modern: bool,
}

impl Default for Wrapped {
    fn default() -> Self {
        Self {
            rows: std::array::from_fn(|_| 0..0),
            len: 0,
            width: 0.0,
            size: 0.0,
            font_height: 0.0,
            font_modern: true,
        }
    }
}

impl Wrapped {
    /// Cache boundaries, never strings. Long words break at a character boundary.
    pub(super) fn update(&mut self, value: &str, font: &UiFont, width: f32, size: f32) {
        if self.len > 0
            && self.width == width
            && self.size == size
            && self.font_height == font.height
            && self.font_modern == font.is_modern()
        {
            return;
        }
        self.width = width;
        self.size = size;
        self.font_height = font.height;
        self.font_modern = font.is_modern();
        self.len = 0;
        let mut start = 0;
        while start < value.len() && self.len < WRAP_LINES {
            let end = fitting_end(&value[start..], font, width, size);
            let mut end = start + end;
            if end < value.len()
                && let Some(space) = value[start..end].rfind(' ')
                && space > 0
            {
                end = start + space;
            }
            self.rows[self.len] = start..end;
            self.len += 1;
            start = end;
            while value.as_bytes().get(start) == Some(&b' ') {
                start += 1;
            }
        }
        if self.len == 0 {
            self.len = 1;
            self.rows[0] = 0..0;
        }
    }
}

/// Work is linear in the bounded text length, with no temporary substring copies.
pub(super) fn fitting_end(value: &str, font: &UiFont, width: f32, size: f32) -> usize {
    let mut used = 0.0;
    let mut chars = value.char_indices().peekable();
    // Colour codes are zero-width, and truncating between `^` and its digit
    // would leave a stray caret in the drawn name.
    while let Some((i, c)) = chars.next() {
        if c == '^' && chars.peek().is_some_and(|(_, n)| n.is_ascii_digit()) {
            chars.next();
            continue;
        }
        let end = i + c.len_utf8();
        used += text::visible_text_width_face(
            font,
            &value[i..end],
            size / font.height,
            TextFace::Regular,
        );
        if (used > width || end > 480) && i > 0 {
            return i;
        }
    }
    value.len()
}
