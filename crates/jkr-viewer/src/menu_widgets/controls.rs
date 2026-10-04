//! Box-free value controls for hero-style forms: a hairline slider rail, a
//! pill toggle and a text-edit underline. Slider values own an edit target.

use super::MenuCanvas;
use super::form::SLIDER_VALUE_COLUMN;
use jkr_ui::{Color, DrawCommand, Rect};

fn dim(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, alpha)
}

impl MenuCanvas {
    /// Hairline rail across `rect` with the leading `ratio` filled in `color`
    /// and a round knob at the split.
    pub(crate) fn slider_rail(&mut self, rect: Rect, ratio: f32, color: Color, scale: f32) {
        let ratio = ratio.clamp(0.0, 1.0);
        let thickness = 2.0 * scale;
        let y = rect.y + (rect.height - thickness) * 0.5;
        let split = rect.x + rect.width * ratio;
        let _ = self.draw.push(DrawCommand::RoundedRect {
            rect: Rect::new(rect.x, y, rect.width, thickness),
            radius: thickness,
            color: Color::new(1.0, 1.0, 1.0, 0.16),
        });
        let _ = self.draw.push(DrawCommand::RoundedRect {
            rect: Rect::new(rect.x, y, split - rect.x, thickness),
            radius: thickness,
            color: dim(color, 0.9),
        });
        let knob = 10.0 * scale;
        let _ = self.draw.push(DrawCommand::RoundedRect {
            rect: Rect::new(
                split - knob * 0.5,
                y + thickness * 0.5 - knob * 0.5,
                knob,
                knob,
            ),
            radius: knob * 0.5,
            color,
        });
    }

    /// A numeric row's control: the rail across the value zone with the
    /// value text in a 72-unit column at its right.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn form_slider(
        &mut self,
        zone: Rect,
        row: usize,
        editing: Option<&super::numeric::NumericEdit>,
        value: &str,
        ratio: f32,
        color: Color,
        scale: f32,
    ) {
        let column = SLIDER_VALUE_COLUMN * scale;
        let rail = Rect::new(
            zone.x,
            zone.y + 18.0 * scale,
            zone.width - column,
            16.0 * scale,
        );
        self.slider_rail(rail, ratio, color, scale);
        let text = Rect::new(rail.right(), zone.y, column, zone.height);
        self.hit_region(super::numeric::VALUE_BASE + row as u16, text);
        if let Some(edit) = editing.filter(|edit| edit.row == row) {
            edit.draw(
                self,
                Rect::new(text.x, text.y + 16.0 * scale, text.width, 22.0 * scale),
                scale,
            );
        } else {
            self.form_value(value, text, color, scale);
        }
    }

    /// A row of plain colour chips filling the value zone in equal cells,
    /// only the `active` one ringed in white. Pointer cells match
    /// [`palette_index`](super::palette_index).
    pub(crate) fn form_palette(&mut self, zone: Rect, chips: &[Color], active: usize, scale: f32) {
        let cell = zone.width / chips.len().max(1) as f32;
        let gap = (6.0 * scale).min(cell * 0.25);
        let height = 18.0 * scale;
        let y = zone.y + (zone.height - height) * 0.5;
        for (index, chip) in chips.iter().enumerate() {
            let rect = Rect::new(
                zone.x + cell * index as f32 + gap * 0.5,
                y,
                cell - gap,
                height,
            );
            let radius = height * 0.5;
            let _ = self.draw.push(DrawCommand::RoundedRect {
                rect,
                radius,
                color: *chip,
            });
            if index == active {
                // A white ring set off from the chip marks the active one.
                let inset = -3.0 * scale;
                let halo = Rect::new(
                    rect.x + inset,
                    rect.y + inset,
                    rect.width - inset * 2.0,
                    rect.height - inset * 2.0,
                );
                let _ = self.draw.push(DrawCommand::Border {
                    rect: halo,
                    radius: radius - inset,
                    width: 2.0 * scale,
                    color: Color::new(1.0, 1.0, 1.0, 0.95),
                });
            }
        }
    }

    /// Pill toggle: filled in `color` with the knob right while `on`, a faint
    /// outline with the knob left otherwise.
    pub(crate) fn toggle_pill(&mut self, rect: Rect, on: bool, color: Color) {
        let radius = rect.height * 0.5;
        let _ = self.draw.push(DrawCommand::RoundedRect {
            rect,
            radius,
            color: if on {
                dim(color, 0.85)
            } else {
                Color::new(1.0, 1.0, 1.0, 0.10)
            },
        });
        let _ = self.draw.push(DrawCommand::Border {
            rect,
            radius,
            width: 1.0,
            color: if on {
                dim(color, 0.0)
            } else {
                Color::new(1.0, 1.0, 1.0, 0.22)
            },
        });
        let inset = rect.height * 0.15;
        let knob = rect.height - inset * 2.0;
        let x = if on {
            rect.right() - inset - knob
        } else {
            rect.x + inset
        };
        let _ = self.draw.push(DrawCommand::RoundedRect {
            rect: Rect::new(x, rect.y + inset, knob, knob),
            radius: knob * 0.5,
            color: if on {
                Color::new(0.03, 0.02, 0.02, 0.95)
            } else {
                Color::new(0.85, 0.89, 0.94, 0.8)
            },
        });
    }

    /// Underline marking an active inline text edit.
    pub(crate) fn edit_underline(&mut self, rect: Rect, color: Color, scale: f32) {
        let _ = self.draw.push(DrawCommand::SolidRect {
            rect: Rect::new(rect.x, rect.bottom() - 2.0 * scale, rect.width, 2.0 * scale),
            color,
        });
    }

    /// Thin, display-only position mark in `track` for a list showing
    /// `page` of its `count` rows from `first`.
    pub(crate) fn list_scroll_mark(
        &mut self,
        track: Rect,
        first: usize,
        page: usize,
        count: usize,
        scale: f32,
    ) {
        let thumb_height = (track.height * page as f32 / count.max(1) as f32).max(24.0 * scale);
        let travel = track.height - thumb_height;
        let at = first as f32 / count.saturating_sub(page).max(1) as f32;
        let _ = self.draw.push(DrawCommand::RoundedRect {
            rect: track,
            radius: track.width * 0.5,
            color: Color::new(1.0, 1.0, 1.0, 0.07),
        });
        let _ = self.draw.push(DrawCommand::RoundedRect {
            rect: Rect::new(
                track.x,
                track.y + travel * at.clamp(0.0, 1.0),
                track.width,
                thumb_height,
            ),
            radius: track.width * 0.5,
            color: Color::new(0.62, 0.72, 0.80, 0.55),
        });
    }
}
