//! Keyboard and pointer actions for the floating conversation layer.

use super::*;
use jkr_client::{ChatDestination, chat_command};
use jkr_ui::{InputEvent, PointerButton, UiEventKind};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, KeyCode, PhysicalKey};

pub(super) const GLOBAL: u16 = 100;
pub(super) const TEAM: u16 = 101;
pub(super) const LATEST: u16 = 102;
pub(super) const WHISPER: u16 = 110;
pub(super) const MUTE: u16 = 111;

/// Keys the composer acts on itself rather than typing their text.
fn handled_key(key: KeyCode) -> bool {
    matches!(
        key,
        KeyCode::Escape
            | KeyCode::Enter
            | KeyCode::NumpadEnter
            | KeyCode::Tab
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::ArrowUp
            | KeyCode::ArrowDown
            | KeyCode::ArrowLeft
            | KeyCode::ArrowRight
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::Backspace
            | KeyCode::Delete
    )
}

impl ChatOverlay {
    pub(crate) fn handle_key(&mut self, event: &KeyEvent) -> ChatInputResult {
        if event.state != ElementState::Pressed {
            return ChatInputResult::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return ChatInputResult::None;
        };
        self.edit_key(key, &event.logical_key, event.text.as_deref())
    }

    pub(super) fn edit_key(
        &mut self,
        key: KeyCode,
        logical: &Key,
        text: Option<&str>,
    ) -> ChatInputResult {
        let Some(input) = &mut self.input else {
            return ChatInputResult::None;
        };
        if handled_key(key) {
            input.dead.other_key(text);
        }
        if let Some(menu) = &mut self.player_menu {
            match key {
                KeyCode::Escape => {
                    self.player_menu = None;
                    return ChatInputResult::None;
                }
                KeyCode::ArrowUp | KeyCode::ArrowDown | KeyCode::Tab => {
                    menu.selected = if menu.selected == WHISPER {
                        MUTE
                    } else {
                        WHISPER
                    };
                    return ChatInputResult::None;
                }
                KeyCode::Enter | KeyCode::NumpadEnter => {
                    let token = menu.selected;
                    self.activate(token);
                    return ChatInputResult::None;
                }
                _ => self.player_menu = None,
            }
        }
        match key {
            KeyCode::Escape => {
                self.input = None;
                self.scroll = 0;
                self.unread = 0;
            }
            KeyCode::Enter | KeyCode::NumpadEnter => return self.submit(),
            KeyCode::PageUp | KeyCode::ArrowUp => self.scroll_by(3),
            KeyCode::PageDown | KeyCode::ArrowDown => self.scroll_by(-3),
            KeyCode::Tab => {
                let channel = self.input.as_ref().expect("active input").channel;
                self.activate(if channel == Channel::Global {
                    TEAM
                } else {
                    GLOBAL
                });
            }
            _ => {
                let input = self.input.as_mut().expect("active input");
                if !input.key(key) {
                    input.type_key(logical, text);
                }
            }
        }
        ChatInputResult::None
    }

    fn submit(&mut self) -> ChatInputResult {
        let input = self.input.as_ref().expect("active input");
        let destination = match input.channel {
            Channel::Global => ChatDestination::Global,
            Channel::Team => ChatDestination::Team,
            Channel::Whisper => {
                let Some(target) = input
                    .recipient
                    .filter(|target| self.roster.name(*target).is_some())
                else {
                    self.notice = "Player left or changed identity. Choose a name again.";
                    return ChatInputResult::None;
                };
                ChatDestination::Player(target.slot())
            }
        };
        let result = chat_command(destination, &input.text);
        self.input = None;
        self.scroll = 0;
        self.unread = 0;
        result.map_or(ChatInputResult::None, ChatInputResult::Submit)
    }

    pub(crate) fn pointer(&mut self, event: InputEvent) {
        if !self.is_typing() {
            return;
        }
        if let InputEvent::PointerPress {
            position,
            button: PointerButton::Primary,
        } = event
        {
            // Match the canvas's reverse paint order: popovers cover the
            // composer, which covers the feed. Otherwise release activates a
            // different widget than the one recorded here and is discarded.
            self.pressed_action = [MUTE, WHISPER, LATEST, TEAM, GLOBAL]
                .into_iter()
                .chain(0..MAX_VISIBLE as u16)
                .find(|token| {
                    self.ui
                        .rect_for(*token)
                        .is_some_and(|rect| rect.contains(position))
                })
                .map(|token| {
                    (
                        token,
                        self.visible_targets
                            .get(usize::from(token))
                            .copied()
                            .flatten(),
                    )
                });
        }
        if let InputEvent::PointerWheel { delta, .. } = event {
            self.player_menu = None;
            if delta.y != 0.0 {
                self.scroll_by(if delta.y > 0.0 { 1 } else { -1 });
            }
            return;
        }
        let result = self.ui.pointer(event);
        if let Some(result) = result
            && result.kind == UiEventKind::Activate
            && let Some(token) = result.token
        {
            let target = self
                .visible_targets
                .get(usize::from(token))
                .copied()
                .flatten();
            // A new message can rebuild the name widgets between press and
            // release. Never reinterpret the old press as a different sender.
            if self.pressed_action.take() != Some((token, target)) {
                return;
            }
            if usize::from(token) < self.visible_targets.len() {
                if let Some(target) = self.visible_targets[usize::from(token)]
                    .filter(|target| self.roster.name(*target).is_some())
                {
                    let rect = self.ui.rect_for(token).unwrap_or_default();
                    self.player_menu = Some(PlayerMenu {
                        target,
                        origin: [rect.x, rect.bottom() + 8.0],
                        selected: WHISPER,
                    });
                }
            } else {
                self.activate(token);
            }
        }
        if let InputEvent::PointerPress {
            position,
            button: PointerButton::Primary,
        } = event
            && self.player_menu.is_some()
            && ![WHISPER, MUTE].iter().any(|token| {
                self.ui
                    .rect_for(*token)
                    .is_some_and(|r| r.contains(position))
            })
        {
            self.player_menu = None;
        }
    }

    fn scroll_by(&mut self, delta: isize) {
        self.scroll = self
            .scroll
            .saturating_add_signed(delta)
            .min(self.lines.len().saturating_sub(1));
        if self.scroll == 0 {
            self.unread = 0;
        }
        for line in &mut self.lines {
            line.y = None;
        }
    }

    pub(super) fn activate(&mut self, token: u16) {
        match token {
            GLOBAL | TEAM => {
                if let Some(input) = &mut self.input {
                    input.channel = if token == GLOBAL {
                        Channel::Global
                    } else {
                        Channel::Team
                    };
                    input.recipient = None;
                    self.notice = "";
                }
                self.player_menu = None;
            }
            LATEST => {
                self.scroll = 0;
                self.unread = 0;
                self.player_menu = None;
            }
            WHISPER | MUTE => {
                let Some(menu) = self.player_menu.take() else {
                    return;
                };
                if self.roster.name(menu.target).is_none() {
                    self.notice = "Player is no longer available.";
                    return;
                }
                if token == WHISPER {
                    if let Some(input) = &mut self.input {
                        input.channel = Channel::Whisper;
                        input.recipient = Some(menu.target);
                        self.notice = "";
                    }
                } else {
                    let muted = if let Some(index) =
                        self.muted.iter().position(|target| *target == menu.target)
                    {
                        self.muted.remove(index);
                        false
                    } else if self.muted.len() < HISTORY_LIMIT {
                        self.muted.push(menu.target);
                        true
                    } else {
                        return;
                    };
                    for line in &mut self.lines {
                        if line.sender == Some(menu.target) {
                            line.muted = muted;
                        }
                    }
                    self.notice = if muted {
                        "Muted here. Click their name again to unmute."
                    } else {
                        "Player unmuted."
                    };
                }
            }
            _ => {}
        }
    }
}

impl crate::GpuState {
    /// Refresh recipient identity at submission time and use the existing reliable
    /// channel. No pointer action sends traffic or enters the gameplay bind path.
    pub(crate) fn chat_key(&mut self, event: &KeyEvent) {
        if let Some(session) = &self.live_session {
            self.chat.update_roster(session.game_state());
        }
        if let ChatInputResult::Submit(command) = self.chat.handle_key(event) {
            let command = self.console.as_ref().map_or_else(
                || command.clone(),
                |console| console.color_chat_command(&command),
            );
            if let Some(session) = &mut self.live_session
                && let Err(error) = session.send_reliable_command(command.as_bytes())
            {
                eprintln!("failed to send chat: {error}");
            }
        }
        self.sync_cursor_policy();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::{NamedKey, SmolStr};

    fn press(chat: &mut ChatOverlay, key: KeyCode, logical: Key, text: Option<&str>) {
        chat.edit_key(key, &logical, text);
    }

    fn typed(chat: &mut ChatOverlay, key: KeyCode, text: &str) {
        press(chat, key, Key::Character(SmolStr::new(text)), Some(text));
    }

    fn dead_caret(chat: &mut ChatOverlay) {
        press(chat, KeyCode::BracketLeft, Key::Dead(Some('^')), None);
    }

    fn draft(chat: &ChatOverlay) -> &str {
        &chat.input.as_ref().expect("composer open").text
    }

    #[test]
    fn azerty_dead_caret_and_digit_type_a_colour_code() {
        let mut chat = ChatOverlay::new();
        chat.open(false);
        dead_caret(&mut chat);
        assert_eq!(draft(&chat), "^");
        press(
            &mut chat,
            KeyCode::ShiftLeft,
            Key::Named(NamedKey::Shift),
            None,
        );
        typed(&mut chat, KeyCode::Digit1, "^1");
        typed(&mut chat, KeyCode::KeyH, "h");
        assert_eq!(draft(&chat), "^1h");
    }

    #[test]
    fn dead_caret_is_replaced_at_the_caret_inside_the_draft() {
        let mut chat = ChatOverlay::new();
        chat.open(false);
        typed(&mut chat, KeyCode::KeyA, "a");
        typed(&mut chat, KeyCode::KeyB, "b");
        press(
            &mut chat,
            KeyCode::ArrowLeft,
            Key::Named(NamedKey::ArrowLeft),
            None,
        );
        dead_caret(&mut chat);
        assert_eq!(draft(&chat), "a^b");
        typed(&mut chat, KeyCode::Digit2, "^2");
        assert_eq!(draft(&chat), "a^2b");
        assert_eq!(chat.input.as_ref().unwrap().cursor, 3);
    }

    #[test]
    fn backspace_after_a_dead_caret_erases_only_the_caret() {
        let mut chat = ChatOverlay::new();
        chat.open(false);
        typed(&mut chat, KeyCode::KeyA, "a");
        dead_caret(&mut chat);
        press(
            &mut chat,
            KeyCode::Backspace,
            Key::Named(NamedKey::Backspace),
            Some("^\u{8}"),
        );
        assert_eq!(draft(&chat), "a");
        typed(&mut chat, KeyCode::Digit1, "1");
        assert_eq!(draft(&chat), "a1");
    }
}
