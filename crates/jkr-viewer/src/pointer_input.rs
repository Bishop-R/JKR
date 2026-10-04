//! Platform pointer conversion, overlay routing, and cursor-grab policy.

use super::*;
use jkr_ui::{InputEvent, PointerButton, UiEventKind, Vec2};
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::window::CursorGrabMode;

/// Mouse-look settings latched from `sensitivity` and `m_invert` each frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MouseLook {
    /// Scales both axes (`cl_sensitivity`, `cl_input.cpp:1155-1159`).
    pub(crate) sensitivity: f32,
    /// Degrees of yaw per mouse count before sensitivity (`m_yaw`).
    pub(crate) yaw_scale: f32,
    /// Degrees of pitch per mouse count before sensitivity (`m_pitch`).
    pub(crate) pitch_scale: f32,
    /// Pull down to look up.
    pub(crate) invert: bool,
}

impl Default for MouseLook {
    fn default() -> Self {
        Self {
            sensitivity: 5.0,
            yaw_scale: 0.022,
            pitch_scale: 0.022,
            invert: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CursorMode {
    Free,
    Captured,
}

/// Cursor policy updates the OS cursor only when its desired state changes.
pub(crate) struct CursorPolicy {
    focused: bool,
    applied: Option<CursorMode>,
    ignore_next_motion: bool,
}

impl CursorPolicy {
    pub(crate) const fn new() -> Self {
        Self {
            focused: true,
            applied: None,
            ignore_next_motion: false,
        }
    }

    pub(crate) const fn desired(&self, live: bool, overlay: bool) -> CursorMode {
        if self.focused && live && !overlay {
            CursorMode::Captured
        } else {
            CursorMode::Free
        }
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn take_ignored_motion(&mut self) -> bool {
        std::mem::take(&mut self.ignore_next_motion)
    }

    fn finish_transition(&mut self, desired: CursorMode, was_captured: bool, captured: bool) {
        if was_captured && !captured {
            self.ignore_next_motion = true;
        }
        self.applied = Some(desired);
    }
}

impl GpuState {
    pub(crate) fn pointer_moved(&mut self, position: PhysicalPosition<f64>) {
        self.cursor_position = [position.x as f32, position.y as f32];
        self.route_pointer(InputEvent::PointerMove(Vec2::new(
            self.cursor_position[0],
            self.cursor_position[1],
        )));
    }

    pub(crate) fn pointer_left(&mut self) {
        self.route_pointer(InputEvent::PointerLeave);
    }

    pub(crate) fn pointer_button(&mut self, button: MouseButton, state: ElementState) {
        if state == ElementState::Pressed
            && self
                .client_menu
                .as_ref()
                .is_some_and(|menu| menu.is_visible())
            && match (&mut self.client_menu, &mut self.console) {
                (Some(menu), Some(console)) => menu.handle_mouse_binding(button, console),
                _ => false,
            }
        {
            return;
        }
        let Some(event) = normalize_button_event(button, state, self.cursor_position) else {
            if self.pointer_captured {
                self.route_stock_mouse_button(button, state == ElementState::Pressed);
            }
            return;
        };
        let button = normalize_button(button).expect("normalized event has a supported button");
        if self.route_pointer(event) {
            return;
        }
        if button == PointerButton::Primary && state == ElementState::Pressed {
            if !self.pointer_captured {
                self.capture_pointer();
                return;
            }
        }
        self.route_stock_mouse_button(
            match button {
                PointerButton::Primary => MouseButton::Left,
                PointerButton::Secondary => MouseButton::Right,
                PointerButton::Middle => MouseButton::Middle,
            },
            state == ElementState::Pressed,
        );
    }

    pub(crate) fn pointer_wheel(&mut self, delta: MouseScrollDelta) {
        let delta = normalize_wheel(delta);
        let event = InputEvent::PointerWheel {
            position: Vec2::new(self.cursor_position[0], self.cursor_position[1]),
            delta,
        };
        if self.route_pointer(event) {
            return;
        }
        self.route_gameplay_wheel(delta.y);
    }

    pub(crate) fn pointer_focus(&mut self, focused: bool) {
        self.cursor_policy.set_focus(focused);
        self.gameplay_input.focus(focused);
        if !focused {
            self.pending_generic_command = 0;
            self.network_command_due = std::time::Instant::now();
        }
        self.sync_cursor_policy();
    }

    pub(crate) fn pointer_motion(&mut self, delta: (f64, f64)) {
        if !self.pointer_captured || self.cursor_policy.take_ignored_motion() {
            return;
        }
        // Accumulate raw counts; filtering and acceleration run once per command frame.
        self.gameplay_input.motion.raw[0] += delta.0 as f32;
        self.gameplay_input.motion.raw[1] += delta.1 as f32;
    }

    pub(crate) fn capture_pointer(&mut self) {
        self.sync_cursor_policy();
    }

    pub(crate) fn release_pointer(&mut self) {
        self.apply_cursor_mode(CursorMode::Free);
        self.gameplay_input.clear();
    }

    /// Keyboard catchers also advertise BUTTON_TALK, as CL_CmdButtons does.
    pub(crate) fn key_catcher_active(&self) -> bool {
        self.game_menu
            || self.chat.is_typing()
            || self
                .console
                .as_ref()
                .is_some_and(|console| console.is_open())
            || self
                .client_menu
                .as_ref()
                .is_some_and(|menu| menu.is_visible())
    }

    pub(crate) fn sync_cursor_policy(&mut self) {
        let overlay = self.key_catcher_active();
        let desired = self.cursor_policy.desired(
            self.live_session.is_some() || self.resident.exploring(),
            overlay,
        );
        self.apply_cursor_mode(desired);
    }

    fn apply_cursor_mode(&mut self, desired: CursorMode) {
        if self.cursor_policy.applied == Some(desired) {
            return;
        }
        let Some(window) = &self.window else {
            self.pointer_captured = false;
            self.cursor_policy.finish_transition(desired, false, false);
            return;
        };
        let was_captured = self.pointer_captured;
        self.pointer_captured = match desired {
            CursorMode::Free => {
                let _ = window.set_cursor_grab(CursorGrabMode::None);
                window.set_cursor_visible(true);
                false
            }
            CursorMode::Captured => {
                let captured = window
                    .set_cursor_grab(CursorGrabMode::Locked)
                    .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
                    .is_ok();
                window.set_cursor_visible(!captured);
                captured
            }
        };
        self.cursor_policy
            .finish_transition(desired, was_captured, self.pointer_captured);
    }

    fn route_pointer(&mut self, event: InputEvent) -> bool {
        if self
            .console
            .as_ref()
            .is_some_and(|console| console.is_open())
        {
            if let Some(console) = &mut self.console {
                console.handle_pointer(event);
            }
            return true;
        }
        if self
            .client_menu
            .as_ref()
            .is_some_and(|menu| menu.is_visible())
        {
            let action = match (&mut self.client_menu, &mut self.console) {
                (Some(menu), Some(console)) => menu.handle_pointer(event, console),
                _ => menu::MenuAction::None,
            };
            self.apply_client_menu_action(action);
            return true;
        }
        if self.game_menu {
            if self.game_menu_page == GameMenuPage::Shot {
                self.shot_pointer(event);
                return true;
            }
            if let Some((kind, row)) = self.in_game_menu.pointer(event) {
                if let Some(tab) = crate::ingame_menu::classic::bar_tab(row) {
                    self.classic_bar_pointer(kind, tab);
                    return true;
                }
                if row < self.game_menu_row_count()
                    && matches!(kind, UiEventKind::HoverEnter | UiEventKind::Hover)
                {
                    self.game_menu_row = row;
                }
                if row < self.game_menu_row_count()
                    && kind == UiEventKind::Activate
                    && self.in_game_menu.activation_allowed(row)
                {
                    self.game_menu_row = row;
                    self.activate_game_menu_row();
                }
            }
            return true;
        }
        if self.chat.is_typing() {
            self.chat.pointer(event);
            return true;
        }
        false
    }

    fn route_stock_mouse_button(&mut self, button: MouseButton, pressed: bool) {
        let Some(key) = input::keys::name(input::keys::Source::Mouse(button)) else {
            return;
        };
        if let Some(console) = &mut self.console {
            console.queue_bound_script(key, pressed);
        }
    }

    fn route_gameplay_wheel(&mut self, vertical: f32) {
        if self.live_session.is_none() || self.chat.is_typing() || self.game_menu {
            return;
        }
        if vertical == 0.0 {
            return;
        }
        let key = input::keys::name(input::keys::Source::Wheel(vertical > 0.0)).unwrap();
        if let Some(console) = &mut self.console {
            console.queue_bound_script(key, true);
            console.queue_bound_script(key, false);
        }
    }
}

fn normalize_button(button: MouseButton) -> Option<PointerButton> {
    match button {
        MouseButton::Left => Some(PointerButton::Primary),
        MouseButton::Right => Some(PointerButton::Secondary),
        MouseButton::Middle => Some(PointerButton::Middle),
        _ => None,
    }
}

pub(crate) fn normalize_button_event(
    button: MouseButton,
    state: ElementState,
    position: [f32; 2],
) -> Option<InputEvent> {
    let button = normalize_button(button)?;
    let position = Vec2::new(position[0], position[1]);
    Some(match state {
        ElementState::Pressed => InputEvent::PointerPress { position, button },
        ElementState::Released => InputEvent::PointerRelease { position, button },
    })
}

fn normalize_wheel(delta: MouseScrollDelta) -> Vec2 {
    match delta {
        MouseScrollDelta::LineDelta(x, y) => Vec2::new(x * 40.0, y * 40.0),
        MouseScrollDelta::PixelDelta(position) => Vec2::new(position.x as f32, position.y as f32),
    }
}
