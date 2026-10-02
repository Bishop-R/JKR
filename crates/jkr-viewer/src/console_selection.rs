//! Pointer gestures over the open console: dragging selects scrollback text,
//! clicking or dragging in the input line moves its caret or selects in it.
//!
//! Pointer events only record positions here. Turning a position into text needs the
//! font and the layout of the frame, so [`super::edit_view`] resolves them
//! while it draws (one frame later at most), and the console applies input-line
//! results afterwards through [`Selection::take_prompt`].
//!
//! Scrollback positions are kept as [`Mark`]s, a line number from
//! [`jkr_shell::Shell::first_line_number`] and a byte offset, so a selection stays on
//! its text while new output arrives, the view scrolls, or old lines are trimmed.

use jkr_ui::{InputEvent, PointerButton, Rect, Vec2};
use std::time::{Duration, Instant};

/// A second press this soon and this close to the first is a double click.
const DOUBLE_CLICK: Duration = Duration::from_millis(500);
const DOUBLE_CLICK_DISTANCE: f32 = 6.0;

/// A position in scrollback: a line by its number, and a byte offset in the text the
/// console displays for it (with or without the timestamp, as `con_timestamps` says).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct Mark {
    pub(super) line: u64,
    pub(super) byte: usize,
}

/// What a pointer gesture asks of the input line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PromptPointer {
    /// Put the caret at a byte; `extend` selects from the caret instead.
    Place { byte: usize, extend: bool },
    /// Select the whitespace-separated token around a byte (a double click).
    Token(usize),
    /// A scrollback selection started; the input selection goes.
    Deselect,
}

/// What the gesture under way selects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Gesture {
    Idle,
    Output,
    Prompt,
}

/// A press waiting for the next frame's layout.
#[derive(Clone, Copy, Debug)]
pub(super) struct Press {
    pub(super) position: Vec2,
    pub(super) double: bool,
    pub(super) extend: bool,
}

/// Scrollback selection and the pointer gesture that edits it.
pub(super) struct Selection {
    /// Where the selection started and where it ends now; empty when they are equal.
    range: Option<(Mark, Mark)>,
    gesture: Gesture,
    press: Option<Press>,
    /// Latest position while the primary button is held, until a frame uses it.
    drag: Option<Vec2>,
    held: bool,
    /// The gesture began with a double click: it selected a token, which a drag
    /// does not change.
    by_token: bool,
    /// The button came up; the gesture ends after the next frame.
    released: bool,
    last_press: Option<(Instant, Vec2)>,
    prompt: Option<PromptPointer>,
}

impl Selection {
    pub(super) fn new() -> Self {
        Self {
            range: None,
            gesture: Gesture::Idle,
            press: None,
            drag: None,
            held: false,
            by_token: false,
            released: false,
            last_press: None,
            prompt: None,
        }
    }

    /// Record a pointer event for the next frame. `shift` extends rather than restarts.
    pub(super) fn pointer(&mut self, event: InputEvent, shift: bool) {
        match event {
            InputEvent::PointerPress {
                position,
                button: PointerButton::Primary,
            } => {
                let now = Instant::now();
                let double = self.last_press.is_some_and(|(time, last)| {
                    now.duration_since(time) <= DOUBLE_CLICK
                        && (last.x - position.x).abs() <= DOUBLE_CLICK_DISTANCE
                        && (last.y - position.y).abs() <= DOUBLE_CLICK_DISTANCE
                });
                // A third press starts over rather than counting as another double click.
                self.last_press = (!double).then_some((now, position));
                self.press = Some(Press {
                    position,
                    double,
                    extend: shift && !double,
                });
                self.drag = None;
                self.held = true;
                self.by_token = double;
                self.released = false;
            }
            InputEvent::PointerMove(position) if self.held && !self.by_token => {
                self.drag = Some(position);
            }
            InputEvent::PointerRelease {
                position,
                button: PointerButton::Primary,
            } if self.held => {
                self.held = false;
                self.released = true;
                if self.press.is_none() && !self.by_token {
                    self.drag = Some(position);
                }
            }
            _ => {}
        }
    }

    /// Start a frame: decide what a waiting press is aimed at. A press elsewhere,
    /// such as the header, clears the scrollback selection.
    pub(super) fn begin_frame(&mut self, output: Rect, prompt: Rect) {
        let Some(press) = self.press else {
            return;
        };
        if prompt.contains(press.position) {
            self.gesture = Gesture::Prompt;
            self.range = None;
        } else if output.contains(press.position) {
            self.gesture = Gesture::Output;
            self.prompt = Some(PromptPointer::Deselect);
        } else {
            self.gesture = Gesture::Idle;
            self.press = None;
            self.range = None;
        }
    }

    /// Finish a frame: positions are used up, and a released gesture is over.
    pub(super) fn end_frame(&mut self) {
        self.press = None;
        self.drag = None;
        if self.released {
            self.released = false;
            self.gesture = Gesture::Idle;
        }
    }

    pub(super) fn gesture(&self) -> Gesture {
        self.gesture
    }

    /// The press this frame must resolve, if any.
    pub(super) fn press(&self) -> Option<Press> {
        self.press
    }

    /// The pointer position this frame must resolve: a new press or drag.
    pub(super) fn pending_position(&self) -> Option<Vec2> {
        self.drag.or(self.press.map(|press| press.position))
    }

    /// A press resolved to scrollback text: start a selection there, or with Shift
    /// move the end of the current one. A double click selects `token` instead.
    pub(super) fn press_output(&mut self, press: Press, at: Mark, token: (Mark, Mark)) {
        self.range = if press.double {
            Some(token)
        } else if press.extend
            && let Some((anchor, _)) = self.range
        {
            Some((anchor, at))
        } else {
            Some((at, at))
        };
    }

    /// The pointer moved to `at` while selecting scrollback text.
    pub(super) fn drag_output(&mut self, at: Mark) {
        if let Some((_, head)) = &mut self.range {
            *head = at;
        }
    }

    /// A pointer result for the input line, resolved by the frame.
    pub(super) fn set_prompt(&mut self, request: PromptPointer) {
        self.prompt = Some(request);
    }

    /// The input-line change the last frame resolved, for the console to apply.
    pub(super) fn take_prompt(&mut self) -> Option<PromptPointer> {
        self.prompt.take()
    }

    /// The selected scrollback span, start first, if it is not empty.
    pub(super) fn range(&self) -> Option<(Mark, Mark)> {
        let (anchor, head) = self.range?;
        (anchor != head).then(|| (anchor.min(head), anchor.max(head)))
    }

    /// Drop the scrollback selection and any gesture under way.
    pub(super) fn clear(&mut self) {
        self.range = None;
        self.gesture = Gesture::Idle;
        self.press = None;
        self.drag = None;
        self.held = false;
        self.released = false;
    }
}

/// Write the text between `start` and `end` into `out`, from scrollback whose
/// oldest displayed line is number `first`. Lines are joined with newlines, Quake
/// colour codes are left out, and trimmed lines or out-of-range offsets are
/// skipped rather than trusted.
pub(super) fn copy_range<'a>(
    lines: impl Iterator<Item = &'a str>,
    first: u64,
    start: Mark,
    end: Mark,
    out: &mut String,
) {
    out.clear();
    let mut wrote_line = false;
    for (number, text) in (first..).zip(lines) {
        if number < start.line {
            continue;
        }
        if number > end.line {
            break;
        }
        let from = if number == start.line {
            floor_boundary(text, start.byte)
        } else {
            0
        };
        let to = if number == end.line {
            floor_boundary(text, end.byte)
        } else {
            text.len()
        };
        if wrote_line {
            out.push('\n');
        }
        wrote_line = true;
        if from < to {
            push_uncoloured(&text[from..to], out);
        }
    }
}

/// `index` moved down onto a character boundary within `text`.
pub(super) fn floor_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// Append `text` without its `^digit` colour codes.
fn push_uncoloured(text: &str, out: &mut String) {
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '^' && characters.peek().is_some_and(char::is_ascii_digit) {
            characters.next();
        } else {
            out.push(character);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(line: u64, byte: usize) -> Mark {
        Mark { line, byte }
    }

    #[test]
    fn copies_part_of_one_line_without_colour_codes() {
        let lines = ["first", "^7Server at ^3127.0.0.1:29070^7 ready"];
        let mut out = String::new();
        copy_range(lines.into_iter(), 10, mark(11, 12), mark(11, 31), &mut out);
        assert_eq!(out, "127.0.0.1:29070");
    }

    #[test]
    fn copies_across_lines_and_skips_trimmed_ones() {
        let lines = ["two", "three\nwrapped", "four"];
        let mut out = String::new();
        // Line 1 was trimmed from scrollback; the copy starts with what remains.
        copy_range(lines.into_iter(), 2, mark(1, 2), mark(4, 2), &mut out);
        assert_eq!(out, "two\nthree\nwrapped\nfo");
    }

    #[test]
    fn offsets_inside_a_character_or_past_the_end_are_clamped() {
        let lines = ["aéb"];
        let mut out = String::new();
        copy_range(lines.into_iter(), 0, mark(0, 2), mark(0, 99), &mut out);
        assert_eq!(out, "éb");
    }

    #[test]
    fn a_click_without_a_drag_selects_nothing() {
        let mut selection = Selection::new();
        let press = Press {
            position: Vec2::new(0.0, 0.0),
            double: false,
            extend: false,
        };
        selection.press_output(press, mark(3, 4), (mark(3, 0), mark(3, 9)));
        assert_eq!(selection.range(), None);
        selection.drag_output(mark(2, 1));
        assert_eq!(selection.range(), Some((mark(2, 1), mark(3, 4))));
    }

    #[test]
    fn a_double_click_keeps_its_token_when_the_pointer_moves() {
        let mut selection = Selection::new();
        let at = Vec2::new(20.0, 50.0);
        let press = InputEvent::PointerPress {
            position: at,
            button: PointerButton::Primary,
        };
        let release = InputEvent::PointerRelease {
            position: at,
            button: PointerButton::Primary,
        };
        selection.pointer(press, false);
        selection.end_frame();
        selection.pointer(release, false);
        selection.end_frame();
        selection.pointer(press, false);
        assert!(selection.press().is_some_and(|press| press.double));
        selection.end_frame();
        selection.pointer(InputEvent::PointerMove(Vec2::new(60.0, 50.0)), false);
        selection.pointer(release, false);
        assert_eq!(selection.pending_position(), None);
    }

    #[test]
    fn presses_outside_the_console_text_clear_the_selection() {
        let mut selection = Selection::new();
        selection.range = Some((mark(0, 0), mark(0, 3)));
        let output = Rect::new(0.0, 40.0, 100.0, 100.0);
        let prompt = Rect::new(0.0, 150.0, 100.0, 20.0);
        selection.pointer(
            InputEvent::PointerPress {
                position: Vec2::new(10.0, 10.0),
                button: PointerButton::Primary,
            },
            false,
        );
        selection.begin_frame(output, prompt);
        assert_eq!(selection.gesture(), Gesture::Idle);
        assert_eq!(selection.range(), None);
    }
}
