//! Caret and selection over the console's single input line.
//!
//! The text itself stays in [`super::ViewerConsole`]'s `input` string, which other
//! code replaces wholesale (history, completion, autoclear); this type only keeps a
//! byte caret and an optional selection anchor beside it. Every operation clamps
//! both to the text it is given first, so a replaced line can never leave them
//! pointing past its end or into the middle of a character.

use std::ops::Range;

/// A caret movement, also used as the extent of a deletion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Motion {
    /// One character left.
    Left,
    /// One character right.
    Right,
    /// To the start of the word before the caret.
    WordLeft,
    /// To the start of the next word.
    WordRight,
    /// To the start of the line.
    Home,
    /// To the end of the line.
    End,
}

/// Caret byte offset and selection anchor of the console input line.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct LineEdit {
    cursor: usize,
    /// The other end of the selection; the selection is empty when it equals the caret.
    anchor: Option<usize>,
}

impl LineEdit {
    /// Caret byte offset in `text`.
    pub(super) fn cursor(&self, text: &str) -> usize {
        clamp(text, self.cursor)
    }

    /// Selected byte range in `text`, if it is not empty.
    pub(super) fn selection(&self, text: &str) -> Option<Range<usize>> {
        let cursor = self.cursor(text);
        let anchor = clamp(text, self.anchor?);
        (anchor != cursor).then(|| anchor.min(cursor)..anchor.max(cursor))
    }

    /// The line was replaced: the caret goes to its end and nothing is selected.
    pub(super) fn to_end(&mut self, text: &str) {
        self.cursor = text.len();
        self.anchor = None;
    }

    /// Drop the selection, keeping the caret.
    pub(super) fn deselect(&mut self) {
        self.anchor = None;
    }

    /// Select the whole line, caret at its end.
    pub(super) fn select_all(&mut self, text: &str) {
        self.anchor = Some(0);
        self.cursor = text.len();
    }

    /// Select `range` (from a double click), caret at its end.
    pub(super) fn select(&mut self, text: &str, range: Range<usize>) {
        self.anchor = Some(clamp(text, range.start));
        self.cursor = clamp(text, range.end);
    }

    /// Put the caret at byte `target` (from the pointer); `extend` keeps or starts a
    /// selection from the current caret instead of dropping it.
    pub(super) fn place(&mut self, text: &str, target: usize, extend: bool) {
        let cursor = self.cursor(text);
        if extend {
            self.anchor.get_or_insert(cursor);
        } else {
            self.anchor = None;
        }
        self.cursor = clamp(text, target);
    }

    /// Move the caret. With `extend` (Shift held) the selection grows or starts;
    /// without it, a left or right step over a selection lands on that side of it.
    pub(super) fn motion(&mut self, text: &str, motion: Motion, extend: bool) {
        let cursor = self.cursor(text);
        if !extend && let Some(range) = self.selection(text) {
            self.anchor = None;
            match motion {
                Motion::Left => {
                    self.cursor = range.start;
                    return;
                }
                Motion::Right => {
                    self.cursor = range.end;
                    return;
                }
                _ => {}
            }
        }
        if extend {
            self.anchor.get_or_insert(cursor);
        } else {
            self.anchor = None;
        }
        self.cursor = target(text, cursor, motion);
    }

    /// Delete the selection, or else the text between the caret and `motion`'s
    /// target (Backspace is `Left`, Delete is `Right`, Ctrl adds the word forms).
    pub(super) fn delete(&mut self, text: &mut String, motion: Motion) {
        let range = self.selection(text).unwrap_or_else(|| {
            let cursor = self.cursor(text);
            let other = target(text, cursor, motion);
            cursor.min(other)..cursor.max(other)
        });
        text.replace_range(range.clone(), "");
        self.cursor = range.start;
        self.anchor = None;
    }

    /// Replace the selection with `value`, or insert it at the caret, dropping control
    /// characters and whatever would take the line past `limit` bytes.
    pub(super) fn insert(&mut self, text: &mut String, value: &str, limit: usize) {
        if let Some(range) = self.selection(text) {
            text.replace_range(range.clone(), "");
            self.cursor = range.start;
        }
        self.anchor = None;
        let mut cursor = self.cursor(text);
        for character in value.chars().filter(|character| !character.is_control()) {
            if text.len() + character.len_utf8() > limit {
                break;
            }
            text.insert(cursor, character);
            cursor += character.len_utf8();
        }
        self.cursor = cursor;
    }
}

/// `index` moved down to a character boundary within `text`.
fn clamp(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// Word characters, as Ctrl+arrows and Ctrl+Backspace see them. Everything else,
/// including `_`, `.` and `/`, separates words, so `cg_drawFPS` and `127.0.0.1` are
/// edited a piece at a time, as EternalJK's Ctrl+Backspace does at `_` and `/`.
fn is_word(character: char) -> bool {
    character.is_alphanumeric()
}

/// Where `motion` takes a caret at `cursor` in `text`.
fn target(text: &str, cursor: usize, motion: Motion) -> usize {
    match motion {
        Motion::Left => text[..cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(index, _)| index),
        Motion::Right => text[cursor..]
            .chars()
            .next()
            .map_or(cursor, |character| cursor + character.len_utf8()),
        Motion::Home => 0,
        Motion::End => text.len(),
        Motion::WordLeft => {
            // Back over separators, then over the word before them.
            let mut start = cursor;
            let mut in_word = false;
            for (index, character) in text[..cursor].char_indices().rev() {
                if is_word(character) {
                    in_word = true;
                } else if in_word {
                    break;
                }
                start = index;
            }
            start
        }
        Motion::WordRight => {
            // Over the rest of this word and the separators after it.
            let mut past_word = false;
            for (index, character) in text[cursor..].char_indices() {
                if !is_word(character) {
                    past_word = true;
                } else if past_word {
                    return cursor + index;
                }
            }
            text.len()
        }
    }
}

/// The whitespace-separated token around byte `at` in `text`, for a double click:
/// a whole address such as `127.0.0.1:29070` rather than one number of it.
pub(super) fn token_at(text: &str, at: usize) -> Range<usize> {
    let at = clamp(text, at);
    let start = text[..at]
        .char_indices()
        .rev()
        .find(|(_, character)| character.is_whitespace())
        .map_or(0, |(index, character)| index + character.len_utf8());
    let end = text[at..]
        .char_indices()
        .find(|(_, character)| character.is_whitespace())
        .map_or(text.len(), |(index, _)| at + index);
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit_at(text: &str, cursor: usize) -> LineEdit {
        LineEdit {
            cursor: clamp(text, cursor),
            anchor: None,
        }
    }

    #[test]
    fn inserts_and_deletes_at_the_caret() {
        let mut text = String::from("conect 1");
        let mut edit = edit_at(&text, 3);
        edit.insert(&mut text, "n", 512);
        assert_eq!(text, "connect 1");
        assert_eq!(edit.cursor(&text), 4);
        edit.delete(&mut text, Motion::Left);
        assert_eq!(text, "conect 1");
        assert_eq!(edit.cursor(&text), 3);
        edit.delete(&mut text, Motion::Right);
        assert_eq!(text, "conct 1");
        assert_eq!(edit.cursor(&text), 3);
    }

    #[test]
    fn insertion_respects_the_byte_limit_and_drops_control_characters() {
        let mut text = String::from("ab");
        let mut edit = edit_at(&text, 1);
        edit.insert(&mut text, "x\u{7}é\ny", 5);
        // "x" and "é" (two bytes) fit in five bytes, "y" does not.
        assert_eq!(text, "axéb");
        assert_eq!(edit.cursor(&text), 4);
    }

    #[test]
    fn moves_over_whole_characters() {
        let text = "aé b";
        let mut edit = edit_at(text, 1);
        edit.motion(text, Motion::Right, false);
        assert_eq!(edit.cursor(text), 3);
        edit.motion(text, Motion::Left, false);
        assert_eq!(edit.cursor(text), 1);
        edit.motion(text, Motion::End, false);
        assert_eq!(edit.cursor(text), text.len());
        edit.motion(text, Motion::Home, false);
        assert_eq!(edit.cursor(text), 0);
    }

    #[test]
    fn word_jumps_stop_at_separators() {
        let text = "seta cg_drawFPS  1";
        let mut edit = edit_at(text, text.len());
        let mut stops = Vec::new();
        for _ in 0..5 {
            edit.motion(text, Motion::WordLeft, false);
            stops.push(edit.cursor(text));
        }
        assert_eq!(stops, [17, 8, 5, 0, 0]);
        stops.clear();
        for _ in 0..5 {
            edit.motion(text, Motion::WordRight, false);
            stops.push(edit.cursor(text));
        }
        assert_eq!(stops, [5, 8, 17, text.len(), text.len()]);
    }

    #[test]
    fn word_deletion_removes_one_word() {
        let mut text = String::from("connect 127.0.0.1");
        let mut edit = edit_at(&text, text.len());
        edit.delete(&mut text, Motion::WordLeft);
        assert_eq!(text, "connect 127.0.0.");
        let mut edit = edit_at(&text, 0);
        edit.delete(&mut text, Motion::WordRight);
        assert_eq!(text, "127.0.0.");
    }

    #[test]
    fn shift_motion_selects_and_typing_replaces_the_selection() {
        let mut text = String::from("map ffa3");
        let mut edit = edit_at(&text, text.len());
        edit.motion(&text, Motion::WordLeft, true);
        assert_eq!(edit.selection(&text), Some(4..8));
        edit.insert(&mut text, "ffa1", 512);
        assert_eq!(text, "map ffa1");
        assert_eq!(edit.selection(&text), None);
        edit.select_all(&text);
        edit.delete(&mut text, Motion::Left);
        assert!(text.is_empty());
    }

    #[test]
    fn plain_arrows_collapse_a_selection_to_its_side() {
        let text = "abcdef";
        let mut edit = edit_at(text, 1);
        edit.motion(text, Motion::End, true);
        edit.motion(text, Motion::Left, false);
        assert_eq!((edit.cursor(text), edit.selection(text)), (1, None));
        edit.motion(text, Motion::Home, true);
        edit.motion(text, Motion::Right, false);
        assert_eq!((edit.cursor(text), edit.selection(text)), (1, None));
    }

    #[test]
    fn a_replaced_line_cannot_leave_the_caret_out_of_range() {
        let edit = LineEdit {
            cursor: 40,
            anchor: Some(2),
        };
        assert_eq!(edit.cursor("é"), 2);
        assert_eq!(edit.selection("é"), None);
        assert_eq!(edit.cursor("aé"), 3);
        // Byte 2 is inside "é", so the anchor falls back to its start.
        assert_eq!(edit.selection("aéz"), Some(1..4));
    }

    #[test]
    fn tokens_span_whitespace_separated_text() {
        let text = "^7Server: 127.0.0.1:29070 (ffa)";
        assert_eq!(&text[token_at(text, 14)], "127.0.0.1:29070");
        assert_eq!(&text[token_at(text, 0)], "^7Server:");
        assert_eq!(&text[token_at(text, text.len())], "(ffa)");
    }
}
