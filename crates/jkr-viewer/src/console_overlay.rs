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
        if let Some(console) = &mut self.console {
            console.append_overlay(&mut self.text_vertices, &self.ui_font, viewport, text_scale);
        }
        if !covers_frame && hud::family::fps(self.console.as_ref()) {
            append_text(
                &mut self.text_vertices,
                &self.ui_font,
                self.frame_pacer.label(),
                [(viewport[0] - 780.0).max(8.0), 18.0],
                text_scale * 0.8,
                viewport,
            );
        }
    }
}
