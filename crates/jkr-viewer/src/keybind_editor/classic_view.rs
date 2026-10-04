//! The key bindings as a classic option panel (`controls.menu` bind items):
//! the action's label, then its keys as retail's `BindingFromName` writes
//! them ("A or B", one key, or "???"); the action being rebound turns red
//! and the description line asks for the new key.

use super::*;
use crate::menu::classic::layout::Span;
use crate::menu::classic::panel::{BINDING, OPTION, PanelFrame, focus_text};
use crate::menu::classic::view::Caps;

/// The rows' wheel target, under them, while the list scrolls; clear of the
/// action rows, the secondary slots (600), the chrome (800) and the footer
/// and scrollbar tokens (902-910).
const ROWS_SCROLL_TOKEN: u16 = 911;

/// Retail `WAITING_FOR_NEW_KEY` (`mp_ingame.str`).
const WAITING: &str = "Enter new key, or ESC to cancel, BACKSPACE to clear.";

impl KeybindEditor {
    /// Show rows `span` of key-binding category `category` as a classic
    /// option panel.
    pub(crate) fn open_classic(&mut self, console: &ViewerConsole, category: usize, span: Span) {
        let tab = category.min(CATEGORIES.len() - 1);
        self.set_tab(tab);
        let all = category_range(tab);
        let rows = span.within(all.start, all.len());
        self.selected = rows.start;
        self.classic = Some(rows);
        self.binding_slot = 0;
        self.refresh(console);
    }

    /// Back to the full tabbed editor on the same category.
    pub(crate) fn leave_classic(&mut self) {
        if self.classic.take().is_some() {
            self.set_tab(self.tab);
        }
    }

    /// Draw the classic panel screen around the open span's bindings.
    pub(crate) fn append_classic(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        reveal: f32,
        frame: &PanelFrame,
    ) {
        let rows = self.rows();
        let place = frame.begin(&mut self.ui, viewport, reveal);
        self.visible = place.capacity();
        self.first = self.first.min(rows.len().saturating_sub(self.visible));
        let shown = rows.start + self.first..rows.end.min(rows.start + self.first + self.visible);
        // A list longer than the panel scrolls with the wheel anywhere over
        // its rows, not only over the thin bar.
        if rows.len() > self.visible {
            let top = place.row(0);
            self.ui.scroll_region(
                ROWS_SCROLL_TOKEN,
                Rect::new(top.x, top.y, top.width, self.visible as f32 * top.height),
            );
        }
        if shown.contains(&self.selected) {
            place.highlight(&mut self.ui, self.selected - shown.start);
        }
        for (slot, action) in shown.clone().enumerate() {
            let focused = action == self.selected;
            let color = if focused { focus_text() } else { OPTION };
            place.label(&mut self.ui, slot, ACTIONS[action].label, color);
            self.ui.hit_region(action as u16, place.row(slot));
            let value_color = if focused && self.capture {
                BINDING
            } else {
                color
            };
            match self.keys.get(action) {
                Some([first, _]) if first == "UNBOUND" => {
                    place.value(&mut self.ui, slot, "???", value_color);
                }
                Some([first, second]) if second == "-" => {
                    place.value(&mut self.ui, slot, first, value_color);
                }
                // Retail writes "A or B" and raises the whole line to capitals.
                Some([first, second]) => place.value_fmt(
                    &mut self.ui,
                    slot,
                    format_args!("{} OR {}", Caps(first), Caps(second)),
                    value_color,
                ),
                None => place.value(&mut self.ui, slot, "???", value_color),
            }
        }
        if rows.len() > self.visible {
            let first = place.row(0);
            let s = place.scale();
            self.ui.scrollbar(
                SCROLLBAR_TOKEN,
                Rect::new(
                    first.right() - 6.0 * s,
                    first.y,
                    3.0 * s,
                    self.visible as f32 * first.height,
                ),
                self.first,
                self.visible,
                rows.len(),
            );
        }
        place.finish(&mut self.ui, self.capture.then_some(WAITING));
        self.ui.finish(self.selected as u16);
        self.ui.append_text(vertices, font, viewport);
    }
}
