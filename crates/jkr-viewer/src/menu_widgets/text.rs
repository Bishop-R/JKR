//! Text storage and vector-font submission for [`MenuCanvas`].

use super::MenuCanvas;
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
        let color = self.legible_text(color);
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
        let color = self.legible_text(color);
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
