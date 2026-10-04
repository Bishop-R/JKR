//! Direct entry for either saber's RGB sliders.
use super::*;
use crate::menu_widgets::numeric::{EditResult, NumericEdit};
use winit::keyboard::KeyCode;

impl PlayerMenu {
    pub(super) fn begin_numeric(&mut self, index: usize) -> bool {
        if self.page != ProfilePage::Saber {
            return false;
        }
        let Some(row) = self.saber_rows().get(index).copied() else {
            return false;
        };
        let Some(channel) = row.channel() else {
            return false;
        };
        self.selected = index;
        self.numeric = Some(NumericEdit::new(
            index,
            self.saber.channel(row.second(), channel).to_string(),
            0.0,
            255.0,
            true,
        ));
        true
    }

    pub(super) fn edit_numeric(
        &mut self,
        key: KeyCode,
        text: Option<&str>,
        console: &mut ViewerConsole,
    ) -> bool {
        let Some(edit) = &mut self.numeric else {
            return false;
        };
        let index = edit.row;
        match edit.key(key, text) {
            EditResult::Pending => {}
            EditResult::Cancel => self.numeric = None,
            EditResult::Commit(value) => {
                if let Some(row) = self.saber_rows().get(index).copied()
                    && let Some(channel) = row.channel()
                {
                    self.saber.set_channel(row.second(), channel, value as u8);
                    self.saber.apply(console);
                }
                self.numeric = None;
            }
        }
        true
    }
}
