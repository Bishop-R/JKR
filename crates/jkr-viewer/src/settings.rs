//! Retained tabbed settings UI backed directly by archived shell cvars.

use crate::console::ViewerConsole;
use crate::menu_widgets::MenuCanvas;
use crate::text::{TextVertex, UiFont};
use jkr_shell::CvarValue;
use jkr_ui::{DrawList, Rect};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

mod catalog;
mod display;
mod pointer;
mod resolution;
mod resolution_list;
mod view;

pub(crate) use catalog::RESOLUTIONS;
use catalog::*;
pub(crate) use display::{
    DisplayMode, EXCLUSIVE_CVAR, MonitorModes, exclusive_supported, exclusive_video_mode,
};
use resolution::{PickResult, ResolutionChoice, ResolutionPicker};
pub(crate) enum SettingsResult {
    None,
    Back,
    OpenKeybinds,
}

pub(crate) struct SettingsMenu {
    tab: usize,
    selected: usize,
    values: Vec<String>,
    editing: Option<String>,
    /// What the window's monitor offers; asked for each time the screen opens.
    monitor: Option<MonitorModes>,
    /// The screen opened and wants fresh [`MonitorModes`].
    wants_monitor: bool,
    /// Scratch list of the resolutions on offer.
    choices: Vec<ResolutionChoice>,
    /// The resolution list, open over the form.
    picker: ResolutionPicker,
    ui: MenuCanvas,
}

impl SettingsMenu {
    pub(crate) fn new() -> Self {
        Self {
            tab: 0,
            selected: 0,
            values: Vec::with_capacity(12),
            editing: None,
            monitor: None,
            wants_monitor: false,
            choices: Vec::with_capacity(48),
            picker: ResolutionPicker::new(),
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
        self.picker.close();
        self.wants_monitor = true;
        self.refresh(console);
    }

    /// Whether the screen wants [`Self::set_monitor_modes`] (it just opened).
    pub(crate) fn wants_monitor_modes(&self) -> bool {
        self.wants_monitor
    }

    /// Take the window's monitor facts, which shape the resolution and
    /// display-mode choices.
    pub(crate) fn set_monitor_modes(&mut self, modes: MonitorModes, console: &ViewerConsole) {
        self.wants_monitor = false;
        self.monitor = Some(modes);
        if self.picker.is_open() {
            self.build_choices(console);
            self.picker.update_choices(&self.choices);
        }
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
        if self.picker.is_open() {
            self.resolution_key(key, event.repeat, console);
            return SettingsResult::None;
        }
        if let Some(buffer) = &mut self.editing {
            match key {
                KeyCode::Escape => self.editing = None,
                KeyCode::Enter | KeyCode::NumpadEnter => {
                    let value = self.editing.take().unwrap_or_default();
                    if let Some(setting) = settings(self.tab).get(self.selected) {
                        console.set_cvar(setting.cvar, value.trim());
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
                        self.editing = Some(value_text(console, setting.cvar));
                    } else if matches!(setting.kind, ValueKind::Resolution) {
                        self.open_resolutions(console);
                    } else {
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
            (ValueKind::Bool, Some(CvarValue::Integer(value))) => {
                if *value != 0 { "0" } else { "1" }.to_owned()
            }
            (ValueKind::Integer { min, max, step }, Some(CvarValue::Integer(value))) => (*value
                + i64::from(direction) * step)
                .clamp(min, max)
                .to_string(),
            (ValueKind::Float { min, max, step }, Some(CvarValue::Float(value))) => {
                ((*value + f64::from(direction) * step).clamp(min, max)).to_string()
            }
            (ValueKind::Choice(values), Some(CvarValue::Text(value))) => {
                let index = values
                    .iter()
                    .position(|candidate| *candidate == value)
                    .unwrap_or(0);
                values[(index as i32 + direction).rem_euclid(values.len() as i32) as usize]
                    .to_owned()
            }
            (ValueKind::Resolution, _) => {
                self.step_resolution(console, direction);
                return;
            }
            (ValueKind::DisplayMode, _) => {
                DisplayMode::requested(console)
                    .step(direction, self.exclusive_available())
                    .store(console);
                self.refresh(console);
                return;
            }
            _ => return,
        };
        console.set_cvar(setting.cvar, &next);
        self.refresh(console);
    }

    /// Whether exclusive fullscreen can be offered; assumed until the
    /// monitor facts arrive, since the window falls back to borderless.
    fn exclusive_available(&self) -> bool {
        self.monitor
            .as_ref()
            .is_none_or(|monitor| monitor.exclusive)
    }

    fn refresh(&mut self, console: &ViewerConsole) {
        let display = DisplayMode::requested(console).effective(self.exclusive_available());
        self.values.clear();
        self.values
            .extend(settings(self.tab).iter().map(|setting| match setting.kind {
                ValueKind::DisplayMode => display.label().to_owned(),
                ValueKind::Bool => toggle_text(console, setting.cvar),
                _ => value_text(console, setting.cvar),
            }));
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
/// ON/OFF for a toggle row; an integer cvar is on when nonzero.
fn toggle_text(console: &ViewerConsole, name: &str) -> String {
    match console.cvar(name) {
        Some(CvarValue::Integer(value)) => if *value != 0 { "ON" } else { "OFF" }.to_owned(),
        _ => value_text(console, name),
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
