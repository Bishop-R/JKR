//! Drawing for console editing, and pointer resolution against what is drawn: the
//! input line with its caret, scrolled sideways to keep the caret in view, and
//! selection highlights on the input line and on scrollback rows. Every position is
//! measured with the [`ConsoleText`] the text is drawn with.

use super::console_text::ConsoleText;
use super::line_edit::token_at;
use super::selection::{Gesture, Mark, PromptPointer, Selection, floor_boundary};
use crate::menu_widgets::MenuCanvas;
use crate::text::glyph_byte_at;
use jkr_ui::{Color, Rect};
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

/// Walk the caret stops of `text`: before each glyph (with any colour codes that lead
/// it) and at the end. `stop(index, x, advance)` gets each stop's byte offset, its
/// distance from the text's start and the advance of the glyph after it (zero at the
/// end); returning `true` ends the walk at that stop, whose offset is returned.
pub(crate) fn walk(
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
pub(crate) fn byte_at(text: &str, x: f32, advance: impl Fn(u8) -> f32) -> usize {
    walk(text, advance, |_, pen, width| x < pen + width * 0.5)
}

/// First caret stop from which `text[..cursor]` fits in `room`: where to start drawing
/// so the caret stays visible.
pub(crate) fn scroll_start(
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

/// Draw the input line in `rect` with `text`: the prompt, the visible part of the
/// input, a selection highlight and the caret. A pointer gesture on the line resolves
/// here.
pub(super) fn prompt(
    ui: &mut MenuCanvas,
    text: ConsoleText<'_>,
    rect: Rect,
    line: &PromptLine<'_>,
    selection: &mut Selection,
) {
    let foreground = ui.theme().foreground;
    let input = line.input;
    let cursor = floor_boundary(input, line.cursor);
    let advance = |glyph| text.advance(glyph);
    let prefix = text.width(PROMPT_PREFIX);
    let caret_width = text.width(CARET);
    let text_x = rect.x + prefix;
    let room = (rect.width - prefix - caret_width).max(0.0);
    let start = scroll_start(input, cursor, room, advance);
    let shown = &input[start..];
    let x_of = |byte: usize| text_x + text.width(&input[start..byte.max(start)]);

    if selection.gesture() == Gesture::Prompt
        && let Some(position) = selection.pending_position()
    {
        let byte = start + byte_at(shown, position.x - text_x, advance);
        selection.set_prompt(match selection.press() {
            Some(press) if press.double => PromptPointer::Token(byte),
            Some(press) => PromptPointer::Place {
                byte,
                extend: press.extend,
            },
            None => PromptPointer::Place { byte, extend: true },
        });
    }

    let size = text.size();
    let (y, height) = band(rect.y, size, size * 1.3);
    if let Some(range) = &line.selection {
        let left = x_of(floor_boundary(input, range.start));
        let right = x_of(floor_boundary(input, range.end)).min(rect.right());
        if right > left {
            let color = highlight(ui);
            ui.accent_bar(Rect::new(left, y, right - left, height), color);
        }
    }
    text.draw(
        ui,
        PROMPT_PREFIX,
        Rect::new(rect.x, rect.y, prefix, rect.height),
        foreground,
    );
    text.draw(
        ui,
        shown,
        Rect::new(text_x, rect.y, rect.width - prefix, rect.height),
        foreground,
    );
    text.draw(
        ui,
        CARET,
        Rect::new(x_of(cursor), rect.y, caret_width, rect.height),
        foreground,
    );
}

/// Scrollback rows of one frame, bottom row first: draws them with their selection
/// highlight and resolves a pointer gesture over them, all with one [`ConsoleText`].
pub(super) struct OutputRows<'a> {
    text: ConsoleText<'a>,
    selection: &'a mut Selection,
    range: Option<(Mark, Mark)>,
    color: Color,
    line_height: f32,
    /// Row (counted up from the bottom, negative below it) and x of a pointer
    /// position still to resolve.
    target: Option<(isize, f32)>,
    /// End of the bottom row and start of the top row seen, for positions past them.
    bottom: Option<Mark>,
    top: Option<Mark>,
}

impl<'a> OutputRows<'a> {
    /// Rows of `text`, `line_height` apart, the bottom one laid out to end at
    /// `bottom`. A row's pointer area is its highlight band, centred on its text.
    pub(super) fn new(
        ui: &MenuCanvas,
        text: ConsoleText<'a>,
        selection: &'a mut Selection,
        line_height: f32,
        bottom: f32,
    ) -> Self {
        let edge = bottom + (text.size() - line_height) * 0.5;
        let target = (selection.gesture() == Gesture::Output)
            .then(|| selection.pending_position())
            .flatten()
            .map(|position| {
                let rows = ((edge - position.y) / line_height.max(1.0)).floor();
                (rows.max(-1.0) as isize, position.x)
            });
        Self {
            text,
            range: selection.range(),
            selection,
            color: highlight(ui),
            line_height,
            target,
            bottom: None,
            top: None,
        }
    }

    /// Draw row `index` (0 at the bottom) in `color` at `rect`: `text`, which starts
    /// at `begin` in its scrollback line.
    pub(super) fn row(
        &mut self,
        ui: &mut MenuCanvas,
        index: usize,
        begin: Mark,
        text: &str,
        rect: Rect,
        color: Color,
    ) {
        let Mark { line, byte: start } = begin;
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
            let left = rect.x + self.text.width(&text[..floor_boundary(text, left)]);
            let mut right = rect.x + self.text.width(&text[..floor_boundary(text, right)]);
            // A selection running on past this row shows its line break as a space.
            if to > end {
                right += self.text.width(" ");
            }
            let right = right.min(rect.right());
            if right > left {
                let (y, height) = band(rect.y, self.text.size(), self.line_height);
                ui.accent_bar(Rect::new(left, y, right - left, height), self.color);
            }
        }
        self.text.draw(ui, text, rect, color);
        if let Some((row, x)) = self.target
            && row == index as isize
        {
            self.target = None;
            let byte = if x > rect.right() {
                text.len()
            } else {
                let measure = self.text;
                byte_at(text, x - rect.x, |glyph| measure.advance(glyph))
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
