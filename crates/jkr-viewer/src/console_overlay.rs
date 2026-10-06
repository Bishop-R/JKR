//! Console overlay ordering, including the full-frame command browser.
use super::*;

impl GpuState {
    pub(super) fn console_covers_frame(&self) -> bool {
        self.console
            .as_ref()
            .is_some_and(|console| console.covers_frame())
    }

    pub(super) fn append_console_overlay(&mut self, viewport: [f32; 2], text_scale: f32) {
        let covers_frame = self.console_covers_frame();
        if covers_frame {
            // Both font batches must be cleared: text is drawn above all UI shapes.
            self.text_vertices.clear();
            self.classic_text_vertices.clear();
        }
        // The console and its notify lines draw with the retail console character
        // set when `ui_gameFont` has it; the full-frame browser keeps Inter.
        let (vertices, font) = if covers_frame {
            (&mut self.text_vertices, &self.ui_font)
        } else {
            self.game_fonts.target(
                game_font::RetailFont::Console,
                &mut self.text_vertices,
                &self.ui_font,
            )
        };
        if let Some(console) = &mut self.console {
            console.append_overlay(vertices, font, viewport, text_scale);
        }
        if !covers_frame && hud::family::fps(self.console.as_ref()) {
            // CG_DrawFPS draws console characters (CG_DrawBigString).
            let size = self.ui_font.height * text_scale * 0.8;
            let scale = game_font::scale_for(font, size);
            append_text(
                vertices,
                font,
                self.frame_pacer.label(),
                [(viewport[0] - 780.0).max(8.0), 18.0],
                scale,
                viewport,
            );
        }
    }
}
