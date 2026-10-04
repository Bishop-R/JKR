//! Force page: two side cards (Light left, Dark right), the eighteen powers
//! in a two-column grid of icon and pip rows, and a line of three actions
//! (Start over, Discard, Apply). Row tokens follow `rows.rs`.
//!
//! The page edits a draft (see `force`): while it differs from the applied
//! profile the points line reads NOT APPLIED, Apply is lit and Discard is
//! live, and the screen's subtitle and footer say that leaving drops it.
//! Powers the chosen side or the gametype rules out are dimmed in place
//! rather than hidden, so the grid never reflows when the side changes.

use super::force::POWER_NAMES;
use super::force_icons::{power_texture, side_texture};
use super::rows::{
    FORCE_APPLY_ROW, FORCE_DISCARD_ROW, FORCE_POWER_ROW, FORCE_RESET_ROW, FORCE_SIDE_ROW,
};
use super::*;
use crate::menu_widgets::FormLayout;
use jkr_client::ForceSide;
use jkr_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};

/// Height of the side picker relative to a form row.
const SIDE_HEIGHT: f32 = 1.5;
/// Height of one power cell relative to a form row.
const CELL_HEIGHT: f32 = 0.8;
/// Gap between the two power columns (and the side cards), in form-scale pixels.
const COLUMN_GAP: f32 = 24.0;
/// Gap between the action buttons, in form-scale pixels.
const ACTION_GAP: f32 = 12.0;

/// Card tints, after the stock force menu's blue `menu_blendbox` and red
/// `menu_blendboxr` side bars.
const LIGHT_TINT: Color = Color::new(0.36, 0.66, 1.0, 1.0);
const DARK_TINT: Color = Color::new(1.0, 0.33, 0.28, 1.0);

/// The side picker's row.
fn side_rect(layout: &FormLayout) -> Rect {
    let row = layout.row_rect(FORCE_SIDE_ROW);
    Rect::new(row.x, row.y, row.width, layout.row_height * SIDE_HEIGHT)
}

/// Cell of power `index` (0..18): two columns under the side picker.
fn power_cell(layout: &FormLayout, index: usize) -> Rect {
    let side = side_rect(layout);
    let gap = COLUMN_GAP * layout.scale;
    let width = (layout.column_width - gap) * 0.5;
    let height = layout.row_height * CELL_HEIGHT;
    Rect::new(
        side.x + (index % 2) as f32 * (width + gap),
        side.bottom() + (index / 2) as f32 * height,
        width,
        height,
    )
}

/// Button `slot` (0 reset, 1 discard, 2 apply) of the line under the grid.
fn action_rect(layout: &FormLayout, slot: usize) -> Rect {
    let s = layout.scale;
    let top = power_cell(layout, POWER_NAMES.len() - 1).bottom() + 10.0 * s;
    let gap = ACTION_GAP * s;
    let width = (layout.column_width - 2.0 * gap) / 3.0;
    Rect::new(
        layout.margin + slot as f32 * (width + gap),
        top,
        width,
        layout.row_height - 10.0 * s,
    )
}

/// Which side a pointer at `x` on the side picker `rect` picks, as an
/// `adjust` direction: the left card is Light (-1), the right Dark (1).
pub(super) fn side_direction(rect: Rect, x: f32) -> isize {
    if x < rect.x + rect.width * 0.5 { -1 } else { 1 }
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, alpha)
}

impl PlayerMenu {
    /// Force rows; returns the y below the action buttons.
    pub(super) fn append_force_rows(&mut self, layout: &FormLayout) -> f32 {
        let s = layout.scale;
        let side = side_rect(layout);
        self.append_points_line(side, s);
        self.append_side_picker(layout, side);
        for (index, name) in POWER_NAMES.iter().enumerate() {
            self.append_power_cell(layout, index, name);
        }
        let dirty = self.force.is_dirty();
        self.append_action(
            action_rect(layout, 0),
            FORCE_RESET_ROW,
            "START OVER",
            true,
            s,
        );
        self.append_action(
            action_rect(layout, 1),
            FORCE_DISCARD_ROW,
            "DISCARD",
            dirty,
            s,
        );
        let apply = if dirty { "APPLY" } else { "APPLIED" };
        self.append_action(action_rect(layout, 2), FORCE_APPLY_ROW, apply, dirty, s);
        action_rect(layout, 2).bottom()
    }

    /// Rank, points left and, while the draft is unapplied, a marker.
    fn append_points_line(&mut self, side: Rect, s: f32) {
        let remaining = self.force.remaining_points();
        let rank = self.force.allocation().rank;
        let pending = if self.force.is_dirty() {
            "   ·   NOT APPLIED"
        } else {
            ""
        };
        self.canvas.text_fmt_aligned(
            format_args!("RANK {rank}   ·   {remaining} POINTS LEFT{pending}"),
            Rect::new(side.x, side.y - 26.0 * s, side.width, 18.0 * s),
            12.0 * s,
            self.canvas.theme().accent,
            FontWeight::Semibold,
            2.0 * s,
            TextAlign::End,
        );
    }

    /// One row holding the Light and Dark cards; the half that is clicked
    /// picks (see [`side_direction`]).
    fn append_side_picker(&mut self, layout: &FormLayout, rect: Rect) {
        let s = layout.scale;
        let selected = self.selected == FORCE_SIDE_ROW;
        self.canvas
            .form_row_frame(rect, FORCE_SIDE_ROW as u16, selected, s);
        let gap = COLUMN_GAP * s;
        let width = (rect.width - gap) * 0.5;
        let cards = [
            (ForceSide::Light, "LIGHT SIDE", LIGHT_TINT, rect.x),
            (
                ForceSide::Dark,
                "DARK SIDE",
                DARK_TINT,
                rect.x + width + gap,
            ),
        ];
        let chosen_side = self.force.allocation().side;
        for (side, label, tint, x) in cards {
            let card = Rect::new(x, rect.y + 7.0 * s, width, rect.height - 14.0 * s);
            self.append_side_card(card, side, label, tint, side == chosen_side, s);
        }
    }

    fn append_side_card(
        &mut self,
        card: Rect,
        side: ForceSide,
        label: &str,
        tint: Color,
        chosen: bool,
        s: f32,
    ) {
        let theme = self.canvas.theme();
        let radius = 6.0 * s;
        let emblem = side_texture(side);
        let emblem_ready = self.force_icons.is_texture_ready(emblem);
        let draw = self.canvas.draw_list_mut();
        let _ = draw.push(DrawCommand::RoundedRect {
            rect: card,
            radius,
            color: if chosen {
                with_alpha(tint, 0.18)
            } else {
                Color::new(0.0, 0.0, 0.0, 0.32)
            },
        });
        let _ = draw.push(DrawCommand::Border {
            rect: card,
            radius,
            width: if chosen { 2.0 * s } else { 1.0 * s },
            color: if chosen {
                with_alpha(tint, 0.9)
            } else {
                Color::new(1.0, 1.0, 1.0, 0.12)
            },
        });
        let mut text_x = card.x + 18.0 * s;
        if emblem_ready {
            let size = card.height - 14.0 * s;
            let _ = draw.push(DrawCommand::TexturedQuad {
                rect: Rect::new(card.x + 7.0 * s, card.y + 7.0 * s, size, size),
                texture: emblem,
                color: Color::new(1.0, 1.0, 1.0, if chosen { 1.0 } else { 0.4 }),
            });
            text_x = card.x + size + 20.0 * s;
        }
        let text_rect = Rect::new(
            text_x,
            card.y + (card.height - 22.0 * s) * 0.5,
            card.right() - text_x - 8.0 * s,
            22.0 * s,
        );
        self.canvas.text(
            label,
            text_rect,
            16.0 * s,
            if chosen {
                theme.foreground
            } else {
                Color::new(0.82, 0.88, 0.94, 0.55)
            },
            FontWeight::Semibold,
            2.0 * s,
        );
    }

    fn append_power_cell(&mut self, layout: &FormLayout, index: usize, name: &str) {
        let s = layout.scale;
        let row = FORCE_POWER_ROW + index;
        let cell = power_cell(layout, index);
        let selected = self.selected == row;
        let available = self.force.is_available(index);
        self.canvas.form_row_frame(cell, row as u16, selected, s);
        let theme = self.canvas.theme();
        let icon = power_texture(index);
        let mut name_x = cell.x;
        if self.force_icons.is_texture_ready(icon) {
            let size = cell.height - 8.0 * s;
            let alpha = match (available, selected) {
                (false, _) => 0.22,
                (true, true) => 1.0,
                (true, false) => 0.85,
            };
            let _ = self.canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: Rect::new(cell.x, cell.y + 4.0 * s, size, size),
                texture: icon,
                color: Color::new(1.0, 1.0, 1.0, alpha),
            });
            name_x += size + 8.0 * s;
        }
        let zone = layout.value_zone(cell);
        let name_color = match (available, selected) {
            (false, _) => Color::new(0.82, 0.88, 0.94, 0.3),
            (true, true) => theme.foreground,
            (true, false) => Color::new(0.82, 0.88, 0.94, 0.78),
        };
        self.canvas.text(
            name,
            Rect::new(name_x, cell.y + 12.0 * s, zone.x - name_x, 18.0 * s),
            14.0 * s,
            name_color,
            if selected && available {
                FontWeight::Semibold
            } else {
                FontWeight::Regular
            },
            0.2 * s,
        );
        let level = self.force.allocation().levels[index];
        let pips = Rect::new(
            zone.x + 18.0 * s,
            cell.y + 15.0 * s,
            zone.width - 58.0 * s,
            10.0 * s,
        );
        self.canvas.pip_row(pips, level, theme.accent);
        let color = if available {
            self.canvas.form_value_color(selected)
        } else {
            with_alpha(theme.muted, 0.3)
        };
        self.canvas.text(
            "<",
            Rect::new(zone.x, cell.y + 9.0 * s, 14.0 * s, 20.0 * s),
            16.0 * s,
            color,
            FontWeight::Semibold,
            0.0,
        );
        self.canvas.text_fmt_aligned(
            format_args!("{level}"),
            Rect::new(
                pips.right() + 4.0 * s,
                cell.y + 11.0 * s,
                16.0 * s,
                18.0 * s,
            ),
            14.0 * s,
            color,
            FontWeight::Semibold,
            0.0,
            TextAlign::Center,
        );
        self.canvas.text_aligned(
            ">",
            Rect::new(
                zone.right() - 14.0 * s,
                cell.y + 9.0 * s,
                14.0 * s,
                20.0 * s,
            ),
            16.0 * s,
            color,
            FontWeight::Semibold,
            0.0,
            TextAlign::End,
        );
    }

    /// One action button answering to row `row`. A disabled one still takes
    /// hover and selection, but activating it changes nothing.
    fn append_action(&mut self, rect: Rect, row: usize, label: &str, enabled: bool, s: f32) {
        let token = row as u16;
        let selected = self.selected == row;
        let hovered = self.canvas.token_hovered(token);
        let primary = row == FORCE_APPLY_ROW && enabled;
        let theme = self.canvas.theme();
        let radius = 5.0 * s;
        let fill = if primary {
            with_alpha(theme.accent, if selected || hovered { 0.42 } else { 0.28 })
        } else if enabled && (selected || hovered) {
            Color::new(1.0, 1.0, 1.0, 0.1)
        } else {
            Color::new(1.0, 1.0, 1.0, 0.04)
        };
        let (border_width, border_color) = if selected {
            (2.0 * s, with_alpha(theme.accent, 0.9))
        } else if primary {
            (1.0 * s, with_alpha(theme.accent, 0.7))
        } else {
            let alpha = if enabled { 0.16 } else { 0.07 };
            (1.0 * s, Color::new(1.0, 1.0, 1.0, alpha))
        };
        let draw = self.canvas.draw_list_mut();
        let _ = draw.push(DrawCommand::RoundedRect {
            rect,
            radius,
            color: fill,
        });
        let _ = draw.push(DrawCommand::Border {
            rect,
            radius,
            width: border_width,
            color: border_color,
        });
        self.canvas.text_aligned(
            label,
            Rect::new(
                rect.x,
                rect.y + (rect.height - 16.0 * s) * 0.5,
                rect.width,
                16.0 * s,
            ),
            13.0 * s,
            if enabled {
                theme.foreground
            } else {
                Color::new(0.82, 0.88, 0.94, 0.35)
            },
            FontWeight::Semibold,
            2.0 * s,
            TextAlign::Center,
        );
        self.canvas.hit_region(token, rect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_cards_split_the_row_down_the_middle() {
        let rect = Rect::new(100.0, 0.0, 400.0, 75.0);
        assert_eq!(side_direction(rect, 120.0), -1);
        assert_eq!(side_direction(rect, 299.0), -1);
        assert_eq!(side_direction(rect, 300.0), 1);
        assert_eq!(side_direction(rect, 480.0), 1);
    }

    #[test]
    fn the_page_fits_above_the_footer() {
        for viewport in [[1280.0, 720.0], [1920.0, 1080.0], [2560.0, 1440.0]] {
            let layout = FormLayout::new(viewport);
            let apply = action_rect(&layout, 2);
            let footer = viewport[1] - 64.0 * layout.scale;
            assert!(
                apply.bottom() < footer - 16.0 * layout.scale,
                "{viewport:?}"
            );
            assert!(apply.right() <= layout.margin + layout.column_width + 0.01);
        }
    }
}
