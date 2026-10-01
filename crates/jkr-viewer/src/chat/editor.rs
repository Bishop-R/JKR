//! Bounded UTF-8 text editing, independent of platform keyboard events.

use super::Channel;
use jkr_client::{CHAT_INPUT_BYTES, ChatTarget};
use winit::keyboard::KeyCode;

pub(super) struct Editor {
    pub(super) text: String,
    pub(super) cursor: usize,
    pub(super) channel: Channel,
    pub(super) recipient: Option<ChatTarget>,
}

impl Editor {
    pub(super) fn new(channel: Channel) -> Self {
        Self {
            text: String::with_capacity(CHAT_INPUT_BYTES),
            cursor: 0,
            channel,
            recipient: None,
        }
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
