//! Chat drafts use the console's UTF-8 caret, selection and word editing rules.

use super::Channel;
use crate::console::line_edit::{LineEdit, Motion};
use jkr_client::{CHAT_INPUT_BYTES, ChatTarget};
use winit::keyboard::KeyCode;

pub(super) struct Editor {
    pub(super) text: String,
    pub(super) edit: LineEdit,
    pub(super) channel: Channel,
    pub(super) recipient: Option<ChatTarget>,
    pub(super) layout: super::editing::DraftLayout,
}

impl Editor {
    pub(super) fn new(channel: Channel) -> Self {
        Self {
            text: String::with_capacity(CHAT_INPUT_BYTES),
            edit: LineEdit::default(),
            channel,
            recipient: None,
            layout: super::editing::DraftLayout::default(),
        }
    }

    pub(super) fn insert(&mut self, value: &str) {
        self.edit.insert(&mut self.text, value, CHAT_INPUT_BYTES);
    }

    pub(super) fn key(&mut self, key: KeyCode, control: bool, shift: bool) -> bool {
        let left = if control {
            Motion::WordLeft
        } else {
            Motion::Left
        };
        let right = if control {
            Motion::WordRight
        } else {
            Motion::Right
        };
        match key {
            KeyCode::ArrowLeft => self.edit.motion(&self.text, left, shift),
            KeyCode::ArrowRight => self.edit.motion(&self.text, right, shift),
            KeyCode::Home => self.edit.motion(&self.text, Motion::Home, shift),
            KeyCode::End => self.edit.motion(&self.text, Motion::End, shift),
            KeyCode::Backspace => self.edit.delete(&mut self.text, left),
            KeyCode::Delete => self.edit.delete(&mut self.text, right),
            _ => return false,
        }
        true
    }
}
