//! Shared bounds keep match results and conversation apart at any viewport size.
pub(crate) struct Layout {
    pub(crate) scale: f32,
    pub(crate) chat_left: f32,
    pub(crate) chat_width: f32,
    pub(crate) table_left: f32,
    pub(crate) table_width: f32,
}

impl Layout {
    pub(crate) fn new(viewport: [f32; 2]) -> Self {
        let scale = (viewport[1] / crate::ui_scale::REFERENCE_HEIGHT)
            .min(viewport[0] / 1400.0)
            .min(crate::ui_scale::MAX);
        let margin = 40.0 * scale;
        let chat_width = (viewport[0] * 0.32).min(600.0 * scale);
        let table_left = (margin + chat_width + 60.0 * scale).max(viewport[0] * 0.38);
        Self {
            scale,
            chat_left: margin,
            chat_width,
            table_left,
            table_width: (viewport[0] - table_left - margin).min(940.0 * scale),
        }
    }
}
