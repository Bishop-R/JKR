//! Console keyboard capture and editing.
use super::*;
use crate::input::dead_key::{TypingField, keep_caret};
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
        // Printable opening shortcuts belong to text once the console is open.
        // Keep Escape and non-text bindings available for closing it.
        if !matches!(
            event.logical_key,
            winit::keyboard::Key::Character(_) | winit::keyboard::Key::Dead(_)
        ) && self.toggles_console(event, key)
        {
            self.set_open(false);
            return true;
        }
        if self.browser.is_open() {
            let action = self.browser.handle_key(event, self.shift);
            self.browser_action(action);
            return true;
        }
        // Keys the console acts on itself end a pending composition when the platform
        // reported text for them (see `input::dead_key`); caret keys leave it pending.
        if matches!(
            key,
            KeyCode::F3
                | KeyCode::Escape
                | KeyCode::Enter
                | KeyCode::NumpadEnter
                | KeyCode::Tab
                | KeyCode::ArrowUp
                | KeyCode::ArrowDown
        ) {
            self.dead_key.other_key(event.text.as_deref());
        }
        match key {
            KeyCode::F3 if !event.repeat => self.browser.open(&self.shell),
            KeyCode::Escape => self.set_open(false),
            KeyCode::Enter | KeyCode::NumpadEnter => self.submit(session),
            KeyCode::Tab => self.complete_command(CompletionKey::Tab),
            KeyCode::ArrowUp if !event.repeat => self.navigate_history(-1),
            KeyCode::ArrowDown if !event.repeat => self.navigate_history(1),
            // Caret, deletion and clipboard keys: see `console_editing.rs`.
            _ if self.edit_key(event, key) => self.dead_key.other_key(event.text.as_deref()),
            _ if !event.repeat => {
                let mut dead = self.dead_key;
                dead.type_key(
                    &mut PromptLine {
                        text: &mut self.input,
                        edit: &mut self.edit,
                    },
                    &event.logical_key,
                    event.text.as_deref(),
                );
                self.dead_key = dead;
            }
            _ => {}
        }
        true
    }
}

/// The console input line as a field dead-key composition types into.
struct PromptLine<'a> {
    text: &'a mut String,
    edit: &'a mut super::line_edit::LineEdit,
}

impl TypingField for PromptLine<'_> {
    fn line(&self) -> &str {
        self.text
    }

    fn caret(&self) -> usize {
        self.edit.cursor(self.text)
    }

    fn insert(&mut self, text: &str) {
        self.edit.insert(self.text, text, INPUT_LIMIT);
    }

    fn remove(&mut self, range: Range<usize>) {
        let caret = keep_caret(self.edit.cursor(self.text), &range);
        self.text.replace_range(range, "");
        self.edit.place(self.text, caret, false);
    }
}

#[cfg(test)]
mod dead_key_tests {
    use super::*;
    use crate::input::dead_key::DeadKey;
    use winit::keyboard::{Key, NamedKey, SmolStr};

    /// Type one press into `input` the way the console's typing path does.
    fn press(
        input: &mut String,
        edit: &mut super::super::line_edit::LineEdit,
        dead: &mut DeadKey,
        logical: Key,
        text: Option<&str>,
    ) {
        dead.type_key(&mut PromptLine { text: input, edit }, &logical, text);
    }

    fn character(text: &str) -> Key {
        Key::Character(SmolStr::new(text))
    }

    #[test]
    fn prompt_line_types_a_dead_key_colour_code_exactly() {
        let mut input = String::from("say ");
        let mut edit = super::super::line_edit::LineEdit::default();
        edit.to_end(&input);
        let mut dead = DeadKey::default();
        press(&mut input, &mut edit, &mut dead, Key::Dead(Some('^')), None);
        assert_eq!(input, "say ^");
        press(
            &mut input,
            &mut edit,
            &mut dead,
            Key::Named(NamedKey::Shift),
            None,
        );
        // Windows reports the uncombined pair as the digit key's text.
        press(&mut input, &mut edit, &mut dead, character("1"), Some("^1"));
        assert_eq!(input, "say ^1");
        assert_eq!(edit.cursor(&input), input.len());
    }

    #[test]
    fn prompt_line_composes_a_circumflex_letter() {
        let mut input = String::from("t");
        let mut edit = super::super::line_edit::LineEdit::default();
        edit.to_end(&input);
        let mut dead = DeadKey::default();
        press(&mut input, &mut edit, &mut dead, Key::Dead(Some('^')), None);
        assert_eq!(input, "t^");
        press(&mut input, &mut edit, &mut dead, character("ê"), Some("ê"));
        press(&mut input, &mut edit, &mut dead, character("t"), Some("t"));
        press(&mut input, &mut edit, &mut dead, character("e"), Some("e"));
        assert_eq!(input, "tête");
    }

    #[test]
    fn prompt_line_replaces_a_dead_key_shown_inside_the_line() {
        let mut input = String::from("ab");
        let mut edit = super::super::line_edit::LineEdit::default();
        edit.place(&input, 1, false);
        let mut dead = DeadKey::default();
        press(&mut input, &mut edit, &mut dead, Key::Dead(Some('^')), None);
        assert_eq!(input, "a^b");
        press(&mut input, &mut edit, &mut dead, character("2"), Some("^2"));
        assert_eq!((input.as_str(), edit.cursor(&input)), ("a^2b", 3));
    }

    #[test]
    fn prompt_line_keeps_its_limit_for_a_dead_key() {
        let mut input = "x".repeat(INPUT_LIMIT);
        let mut edit = super::super::line_edit::LineEdit::default();
        edit.to_end(&input);
        let mut dead = DeadKey::default();
        press(&mut input, &mut edit, &mut dead, Key::Dead(Some('^')), None);
        assert_eq!(input.len(), INPUT_LIMIT);
        assert_eq!(dead, DeadKey::default());
    }
}
