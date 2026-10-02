//! Typed entry for form sliders: the inline edit a slider row opens when its
//! value column is clicked, Enter is pressed on it, or a number is typed
//! while it is selected. The screen owning the slider applies the committed
//! number with its own range and step; this type only keeps the text, filters
//! it to a number and tracks the pointer gesture that opens it.

use super::form::slider_value_hit;
use super::{FormLayout, MenuCanvas};
use jkr_ui::Rect;
use winit::keyboard::KeyCode;

/// Longest typed number kept; wider than any slider value in the menus.
const MAX_LEN: usize = 10;

/// Which characters a slider's number accepts besides digits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NumberFormat {
    /// A decimal point (`.`, or `,` read as one).
    pub(crate) fraction: bool,
    /// A leading minus sign.
    pub(crate) negative: bool,
}

/// What a key press did to an open entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum EntryKey {
    /// The entry is still open.
    Editing,
    /// Escape closed it without a value.
    Cancelled,
    /// Enter closed it; `None` when the text is not a number.
    Committed(Option<f64>),
}

/// Inline numeric edit of one slider row, with fixed storage.
///
/// The entry opens holding the current value as if selected: the first typed
/// character replaces it, Backspace edits it.
pub(crate) struct SliderEntry {
    text: String,
    row: Option<usize>,
    replace: bool,
    format: NumberFormat,
    /// Row whose value column the primary button went down on.
    value_press: Option<usize>,
}

impl SliderEntry {
    pub(crate) fn new() -> Self {
        Self {
            text: String::with_capacity(MAX_LEN * 4),
            row: None,
            replace: false,
            format: NumberFormat {
                fraction: false,
                negative: false,
            },
            value_press: None,
        }
    }

    /// The row being edited, if any.
    pub(crate) fn row(&self) -> Option<usize> {
        self.row
    }

    /// The typed text, or the starting value until something is typed.
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// Whether the next typed character replaces the whole text.
    pub(crate) fn replacing(&self) -> bool {
        self.replace
    }

    /// Open the entry on `row` holding `current`.
    pub(crate) fn begin(&mut self, row: usize, current: &str, format: NumberFormat) {
        self.text.clear();
        self.text.push_str(current);
        self.row = Some(row);
        self.replace = true;
        self.format = format;
    }

    /// Open the entry on `row` with the typed `text` when it starts a number
    /// (a digit, or a sign or point `format` allows); false otherwise.
    pub(crate) fn begin_typed(&mut self, row: usize, text: &str, format: NumberFormat) -> bool {
        let starts_number = text.chars().next().is_some_and(|c| accepts(format, "", c));
        if starts_number {
            self.begin(row, "", format);
            self.replace = false;
            self.push_text(text);
        }
        starts_number
    }

    /// Close the entry without a value.
    pub(crate) fn cancel(&mut self) {
        self.row = None;
        self.replace = false;
    }

    /// Close the entry and return its number, if it is one.
    pub(crate) fn commit(&mut self) -> Option<f64> {
        self.cancel();
        parse(&self.text)
    }

    /// Apply one key press to the open entry; `text` is the key's typed text.
    pub(crate) fn key(&mut self, key: KeyCode, text: Option<&str>, repeat: bool) -> EntryKey {
        match key {
            KeyCode::Escape => {
                self.cancel();
                EntryKey::Cancelled
            }
            KeyCode::Enter | KeyCode::NumpadEnter => EntryKey::Committed(self.commit()),
            KeyCode::Backspace => {
                if self.replace {
                    self.text.clear();
                    self.replace = false;
                } else {
                    self.text.pop();
                }
                EntryKey::Editing
            }
            _ => {
                if let Some(text) = text.filter(|_| !repeat) {
                    self.push_text(text);
                }
                EntryKey::Editing
            }
        }
    }

    /// Append the characters of `typed` that keep the text a number.
    fn push_text(&mut self, typed: &str) {
        for c in typed.chars() {
            let c = if c == ',' { '.' } else { c };
            let prefix = if self.replace { "" } else { self.text.as_str() };
            if (self.text.len() >= MAX_LEN && !self.replace) || !accepts(self.format, prefix, c) {
                continue;
            }
            if self.replace {
                self.text.clear();
                self.replace = false;
            }
            self.text.push(c);
        }
    }

    /// Note a primary press on `row` (a slider row, or `None` for anything
    /// else) and whether it landed in the row's value column.
    pub(crate) fn press(&mut self, row: Option<usize>, in_value: bool) {
        self.value_press = row.filter(|_| in_value);
    }

    /// Whether a drag over slider `row` should move it. A press in the value
    /// column only starts dragging once the pointer leaves the column, so a
    /// click there that wobbles a pixel does not jump the slider to its end.
    pub(crate) fn drag_moves(&mut self, row: usize, in_value: bool) -> bool {
        if self.value_press == Some(row) && in_value {
            return false;
        }
        self.value_press = None;
        true
    }

    /// Whether a click released on slider `row` opens the entry: it went
    /// down and came up in the value column without dragging out of it.
    pub(crate) fn click_opens(&mut self, row: usize, in_value: bool) -> bool {
        self.value_press.take() == Some(row) && in_value
    }
}

/// Whether `c` may follow `prefix` in a number of `format`.
fn accepts(format: NumberFormat, prefix: &str, c: char) -> bool {
    match c {
        '0'..='9' => true,
        '.' => format.fraction && !prefix.contains('.'),
        '-' => format.negative && prefix.is_empty(),
        _ => false,
    }
}

/// The typed number, if the text is one.
fn parse(text: &str) -> Option<f64> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

impl MenuCanvas {
    /// Whether the pointer at `x` is over the value column of slider row
    /// `rect`, at the form scale of the viewport this canvas was last begun
    /// with.
    pub(crate) fn slider_value_hit(&self, rect: Rect, x: f32) -> bool {
        slider_value_hit(rect, x, FormLayout::new(self.viewport).scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTEGER: NumberFormat = NumberFormat {
        fraction: false,
        negative: false,
    };
    const DECIMAL: NumberFormat = NumberFormat {
        fraction: true,
        negative: false,
    };

    fn typed(entry: &mut SliderEntry, text: &str) {
        for c in text.chars() {
            let mut buffer = [0; 4];
            entry.key(KeyCode::KeyX, Some(c.encode_utf8(&mut buffer)), false);
        }
    }

    #[test]
    fn first_typed_character_replaces_the_starting_value() {
        let mut entry = SliderEntry::new();
        entry.begin(3, "125", INTEGER);
        assert!(entry.replacing());
        typed(&mut entry, "144");
        assert_eq!(entry.text(), "144");
        assert_eq!(
            entry.key(KeyCode::Enter, None, false),
            EntryKey::Committed(Some(144.0))
        );
        assert_eq!(entry.row(), None);
    }

    #[test]
    fn backspace_edits_the_starting_value() {
        let mut entry = SliderEntry::new();
        entry.begin(0, "90", INTEGER);
        entry.key(KeyCode::Backspace, None, false);
        assert_eq!(entry.text(), "");
        typed(&mut entry, "95");
        entry.key(KeyCode::Backspace, None, false);
        assert_eq!(entry.text(), "9");
    }

    #[test]
    fn only_number_characters_are_kept() {
        let mut entry = SliderEntry::new();
        entry.begin(0, "", INTEGER);
        typed(&mut entry, "1a.2-3 ");
        assert_eq!(entry.text(), "123");

        entry.begin(0, "", DECIMAL);
        typed(&mut entry, "-1,5.5");
        assert_eq!(entry.text(), "1.55");
        assert_eq!(entry.commit(), Some(1.55));
    }

    #[test]
    fn minus_only_leads_when_allowed() {
        let mut entry = SliderEntry::new();
        let signed = NumberFormat {
            fraction: false,
            negative: true,
        };
        entry.begin(0, "", signed);
        typed(&mut entry, "-4-2");
        assert_eq!(entry.text(), "-42");
    }

    #[test]
    fn length_is_bounded() {
        let mut entry = SliderEntry::new();
        entry.begin(0, "", INTEGER);
        typed(&mut entry, "123456789012345");
        assert_eq!(entry.text().len(), MAX_LEN);
    }

    #[test]
    fn typing_a_number_opens_the_entry_and_letters_do_not() {
        let mut entry = SliderEntry::new();
        assert!(!entry.begin_typed(2, "d", INTEGER));
        assert!(!entry.begin_typed(2, ".", INTEGER));
        assert_eq!(entry.row(), None);
        assert!(entry.begin_typed(2, "7", INTEGER));
        assert_eq!(entry.row(), Some(2));
        assert!(!entry.replacing());
        typed(&mut entry, "5");
        assert_eq!(entry.commit(), Some(75.0));
    }

    #[test]
    fn escape_and_non_numbers_commit_nothing() {
        let mut entry = SliderEntry::new();
        entry.begin(1, "", DECIMAL);
        typed(&mut entry, ".");
        assert_eq!(
            entry.key(KeyCode::NumpadEnter, None, false),
            EntryKey::Committed(None)
        );
        entry.begin(1, "80", DECIMAL);
        assert_eq!(entry.key(KeyCode::Escape, None, false), EntryKey::Cancelled);
        assert_eq!(entry.row(), None);
    }

    #[test]
    fn repeated_typing_is_ignored() {
        let mut entry = SliderEntry::new();
        entry.begin(0, "", INTEGER);
        entry.key(KeyCode::Digit1, Some("1"), false);
        entry.key(KeyCode::Digit1, Some("1"), true);
        assert_eq!(entry.text(), "1");
    }

    #[test]
    fn a_click_in_the_value_column_opens_and_a_drag_out_does_not() {
        let mut entry = SliderEntry::new();
        entry.press(Some(4), true);
        assert!(!entry.drag_moves(4, true));
        assert!(entry.click_opens(4, true));

        entry.press(Some(4), true);
        assert!(entry.drag_moves(4, false));
        assert!(!entry.click_opens(4, true));

        entry.press(Some(4), false);
        assert!(entry.drag_moves(4, true));
        assert!(!entry.click_opens(4, true));

        entry.press(None, true);
        assert!(!entry.click_opens(4, true));
    }
}
