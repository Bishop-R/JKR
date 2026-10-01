//! Pointer interaction for the console browser.

use super::{Browser, BrowserAction, TABS};
use crate::menu_widgets::{BACK_TOKEN, TAB_BASE};
use jkr_ui::{InputEvent, UiEventKind};

/// Row `slot` on screen answers to `ROW_BASE + slot`.
pub(super) const ROW_BASE: u16 = 1_000;
/// The search field.
pub(super) const SEARCH_TOKEN: u16 = 902;
/// Footer cap that restores the selected cvar's default.
pub(super) const RESET_TOKEN: u16 = 903;
/// Footer cap that edits the selected cvar or inserts the selected command.
pub(super) const ACTIVATE_TOKEN: u16 = 904;
/// Draggable thumb beside the rows.
pub(super) const SCROLLBAR_TOKEN: u16 = 910;
/// Rows one wheel notch scrolls.
const WHEEL_ROWS: isize = 3;

impl Browser {
    pub(crate) fn handle_pointer(&mut self, event: InputEvent) -> BrowserAction {
        let Some(event) = self.ui.pointer(event) else {
            return BrowserAction::None;
        };
        if event.kind == UiEventKind::Wheel {
            let direction = event.delta.map_or(0, |delta| -delta.y.signum() as isize);
            self.scroll_by(direction * WHEEL_ROWS);
            return BrowserAction::None;
        }
        let Some(token) = event.token else {
            return BrowserAction::None;
        };
        let row = (token >= ROW_BASE)
            .then(|| self.first + usize::from(token - ROW_BASE))
            .filter(|&row| row < self.visible.len());
        if let Some(row) = row
            && self.editing.is_none()
            && matches!(event.kind, UiEventKind::HoverEnter | UiEventKind::Hover)
        {
            self.selected = row;
        }
        if event.kind == UiEventKind::Drag && token == SCROLLBAR_TOKEN {
            if let (Some(point), Some(track)) = (event.position, self.ui.rect_for(SCROLLBAR_TOKEN))
            {
                self.scroll_to_ratio((point.y - track.y) / track.height);
            }
            return BrowserAction::None;
        }
        if event.kind != UiEventKind::Activate {
            return BrowserAction::None;
        }
        match token {
            BACK_TOKEN => BrowserAction::Close,
            SEARCH_TOKEN => {
                self.editing = None;
                BrowserAction::None
            }
            RESET_TOKEN => self.reset_selected(),
            ACTIVATE_TOKEN => self.activate(),
            TAB_BASE.. if usize::from(token - TAB_BASE) < TABS.len() => {
                self.set_tab(usize::from(token - TAB_BASE));
                BrowserAction::None
            }
            _ => match row {
                Some(row) if self.editing.is_none() || row != self.selected => {
                    self.selected = row;
                    self.editing = None;
                    self.activate()
                }
                _ => BrowserAction::None,
            },
        }
    }
}
