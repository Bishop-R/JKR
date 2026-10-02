//! Saber page rows: style, hilt, blade colour palette and its RGB
//! channels, twice for Dual.

use super::rows::{SaberRow, saber_color};
use super::saber::PALETTE;
use super::*;
use crate::menu_widgets::FormLayout;

impl PlayerMenu {
    /// Saber rows; returns the y below the last row.
    pub(super) fn append_saber_rows(&mut self, layout: &FormLayout) -> f32 {
        let s = layout.scale;
        let rows = self.saber_rows();
        let catalog = catalog_of(&self.loader);
        for (index, row) in rows.iter().enumerate() {
            let rect = layout.row_rect(index);
            let selected = index == self.selected;
            self.canvas.form_row_frame(rect, index as u16, selected, s);
            self.canvas.form_label(rect, row.label(), selected, s);
            let zone = layout.value_zone(rect);
            let color = self.canvas.form_value_color(selected);
            match row {
                SaberRow::Style => {
                    let label = self.saber.style().label();
                    self.canvas.form_cycler(zone, label, None, color, s);
                }
                SaberRow::Hilt | SaberRow::SecondHilt => {
                    let label = self.saber.hilt_label(catalog, row.second());
                    self.canvas.form_cycler(zone, label, None, color, s);
                }
                SaberRow::Blade | SaberRow::SecondBlade => {
                    // Six stock chips and one carrying the custom tint.
                    let custom = self.saber.custom_rgb(row.second());
                    let chips = PALETTE.map(|index| saber_color(index, custom));
                    let active = usize::from(self.saber.color(row.second()));
                    self.canvas.form_palette(zone, &chips, active, s);
                }
                _ => {
                    let Some(channel) = row.channel() else {
                        continue;
                    };
                    let value = self.saber.channel(row.second(), channel);
                    let ratio = f32::from(value) / 255.0;
                    if self.channel_entry.row() == Some(index) {
                        let (text, replacing) =
                            (self.channel_entry.text(), self.channel_entry.replacing());
                        self.canvas
                            .form_slider_entry(zone, text, replacing, ratio, s);
                    } else {
                        self.canvas
                            .form_slider(zone, &value.to_string(), ratio, color, s);
                    }
                }
            }
        }
        layout.row_rect(rows.len()).y
    }
}
