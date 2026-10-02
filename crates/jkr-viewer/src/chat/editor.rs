//! Bounded UTF-8 text editing, independent of platform keyboard events.

use super::Channel;
use crate::input::dead_key::{DeadKey, TypingField};
use jkr_client::{CHAT_INPUT_BYTES, ChatTarget};
use std::ops::Range;
use winit::keyboard::{Key, KeyCode};

pub(super) struct Editor {
    pub(super) text: String,
    pub(super) cursor: usize,
    pub(super) channel: Channel,
    pub(super) recipient: Option<ChatTarget>,
    /// Dead key shown at the caret until its composition arrives.
    pub(super) dead: DeadKey,
}

impl Editor {
    pub(super) fn new(channel: Channel) -> Self {
        Self {
            text: String::with_capacity(CHAT_INPUT_BYTES),
            cursor: 0,
            channel,
            recipient: None,
            dead: DeadKey::default(),
        }
    }

    /// Type a pressed character key, showing a dead key until it composes.
    pub(super) fn type_key(&mut self, logical: &Key, text: Option<&str>) {
        let mut dead = self.dead;
        dead.type_key(self, logical, text);
        self.dead = dead;
    }

    pub(super) fn insert(&mut self, value: &str) {
        for c in value.chars().filter(|c| !c.is_control()) {
            if self.text.len() + c.len_utf8() > CHAT_INPUT_BYTES {
                break;
            }
            self.text.insert(self.cursor, c);
            self.cursor += c.len_utf8();
        }
    }

    pub(super) fn key(&mut self, key: KeyCode) -> bool {
        match key {
            KeyCode::ArrowLeft => self.cursor = self.previous(),
            KeyCode::ArrowRight => self.cursor = self.next(),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.text.len(),
            KeyCode::Backspace if self.cursor > 0 => {
                let start = self.previous();
                self.text.drain(start..self.cursor);
                self.cursor = start;
            }
            KeyCode::Delete if self.cursor < self.text.len() => {
                self.text.drain(self.cursor..self.next());
            }
            KeyCode::Backspace | KeyCode::Delete => {}
            _ => return false,
        }
        true
    }

    fn previous(&self) -> usize {
        self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    fn next(&self) -> usize {
        self.text[self.cursor..]
            .chars()
            .next()
            .map_or(self.cursor, |c| self.cursor + c.len_utf8())
    }
}

impl TypingField for Editor {
    fn line(&self) -> &str {
        &self.text
    }

    fn caret(&self) -> usize {
        self.cursor
    }

    fn insert(&mut self, text: &str) {
        Editor::insert(self, text);
    }

    fn remove(&mut self, range: Range<usize>) {
        if self.cursor >= range.end {
            self.cursor -= range.len();
        } else if self.cursor > range.start {
            self.cursor = range.start;
        }
        self.text.drain(range);
    }
}
