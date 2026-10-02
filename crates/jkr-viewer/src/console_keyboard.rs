//! Console keyboard capture and editing.
use super::*;
use crate::input::dead_key::TypingField;
use std::ops::Range;

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
        // These keys end or replace the line, so a dead key shown at its end stays
        // typed (see `input::dead_key`).
        if matches!(
            key,
            KeyCode::Escape
                | KeyCode::Enter
                | KeyCode::NumpadEnter
                | KeyCode::Backspace
                | KeyCode::Tab
                | KeyCode::ArrowUp
                | KeyCode::ArrowDown
        ) || event.text.as_deref() == Some("\u{16}")
        {
            self.dead_key.settle();
        }
        match key {
            KeyCode::Escape => self.set_open(false),
            KeyCode::Enter | KeyCode::NumpadEnter => self.submit(session),
            KeyCode::Backspace => {
                self.input.pop();
                self.rebuild_prompt();
            }
            KeyCode::Tab => self.complete_command(CompletionKey::Tab),
            KeyCode::ArrowUp if !event.repeat => self.navigate_history(-1),
            KeyCode::ArrowDown if !event.repeat => self.navigate_history(1),
            // Ctrl+V and Ctrl+C arrive as the control characters they have always been.
            _ if event.text.as_deref() == Some("\u{16}") => {
                if let Some(text) = super::clipboard::paste() {
                    self.type_text(&text);
                }
            }
            _ if event.text.as_deref() == Some("\u{3}") => {
                super::clipboard::copy(if self.input.is_empty() {
                    &self.copied
                } else {
                    &self.input
                });
            }
            _ if !event.repeat => {
                let mut dead = self.dead_key;
                dead.type_key(
                    &mut PromptLine(&mut self.input),
                    &event.logical_key,
                    event.text.as_deref(),
                );
                self.dead_key = dead;
                self.rebuild_prompt();
            }
            _ => {}
        }
        true
    }
}

impl ViewerConsole {
    /// Append typed or pasted text to the prompt: no control characters, up to the limit.
    fn type_text(&mut self, text: &str) {
        PromptLine(&mut self.input).insert(text);
        self.rebuild_prompt();
    }
}

/// The console input line, typed at its end.
struct PromptLine<'a>(&'a mut String);

impl TypingField for PromptLine<'_> {
    fn line(&self) -> &str {
        self.0
    }

    fn caret(&self) -> usize {
        self.0.len()
    }

    fn insert(&mut self, text: &str) {
        let remaining = INPUT_LIMIT.saturating_sub(self.0.len());
        self.0.extend(
            text.chars()
                .filter(|character| !character.is_control())
                .take(remaining),
        );
    }

    fn remove(&mut self, range: Range<usize>) {
        self.0.replace_range(range, "");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::dead_key::DeadKey;
    use winit::keyboard::{Key, NamedKey, SmolStr};

    #[test]
    fn prompt_line_types_a_dead_key_colour_code_exactly() {
        let mut input = String::from("say ");
        let mut dead = DeadKey::default();
        dead.type_key(&mut PromptLine(&mut input), &Key::Dead(Some('^')), None);
        assert_eq!(input, "say ^");
        dead.type_key(
            &mut PromptLine(&mut input),
            &Key::Named(NamedKey::Shift),
            None,
        );
        dead.type_key(
            &mut PromptLine(&mut input),
            &Key::Character(SmolStr::new("1")),
            Some("^1"),
        );
        assert_eq!(input, "say ^1");
    }

    #[test]
    fn prompt_line_keeps_its_limit_for_a_dead_key() {
        let mut input = "x".repeat(INPUT_LIMIT);
        let mut dead = DeadKey::default();
        dead.type_key(&mut PromptLine(&mut input), &Key::Dead(Some('^')), None);
        assert_eq!(input.len(), INPUT_LIMIT);
        assert_eq!(dead, DeadKey::default());
    }
}
