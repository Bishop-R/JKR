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

/// Which set of tabs the form shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Section {
    /// The general settings ([`TABS`]).
    General,
    /// JKR's renderer settings ([`RENDERER_TABS`]).
    Renderer,
}

/// A row after a tab's settings that opens another screen.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    Keybinds,
    Renderer,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Self::Keybinds => "Key bindings",
            Self::Renderer => "Renderer",
        }
    }
}

pub(crate) struct SettingsMenu {
    section: Section,
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
            section: Section::General,
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
        self.section = Section::General;
        self.tab = tab.min(TABS.len() - 1);
        self.selected = 0;
        self.editing = None;
        self.numeric = None;
        self.refresh(console);
    }

    fn enter_renderer(&mut self, console: &ViewerConsole) {
        self.section = Section::Renderer;
        self.tab = 0;
        self.selected = 0;
        self.editing = None;
        self.numeric = None;
        self.refresh(console);
    }

    /// Back out of the form: the renderer section returns to the Video tab's
    /// Renderer row, as JoF EJK's advanced renderer page returns to Video.
    fn back(&mut self, console: &ViewerConsole) -> SettingsResult {
        if self.section == Section::Renderer {
            self.section = Section::General;
            self.tab = RENDERER_TAB;
            self.selected = self.rows().len();
            self.editing = None;
            self.numeric = None;
            self.refresh(console);
            return SettingsResult::None;
        }
        SettingsResult::Back
    }

    /// Tab names of the current section.
    fn tabs(&self) -> &'static [&'static str] {
        match self.section {
            Section::General => &TABS,
            Section::Renderer => &RENDERER_TABS,
        }
    }

    /// Settings of the current tab.
    fn rows(&self) -> &'static [Setting] {
        settings(self.section, self.tab)
    }

    /// The row after the settings that opens another screen, if this tab has one.
    fn action(&self) -> Option<Action> {
        match (self.section, self.tab) {
            (Section::General, KEYBINDS_TAB) => Some(Action::Keybinds),
            (Section::General, RENDERER_TAB) => Some(Action::Renderer),
            _ => None,
        }
    }

    /// Selectable rows: the settings plus the action row.
    fn row_count(&self) -> usize {
        self.rows().len() + usize::from(self.action().is_some())
    }

    /// Activate the action row.
    fn activate_action(&mut self, console: &ViewerConsole) -> SettingsResult {
        match self.action() {
            Some(Action::Keybinds) => SettingsResult::OpenKeybinds,
            Some(Action::Renderer) => {
                self.enter_renderer(console);
                SettingsResult::None
            }
            None => SettingsResult::None,
        }
    }

    /// Switch to tab `tab` of the current section.
    fn select_tab(&mut self, console: &ViewerConsole, tab: usize) {
        self.tab = tab % self.tabs().len();
        self.selected = 0;
        self.editing = None;
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
                        && let Some(setting) = self.rows().get(draft.row)
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
        let count = self.row_count();
        match key {
            KeyCode::Tab | KeyCode::BracketRight => self.select_tab(console, self.tab + 1),
            KeyCode::BracketLeft => {
                let tab = self.tab.checked_sub(1).unwrap_or(self.tabs().len() - 1);
                self.select_tab(console, tab);
            }
            KeyCode::ArrowUp | KeyCode::KeyW => {
                self.selected = self.selected.checked_sub(1).unwrap_or(count - 1)
            }
            KeyCode::ArrowDown | KeyCode::KeyS => self.selected = (self.selected + 1) % count,
            KeyCode::ArrowLeft | KeyCode::KeyA => self.adjust(console, -1),
            KeyCode::ArrowRight | KeyCode::KeyD => self.adjust(console, 1),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space
                if self.action().is_some() && self.selected == self.rows().len() =>
            {
                return self.activate_action(console);
            }
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                if let Some(setting) = self.rows().get(self.selected) {
                    if matches!(setting.kind, ValueKind::Text) {
                        self.begin_text(console, self.selected);
                    } else if !self.begin_numeric(console, self.selected) {
                        self.adjust(console, 1);
                    }
                }
            }
            KeyCode::Escape => return self.back(console),
            _ => {}
        }
        SettingsResult::None
    }

    fn adjust(&mut self, console: &mut ViewerConsole, direction: i32) {
        let Some(setting) = self.rows().get(self.selected) else {
            return;
        };
        let next = match (setting.kind, console.cvar(setting.cvar)) {
            (ValueKind::Bool, Some(CvarValue::Bool(value))) => (!value).to_string(),
            (ValueKind::Bool, Some(value)) => if switch_on(value) { "0" } else { "1" }.to_owned(),
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
            (ValueKind::Choice(values), Some(value)) => {
                // Choices name text or integer cvars (`cg_saberTrail` is an integer).
                let value = value.as_text();
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
        let Some(setting) = self.rows().get(row) else {
            return;
        };
        self.numeric = None;
        self.selected = row;
        self.editing = Some(TextDraft {
            row,
            text: value_text(console, setting),
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
        let count = self.row_count();
        if direction != 0 && count > 0 && !self.drafting() {
            self.selected = (self.selected as i32 + direction)
                .clamp(0, count.saturating_sub(1) as i32) as usize;
        }
    }

    fn refresh(&mut self, console: &ViewerConsole) {
        self.values.clear();
        self.values.extend(
            self.rows()
                .iter()
                .map(|setting| value_text(console, setting)),
        );
    }
}

fn settings(section: Section, tab: usize) -> &'static [Setting] {
    match (section, tab) {
        (Section::General, 0) => VIDEO,
        (Section::General, 1) => AUDIO,
        (Section::General, 2) => HUD,
        (Section::General, 3) => CONTROLS,
        (Section::General, 4) => GAME,
        (Section::General, 5) => NETWORK,
        (Section::General, 6) => HUD_OPTIONS,
        (Section::Renderer, 0) => RENDER_IMAGE,
        (Section::Renderer, 1) => RENDER_LIGHTING,
        (Section::Renderer, 2) => RENDER_SHADOWS,
        _ => &[],
    }
}

/// Whether a switch row's cvar is on: true, or any nonzero number.
fn switch_on(value: &CvarValue) -> bool {
    match value {
        CvarValue::Bool(value) => *value,
        CvarValue::Integer(value) => *value != 0,
        CvarValue::Float(value) => *value != 0.0,
        CvarValue::Text(value) => value.trim().parse::<f64>().is_ok_and(|value| value != 0.0),
    }
}

fn value_text(console: &ViewerConsole, setting: &Setting) -> String {
    console.cvar(setting.cvar).map_or_else(
        || "?".to_owned(),
        |value| match (setting.kind, value) {
            (ValueKind::Bool, value) | (_, value @ CvarValue::Bool(_)) => {
                if switch_on(value) {
                    "ON".to_owned()
                } else {
                    "OFF".to_owned()
                }
            }
            (_, value) => value.as_text(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECTIONS: [(Section, usize); 2] = [
        (Section::General, TABS.len()),
        (Section::Renderer, RENDERER_TABS.len()),
    ];

    fn console() -> (tempfile::TempDir, ViewerConsole) {
        let directory = tempfile::tempdir().unwrap();
        let console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        (directory, console)
    }

    fn rows() -> impl Iterator<Item = &'static Setting> {
        SECTIONS
            .into_iter()
            .flat_map(|(section, tabs)| (0..tabs).flat_map(move |tab| settings(section, tab)))
    }

    #[test]
    fn every_row_names_a_registered_cvar_of_its_kind() {
        let (_directory, console) = console();
        for setting in rows() {
            let value = console
                .cvar(setting.cvar)
                .unwrap_or_else(|| panic!("{} is not registered", setting.cvar));
            let fits = match (setting.kind, value) {
                (
                    ValueKind::Bool,
                    CvarValue::Bool(_) | CvarValue::Integer(_) | CvarValue::Float(_),
                ) => true,
                (ValueKind::Integer { min, max, .. }, CvarValue::Integer(value)) => {
                    (min..=max).contains(value)
                }
                (ValueKind::Float { min, max, .. }, CvarValue::Float(value)) => {
                    (min..=max).contains(value)
                }
                // Choices cycle the cvar's text form, whatever its type.
                (ValueKind::Choice(values), value) => values.contains(&value.as_text().as_str()),
                (ValueKind::Text, CvarValue::Text(_)) => true,
                _ => false,
            };
            assert!(
                fits,
                "{} default {value:?} does not fit its row",
                setting.cvar
            );
        }
    }

    #[test]
    fn the_renderer_tabs_hold_every_jkr_rendering_cvar() {
        // Bookkeeping, diagnostics and the ground HUD (on the HUD tab) stay off it.
        const NOT_RENDERER: [&str; 4] = [
            "jkr_bindDefaultsVersion",
            "jkr_sensitivityScaleVersion",
            "jkr_dayDebug",
            "jkr_groundHud",
        ];
        let (_directory, console) = console();
        let renderer: Vec<_> = (0..RENDERER_TABS.len())
            .flat_map(|tab| settings(Section::Renderer, tab))
            .map(|setting| setting.cvar)
            .collect();
        for name in console.cvar_names().filter(|name| name.starts_with("jkr_")) {
            if !NOT_RENDERER.contains(&name) {
                assert!(
                    renderer.contains(&name),
                    "{name} is missing from the renderer tabs"
                );
            }
        }
        for tab in 0..TABS.len() {
            for setting in settings(Section::General, tab) {
                assert!(
                    !renderer.contains(&setting.cvar),
                    "{} is on both pages",
                    setting.cvar
                );
            }
        }
    }

    #[test]
    fn the_video_row_opens_the_renderer_and_back_returns_to_it() {
        let (_directory, console) = console();
        let mut menu = SettingsMenu::new();
        menu.open_tab(&console, RENDERER_TAB);
        assert_eq!(menu.action(), Some(Action::Renderer));
        assert_eq!(menu.row_count(), VIDEO.len() + 1);
        menu.selected = VIDEO.len();
        assert!(matches!(
            menu.activate_action(&console),
            SettingsResult::None
        ));
        assert_eq!(
            (menu.section, menu.tab, menu.selected),
            (Section::Renderer, 0, 0)
        );
        assert_eq!(menu.tabs(), &RENDERER_TABS);
        assert_eq!(menu.values.len(), RENDER_IMAGE.len());
        menu.select_tab(&console, 3);
        assert_eq!(menu.tab, 0, "renderer tabs wrap within their own section");
        assert!(matches!(menu.back(&console), SettingsResult::None));
        assert_eq!(
            (menu.section, menu.tab, menu.selected),
            (Section::General, RENDERER_TAB, VIDEO.len())
        );
        assert!(matches!(menu.back(&console), SettingsResult::Back));
    }

    #[test]
    fn switch_rows_toggle_integer_and_float_cvars() {
        let (_directory, mut console) = console();
        let mut menu = SettingsMenu::new();
        menu.open_tab(&console, RENDERER_TAB);
        menu.enter_renderer(&console);
        for (tab, cvar) in [(0, "jkr_bloom"), (2, "jkr_contactShadows")] {
            menu.select_tab(&console, tab);
            menu.selected = menu
                .rows()
                .iter()
                .position(|setting| setting.cvar == cvar)
                .unwrap();
            let before = switch_on(console.cvar(cvar).unwrap());
            menu.adjust(&mut console, 1);
            assert_eq!(switch_on(console.cvar(cvar).unwrap()), !before, "{cvar}");
            assert_eq!(
                menu.values[menu.selected],
                if before { "OFF" } else { "ON" }
            );
            menu.adjust(&mut console, 1);
            assert_eq!(switch_on(console.cvar(cvar).unwrap()), before, "{cvar}");
        }
    }
}
