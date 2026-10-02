//! Typed entry for the saber page's RGB channel sliders: Enter, a typed
//! digit or a click on the value column opens it, and the committed number
//! is clamped to 0..=255.

use super::*;
use crate::menu_widgets::{EntryKey, NumberFormat};
use winit::keyboard::KeyCode;

/// Channels are whole numbers from 0 to 255.
const CHANNEL_FORMAT: NumberFormat = NumberFormat {
    fraction: false,
    negative: false,
};

impl PlayerMenu {
    /// Whether saber row `row` is an RGB channel slider on the shown page.
    pub(super) fn is_channel_row(&self, row: usize) -> bool {
        self.page == ProfilePage::Saber
            && self
                .saber_rows()
                .get(row)
                .is_some_and(|row| row.channel().is_some())
    }

    /// Open typed entry on channel row `row`, holding its value; false when
    /// the row is not a channel slider.
    pub(super) fn begin_channel_entry(&mut self, row: usize) -> bool {
        let Some(saber_row) = self.saber_rows().get(row).copied() else {
            return false;
        };
        let Some(channel) = saber_row
            .channel()
            .filter(|_| self.page == ProfilePage::Saber)
        else {
            return false;
        };
        let value = self.saber.channel(saber_row.second(), channel);
        self.channel_entry
            .begin(row, &value.to_string(), CHANNEL_FORMAT);
        true
    }

    /// Open entry on the selected channel row with typed `text` when it
    /// starts a number.
    pub(super) fn begin_typed_channel(&mut self, text: &str) -> bool {
        self.is_channel_row(self.selected)
            && self
                .channel_entry
                .begin_typed(self.selected, text, CHANNEL_FORMAT)
    }

    /// Feed one key to the open entry on `row`, applying a committed number.
    pub(super) fn channel_entry_key(
        &mut self,
        row: usize,
        key: KeyCode,
        text: Option<&str>,
        repeat: bool,
        console: &mut ViewerConsole,
    ) {
        if let EntryKey::Committed(Some(value)) = self.channel_entry.key(key, text, repeat) {
            self.set_typed_channel(row, value, console);
        }
    }

    /// Apply whatever is being typed, as Enter would; for a click elsewhere.
    pub(super) fn commit_channel_entry(&mut self, console: &mut ViewerConsole) {
        if let Some(row) = self.channel_entry.row() {
            if let Some(value) = self.channel_entry.commit() {
                self.set_typed_channel(row, value, console);
            }
        }
    }

    fn set_typed_channel(&mut self, row: usize, value: f64, console: &mut ViewerConsole) {
        let Some(saber_row) = self.saber_rows().get(row).copied() else {
            return;
        };
        if let Some(channel) = saber_row.channel() {
            let value = value.round().clamp(0.0, 255.0) as u8;
            self.saber.set_channel(saber_row.second(), channel, value);
            self.saber.apply(console);
        }
    }
}
