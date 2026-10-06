//! A compact action-progress bar below the reticle, independent of resolution.

use jkr_ui::{Color, DrawCommand, DrawList, Rect};

pub(super) fn emit(draw: &mut DrawList, remaining: Option<f32>, viewport: [f32; 2], scale: f32) {
    let Some(remaining) = remaining else {
        return;
    };
    let unit = (viewport[1] / 1080.0).clamp(2.0 / 3.0, 4.0 / 3.0) * scale.clamp(0.25, 2.0);
    let width = 108.0 * unit;
    let height = 6.0 * unit;
    let x = (viewport[0] - width) * 0.5;
    let y = viewport[1] * 0.5 + 44.0 * unit;
    let _ = draw.push(DrawCommand::SolidRect {
        rect: Rect::new(x - unit, y - unit, width + 2.0 * unit, height + 2.0 * unit),
        color: Color::new(0.0, 0.0, 0.0, 0.75),
    });
    let _ = draw.push(DrawCommand::SolidRect {
        rect: Rect::new(x, y, width * remaining, height),
        color: Color::new(1.0, 1.0, 0.0, 1.0),
    });
}
