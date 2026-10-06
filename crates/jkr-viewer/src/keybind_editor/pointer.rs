//! Pointer interaction for the visual key-binding form.

use super::*;
use jkr_ui::{InputEvent, UiEventKind};

impl KeybindEditor {
    pub(crate) fn handle_pointer(
        &mut self,
        event: InputEvent,
        console: &mut ViewerConsole,
    ) -> EditorResult {
        let Some(event) = self.ui.pointer(event) else {
            return EditorResult::None;
        };
        let Some(token) = event.token else {
            return EditorResult::None;
        };
        let row = usize::from(token);
        if self.rows().contains(&row)
            && !self.capture
            && matches!(event.kind, UiEventKind::HoverEnter | UiEventKind::Hover)
        {
            self.selected = row;
        }
        if event.kind == UiEventKind::Wheel {
            let direction = event.delta.map_or(0, |delta| -delta.y.signum() as i32);
            self.scroll_by(direction * WHEEL_ROWS as i32);
            return EditorResult::None;
        }
        if event.kind == UiEventKind::Drag && token == SCROLLBAR_TOKEN {
            if let (Some(point), Some(track)) = (event.position, self.ui.rect_for(SCROLLBAR_TOKEN))
            {
                self.scroll_to_ratio((point.y - track.y) / track.height);
            }
            return EditorResult::None;
        }
        if event.kind != UiEventKind::Activate {
            return EditorResult::None;
        }
        match token {
            BACK_TOKEN => EditorResult::Back,
            RESET_TOKEN => {
                console.reset_default_binds();
                self.refresh(console);
                EditorResult::None
            }
            UNBIND_TOKEN => {
                self.clear_selected_slot(console);
                EditorResult::None
            }
            TAB_BASE.. if usize::from(token - TAB_BASE) < CATEGORIES.len() => {
                self.set_tab(usize::from(token - TAB_BASE));
                EditorResult::None
            }
            _ if self.rows().contains(&row) => {
                self.selected = row;
                self.binding_slot = 0;
                self.begin_capture();
                EditorResult::None
            }
            _ if token >= SECONDARY_BASE
                && self.rows().contains(&usize::from(token - SECONDARY_BASE)) =>
            {
                self.selected = usize::from(token - SECONDARY_BASE);
                self.binding_slot = 1;
                self.begin_capture();
                EditorResult::None
            }
            _ => EditorResult::None,
        }
    }
}
