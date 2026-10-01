//! Force page: the side cycler, the eighteen powers in a two-column grid
//! of pip rows, and the reset action. Row tokens follow `rows.rs`.

use super::force::POWER_NAMES;
use super::rows::{FORCE_POWER_ROW, FORCE_RESET_ROW, FORCE_SIDE_ROW};
use super::*;
use crate::menu_widgets::FormLayout;
use jkr_client::ForceSide;
use jkr_ui::{FontWeight, Rect, TextAlign};

/// Height of one power cell relative to a form row.
const CELL_HEIGHT: f32 = 0.8;
/// Gap between the two power columns, in form-scale pixels.
const COLUMN_GAP: f32 = 24.0;

/// Cell of power `index` (0..18): two columns under the side row.
fn power_cell(layout: &FormLayout, index: usize) -> Rect {
    let side = layout.row_rect(FORCE_SIDE_ROW);
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

impl PlayerMenu {
    /// Force rows; returns the y below the reset row.
    pub(super) fn append_force_rows(&mut self, layout: &FormLayout) -> f32 {
        let s = layout.scale;
        let side = layout.row_rect(FORCE_SIDE_ROW);
        let selected = self.selected == FORCE_SIDE_ROW;
        self.canvas
            .form_row_frame(side, FORCE_SIDE_ROW as u16, selected, s);
        self.canvas.form_label(side, "Side", selected, s);
        let color = self.canvas.form_value_color(selected);
        let label = match self.force.allocation().side {
            ForceSide::Light => "Light",
            ForceSide::Dark => "Dark",
        };
        self.canvas
            .form_cycler(layout.value_zone(side), label, None, color, s);
        let remaining = self.force.remaining_points();
        let rank = self.force.allocation().rank;
        self.canvas.text_fmt_aligned(
            format_args!("RANK {rank}   ·   {remaining} POINTS LEFT"),
            Rect::new(side.x, side.y - 26.0 * s, side.width, 18.0 * s),
            12.0 * s,
            self.canvas.theme().accent,
            FontWeight::Semibold,
            2.0 * s,
            TextAlign::End,
        );
        for (index, name) in POWER_NAMES.iter().enumerate() {
            self.append_power_cell(layout, index, name);
        }
        let reset_y = power_cell(layout, POWER_NAMES.len() - 1).bottom();
        let reset = Rect::new(side.x, reset_y, side.width, layout.row_height);
        self.append_reset_row(layout, reset);
        reset.bottom()
    }

    fn append_power_cell(&mut self, layout: &FormLayout, index: usize, name: &str) {
        let s = layout.scale;
        let row = FORCE_POWER_ROW + index;
        let cell = power_cell(layout, index);
        let selected = self.selected == row;
        self.canvas.form_row_frame(cell, row as u16, selected, s);
        let theme = self.canvas.theme();
        self.canvas.text(
            name,
            Rect::new(cell.x, cell.y + 12.0 * s, cell.width * 0.55, 18.0 * s),
            14.0 * s,
            if selected {
                theme.foreground
            } else {
                jkr_ui::Color::new(0.82, 0.88, 0.94, 0.78)
            },
            if selected {
                FontWeight::Semibold
            } else {
                FontWeight::Regular
            },
            0.2 * s,
        );
        let zone = layout.value_zone(cell);
        let level = self.force.allocation().levels[index];
        let pips = Rect::new(
            zone.x + 18.0 * s,
            cell.y + 15.0 * s,
            zone.width - 58.0 * s,
            10.0 * s,
        );
        self.canvas.pip_row(pips, level, theme.accent);
        let color = self.canvas.form_value_color(selected);
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

    fn append_reset_row(&mut self, layout: &FormLayout, rect: Rect) {
        let s = layout.scale;
        let selected = self.selected == FORCE_RESET_ROW;
        self.canvas
            .form_row_frame(rect, FORCE_RESET_ROW as u16, selected, s);
        self.canvas.form_label(rect, "Start over", selected, s);
        let zone = layout.value_zone(rect);
        let accent = self.canvas.theme().accent;
        self.canvas.text_aligned(
            "RESET  >",
            Rect::new(zone.x, rect.y + 16.0 * s, zone.width, 20.0 * s),
            14.0 * s,
            accent,
            FontWeight::Semibold,
            2.0 * s,
            TextAlign::End,
        );
    }
}
