//! Shared bounded numeric drafts for settings, saber colours and shot controls.

use super::MenuCanvas;
use crate::console::line_edit::{LineEdit, Motion};
use jkr_ui::{Color, FontWeight, Rect, TextAlign};
use winit::keyboard::KeyCode;

/// Separate targets keep clicks on a value from moving its slider.
pub(crate) const VALUE_BASE: u16 = 700;

pub(crate) fn value_row(token: u16) -> Option<usize> {
    (VALUE_BASE..VALUE_BASE + 100)
        .contains(&token)
        .then(|| usize::from(token - VALUE_BASE))
}

pub(crate) enum EditResult {
    Pending,
    Cancel,
    Commit(f64),
}

/// Owns the target row so pointer hover cannot redirect an unfinished edit.
pub(crate) struct NumericEdit {
    pub(crate) row: usize,
    text: String,
    caret: LineEdit,
    min: f64,
    max: f64,
    integer: bool,
    invalid: bool,
}

impl NumericEdit {
    pub(crate) fn new(row: usize, text: String, min: f64, max: f64, integer: bool) -> Self {
        let mut caret = LineEdit::default();
        caret.select_all(&text);
        Self {
            row,
            text,
            caret,
            min,
            max,
            integer,
            invalid: false,
        }
    }

    /// Enter validates without step quantization; Escape discards the draft.
    pub(crate) fn key(&mut self, key: KeyCode, text: Option<&str>) -> EditResult {
        match key {
            KeyCode::Escape => return EditResult::Cancel,
            KeyCode::Enter | KeyCode::NumpadEnter => {
                if let Some(value) = self.value() {
                    return EditResult::Commit(value);
                }
                self.invalid = true;
                return EditResult::Pending;
            }
            KeyCode::ArrowLeft => self.caret.motion(&self.text, Motion::Left, false),
            KeyCode::ArrowRight => self.caret.motion(&self.text, Motion::Right, false),
            KeyCode::Home => self.caret.motion(&self.text, Motion::Home, false),
            KeyCode::End => self.caret.motion(&self.text, Motion::End, false),
            KeyCode::Backspace => self.caret.delete(&mut self.text, Motion::Left),
            KeyCode::Delete => self.caret.delete(&mut self.text, Motion::Right),
            _ => {
                if let Some(text) = text {
                    if text == "\u{1}" {
                        self.caret.select_all(&self.text);
                    } else if !text.is_empty()
                        && text
                            .chars()
                            .all(|c| c.is_ascii_digit() || "+-.,eE".contains(c))
                    {
                        self.caret.insert(&mut self.text, text, 24);
                    }
                }
            }
        }
        self.invalid = false;
        EditResult::Pending
    }

    fn value(&self) -> Option<f64> {
        let value = self.text.trim().replace(',', ".").parse::<f64>().ok()?;
        (value.is_finite() && (!self.integer || value.fract() == 0.0))
            .then(|| value.clamp(self.min, self.max))
    }

    /// Compact inline editing, using retained text storage instead of frame allocations.
    pub(crate) fn draw(&self, ui: &mut MenuCanvas, rect: Rect, scale: f32) {
        let color = if self.invalid {
            Color::new(1.0, 0.3, 0.25, 1.0)
        } else {
            ui.theme().accent
        };
        if self.caret.selection(&self.text).is_some() {
            ui.accent_bar(rect, Color::new(color.r, color.g, color.b, 0.2));
        }
        let cursor = self.caret.cursor(&self.text);
        let marker = if self.caret.selection(&self.text).is_some() {
            ""
        } else {
            "|"
        };
        let size = (15.0 * scale).min(rect.width / (self.text.len() + 1) as f32);
        ui.text_fmt_aligned(
            format_args!("{}{marker}{}", &self.text[..cursor], &self.text[cursor..]),
            rect,
            size,
            color,
            FontWeight::Semibold,
            0.0,
            TextAlign::End,
        );
        ui.edit_underline(rect, color, scale);
    }
}
