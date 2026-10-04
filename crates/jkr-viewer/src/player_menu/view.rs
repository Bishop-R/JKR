//! Hero-form presentation of the player screen: header, tab strip, the
//! current page's rows, a catalogue status line and the key-cap footer. The
//! right of the screen is left open for the stage model.

use super::grid::{GridLayout, PART_ROW};
use super::rows::CharacterRow;
use super::*;
use crate::menu_widgets::{FormLayout, Scrim};
use crate::text::{TextVertex, UiFont};
use jkr_client::LegacyCatalogStatus;
use jkr_ui::{Color, FontWeight, Rect};

/// Footer caps; only the back cap, which doubles as the pointer's way out.
const KEY_HINTS: [(&str, &str); 1] = [("ESC", "Back")];
/// The same while the Force page holds an unapplied draft, which leaving drops.
const KEY_HINTS_FORCE_PENDING: [(&str, &str); 1] =
    [("ESC", "Back, dropping unapplied Force changes")];

impl PlayerMenu {
    pub(crate) fn append(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        reveal: f32,
    ) {
        let layout = FormLayout::new(viewport);
        self.canvas.begin_hero(viewport, reveal, Scrim::Column);
        let pending = self.force.is_dirty();
        let subtitle = match (self.page, pending) {
            (ProfilePage::Force, false) => "Pick a side and spend your points, then Apply.",
            (ProfilePage::Force, true) => "Not applied yet: Apply sends these powers.",
            _ => "Changes apply immediately.",
        };
        self.canvas.form_header(
            &layout,
            "JKR   /   PLAYER",
            PAGE_TABS[self.page.index()],
            subtitle,
        );
        self.canvas
            .form_tabs(&layout, &PAGE_TABS, self.page.index());
        let rows_bottom = match self.page {
            ProfilePage::Character => self.append_character_rows(&layout),
            ProfilePage::Saber => self.append_saber_rows(&layout),
            ProfilePage::Force => self.append_force_rows(&layout),
        };
        self.append_status(&layout, rows_bottom);
        let hints = if pending {
            &KEY_HINTS_FORCE_PENDING
        } else {
            &KEY_HINTS
        };
        self.canvas.form_footer(&layout, hints);
        self.canvas.end_hero();
        self.canvas.finish(self.selected as u16);
        self.canvas.append_text(vertices, font, viewport);
    }

    /// Name, Team colour and Model rows, the icon grid and (for a species)
    /// the part cyclers in two columns; returns the y below the last of them.
    fn append_character_rows(&mut self, layout: &FormLayout) -> f32 {
        let s = layout.scale;
        let rows = self.character_rows();
        let part_rows = rows.len().saturating_sub(PART_ROW);
        let grid = GridLayout::new(layout, self.tiles.len(), part_rows);
        let catalog = catalog_of(&self.loader);
        let species = match self.choice {
            Some(Choice::Species(index)) => catalog.and_then(|catalog| catalog.species.get(index)),
            _ => None,
        };
        let mut bottom = grid.rect.bottom();
        for (index, row) in rows.iter().enumerate() {
            let rect = match row.axis() {
                Some(axis) => grid.part_cell(layout, axis),
                None => layout.row_rect(index),
            };
            bottom = bottom.max(rect.bottom());
            let selected = index == self.selected;
            self.canvas.form_row_frame(rect, index as u16, selected, s);
            self.canvas.form_label(rect, row.label(), selected, s);
            let zone = layout.value_zone(rect);
            let color = self.canvas.form_value_color(selected);
            match row {
                CharacterRow::Name => {
                    self.canvas.form_value(&self.draft.name, zone, color, s);
                    if self.name_editing {
                        let field =
                            Rect::new(zone.x, rect.y + 6.0 * s, zone.width, rect.height - 12.0 * s);
                        let accent = self.canvas.theme().accent;
                        self.canvas.edit_underline(field, accent, s);
                    }
                }
                CharacterRow::Team => {
                    self.canvas
                        .form_cycler(zone, self.team.label(), None, color, s);
                }
                CharacterRow::Model => {
                    let label = match (self.choice, catalog) {
                        (Some(Choice::Character(index)), Some(catalog)) => catalog
                            .characters
                            .get(index)
                            .map_or(self.draft.model.as_str(), |entry| entry.cvar_value.as_str()),
                        _ => species.map_or(self.draft.model.as_str(), |species| &species.model),
                    };
                    self.canvas.form_cycler(zone, label, None, color, s);
                }
                CharacterRow::Skin => {
                    // The legacy entries are tint-icon shader paths; the chip
                    // shows the colour, the text just counts the choices.
                    let count = species.map_or(0, |species| species.colors.len());
                    let swatch = Some(rgb_color(self.draft.rgb));
                    if count == 0 {
                        self.canvas.form_cycler(zone, "-", swatch, color, s);
                    } else {
                        self.canvas.form_cycler_fmt(
                            zone,
                            format_args!("{} / {count}", self.variants[3] + 1),
                            swatch,
                            color,
                            s,
                        );
                    }
                }
                part => {
                    let axis = part.axis().unwrap_or(0);
                    let label = species
                        .and_then(|species| {
                            let parts = match axis {
                                0 => &species.heads,
                                1 => &species.torsos,
                                _ => &species.legs,
                            };
                            parts.get(self.variants[axis]).map(String::as_str)
                        })
                        .unwrap_or("-");
                    self.canvas.form_cycler(zone, label, None, color, s);
                }
            }
        }
        self.append_grid(layout, &grid);
        bottom
    }

    /// Catalogue progress or failure under the rows.
    fn append_status(&mut self, layout: &FormLayout, y: f32) {
        let s = layout.scale;
        let (text, color) = match self.status() {
            LegacyCatalogStatus::Ready => return,
            LegacyCatalogStatus::Idle | LegacyCatalogStatus::Loading => (
                "Reading the character catalogue...",
                self.canvas.theme().muted,
            ),
            LegacyCatalogStatus::Failed => (
                self.loader
                    .as_ref()
                    .and_then(LegacyAssetCatalogLoader::error)
                    .unwrap_or("The character catalogue is unavailable."),
                Color::new(1.0, 0.62, 0.55, 0.95),
            ),
        };
        self.canvas.text(
            text,
            Rect::new(layout.margin, y + 18.0 * s, layout.column_width, 20.0 * s),
            14.0 * s,
            color,
            FontWeight::Regular,
            0.2 * s,
        );
    }
}

fn rgb_color(rgb: [u8; 3]) -> Color {
    Color::new(
        f32::from(rgb[0]) / 255.0,
        f32::from(rgb[1]) / 255.0,
        f32::from(rgb[2]) / 255.0,
        1.0,
    )
}
