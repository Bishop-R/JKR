//! Keyboard and pointer actions for the floating conversation layer.

use super::player_actions::{ACTIONS, MENU_BACK};
use super::*;
use jkr_client::{ChatDestination, chat_command};
use jkr_ui::{InputEvent, PointerButton, UiEventKind};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

pub(super) const GLOBAL: u16 = 100;
pub(super) const TEAM: u16 = 101;
pub(super) const LATEST: u16 = 102;

impl ChatOverlay {
    pub(crate) fn handle_key(&mut self, event: &KeyEvent) -> ChatInputResult {
        if event.state != ElementState::Pressed {
            return ChatInputResult::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return ChatInputResult::None;
        };
        if event.logical_key == winit::keyboard::Key::Dead(Some('^')) {
            if let Some(input) = &mut self.input {
                input.insert("^");
            }
            self.player_menu = None;
            return ChatInputResult::None;
        }
        if self.edit_shortcut(event) {
            return ChatInputResult::None;
        }
        self.edit_key(key, event.text.as_deref())
    }

    pub(super) fn edit_key(&mut self, key: KeyCode, text: Option<&str>) -> ChatInputResult {
        if self.input.is_none() {
            return ChatInputResult::None;
        }
        if let Some(menu) = &mut self.player_menu {
            match key {
                KeyCode::Escape => {
                    self.player_menu = None;
                    return ChatInputResult::None;
                }
                KeyCode::ArrowUp | KeyCode::ArrowDown | KeyCode::Tab => {
                    let previous = key == KeyCode::ArrowUp
                        || (key == KeyCode::Tab && self.modifiers.shift_key());
                    let index = menu
                        .selected
                        .and_then(|selected| ACTIONS.iter().position(|token| *token == selected));
                    let next = match index {
                        Some(index) if previous => (index + ACTIONS.len() - 1) % ACTIONS.len(),
                        Some(index) => (index + 1) % ACTIONS.len(),
                        None if previous => ACTIONS.len() - 1,
                        None => 0,
                    };
                    menu.selected = Some(ACTIONS[next]);
                    return ChatInputResult::None;
                }
                KeyCode::Enter | KeyCode::NumpadEnter => {
                    if let Some(token) = menu.selected {
                        self.activate(token);
                    }
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
                if !input.key(
                    key,
                    self.modifiers.control_key(),
                    self.modifiers.shift_key(),
                ) && let Some(text) = text
                {
                    if !self.modifiers.control_key() || self.modifiers.alt_key() {
                        input.insert(text);
                    }
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
        if matches!(
            event,
            InputEvent::PointerMove(_) | InputEvent::PointerLeave | InputEvent::PointerPress { .. }
        ) && let Some(menu) = &mut self.player_menu
        {
            menu.selected = None;
        }
        if self.edit_pointer(event) {
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
            self.pressed_action = ACTIONS
                .into_iter()
                .rev()
                .chain([MENU_BACK, LATEST, TEAM, GLOBAL])
                .chain(0..MAX_VISIBLE as u16)
                .find(|token| {
                    self.ui
                        .rect_for(*token)
                        .is_some_and(|rect| rect.contains(position))
                })
                .map(|token| (token, self.action_target(token)));
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
            let target = self.action_target(token);
            // A new message can rebuild the name widgets between press and
            // release. Never reinterpret the old press as a different sender.
            if self.pressed_action.take() != Some((token, target)) {
                return;
            }
            if usize::from(token) < self.visible_targets.len() {
                if let Some(target) = self.visible_targets[usize::from(token)]
                    .filter(|target| self.roster.name(*target).is_some())
                {
                    self.player_menu = Some(PlayerMenu {
                        target,
                        anchor_y: self.ui.rect_for(token).map_or(0.0, |rect| rect.y),
                        selected: None,
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
            && !self
                .ui
                .rect_for(MENU_BACK)
                .is_some_and(|r| r.contains(position))
        {
            self.player_menu = None;
        }
    }

    fn action_target(&self, token: u16) -> Option<ChatTarget> {
        if ACTIONS.contains(&token) || token == MENU_BACK {
            self.player_menu.map(|menu| menu.target)
        } else {
            self.visible_targets
                .get(usize::from(token))
                .copied()
                .flatten()
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
            token if ACTIONS.contains(&token) => self.player_action(token),
            _ => {}
        }
    }
}

impl crate::GpuState {
    /// Refresh recipient identity at submission time and use the existing reliable
    /// channel. No pointer action sends traffic or enters the gameplay bind path.
    pub(crate) fn chat_key(&mut self, event: &KeyEvent) {
        if let Some(session) = self
            .resident
            .session
            .as_ref()
            .or(self.live_session.as_ref())
        {
            self.chat.update_roster(session.game_state());
        }
        if event.state == ElementState::Pressed
            && event.logical_key == winit::keyboard::Key::Dead(Some('^'))
            && let Some(window) = &self.window
        {
            window.reset_dead_keys();
        }
        if let ChatInputResult::Submit(command) = self.chat.handle_key(event) {
            let command = self.console.as_ref().map_or_else(
                || command.clone(),
                |console| console.color_chat_command(&command),
            );
            self.send_chat_command(&command);
        }
        self.sync_cursor_policy();
    }

    /// Composer and console messages share the real server's reliable channel.
    pub(crate) fn send_chat_command(&mut self, command: &str) {
        if let Some(session) = self.communication_session_mut()
            && let Err(error) = session.send_reliable_command(command.as_bytes())
        {
            eprintln!("failed to send chat: {error}");
        }
    }
}
