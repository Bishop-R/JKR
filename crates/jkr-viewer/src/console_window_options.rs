//! Window events and cached focus-dependent frame caps; no simulation policy.
use super::console_cvars::IntegerSetting;
use super::*;

/// Window preferences retained across world installs with the console.
pub(super) struct Options {
    unfocused_cap: IntegerSetting,
    minimized_cap: IntegerSetting,
    unfocused: bool,
    minimized: bool,
    alt: bool,
    applied: Option<(bool, bool)>,
    attention: bool,
}

impl Options {
    /// Register state separately from archived preferences before config loading.
    pub(super) fn register(cvars: &mut CvarRegistry) -> Result<Self, jkr_shell::CvarError> {
        for (name, default, help) in [
            (
                "com_maxfpsUnfocused",
                0_i64,
                "Unfocused frame cap; zero uses the normal cap",
            ),
            (
                "com_maxfpsMinimized",
                50,
                "Minimized frame cap; zero uses the normal cap",
            ),
        ] {
            cvars.register(CvarDefinition::new(name, default, CvarFlags::ARCHIVE, help))?;
        }
        for (name, default, help) in [
            (
                "com_unfocused",
                false,
                "Native window has no keyboard focus",
            ),
            ("com_minimized", false, "Native window is minimized"),
        ] {
            cvars.register(CvarDefinition::new(
                name,
                default,
                CvarFlags::READ_ONLY,
                help,
            ))?;
        }
        for (name, default, help) in [
            ("cl_allowAltEnter", true, "Alt+Enter toggles fullscreen"),
            ("r_noborder", false, "Hide window decorations"),
            ("r_centerWindow", false, "Center window on its monitor"),
            (
                "con_notifyconnect",
                false,
                "Request attention for server connection notices",
            ),
            ("con_notifyvote", true, "Request attention for vote notices"),
        ] {
            cvars.register(CvarDefinition::new(name, default, CvarFlags::ARCHIVE, help))?;
        }
        cvars.register(CvarDefinition::new(
            "con_notifywords",
            "0",
            CvarFlags::ARCHIVE,
            "Attention words (space separated); -1 any chat, 0 disabled",
        ))?;
        Ok(Self {
            unfocused_cap: IntegerSetting::bind(cvars, "com_maxfpsUnfocused", 0)?,
            minimized_cap: IntegerSetting::bind(cvars, "com_maxfpsMinimized", 50)?,
            unfocused: false,
            minimized: false,
            alt: false,
            applied: None,
            attention: false,
        })
    }
}

impl ViewerConsole {
    /// Publish focus/minimize events without persisting engine state.
    pub(crate) fn window_state(&mut self, focused: Option<bool>, minimized: Option<bool>) {
        if let Some(focused) = focused {
            self.window_options.unfocused = !focused;
            if !focused {
                self.window_options.alt = false;
            }
            self.window_options.attention = false;
            let _ = self
                .shell
                .cvars
                .restore_text("com_unfocused", if focused { "0" } else { "1" });
        }
        if let Some(minimized) = minimized {
            self.window_options.minimized = minimized;
            let _ = self
                .shell
                .cvars
                .restore_text("com_minimized", if minimized { "1" } else { "0" });
        }
    }

    /// Stock common.cpp:1528-1533 priority, using callbacks rather than frame lookups.
    pub(crate) fn window_fps(&self, normal: u32) -> u32 {
        let options = &self.window_options;
        let cap = if options.minimized && options.minimized_cap.value() > 0 {
            options.minimized_cap.value()
        } else if options.unfocused && options.unfocused_cap.value() > 0 {
            options.unfocused_cap.value()
        } else {
            return normal;
        };
        cap.min(i64::from(u32::MAX)) as u32
    }

    /// Update modifier state from the same native event used by console shortcuts.
    pub(crate) fn window_alt(&mut self, alt: bool) {
        self.window_options.alt = alt;
    }

    /// Consume Alt+Enter before it reaches gameplay bindings, ignoring key repeat.
    pub(crate) fn window_key(&mut self, event: &winit::event::KeyEvent) -> bool {
        use winit::event::ElementState;
        use winit::keyboard::{KeyCode, PhysicalKey};
        self.alt_enter(
            event.physical_key == PhysicalKey::Code(KeyCode::Enter),
            event.state == ElementState::Pressed,
            event.repeat,
        )
    }

    fn alt_enter(&mut self, enter: bool, pressed: bool, repeat: bool) -> bool {
        if !enter || !self.window_options.alt || !self.bool_cvar("cl_allowaltenter").unwrap_or(true)
        {
            return false;
        }
        if pressed && !repeat {
            let fullscreen = self.bool_cvar("r_fullscreen").unwrap_or(false);
            self.set_cvar("r_fullscreen", if fullscreen { "0" } else { "1" });
        }
        true
    }

    /// Apply changed decoration/centering preferences, never reposition every frame.
    pub(crate) fn apply_window_options(&mut self, window: &winit::window::Window) {
        if std::mem::take(&mut self.window_options.attention) {
            window.request_user_attention(Some(winit::window::UserAttentionType::Informational));
        }
        let (borderless, center) = self.window_preferences();
        if self.window_options.applied == Some((borderless, center)) {
            return;
        }
        window.set_decorations(!borderless);
        if center && window.fullscreen().is_none() {
            if let Some(monitor) = window.current_monitor() {
                let origin = monitor.position();
                let screen = monitor.size();
                let size = window.outer_size();
                window.set_outer_position(winit::dpi::PhysicalPosition::new(
                    origin.x + (i64::from(screen.width) - i64::from(size.width)) as i32 / 2,
                    origin.y + (i64::from(screen.height) - i64::from(size.height)) as i32 / 2,
                ));
            }
        }
        self.window_options.applied = Some((borderless, center));
    }

    fn window_preferences(&self) -> (bool, bool) {
        (
            self.bool_cvar("r_noborder").unwrap_or(false),
            self.bool_cvar("r_centerwindow").unwrap_or(false),
        )
    }

    /// Consume raw server text before localization removes notification tokens.
    pub(crate) fn notify_server_text(&mut self, kind: &jkr_client::ServerEventKind, text: &str) {
        use jkr_client::ServerEventKind;
        let alert = match kind {
            ServerEventKind::Print => {
                (self.bool_cvar("con_notifyconnect").unwrap_or(false)
                    && [
                        "@@@PLCONNECT",
                        "@@@DISCONNECT",
                        "@@@WAS_KICKED",
                        "timed out",
                    ]
                    .iter()
                    .any(|token| contains_ascii(text, token)))
                    || (self.bool_cvar("con_notifyvote").unwrap_or(true)
                        && contains_ascii(text, "@@@PLCALLEDVOTE"))
            }
            ServerEventKind::Chat | ServerEventKind::TeamChat => {
                let words = self.text_value("con_notifywords").unwrap_or("0");
                words == "-1"
                    || (words != "0"
                        && text.rsplit_once(':').is_some_and(|(_, body)| {
                            words
                                .split_ascii_whitespace()
                                .take(8)
                                .any(|word| contains_ascii(body, word))
                        }))
            }
            _ => false,
        };
        self.window_options.attention |= alert && self.window_options.unfocused;
    }
}

fn contains_ascii(text: &str, word: &str) -> bool {
    !word.is_empty()
        && text
            .as_bytes()
            .windows(word.len())
            .any(|part| part.eq_ignore_ascii_case(word.as_bytes()))
}
