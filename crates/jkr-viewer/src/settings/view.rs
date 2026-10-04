//! Hero-style settings presentation over the live map: a left column of
//! box-free rows with inline sliders, toggles and cyclers, a tab strip and a
//! back cap, all built from the shared form widgets.

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
        let (title, note) = match self.section {
            Section::General => (
                "JKR   /   SETTINGS",
                "Changes apply immediately and are saved.",
            ),
            Section::Renderer => (
                "JKR   /   RENDERER",
                "Saved immediately; (restart) rows apply after restarting.",
            ),
        };
        let tabs = self.tabs();
        self.ui.form_header(&layout, title, tabs[self.tab], note);
        self.ui.form_tabs(&layout, tabs, self.tab);
        for (row, setting) in self.rows().iter().enumerate() {
            let draft = self.editing.as_ref().filter(|draft| draft.row == row);
            let value = draft
                .map(|draft| draft.text.as_str())
                .or_else(|| self.values.get(row).map(String::as_str))
                .unwrap_or("?");
            let editing = draft.is_some();
            row_view(
                &mut self.ui,
                &layout,
                row,
                row == self.selected,
                setting,
                value,
                editing,
                self.numeric.as_ref(),
            );
        }
        if let Some(action) = self.action() {
            let row = self.rows().len();
            let selected = row == self.selected;
            let value = match action {
                Action::Keybinds => "EDIT  >",
                Action::Renderer => "OPEN  >",
            };
            self.ui
                .form_action_row(&layout, row, selected, action.label(), value);
        }
        self.ui.form_footer(&layout, &KEY_HINTS);
        self.ui.end_hero();
        self.ui.finish(self.selected as u16);
        self.ui.append_text(vertices, font, viewport);
    }
}

/// One settings row: label, selection sweep and the value control.
#[allow(clippy::too_many_arguments)]
fn row_view(
    ui: &mut MenuCanvas,
    layout: &FormLayout,
    row: usize,
    selected: bool,
    setting: &Setting,
    value: &str,
    editing: bool,
    numeric: Option<&crate::menu_widgets::numeric::NumericEdit>,
) {
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
            ui.form_slider(value_zone, row, numeric, value, ratio, value_color, s);
        }
        ValueKind::Float { min, max, .. } => {
            let ratio = value
                .parse::<f64>()
                .map_or(0.0, |v| ((v - min) / (max - min)) as f32);
            ui.form_slider(value_zone, row, numeric, value, ratio, value_color, s);
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
