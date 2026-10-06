//! Text storage and vector-font submission for [`MenuCanvas`].

use super::MenuCanvas;
use crate::game_font::{GameFonts, RetailFont};
use crate::text::{TextVertex, UiFont};
use crate::ui_renderer;
use jkr_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign, TextId, TextOverflow};
use std::fmt::{Arguments, Write as _};

impl MenuCanvas {
    /// Add a non-interactive text run.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn text(
        &mut self,
        value: &str,
        rect: Rect,
        size: f32,
        color: Color,
        weight: FontWeight,
        letter_spacing: f32,
    ) {
        self.text_aligned(
            value,
            rect,
            size,
            color,
            weight,
            letter_spacing,
            TextAlign::Start,
        );
    }

    /// Add aligned, non-interactive text without allocating on the frame path.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn text_aligned(
        &mut self,
        value: &str,
        rect: Rect,
        size: f32,
        color: Color,
        weight: FontWeight,
        letter_spacing: f32,
        align: TextAlign,
    ) {
        let Some(id) = self.store_text(value) else {
            return;
        };
        let _ = self.draw.push(DrawCommand::Text {
            rect,
            text: id,
            size,
            color,
            align,
            overflow: TextOverflow::Ellipsis,
            weight,
            letter_spacing,
        });
    }

    /// Format directly into retained scratch storage and append aligned text.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn text_fmt_aligned(
        &mut self,
        value: Arguments<'_>,
        rect: Rect,
        size: f32,
        color: Color,
        weight: FontWeight,
        letter_spacing: f32,
        align: TextAlign,
    ) {
        let Some(id) = self.store_format(value) else {
            return;
        };
        let _ = self.draw.push(DrawCommand::Text {
            rect,
            text: id,
            size,
            color,
            align,
            overflow: TextOverflow::Ellipsis,
            weight,
            letter_spacing,
        });
    }

    /// Append retained text commands to the existing cached vector-font path.
    pub(crate) fn append_text(
        &self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        ui_renderer::append_text_commands(
            &self.draw,
            |id| self.resolve(id),
            vertices,
            font,
            viewport,
        );
    }

    /// Append retained text, each command in the game font `font_of` names for
    /// it when that font is on (see [`GameFonts::append_routed`]), the rest to
    /// `vertices` with `font`.
    pub(crate) fn append_text_routed(
        &self,
        fonts: &mut GameFonts,
        font_of: impl Fn(TextId, &str) -> Option<RetailFont> + Copy,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        fonts.append_routed(
            &self.draw,
            |id| self.resolve(id),
            font_of,
            (vertices, font),
            viewport,
        );
    }

    /// Id the next stored text run will get, to mark where a group of runs
    /// starts and ends.
    pub(crate) fn next_text_id(&self) -> u32 {
        self.text_len as u32
    }

    fn store_text(&mut self, value: &str) -> Option<TextId> {
        let slot = self.text.get_mut(self.text_len)?;
        slot.clear();
        slot.push_str(value);
        let id = TextId(self.text_len as u32);
        self.text_len += 1;
        Some(id)
    }

    fn store_format(&mut self, value: Arguments<'_>) -> Option<TextId> {
        let slot = self.text.get_mut(self.text_len)?;
        slot.clear();
        let _ = slot.write_fmt(value);
        let id = TextId(self.text_len as u32);
        self.text_len += 1;
        Some(id)
    }

    fn resolve(&self, id: TextId) -> &str {
        self.text
            .get(id.0 as usize)
            .map(String::as_str)
            .unwrap_or("")
    }
}
