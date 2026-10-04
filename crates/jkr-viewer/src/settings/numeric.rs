//! Direct numeric entry, separate from cvar navigation and text settings.
use super::*;
use crate::menu_widgets::numeric::{EditResult, NumericEdit};

impl SettingsMenu {
    pub(super) fn begin_numeric(&mut self, console: &ViewerConsole, row: usize) -> bool {
        let Some(setting) = settings(self.tab).get(row) else {
            return false;
        };
        let (min, max, integer) = match setting.kind {
            ValueKind::Integer { min, max, .. } => (min as f64, max as f64, true),
            ValueKind::Float { min, max, .. } => (min, max, false),
            _ => return false,
        };
        self.editing = None;
        self.selected = row;
        self.numeric = Some(NumericEdit::new(
            row,
            value_text(console, setting.cvar),
            min,
            max,
            integer,
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
        match edit.key(key, text) {
            EditResult::Pending => {}
            EditResult::Cancel => self.numeric = None,
            EditResult::Commit(value) => {
                let setting = &settings(self.tab)[edit.row];
                let value = if matches!(setting.kind, ValueKind::Integer { .. }) {
                    (value as i64).to_string()
                } else {
                    value.to_string()
                };
                console.set_cvar(setting.cvar, &value);
                self.numeric = None;
                self.refresh(console);
            }
        }
        true
    }
}
