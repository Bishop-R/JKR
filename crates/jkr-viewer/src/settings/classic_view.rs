//! The settings rows as a classic option panel (`setup.menu` items): the
//! label set against the label column, the value after it; toggles read
//! Yes or No, numbers are retail sliders with their value beside them, and
//! the focused item sits on the `menu_blendbox` highlight.

use super::*;
use crate::menu::classic::layout::Span;
use crate::menu::classic::panel::{FOCUS_TEXT, OPTION, PanelFrame};

impl SettingsMenu {
    /// Show rows `span` of tab `tab` as a classic option panel drawn in
    /// `frame`'s geometry.
    pub(crate) fn open_classic(
        &mut self,
        console: &ViewerConsole,
        tab: usize,
        span: Span,
        frame: crate::menu::classic::panel::Frame,
    ) {
        self.open_tab(console, tab);
        let rows = span.within(0, settings(self.tab).len());
        self.selected = rows.start;
        self.classic = Some(ClassicRows {
            rows,
            slider_span: frame.slider_span(),
        });
    }

    /// Back to the full tabbed screen, keeping the tab.
    pub(crate) fn leave_classic(&mut self) {
        if self.classic.take().is_some() {
            self.editing = None;
            self.numeric = None;
        }
    }

    /// Draw the classic panel screen around the rows of the open span.
    pub(crate) fn append_classic(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        reveal: f32,
        frame: &PanelFrame,
    ) {
        let rows = self.row_span();
        let place = frame.begin(&mut self.ui, viewport, reveal);
        let shown = rows.start..rows.end.min(rows.start + place.capacity());
        if shown.contains(&self.selected) {
            place.highlight(&mut self.ui, self.selected - rows.start);
        }
        for row in shown.clone() {
            let slot = row - rows.start;
            let Some(setting) = settings(self.tab).get(row) else {
                continue;
            };
            let focused = row == self.selected;
            let color = if focused { FOCUS_TEXT } else { OPTION };
            place.label(&mut self.ui, slot, setting.label, color);
            self.ui.hit_region(row as u16, place.row(slot));
            let value = self.values.get(row).map_or("?", String::as_str);
            match setting.kind {
                ValueKind::Bool => {
                    let yes = value.eq_ignore_ascii_case("on");
                    place.value(&mut self.ui, slot, if yes { "Yes" } else { "No" }, color);
                }
                ValueKind::Integer { .. } | ValueKind::Float { .. } => {
                    place.draw_slider_bar(&mut self.ui, slot, color);
                    let editing = self.numeric.as_ref().filter(|edit| edit.row == row);
                    let target = place.slider_value_rect(slot);
                    self.ui.hit_region(
                        crate::menu_widgets::numeric::VALUE_BASE + row as u16,
                        target,
                    );
                    match editing {
                        Some(edit) => edit.draw(&mut self.ui, target, place.scale()),
                        None => place.slider_value(&mut self.ui, slot, value, color),
                    }
                }
                ValueKind::Choice(_) => place.value(&mut self.ui, slot, value, color),
                ValueKind::Text => match self.editing.as_deref().filter(|_| focused) {
                    Some(buffer) => {
                        place.value_fmt(&mut self.ui, slot, format_args!("{buffer}_"), color)
                    }
                    None => place.value(&mut self.ui, slot, value, color),
                },
            }
        }
        // Thumbs after every bar, so the slider art binds once each.
        for row in shown {
            let slot = row - rows.start;
            let Some(setting) = settings(self.tab).get(row) else {
                continue;
            };
            let value = self
                .values
                .get(row)
                .and_then(|value| value.parse::<f64>().ok());
            let ratio = match (setting.kind, value) {
                (ValueKind::Integer { min, max, .. }, Some(value)) if max > min => {
                    ((value - min as f64) / (max - min) as f64) as f32
                }
                (ValueKind::Float { min, max, .. }, Some(value)) if max > min => {
                    ((value - min) / (max - min)) as f32
                }
                (ValueKind::Integer { .. } | ValueKind::Float { .. }, _) => 0.0,
                _ => continue,
            };
            place.draw_slider_thumb(&mut self.ui, slot, ratio);
        }
        let hint = if self.numeric.is_some() {
            Some("Type a value, then ENTER to set it or ESC to cancel.")
        } else if self.editing.is_some() {
            Some("Type the new text, then ENTER to set it or ESC to cancel.")
        } else {
            None
        };
        place.finish(&mut self.ui, hint);
        self.ui.finish(self.selected as u16);
        self.ui.append_text(vertices, font, viewport);
    }
}
