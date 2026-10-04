//! Direct entry shares the same clamping and preview path as dragging.
use super::*;
use crate::menu_widgets::numeric::{EditResult, NumericEdit, VALUE_BASE};
use winit::keyboard::KeyCode;

impl Panel {
    pub(super) fn begin_numeric(&mut self, row: usize) {
        let Some(&(_, min, max, _)) = SLIDERS.get(row) else {
            return;
        };
        if self.sun_tab && !self.sun_available && row != 6 {
            return;
        }
        if self.numeric.as_ref().is_some_and(|edit| edit.row == row) {
            return;
        }
        self.selected = VALUE_BASE + row as u16;
        self.numeric = Some(NumericEdit::new(
            row,
            self.values[row].to_string(),
            f64::from(min),
            f64::from(max),
            false,
        ));
    }

    pub(super) fn edit_numeric(&mut self, key: KeyCode, text: Option<&str>) -> Option<Action> {
        let edit = self.numeric.as_mut()?;
        let row = edit.row;
        match edit.key(key, text) {
            EditResult::Pending => None,
            EditResult::Cancel => {
                self.numeric = None;
                None
            }
            EditResult::Commit(value) => {
                self.numeric = None;
                self.set_value(row, value as f32)
            }
        }
    }
}
