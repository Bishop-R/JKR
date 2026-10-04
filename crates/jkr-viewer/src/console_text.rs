//! One measure for console text. The input line, its caret and selection, and the
//! scrollback rows are drawn and measured through the same [`ConsoleText`], so a
//! caret, a highlight or a pointer hit always lands on the glyph it names, whatever
//! size or letter spacing the console uses.

use crate::menu_widgets::MenuCanvas;
use crate::text::{TextFace, UiFont, visible_text_width_style};
use jkr_ui::{Color, FontWeight, Rect};

/// Console text of one size and letter spacing: what it draws is what it measures.
#[derive(Clone, Copy)]
pub(super) struct ConsoleText<'a> {
    font: &'a UiFont,
    /// Text size (line-box height) in physical pixels, as drawn.
    size: f32,
    /// Extra pen advance after every glyph, in physical pixels.
    spacing: f32,
}

impl<'a> ConsoleText<'a> {
    /// Text of `size` with `tracking` extra advance per glyph, as a fraction of `size`
    /// (0 keeps the font's own spacing).
    pub(super) fn new(font: &'a UiFont, size: f32, tracking: f32) -> Self {
        Self {
            font,
            size,
            spacing: tracking * size,
        }
    }

    /// Text size (line-box height) in physical pixels.
    pub(super) const fn size(&self) -> f32 {
        self.size
    }

    /// Glyph scale the text renderer applies for this size.
    fn scale(&self) -> f32 {
        self.size / self.font.height.max(1.0)
    }

    /// Pen advance of `glyph`, letter spacing included, exactly as drawn.
    pub(super) fn advance(&self, glyph: u8) -> f32 {
        self.font.glyph(TextFace::Regular, glyph).advance * self.scale() + self.spacing
    }

    /// Pen advance of `text` (colour codes take no room), exactly as drawn.
    pub(super) fn width(&self, text: &str) -> f32 {
        visible_text_width_style(
            self.font,
            text,
            self.scale(),
            TextFace::Regular,
            self.spacing,
        )
    }

    /// Draw `text` from the left edge of `rect`.
    pub(super) fn draw(&self, ui: &mut MenuCanvas, text: &str, rect: Rect, color: Color) {
        ui.text(
            text,
            rect,
            self.size,
            color,
            FontWeight::Regular,
            self.spacing,
        );
    }
}
