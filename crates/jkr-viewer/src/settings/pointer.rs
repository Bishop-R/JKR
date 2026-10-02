//! Pointer interaction for the retained settings form.

use super::*;
use crate::menu_widgets::cycler_direction;
use jkr_ui::{InputEvent, UiEventKind};

impl SettingsMenu {
    pub(crate) fn handle_pointer(
        &mut self,
        event: InputEvent,
        console: &mut ViewerConsole,
    ) -> SettingsResult {
        if self.picker.is_open() {
            self.resolution_pointer(event, console);
            return SettingsResult::None;
        }
        let Some(event) = self.ui.pointer(event) else {
            return SettingsResult::None;
        };
        let Some(token) = event.token else {
            return SettingsResult::None;
        };
        if event.kind == UiEventKind::Wheel {
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
            if let Some(row) = self.setting_row(token) {
                self.selected = row;
            }
            return SettingsResult::None;
        }
        if event.kind == UiEventKind::Drag {
            if let (Some(row), Some(position)) = (self.setting_row(token), event.position) {
                self.selected = row;
                self.set_numeric_from_pointer(console, row, position.x);
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
                self.refresh(console);
            }
            900 => return SettingsResult::Back,
            _ if self.tab == KEYBINDS_TAB && usize::from(token) == settings(KEYBINDS_TAB).len() => {
                return SettingsResult::OpenKeybinds;
            }
            _ => {
                let Some(row) = self.setting_row(token) else {
                    return SettingsResult::None;
                };
                self.selected = row;
                if let Some(setting) = settings(self.tab).get(row) {
                    if matches!(setting.kind, ValueKind::Text) {
                        self.editing = Some(value_text(console, setting.cvar));
                    } else if matches!(setting.kind, ValueKind::Resolution) {
                        self.open_resolutions(console);
                    } else if let Some(position) = event.position {
                        if !self.set_numeric_from_pointer(console, row, position.x) {
                            let direction = self.click_direction(setting.kind, row, position.x);
                            self.adjust(console, direction);
                        }
                    } else {
                        self.adjust(console, 1);
                    }
                }
            }
        }
        SettingsResult::None
    }

    /// Which way a click at `x` turns row `row`: a cycler steps the way the
    /// clicked half points, like its `<` and `>`; anything else steps on.
    fn click_direction(&self, kind: ValueKind, row: usize, x: f32) -> i32 {
        match (kind, self.ui.rect_for(row as u16)) {
            (ValueKind::Choice(_) | ValueKind::DisplayMode, Some(rect)) => {
                cycler_direction(rect, x) as i32
            }
            _ => 1,
        }
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
        let ratio = self.ui.slider_ratio(rect, pointer_x);
        let value = match setting.kind {
            ValueKind::Integer { min, max, step } => {
                let raw = min as f32 + (max - min) as f32 * ratio;
                let snapped = ((raw - min as f32) / step as f32).round() as i64 * step + min;
                snapped.clamp(min, max).to_string()
            }
            ValueKind::Float { min, max, step } => {
                let raw = min + (max - min) * f64::from(ratio);
                let snapped = ((raw - min) / step).round() * step + min;
                snapped.clamp(min, max).to_string()
            }
            _ => return false,
        };
        console.set_cvar(setting.cvar, &value);
        self.refresh(console);
        true
    }
}
