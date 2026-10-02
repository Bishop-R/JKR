//! Pointer-only console scrolling; command behavior remains keyboard driven.

use super::ViewerConsole;
use jkr_ui::InputEvent;

impl ViewerConsole {
    pub(crate) fn handle_pointer(&mut self, event: InputEvent) {
        let Some(delta) = self.presentation.pointer(event) else {
            self.selection.pointer(event, self.shift);
            return;
        };
        let line_count = self
            .shell
            .lines()
            .map(|line| line.text.split('\n').count())
            .sum();
        if delta > 0.0 {
            self.scroll_offset = self.scroll_offset.saturating_add(3).min(line_count);
        } else if delta < 0.0 {
            self.scroll_offset = self.scroll_offset.saturating_sub(3);
        }
    }
}
