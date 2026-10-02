//! Hero-style settings presentation over the live map: a left column of
//! box-free rows with inline sliders, toggles and cyclers, a tab strip and a
//! back cap, all built from the shared form widgets. A slider whose value is
//! being typed shows the typed text in its value column.

use super::*;
use crate::menu_widgets::{FormLayout, MenuCanvas, Scrim};

/// Footer caps; only the back cap, which doubles as the pointer's way out.
const KEY_HINTS: [(&str, &str); 1] = [("ESC", "Back")];

impl SettingsMenu {
    pub(crate) fn append(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        _scale: f32,
        reveal: f32,
    ) {
        let layout = FormLayout::new(viewport);
        self.ui.begin_hero(viewport, reveal, Scrim::Full);
        self.ui.form_header(
            &layout,
            "JKR   /   SETTINGS",
            TABS[self.tab],
            "Changes apply immediately and are saved.",
        );
        self.ui.form_tabs(&layout, &TABS, self.tab);
        for (row, setting) in settings(self.tab).iter().enumerate() {
            let value = self
                .editing
                .as_deref()
                .filter(|_| row == self.selected)
                .or_else(|| self.values.get(row).map(String::as_str))
                .unwrap_or("?");
            let editing = self.editing.is_some() && row == self.selected;
            let entry = (self.entry.row() == Some(row))
                .then(|| (self.entry.text(), self.entry.replacing()));
            row_view(
                &mut self.ui,
                &layout,
                RowState {
                    row,
                    selected: row == self.selected,
                    value,
                    editing,
                    entry,
                },
                setting,
            );
        }
        if self.tab == KEYBINDS_TAB {
            let row = settings(KEYBINDS_TAB).len();
            let selected = row == self.selected;
            self.ui
                .form_action_row(&layout, row, selected, "Key bindings", "EDIT  >");
        }
        self.ui.form_footer(&layout, &KEY_HINTS);
        self.ui.end_hero();
        self.ui.finish(self.selected as u16);
        self.ui.append_text(vertices, font, viewport);
    }
}

/// What one row shows this frame.
struct RowState<'a> {
    row: usize,
    selected: bool,
    /// Current value text (or the open text edit's buffer).
    value: &'a str,
    /// A text row's inline edit is open.
    editing: bool,
    /// A slider's typed entry and whether its next key replaces it.
    entry: Option<(&'a str, bool)>,
}

/// One settings row: label, selection sweep and the value control.
fn row_view(ui: &mut MenuCanvas, layout: &FormLayout, state: RowState<'_>, setting: &Setting) {
    let RowState {
        row,
        selected,
        value,
        editing,
        entry,
    } = state;
    let s = layout.scale;
    let rect = layout.row_rect(row);
    let theme = ui.theme();
    ui.form_row_frame(rect, row as u16, selected, s);
    ui.form_label(rect, setting.label, selected, s);
    let value_zone = layout.value_zone(rect);
    let value_color = ui.form_value_color(selected);
    match setting.kind {
        ValueKind::Bool => {
            let on = value.eq_ignore_ascii_case("on");
            let pill = Rect::new(
                value_zone.right() - 44.0 * s,
                rect.y + 15.0 * s,
                44.0 * s,
                22.0 * s,
            );
            ui.toggle_pill(pill, on, theme.accent);
            let label = Rect::new(
                value_zone.x,
                rect.y,
                value_zone.width - 58.0 * s,
                rect.height,
            );
            ui.form_value(value, label, value_color, s);
        }
        ValueKind::Integer { min, max, .. } => {
            let span = (max - min) as f32;
            let ratio = value
                .parse::<f32>()
                .map_or(0.0, |v| (v - min as f32) / span);
            slider(ui, value_zone, value, entry, ratio, value_color, s);
        }
        ValueKind::Float { min, max, .. } => {
            let ratio = value
                .parse::<f64>()
                .map_or(0.0, |v| ((v - min) / (max - min)) as f32);
            slider(ui, value_zone, value, entry, ratio, value_color, s);
        }
        ValueKind::Choice(_) => ui.form_cycler(value_zone, value, None, value_color, s),
        ValueKind::Text => {
            ui.form_value(value, value_zone, value_color, s);
            if editing {
                let field = Rect::new(
                    value_zone.x,
                    rect.y + 6.0 * s,
                    value_zone.width,
                    rect.height - 12.0 * s,
                );
                ui.edit_underline(field, theme.accent, s);
            }
        }
    }
}

/// A slider control, showing the typed `entry` instead of `value` while one
/// is open.
fn slider(
    ui: &mut MenuCanvas,
    zone: Rect,
    value: &str,
    entry: Option<(&str, bool)>,
    ratio: f32,
    color: jkr_ui::Color,
    s: f32,
) {
    match entry {
        Some((text, replacing)) => ui.form_slider_entry(zone, text, replacing, ratio, s),
        None => ui.form_slider(zone, value, ratio, color, s),
    }
}
