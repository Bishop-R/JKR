//! Pointer interaction for the retained settings form: hover and the wheel
//! select rows, a click or drag on a slider's rail sets it, and a click on a
//! slider's value column opens typed entry. While a value is being typed the
//! selection stays put; a click elsewhere applies it first.

use super::*;
use jkr_ui::{InputEvent, UiEventKind};

impl SettingsMenu {
    pub(crate) fn handle_pointer(
        &mut self,
        event: InputEvent,
        console: &mut ViewerConsole,
    ) -> SettingsResult {
        let Some(event) = self.ui.pointer(event) else {
            return SettingsResult::None;
        };
        let Some(token) = event.token else {
            return SettingsResult::None;
        };
        let editing = self.entry.row().is_some() || self.editing.is_some();
        if event.kind == UiEventKind::Wheel {
            if editing {
                return SettingsResult::None;
            }
            let direction = event.delta.map_or(0, |delta| -delta.y.signum() as i32);
            let count = settings(self.tab).len() + usize::from(self.tab == KEYBINDS_TAB);
            if direction != 0 && count > 0 {
                self.selected = (self.selected as i32 + direction)
                    .clamp(0, count.saturating_sub(1) as i32)
                    as usize;
            }
            return SettingsResult::None;
        }
        if matches!(event.kind, UiEventKind::HoverEnter | UiEventKind::Hover) {
            if let Some(row) = self.setting_row(token).filter(|_| !editing) {
                self.selected = row;
            }
            return SettingsResult::None;
        }
        let row = self.setting_row(token);
        let in_value = row.is_some_and(|_| self.in_value_column(token, event.position));
        if event.kind == UiEventKind::Press {
            let slider = row.filter(|row| self.number_format(*row).is_some());
            self.entry.press(slider, in_value);
            return SettingsResult::None;
        }
        if event.kind == UiEventKind::Drag {
            if let (Some(row), Some(position)) = (row, event.position) {
                if !editing && self.entry.drag_moves(row, in_value) {
                    self.selected = row;
                    self.set_numeric_from_pointer(console, row, position.x);
                }
            }
            return SettingsResult::None;
        }
        if event.kind != UiEventKind::Activate {
            return SettingsResult::None;
        }
        match token {
            500.. if usize::from(token - 500) < TABS.len() => {
                self.tab = usize::from(token - 500);
                self.selected = 0;
                self.editing = None;
                self.entry.cancel();
                self.refresh(console);
            }
            900 => return SettingsResult::Back,
            _ if self.tab == KEYBINDS_TAB && usize::from(token) == settings(KEYBINDS_TAB).len() => {
                self.commit_edits(console);
                return SettingsResult::OpenKeybinds;
            }
            _ => {
                let Some(row) = row else {
                    return SettingsResult::None;
                };
                let opens_entry = self.entry.click_opens(row, in_value);
                let on_open_field = (self.entry.row() == Some(row) && in_value)
                    || (self.editing.is_some() && row == self.selected);
                if on_open_field {
                    return SettingsResult::None;
                }
                self.commit_edits(console);
                self.selected = row;
                if opens_entry && self.begin_entry(row) {
                    return SettingsResult::None;
                }
                if let Some(setting) = settings(self.tab).get(row) {
                    if matches!(setting.kind, ValueKind::Text) {
                        self.editing = Some(value_text(console, setting.cvar));
                    } else if let Some(position) = event.position {
                        if !self.set_numeric_from_pointer(console, row, position.x) {
                            self.adjust(console, 1);
                        }
                    } else {
                        self.adjust(console, 1);
                    }
                }
            }
        }
        SettingsResult::None
    }

    /// Whether `position` is over the value column of the slider row under
    /// `token`.
    fn in_value_column(&self, token: u16, position: Option<jkr_ui::Vec2>) -> bool {
        position
            .zip(self.ui.rect_for(token))
            .is_some_and(|(position, rect)| self.ui.slider_value_hit(rect, position.x))
    }

    fn setting_row(&self, token: u16) -> Option<usize> {
        let row = usize::from(token);
        (row < settings(self.tab).len()).then_some(row)
    }

    fn set_numeric_from_pointer(
        &mut self,
        console: &mut ViewerConsole,
        row: usize,
        pointer_x: f32,
    ) -> bool {
        let Some(setting) = settings(self.tab).get(row) else {
            return false;
        };
        let Some(rect) = self.ui.rect_for(row as u16) else {
            return false;
        };
        let ratio = f64::from(self.ui.slider_ratio(rect, pointer_x));
        let raw = match setting.kind {
            ValueKind::Integer { min, max, .. } => min as f64 + (max - min) as f64 * ratio,
            ValueKind::Float { min, max, .. } => min + (max - min) * ratio,
            _ => return false,
        };
        let Some(value) = setting.kind.snapped(raw) else {
            return false;
        };
        console.set_cvar(setting.cvar, &value);
        self.refresh(console);
        true
    }
}
