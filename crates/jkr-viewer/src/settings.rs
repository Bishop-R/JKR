//! Retained tabbed settings UI backed directly by archived shell cvars.

use crate::console::ViewerConsole;
use crate::menu_widgets::MenuCanvas;
use crate::text::{TextVertex, UiFont};
use jkr_shell::CvarValue;
use jkr_ui::{DrawList, Rect};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

mod catalog;
mod numeric;
mod pointer;
mod view;

pub(crate) use catalog::RESOLUTIONS;
use catalog::*;
pub(crate) enum SettingsResult {
    None,
    Back,
    OpenKeybinds,
}

/// A Text setting being typed. Its row is fixed when typing starts, so Enter
/// writes the setting that was opened even if the pointer moved meanwhile.
struct TextDraft {
    row: usize,
    text: String,
}

pub(crate) struct SettingsMenu {
    tab: usize,
    selected: usize,
    values: Vec<String>,
    editing: Option<TextDraft>,
    numeric: Option<crate::menu_widgets::numeric::NumericEdit>,
    ui: MenuCanvas,
}

impl SettingsMenu {
    pub(crate) fn new() -> Self {
        Self {
            tab: 0,
            selected: 0,
            values: Vec::with_capacity(12),
            editing: None,
            numeric: None,
            ui: MenuCanvas::new(),
        }
    }

    /// Index of the tab that carries the "Key bindings" row.
    pub(crate) fn keybinds_tab() -> usize {
        KEYBINDS_TAB
    }

    pub(crate) fn open(&mut self, console: &ViewerConsole) {
        self.open_tab(console, 0);
    }

    /// Open on tab `tab` (clamped to the catalogue).
    pub(crate) fn open_tab(&mut self, console: &ViewerConsole, tab: usize) {
        self.tab = tab.min(TABS.len() - 1);
        self.selected = 0;
        self.editing = None;
        self.numeric = None;
        self.refresh(console);
    }
    pub(crate) fn visual_selection(&self) -> (usize, bool) {
        (self.selected, false)
    }
    pub(crate) fn draw_list(&self) -> &DrawList {
        self.ui.draw_list()
    }

    pub(crate) fn handle_key(
        &mut self,
        event: &KeyEvent,
        console: &mut ViewerConsole,
    ) -> SettingsResult {
        if event.state != ElementState::Pressed {
            return SettingsResult::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return SettingsResult::None;
        };
        if self.edit_numeric(key, event.text.as_deref(), console) {
            return SettingsResult::None;
        }
        if let Some(draft) = &mut self.editing {
            let buffer = &mut draft.text;
            match key {
                KeyCode::Escape => self.editing = None,
                KeyCode::Enter | KeyCode::NumpadEnter => {
                    if let Some(draft) = self.editing.take()
                        && let Some(setting) = settings(self.tab).get(draft.row)
                    {
                        console.set_cvar(setting.cvar, draft.text.trim());
                    }
                    self.refresh(console);
                }
                KeyCode::Backspace => {
                    buffer.pop();
                }
                _ if !event.repeat => {
                    if let Some(text) = event.text.as_deref() {
                        buffer.extend(
                            text.chars()
                                .filter(|c| !c.is_control())
                                .take(128usize.saturating_sub(buffer.len())),
                        );
                    }
                }
                _ => {}
            }
            return SettingsResult::None;
        }
        if event.repeat {
            return SettingsResult::None;
        }
        let count = settings(self.tab).len() + usize::from(self.tab == KEYBINDS_TAB);
        match key {
            KeyCode::Tab | KeyCode::BracketRight => {
                self.tab = (self.tab + 1) % TABS.len();
                self.selected = 0;
                self.refresh(console);
            }
            KeyCode::BracketLeft => {
                self.tab = self.tab.checked_sub(1).unwrap_or(TABS.len() - 1);
                self.selected = 0;
                self.refresh(console);
            }
            KeyCode::ArrowUp | KeyCode::KeyW => {
                self.selected = self.selected.checked_sub(1).unwrap_or(count - 1)
            }
            KeyCode::ArrowDown | KeyCode::KeyS => self.selected = (self.selected + 1) % count,
            KeyCode::ArrowLeft | KeyCode::KeyA => self.adjust(console, -1),
            KeyCode::ArrowRight | KeyCode::KeyD => self.adjust(console, 1),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space
                if self.tab == KEYBINDS_TAB && self.selected == settings(KEYBINDS_TAB).len() =>
            {
                return SettingsResult::OpenKeybinds;
            }
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                if let Some(setting) = settings(self.tab).get(self.selected) {
                    if matches!(setting.kind, ValueKind::Text) {
                        self.begin_text(console, self.selected);
                    } else if !self.begin_numeric(console, self.selected) {
                        self.adjust(console, 1);
                    }
                }
            }
            KeyCode::Escape => return SettingsResult::Back,
            _ => {}
        }
        SettingsResult::None
    }

    fn adjust(&mut self, console: &mut ViewerConsole, direction: i32) {
        let Some(setting) = settings(self.tab).get(self.selected) else {
            return;
        };
        let next = match (setting.kind, console.cvar(setting.cvar)) {
            (ValueKind::Bool, Some(CvarValue::Bool(value))) => (!value).to_string(),
            (ValueKind::Integer { min, max, step }, Some(CvarValue::Integer(value))) => (*value
                + i64::from(direction) * step)
                .clamp(min, max)
                .to_string(),
            (ValueKind::Float { .. }, Some(CvarValue::Float(value))) => {
                match setting.kind.stepped(*value, direction) {
                    Some(next) => next,
                    None => return,
                }
            }
            (ValueKind::Choice(values), Some(CvarValue::Text(value))) => {
                let index = values
                    .iter()
                    .position(|candidate| *candidate == value)
                    .unwrap_or(0);
                values[(index as i32 + direction).rem_euclid(values.len() as i32) as usize]
                    .to_owned()
            }
            _ => return,
        };
        console.set_cvar(setting.cvar, &next);
        self.refresh(console);
    }

    /// Start typing Text row `row`, keeping an edit already open on it.
    fn begin_text(&mut self, console: &ViewerConsole, row: usize) {
        if self.editing.as_ref().is_some_and(|draft| draft.row == row) {
            return;
        }
        let Some(setting) = settings(self.tab).get(row) else {
            return;
        };
        self.numeric = None;
        self.selected = row;
        self.editing = Some(TextDraft {
            row,
            text: value_text(console, setting.cvar),
        });
    }

    /// Whether a typed draft (text or number) is open. While one is, the pointer
    /// does not move the selection away from it.
    fn drafting(&self) -> bool {
        self.editing.is_some() || self.numeric.is_some()
    }

    /// A press on anything but the text draft's own row discards the draft, as
    /// clicking another control discards a numeric one.
    fn press_elsewhere(&mut self, row: Option<usize>) {
        if self
            .editing
            .as_ref()
            .is_some_and(|draft| Some(draft.row) != row)
        {
            self.editing = None;
        }
    }

    /// Hover selects `row` unless a draft is open.
    fn hover_row(&mut self, row: usize) {
        if !self.drafting() {
            self.selected = row;
        }
    }

    /// The wheel moves the selection by `direction` unless a draft is open.
    fn wheel(&mut self, direction: i32) {
        let count = settings(self.tab).len() + usize::from(self.tab == KEYBINDS_TAB);
        if direction != 0 && count > 0 && !self.drafting() {
            self.selected = (self.selected as i32 + direction)
                .clamp(0, count.saturating_sub(1) as i32) as usize;
        }
    }

    fn refresh(&mut self, console: &ViewerConsole) {
        self.values.clear();
        self.values.extend(
            settings(self.tab)
                .iter()
                .map(|setting| value_text(console, setting.cvar)),
        );
    }
}

fn settings(tab: usize) -> &'static [Setting] {
    match tab {
        0 => VIDEO,
        1 => AUDIO,
        2 => HUD,
        3 => CONTROLS,
        4 => GAME,
        5 => NETWORK,
        6 => HUD_OPTIONS,
        _ => &[],
    }
}
fn value_text(console: &ViewerConsole, name: &str) -> String {
    console.cvar(name).map_or_else(
        || "?".to_owned(),
        |value| match value {
            CvarValue::Bool(v) => {
                if *v {
                    "ON".to_owned()
                } else {
                    "OFF".to_owned()
                }
            }
            _ => value.as_text(),
        },
    )
}
