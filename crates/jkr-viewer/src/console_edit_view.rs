//! Drawing for console editing, and pointer resolution against what is drawn: the
//! input line with its caret, scrolled sideways to keep the caret in view, and
//! selection highlights on the input line and on scrollback rows.

use super::line_edit::token_at;
use super::selection::{Gesture, Mark, PromptPointer, Selection, floor_boundary};
use crate::menu_widgets::MenuCanvas;
use crate::text::{TextFace, UiFont, glyph_byte_at, visible_text_width_face};
use jkr_ui::{Color, FontWeight, Rect};
use std::ops::Range;

/// Drawn before the input line, as stock does.
const PROMPT_PREFIX: &str = "] ";
/// The caret: stock's underscore, drawn under the character it stands before.
const CARET: &str = "_";

/// What console editing contributes to one frame of the console.
pub(super) struct EditFrame<'a> {
    pub(super) prompt: PromptLine<'a>,
    pub(super) selection: &'a mut Selection,
    /// One past the number of the newest scrollback line.
    pub(super) lines_end: u64,
}

/// The input line as the console holds it.
pub(super) struct PromptLine<'a> {
    pub(super) input: &'a str,
    /// Caret byte offset in `input`.
    pub(super) cursor: usize,
    pub(super) selection: Option<Range<usize>>,
}

/// Selection highlight: the theme accent, translucent so the text stays readable.
fn highlight(ui: &MenuCanvas) -> Color {
    let accent = ui.theme().accent;
    Color::new(accent.r, accent.g, accent.b, 0.38)
}

/// The vertical band a row of `size` text drawn at `y` occupies, `pitch` tall.
fn band(y: f32, size: f32, pitch: f32) -> (f32, f32) {
    (y + (size - pitch) * 0.5, pitch)
}

/// Pen advance of each glyph at `size`.
fn advance(font: &UiFont, size: f32) -> impl Fn(u8) -> f32 + Copy + '_ {
    let scale = size / font.height.max(1.0);
    move |glyph| font.glyph(TextFace::Regular, glyph).advance * scale
}

fn width(font: &UiFont, text: &str, size: f32) -> f32 {
    visible_text_width_face(font, text, size / font.height.max(1.0), TextFace::Regular)
}

/// Walk the caret stops of `text`: before each glyph (with any colour codes that lead
/// it) and at the end. `stop(index, x, advance)` gets each stop's byte offset, its
/// distance from the text's start and the advance of the glyph after it (zero at the
/// end); returning `true` ends the walk at that stop, whose offset is returned.
fn walk(
    text: &str,
    advance: impl Fn(u8) -> f32,
    mut stop: impl FnMut(usize, f32, f32) -> bool,
) -> usize {
    let bytes = text.as_bytes();
    let (mut index, mut unit, mut pen) = (0, 0, 0.0);
    while index < bytes.len() {
        if bytes[index] == b'^' && bytes.get(index + 1).is_some_and(u8::is_ascii_digit) {
            index += 2;
            continue;
        }
        let (glyph, step) = glyph_byte_at(text, index);
        let width = advance(glyph);
        if stop(unit, pen, width) {
            return unit;
        }
        pen += width;
        index += step;
        unit = index;
    }
    stop(text.len(), pen, 0.0);
    text.len()
}

/// Caret stop of `text` nearest to `x` (measured from the text's start).
pub(super) fn byte_at(text: &str, x: f32, advance: impl Fn(u8) -> f32) -> usize {
    walk(text, advance, |_, pen, width| x < pen + width * 0.5)
}

/// First caret stop from which `text[..cursor]` fits in `room`: where to start drawing
/// so the caret stays visible.
pub(super) fn scroll_start(
    text: &str,
    cursor: usize,
    room: f32,
    advance: impl Fn(u8) -> f32 + Copy,
) -> usize {
    let mut caret = 0.0;
    walk(&text[..cursor], advance, |_, pen, _| {
        caret = pen;
        false
    });
    if caret <= room {
        return 0;
    }
    walk(&text[..cursor], advance, |_, pen, _| caret - pen <= room)
}

/// Draw the input line in `rect`: the prompt, the visible part of the text, a
/// selection highlight and the caret. A pointer gesture on the line resolves here.
pub(super) fn prompt(
    ui: &mut MenuCanvas,
    font: &UiFont,
    rect: Rect,
    size: f32,
    line: &PromptLine<'_>,
    selection: &mut Selection,
) {
    let foreground = ui.theme().foreground;
    let input = line.input;
    let cursor = floor_boundary(input, line.cursor);
    let prefix = width(font, PROMPT_PREFIX, size);
    let caret_width = width(font, CARET, size);
    let text_x = rect.x + prefix;
    let room = (rect.width - prefix - caret_width).max(0.0);
    let start = scroll_start(input, cursor, room, advance(font, size));
    let shown = &input[start..];
    let x_of = |byte: usize| text_x + width(font, &input[start..byte.max(start)], size);

    if selection.gesture() == Gesture::Prompt
        && let Some(position) = selection.pending_position()
    {
        let byte = start + byte_at(shown, position.x - text_x, advance(font, size));
        selection.set_prompt(match selection.press() {
            Some(press) if press.double => PromptPointer::Token(byte),
            Some(press) => PromptPointer::Place {
                byte,
                extend: press.extend,
            },
            None => PromptPointer::Place { byte, extend: true },
        });
    }

    let (y, height) = band(rect.y, size, size * 1.3);
    if let Some(range) = &line.selection {
        let left = x_of(floor_boundary(input, range.start));
        let right = x_of(floor_boundary(input, range.end)).min(rect.right());
        if right > left {
            let color = highlight(ui);
            ui.accent_bar(Rect::new(left, y, right - left, height), color);
        }
    }
    ui.text(
        PROMPT_PREFIX,
        Rect::new(rect.x, rect.y, prefix, rect.height),
        size,
        foreground,
        FontWeight::Regular,
        0.0,
    );
    ui.text(
        shown,
        Rect::new(text_x, rect.y, rect.width - prefix, rect.height),
        size,
        foreground,
        FontWeight::Regular,
        0.0,
    );
    ui.text(
        CARET,
        Rect::new(x_of(cursor), rect.y, caret_width, rect.height),
        size,
        foreground,
        FontWeight::Regular,
        0.0,
    );
}

/// Scrollback rows of one frame, bottom row first: draws their selection highlight
/// and resolves a pointer gesture over them.
pub(super) struct OutputRows<'a> {
    font: &'a UiFont,
    selection: &'a mut Selection,
    range: Option<(Mark, Mark)>,
    color: Color,
    size: f32,
    line_height: f32,
    /// Row (counted up from the bottom, negative below it) and x of a pointer
    /// position still to resolve.
    target: Option<(isize, f32)>,
    /// End of the bottom row and start of the top row seen, for positions past them.
    bottom: Option<Mark>,
    top: Option<Mark>,
}

impl<'a> OutputRows<'a> {
    /// Rows of `size` text, `line_height` apart, the bottom one laid out to end at
    /// `bottom`. A row's pointer area is its highlight band, centred on its text.
    pub(super) fn new(
        ui: &MenuCanvas,
        font: &'a UiFont,
        selection: &'a mut Selection,
        size: f32,
        line_height: f32,
        bottom: f32,
    ) -> Self {
        let edge = bottom + (size - line_height) * 0.5;
        let target = (selection.gesture() == Gesture::Output)
            .then(|| selection.pending_position())
            .flatten()
            .map(|position| {
                let rows = ((edge - position.y) / line_height.max(1.0)).floor();
                (rows.max(-1.0) as isize, position.x)
            });
        Self {
            font,
            range: selection.range(),
            selection,
            color: highlight(ui),
            size,
            line_height,
            target,
            bottom: None,
            top: None,
        }
    }

    /// Row `index` (0 at the bottom) shows `text`, which starts `start` bytes into
    /// scrollback line `line`, drawn at `rect`.
    pub(super) fn row(
        &mut self,
        ui: &mut MenuCanvas,
        index: usize,
        line: u64,
        start: usize,
        text: &str,
        rect: Rect,
    ) {
        let begin = Mark { line, byte: start };
        let end = Mark {
            line,
            byte: start + text.len(),
        };
        if index == 0 {
            self.bottom = Some(end);
        }
        self.top = Some(begin);
        if let Some((from, to)) = self.range
            && from < end
            && to > begin
        {
            let left = from.max(begin).byte - start;
            let right = to.min(end).byte - start;
            let left = rect.x + width(self.font, &text[..floor_boundary(text, left)], self.size);
            let mut right =
                rect.x + width(self.font, &text[..floor_boundary(text, right)], self.size);
            // A selection running on past this row shows its line break as a space.
            if to > end {
                right += width(self.font, " ", self.size);
            }
            let right = right.min(rect.right());
            if right > left {
                let (y, height) = band(rect.y, self.size, self.line_height);
                ui.accent_bar(Rect::new(left, y, right - left, height), self.color);
            }
        }
        if let Some((row, x)) = self.target
            && row == index as isize
        {
            self.target = None;
            let byte = if x > rect.right() {
                text.len()
            } else {
                byte_at(text, x - rect.x, advance(self.font, self.size))
            };
            let token = token_at(text, byte);
            self.resolve(
                Mark {
                    line,
                    byte: start + byte,
                },
                (
                    Mark {
                        line,
                        byte: start + token.start,
                    },
                    Mark {
                        line,
                        byte: start + token.end,
                    },
                ),
            );
        }
    }

    /// After the last row: a position below the rows takes the end of the bottom row,
    /// one above them the start of the top row.
    pub(super) fn finish(mut self) {
        let Some((row, _)) = self.target.take() else {
            return;
        };
        if let Some(at) = if row < 0 { self.bottom } else { self.top } {
            self.resolve(at, (at, at));
        }
    }

    fn resolve(&mut self, at: Mark, token: (Mark, Mark)) {
        match self.selection.press() {
            Some(press) => self.selection.press_output(press, at, token),
            None => self.selection.drag_output(at),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every glyph ten units wide.
    fn even(_: u8) -> f32 {
        10.0
    }

    #[test]
    fn pointer_x_picks_the_nearest_caret_stop() {
        assert_eq!(byte_at("abc", -5.0, even), 0);
        assert_eq!(byte_at("abc", 4.0, even), 0);
        assert_eq!(byte_at("abc", 6.0, even), 1);
        assert_eq!(byte_at("abc", 26.0, even), 3);
        assert_eq!(byte_at("abc", 400.0, even), 3);
    }

    #[test]
    fn colour_codes_take_no_width_and_stay_with_their_glyph() {
        // "^1a^2b": stops before "^1a" (0), before "^2b" (3) and at the end (6).
        assert_eq!(byte_at("^1a^2b", 1.0, even), 0);
        assert_eq!(byte_at("^1a^2b", 9.0, even), 3);
        assert_eq!(byte_at("^1a^2b", 19.0, even), 6);
    }

    #[test]
    fn long_lines_scroll_just_enough_to_show_the_caret() {
        let text = "abcdefghij";
        assert_eq!(scroll_start(text, 5, 50.0, even), 0);
        assert_eq!(scroll_start(text, 10, 50.0, even), 5);
        assert_eq!(scroll_start(text, 10, 45.0, even), 6);
        assert_eq!(scroll_start(text, 0, 0.0, even), 0);
    }
}
