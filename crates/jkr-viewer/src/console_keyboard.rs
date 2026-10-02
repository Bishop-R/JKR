//! Console keyboard capture and editing.
use super::*;

/// Does `text` (the characters this key press produced) appear in a
/// `cl_consoleKeys` list? Entries are literal characters or `0x` hex
/// codepoints (`cl_main.cpp:2826`).
pub(crate) fn is_console_key(list: &str, text: &str) -> bool {
    let mut characters = text.chars();
    let (Some(typed), None) = (characters.next(), characters.next()) else {
        return false;
    };
    list.split_whitespace().any(|entry| {
        match entry
            .strip_prefix("0x")
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
            .and_then(char::from_u32)
        {
            Some(code) => code == typed,
            None => {
                let mut entry = entry.chars();
                (entry.next(), entry.next()) == (Some(typed), None)
            }
        }
    })
}

impl ViewerConsole {
    /// Stock console keys: a `cl_consoleKeys` character, Shift+Escape
    /// (`cl_keys.cpp:1318`), or a key the user explicitly bound to
    /// `toggleconsole`. The physical key is not bound by default, so layouts
    /// where it types `^` keep that character for colour codes.
    pub(super) fn configured_console_key(&self, key: KeyCode, text: Option<&str>) -> bool {
        if key == KeyCode::Escape {
            return self.shift;
        }
        let native = self.integer_cvar("cl_consoleusescancode").unwrap_or(0) != 0;
        if native
            && key == KeyCode::Backquote
            && super::client_options::native_console(
                self.integer_cvar("cl_consoleshiftrequirement").unwrap_or(0),
                self.shift,
                self.open,
            )
        {
            return true;
        }
        let list = self
            .shell
            .cvars
            .get("cl_consoleKeys")
            .and_then(|cvar| match &cvar.value {
                CvarValue::Text(value) => Some(value.as_str()),
                _ => None,
            })
            .unwrap_or("");
        !native && text.is_some_and(|text| is_console_key(list, text))
    }

    fn toggles_console(&self, event: &KeyEvent, key: KeyCode) -> bool {
        if self.configured_console_key(key, event.text.as_deref()) {
            return true;
        }
        if key == KeyCode::Escape {
            return false;
        }
        let key_name = crate::input::keys::name(crate::input::keys::Source::Key(key)).unwrap_or("");
        self.shell
            .binds
            .commands_for_event(&key_name, true)
            .is_ok_and(|commands| {
                commands
                    .iter()
                    .any(|command| command.eq_ignore_ascii_case("toggleconsole"))
            })
    }
    /// Consume console input before gameplay input gets a chance to observe it.
    pub(crate) fn handle_key(
        &mut self,
        event: &KeyEvent,
        session: Option<&mut ClientSession>,
    ) -> bool {
        let PhysicalKey::Code(key) = event.physical_key else {
            return self.open;
        };
        if !self.open {
            if event.state == ElementState::Pressed && self.toggles_console(event, key) {
                self.set_open(true);
                return true;
            }
            let key_name =
                crate::input::keys::name(crate::input::keys::Source::Key(key)).unwrap_or("");
            let commands = match self
                .shell
                .binds
                .commands_for_event(&key_name, event.state == ElementState::Pressed)
            {
                Ok(commands) => commands,
                Err(error) => {
                    self.shell.push_log(format!("^1Bind error: {error}"));
                    return true;
                }
            };
            let mut consumed = false;
            for command in commands {
                if command.eq_ignore_ascii_case("toggleconsole") {
                    self.execute_bound_command(&command);
                    consumed = true;
                }
            }
            return consumed;
        }

        if event.state != ElementState::Pressed {
            return true;
        }
        if self.toggles_console(event, key) {
            self.set_open(false);
            return true;
        }
        match key {
            KeyCode::Escape => self.set_open(false),
            KeyCode::Enter | KeyCode::NumpadEnter => self.submit(session),
            KeyCode::Tab => self.complete_command(CompletionKey::Tab),
            KeyCode::ArrowUp if !event.repeat => self.navigate_history(-1),
            KeyCode::ArrowDown if !event.repeat => self.navigate_history(1),
            // Caret, deletion and clipboard keys: see `console_editing.rs`.
            _ if self.edit_key(event, key) => {}
            _ if !event.repeat => {
                if let Some(text) = event.text.as_deref() {
                    self.type_text(text);
                }
            }
            _ => {}
        }
        true
    }
}
