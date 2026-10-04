//! Visual action-to-key editor and stock multiplayer bindings.

use super::{TextVertex, UiFont};
use crate::console::ViewerConsole;
use crate::menu_widgets::{BACK_TOKEN, FormLayout, MenuCanvas, Scrim, TAB_BASE};
mod catalog;
mod pointer;

pub(crate) use catalog::{ACTIONS, default_bindings, migrate_missing_defaults};
use catalog::{CATEGORIES, category_range};
use jkr_ui::{DrawList, Rect};
use std::ops::Range;
use winit::event::{ElementState, KeyEvent, MouseButton};
use winit::keyboard::{KeyCode, PhysicalKey};

/// Footer cap that restores the stock bindings.
const RESET_TOKEN: u16 = 903;
/// Footer cap that unbinds the selected action.
const UNBIND_TOKEN: u16 = 902;
/// Draggable thumb beside the row column.
const SCROLLBAR_TOKEN: u16 = 910;
/// Rows one wheel notch scrolls.
const WHEEL_ROWS: usize = 3;
/// Secondary-slot hit targets, separate from the action row.
const SECONDARY_BASE: u16 = 600;
/// Keys the editor never binds, clears or moves: the menu's own pointer
/// buttons and its cancel key. A stray click or Escape while a capture is
/// pending must not take `+attack` off the mouse or bind the menu key; the
/// console's `bind` and `unbind` still change them.
const LOCKED_KEYS: [&str; 3] = ["MOUSE1", "MOUSE2", "ESCAPE"];

/// Whether `key` is one of the [`LOCKED_KEYS`] (any case, as binds match).
pub(crate) fn is_locked_key(key: &str) -> bool {
    LOCKED_KEYS
        .iter()
        .any(|locked| locked.eq_ignore_ascii_case(key))
}

/// Outcome of one key or pointer event on the editor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EditorResult {
    None,
    Back,
}

pub(crate) struct KeybindEditor {
    tab: usize,
    /// Index into `ACTIONS`; always inside the current tab's range.
    selected: usize,
    /// First row of the current tab that is on screen.
    first: usize,
    /// Rows that fit, as measured by the last `append`.
    visible: usize,
    capture: bool,
    binding_slot: usize,
    keys: Vec<[String; 2]>,
    ui: MenuCanvas,
}

impl KeybindEditor {
    pub(crate) fn new() -> Self {
        Self {
            tab: 0,
            selected: 0,
            first: 0,
            visible: 1,
            capture: false,
            binding_slot: 0,
            keys: Vec::with_capacity(ACTIONS.len()),
            ui: MenuCanvas::new(),
        }
    }

    pub(crate) fn open(&mut self, console: &ViewerConsole) {
        self.capture = false;
        self.refresh(console);
    }

    pub(crate) fn visual_selection(&self) -> usize {
        self.selected
    }

    pub(crate) fn draw_list(&self) -> &DrawList {
        self.ui.draw_list()
    }

    pub(crate) fn handle_key(
        &mut self,
        event: &KeyEvent,
        console: &mut ViewerConsole,
    ) -> EditorResult {
        if event.state != ElementState::Pressed || event.repeat {
            return EditorResult::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return EditorResult::None;
        };
        if self.capture {
            if key == KeyCode::Escape {
                self.capture = false;
                return EditorResult::None;
            }
            let Some(key) = crate::input::keys::name(crate::input::keys::Source::Key(key)) else {
                return EditorResult::None;
            };
            console.rebind_action(ACTIONS[self.selected].command, self.binding_slot, key);
            self.capture = false;
            self.refresh(console);
            return EditorResult::None;
        }
        match key {
            KeyCode::ArrowUp | KeyCode::KeyW => self.move_selection(-1),
            KeyCode::ArrowDown | KeyCode::KeyS => self.move_selection(1),
            KeyCode::Tab | KeyCode::ArrowRight | KeyCode::KeyD => self.cycle_tab(1),
            KeyCode::ArrowLeft | KeyCode::KeyA => self.cycle_tab(-1),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => self.begin_capture(),
            KeyCode::Delete | KeyCode::Backspace => self.clear_selected_slot(console),
            KeyCode::KeyR => {
                console.reset_default_binds();
                self.refresh(console);
            }
            KeyCode::Escape => return EditorResult::Back,
            _ => {}
        }
        EditorResult::None
    }

    pub(crate) fn capture_mouse(
        &mut self,
        button: MouseButton,
        console: &mut ViewerConsole,
    ) -> bool {
        if !self.capture {
            return false;
        }
        let Some(key) = crate::input::keys::name(crate::input::keys::Source::Mouse(button)) else {
            return true;
        };
        // A click on a locked button is the player clicking away, not a key to
        // bind: it cancels, as Escape does, and is consumed so it activates
        // nothing under the pointer.
        if is_locked_key(key) {
            self.capture = false;
            return true;
        }
        console.rebind_action(ACTIONS[self.selected].command, self.binding_slot, key);
        self.capture = false;
        self.refresh(console);
        true
    }

    /// Whether the selected action's chosen slot holds a locked key.
    fn selected_slot_locked(&self) -> bool {
        self.keys
            .get(self.selected)
            .is_some_and(|keys| is_locked_key(&keys[self.binding_slot]))
    }

    /// Await a key for the selected slot, unless that slot is locked.
    fn begin_capture(&mut self) {
        self.capture = !self.selected_slot_locked();
    }

    /// Unbind the selected slot, unless it holds a locked key.
    fn clear_selected_slot(&mut self, console: &mut ViewerConsole) {
        if !self.selected_slot_locked() {
            console.clear_action(ACTIONS[self.selected].command, self.binding_slot);
            self.refresh(console);
        }
    }

    /// `ACTIONS` range of the current tab.
    fn rows(&self) -> Range<usize> {
        category_range(self.tab)
    }

    fn set_tab(&mut self, tab: usize) {
        self.tab = tab;
        self.selected = self.rows().start;
        self.first = 0;
        self.capture = false;
    }

    fn cycle_tab(&mut self, direction: i32) {
        let count = CATEGORIES.len() as i32;
        self.set_tab(((self.tab as i32 + direction).rem_euclid(count)) as usize);
    }

    /// Move the keyboard selection within the tab, wrapping, and scroll so
    /// it stays on screen.
    fn move_selection(&mut self, direction: i32) {
        let rows = self.rows();
        let count = rows.len() as i32;
        if count == 0 {
            return;
        }
        let offset = (self.selected - rows.start) as i32 + direction;
        let offset = offset.rem_euclid(count) as usize;
        self.selected = rows.start + offset;
        self.first = self
            .first
            .min(offset)
            .max(offset.saturating_sub(self.visible - 1));
    }

    /// Scroll the tab by `rows` (negative = up) without moving the selection.
    fn scroll_by(&mut self, rows: i32) {
        let max_first = self.rows().len().saturating_sub(self.visible);
        self.first = (self.first as i32 + rows).clamp(0, max_first as i32) as usize;
    }

    /// Scroll so the row column shows `ratio` (0 = top, 1 = bottom) of the tab.
    fn scroll_to_ratio(&mut self, ratio: f32) {
        let max_first = self.rows().len().saturating_sub(self.visible);
        self.first = (ratio.clamp(0.0, 1.0) * max_first as f32).round() as usize;
    }

    pub(crate) fn append(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        reveal: f32,
    ) {
        let layout = FormLayout::new(viewport);
        let s = layout.scale;
        self.ui.begin_hero(viewport, reveal, Scrim::Full);
        self.ui.form_header(
            &layout,
            "JKR   /   SETTINGS",
            "KEY BINDINGS",
            if self.capture {
                "Press a key or mouse button.  Escape or a click cancels; a conflicting bind moves here."
            } else if self.selected_slot_locked() {
                "MOUSE1, MOUSE2 and ESCAPE are locked here; the console's bind command can still change them."
            } else {
                "Click either key slot to bind it. Delete clears the selected slot."
            },
        );
        self.ui.form_tabs(&layout, &CATEGORIES, self.tab);
        let rows_bottom = viewport[1] - 80.0 * s;
        self.visible = (((rows_bottom - layout.rows_y) / layout.row_height)
            .floor()
            .max(1.0)) as usize;
        let rows = self.rows();
        self.first = self.first.min(rows.len().saturating_sub(self.visible));
        let shown = rows.start + self.first..rows.end.min(rows.start + self.first + self.visible);
        for (slot, action) in shown.enumerate() {
            self.row_view(&layout, action, slot);
        }
        if rows.len() > self.visible {
            let first_row = layout.row_rect(0);
            self.ui.scrollbar(
                SCROLLBAR_TOKEN,
                Rect::new(
                    first_row.right() + 14.0 * s,
                    first_row.y,
                    5.0 * s,
                    self.visible as f32 * layout.row_height,
                ),
                self.first,
                self.visible,
                rows.len(),
            );
        }
        self.ui.form_footer_actions(
            &layout,
            &[
                ("DEL", "Unbind", UNBIND_TOKEN),
                ("R", "Reset defaults", RESET_TOKEN),
                ("ESC", "Back", BACK_TOKEN),
            ],
        );
        self.ui.end_hero();
        self.ui.finish(self.selected as u16);
        self.ui.append_text(vertices, font, viewport);
    }

    /// One action row at slot `slot`: label left, secondary and primary
    /// keys right-aligned in the value zone; the capture prompt replaces the
    /// primary key while a press is awaited.
    fn row_view(&mut self, layout: &FormLayout, action: usize, slot: usize) {
        let s = layout.scale;
        let rect = layout.row_rect(slot);
        let selected = action == self.selected;
        self.ui.form_row_frame(rect, action as u16, selected, s);
        self.ui.form_label(rect, ACTIONS[action].label, selected, s);
        let zone = layout.value_zone(rect);
        let half = zone.width * 0.5;
        let primary = Rect::new(zone.x + half, zone.y, half, zone.height);
        let secondary = Rect::new(zone.x, zone.y, half - 12.0 * s, zone.height);
        self.ui
            .hit_region(SECONDARY_BASE + action as u16, secondary);
        let color = self.ui.form_value_color(selected);
        if self.capture && selected {
            let accent = self.ui.theme().accent;
            let target = if self.binding_slot == 0 {
                primary
            } else {
                secondary
            };
            self.ui.form_value("PRESS A KEY", target, accent, s);
            return;
        }
        let [first, second] = match self.keys.get(action) {
            Some(keys) => [keys[0].as_str(), keys[1].as_str()],
            None => ["UNBOUND", "-"],
        };
        // Locked keys read as fixed: muted, like the secondary slot.
        let muted = self.ui.theme().muted;
        let first_color = if is_locked_key(first) { muted } else { color };
        self.ui.form_value(first, primary, first_color, s);
        self.ui.form_value(second, secondary, muted, s);
    }

    fn refresh(&mut self, console: &ViewerConsole) {
        self.keys.clear();
        for action in ACTIONS {
            let keys = console.keys_for_command(action.command);
            self.keys.push([
                keys.first()
                    .cloned()
                    .unwrap_or_else(|| "UNBOUND".to_owned()),
                keys.get(1).cloned().unwrap_or_else(|| "-".to_owned()),
            ]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn console() -> (tempfile::TempDir, ViewerConsole) {
        let directory = tempfile::tempdir().unwrap();
        let console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        (directory, console)
    }

    fn action(command: &str) -> usize {
        ACTIONS
            .iter()
            .position(|action| action.command == command)
            .unwrap()
    }

    #[test]
    fn mouse_buttons_one_two_and_escape_are_locked_in_any_case() {
        for key in ["MOUSE1", "mouse2", "Escape"] {
            assert!(is_locked_key(key), "{key}");
        }
        for key in ["MOUSE3", "a", "SPACE", "ESC"] {
            assert!(!is_locked_key(key), "{key}");
        }
    }

    #[test]
    fn the_editor_never_unbinds_or_moves_a_locked_key() {
        let (_directory, mut console) = console();
        console.clear_action("+attack", 0);
        assert_eq!(console.keys_for_command("+attack"), ["MOUSE1"]);
        console.rebind_action("+use", 0, "MOUSE1");
        console.rebind_action("+attack", 0, "k");
        assert_eq!(console.keys_for_command("+attack"), ["MOUSE1"]);
        assert!(
            !console
                .keys_for_command("+use")
                .iter()
                .any(|key| key == "MOUSE1")
        );
    }

    #[test]
    fn a_locked_slot_does_not_start_a_capture() {
        let (_directory, console) = console();
        let mut editor = KeybindEditor::new();
        editor.open(&console);
        editor.selected = action("+altattack");
        editor.binding_slot = 0;
        editor.begin_capture();
        assert!(!editor.capture);
        // The empty second slot is free.
        editor.binding_slot = 1;
        editor.begin_capture();
        assert!(editor.capture);
    }

    #[test]
    fn a_click_cancels_a_capture_without_binding_it() {
        let (_directory, mut console) = console();
        let mut editor = KeybindEditor::new();
        editor.open(&console);
        let jump = action("+moveup");
        editor.selected = jump;
        editor.binding_slot = 0;
        editor.begin_capture();
        let before = console.keys_for_command("+moveup");
        for button in [MouseButton::Left, MouseButton::Right] {
            editor.begin_capture();
            assert!(editor.capture);
            assert!(editor.capture_mouse(button, &mut console));
            assert!(!editor.capture);
        }
        assert_eq!(console.keys_for_command("+moveup"), before);
        assert_eq!(console.keys_for_command("+attack"), ["MOUSE1"]);
        // Other mouse buttons still bind.
        editor.begin_capture();
        assert!(editor.capture_mouse(MouseButton::Middle, &mut console));
        assert!(
            console
                .keys_for_command("+moveup")
                .iter()
                .any(|key| key == "MOUSE3")
        );
    }
}
