//! Viewer presentation and input adapter for the portable client shell.

use super::{TextVertex, UiFont};
use crate::keybind_editor;
#[path = "console_chat_log.rs"]
mod chat_log;
#[path = "console_client_options.rs"]
mod client_options;
#[path = "clipboard.rs"]
pub(crate) mod clipboard;
#[path = "console_connect.rs"]
mod console_connect;
#[path = "console_cvars.rs"]
mod console_cvars;
#[path = "demo_director.rs"]
pub(crate) mod director;
#[path = "console_qcommon.rs"]
mod qcommon;
#[path = "console_window_options.rs"]
mod window_options;
pub(crate) use qcommon::wait_control;
#[path = "console_ui_options.rs"]
mod ui_options;
pub(crate) use console_connect::Action as ConnectionAction;
#[path = "console_demo.rs"]
mod console_demo;
pub(crate) use console_demo::Action as DemoAction;
#[path = "console_client.rs"]
mod console_client;
#[path = "console_command.rs"]
mod console_command;
#[path = "console_forward.rs"]
mod console_forward;
#[path = "console_keyboard.rs"]
mod console_keyboard;

#[path = "console_options.rs"]
mod console_options;
#[path = "console_pointer.rs"]
mod console_pointer;
#[path = "console_profile.rs"]
mod console_profile;
#[path = "console_scripting.rs"]
mod console_scripting;
#[path = "console_session.rs"]
mod console_session;
#[path = "console_tell.rs"]
mod console_tell;
#[path = "console_view.rs"]
mod console_view;
use console_cvars::register_daily_cvars;
use console_forward::{ForwardAction, forward_payload};
use console_view::ConsolePresentation;

use jkr_client::{ClientSession, ForceRankUpdate, compat_console_commands};
use jkr_shell::{CompletionKey, CvarDefinition, CvarFlags, CvarRegistry, CvarValue, Shell};
use std::error::Error;
use std::fmt::Write as _;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

const INPUT_LIMIT: usize = 512;

/// Toggleable console state owned by the viewer frontend.
pub(crate) struct ViewerConsole {
    shell: Shell,
    open: bool,
    /// Shift held, for the Shift+Escape console toggle.
    shift: bool,
    input: String,
    prompt: String,
    history: Vec<String>,
    history_index: Option<usize>,
    scroll_offset: usize,
    server_status: Arc<RwLock<String>>,
    presentation: ConsolePresentation,
    userinfo_dirty: Arc<AtomicBool>,
    show_timedelta: crate::net_timing::CvarSetting,
    time_nudge: crate::presentation_clock::CvarSetting,
    smooth_clients: console_cvars::IntegerSetting,
    draw_fog: console_cvars::IntegerSetting,
    packet_dup: console_cvars::IntegerSetting,
    pub(crate) geometry_controls: crate::shared_geometry::environment::Cvars,
    pub(crate) post_color: crate::frame_target::aa::color::Settings,
    /// Seeded scene-light control shared with graphics contexts and map-install workers.
    pub(crate) dynamic_light_settings: crate::dynamic_lights::Settings,
    /// Live soft-particle policy survives background world installation.
    pub(crate) soft_particles: crate::particle_draw::settings::Settings,
    /// Optional ambient correction for supported main-view world surfaces.
    pub(crate) ssao: crate::world_materials::ssao::settings::Settings,
    pub(crate) director: director::Director,
    pending_connection: Option<console_connect::Action>,
    pending_demo: Option<console_demo::Action>,
    pending_screenshot: Option<crate::screenshot::Request>,
    /// `viewpos` was typed; the application answers with its camera pose.
    pending_viewpos: bool,
    /// `mark <note>` was typed: the pose is also kept, with the note, in [`Self::marks`].
    pending_mark: Option<String>,
    /// Where marks are appended: `marks.txt` beside the configuration.
    marks: std::path::PathBuf,
    /// The last line `viewpos` or `mark` answered, for Ctrl+C on an empty prompt.
    copied: String,
    pending_quit: bool,
    pending_input: Vec<String>,

    client_commands: console_client::Commands,
    script_vfs: Option<Arc<jkr_vfs::VirtualFileSystem>>,
    config_directory: PathBuf,
    force_profile: jkr_client::ForceProfileNegotiator,
    movement_policy_log: Option<[i32; 4]>,
    window_options: window_options::Options,
    chat_log: chat_log::ChatLog,
    qcommon: qcommon::Settings,
}

impl ViewerConsole {
    /// Cached `cg_smoothClients` for live and demo snapshot translation.
    pub(crate) fn smooth_clients(&self) -> bool {
        self.smooth_clients.enabled()
    }

    /// Cached `cl_showtimedelta` state for the allocation-free runtime read.
    pub(crate) fn show_timedelta(&self) -> bool {
        self.show_timedelta.enabled()
    }

    /// Cached, clamped `cl_timenudge` for the live presentation clock.
    pub(crate) fn time_nudge_millis(&self) -> i64 {
        self.time_nudge.millis()
    }

    /// Cached `cl_packetdup`, clamped like the stock client (0 to 5).
    pub(crate) fn packet_dup(&self) -> usize {
        self.packet_dup
            .value()
            .clamp(0, jkr_client::command_history::MAX_PACKET_DUP as i64) as usize
    }

    /// Configured retail-data directory, if the user selected one.
    pub(crate) fn configured_game_data(&self) -> Option<PathBuf> {
        self.text_cvar("fs_gameData")
            .ok()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
    }

    pub(crate) fn config_directory(&self) -> &std::path::Path {
        &self.config_directory
    }

    /// Master server address used by the browser.
    pub(crate) fn master_server(&self) -> Result<String, Box<dyn Error>> {
        Ok(self.text_cvar("cl_master")?.to_owned())
    }

    pub(crate) fn text_value(&self, name: &str) -> Option<&str> {
        match self.cvar(name) {
            Some(CvarValue::Text(value)) => Some(value),
            _ => None,
        }
    }

    /// Whether the overlay currently owns keyboard input.
    /// Track Shift for the stock Shift+Escape toggle (`cl_keys.cpp:1318`).
    pub(crate) fn set_shift(&mut self, held: bool) {
        self.shift = held;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Add an application diagnostic to the visible bounded scrollback.
    pub(crate) fn push_log(&mut self, text: impl Into<String>) {
        self.shell.push_log(text);
    }

    /// Queue a bound script through the same frame-buffered path as cfg text.
    pub(crate) fn queue_bound_script(&mut self, key: &str, pressed: bool) {
        let time = self.input_millis();
        let id = key
            .bytes()
            .fold(0_u64, |hash, byte| hash.wrapping_mul(31) + u64::from(byte));
        match self.shell.binds.commands_for_event(key, pressed) {
            Ok(commands) => {
                for command in commands {
                    let command = if command.starts_with(['+', '-']) {
                        format!("{command} {id} {time}")
                    } else {
                        command
                    };
                    if let Err(error) = self.shell.queue_script(&command) {
                        self.shell.push_log(format!("^1Bind error: {error}"));
                    }
                }
            }
            Err(error) => self.shell.push_log(format!("^1Bind error: {error}")),
        }
    }

    pub(crate) fn cvar(&self, name: &str) -> Option<&CvarValue> {
        self.shell.cvars.get(name).map(|cvar| &cvar.value)
    }

    pub(crate) fn bool_cvar(&self, name: &str) -> Option<bool> {
        match self.cvar(name) {
            Some(CvarValue::Bool(value)) => Some(*value),
            _ => None,
        }
    }

    pub(crate) fn integer_cvar(&self, name: &str) -> Option<i64> {
        match self.cvar(name) {
            Some(CvarValue::Integer(value)) => Some(*value),
            _ => None,
        }
    }

    pub(crate) fn float_cvar(&self, name: &str) -> Option<f64> {
        match self.cvar(name) {
            Some(CvarValue::Float(value)) => Some(*value),
            _ => None,
        }
    }

    pub(crate) fn set_cvar(&mut self, name: &str, value: &str) -> bool {
        match self.shell.cvars.set_text(name, value) {
            Ok(changed) => {
                if changed {
                    if name.eq_ignore_ascii_case("r_resolution") {
                        self.sync_custom_resolution(value);
                    }
                    self.persist();
                }
                true
            }
            Err(error) => {
                self.shell.push_log(format!("^1{error}"));
                false
            }
        }
    }

    pub(crate) fn keys_for_command(&self, command: &str) -> Vec<String> {
        self.shell
            .binds
            .iter()
            .filter(|(_, bound)| bound.eq_ignore_ascii_case(command))
            .map(|(key, _)| key.to_owned())
            .collect()
    }

    /// Whether a stock key has an explicit script, before UI fallback handling.
    pub(crate) fn has_key_binding(&self, key: &str) -> bool {
        self.shell
            .binds
            .iter()
            .any(|(bound, _)| bound.eq_ignore_ascii_case(key))
    }

    /// Format all physical keys bound to `command` into retained storage.
    pub(crate) fn write_keys_for_command(&self, command: &str, output: &mut String) {
        output.clear();
        for (key, bound) in self.shell.binds.iter() {
            if !bound.eq_ignore_ascii_case(command) {
                continue;
            }
            if !output.is_empty() {
                output.push_str(" / ");
            }
            let _ = write!(output, "{key}");
        }
        if output.is_empty() {
            output.push_str("unbound");
        }
    }

    /// Replace one displayed binding without removing the other slot.
    pub(crate) fn rebind_action(&mut self, command: &str, slot: usize, key: &str) {
        let Some(key) = jkr_shell::key_names::canonical_key(key) else {
            self.shell.push_log(format!("^1Unsupported key: {key}"));
            return;
        };
        if slot > 1 {
            return;
        }
        let keys = self.keys_for_command(command);
        if keys.iter().any(|old| old.eq_ignore_ascii_case(key)) {
            return;
        }
        if let Some(old) = keys.get(slot) {
            self.shell.binds.unbind(old);
        }
        if let Err(error) = self.shell.binds.bind(key, command) {
            self.shell.push_log(format!("^1Bind error: {error}"));
        }
        self.persist();
    }

    /// Clear only the selected displayed binding.
    pub(crate) fn clear_action(&mut self, command: &str, slot: usize) {
        if let Some(key) = self.keys_for_command(command).get(slot) {
            self.shell.binds.unbind(key);
        }
        self.persist();
    }

    pub(crate) fn reset_default_binds(&mut self) {
        self.shell.binds = keybind_editor::default_bindings();
        self.persist();
    }

    /// Append console text vertices without allocating additional line strings.
    pub(crate) fn append_overlay(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        _scale: f32,
    ) {
        let options = self.options();
        let configured = self
            .shell
            .cvars
            .get("con_maxLines")
            .and_then(|cvar| match cvar.value {
                CvarValue::Integer(value) => usize::try_from(value).ok(),
                _ => None,
            })
            .unwrap_or(18);
        let completion = self.shell.completion_hint(&self.input).unwrap_or("");
        self.presentation.append_options(
            self.shell.lines(),
            configured,
            self.scroll_offset,
            &self.prompt,
            completion,
            vertices,
            font,
            viewport,
            options,
            self.open,
            self.shell.command_clock_millis(),
        );
    }

    pub(crate) fn draw_list(&self) -> &jkr_ui::DrawList {
        self.presentation.draw_list()
    }

    fn execute_bound_command(&mut self, command: &str) {
        if command.eq_ignore_ascii_case("toggleconsole") {
            self.set_open(!self.open);
        } else {
            let _ = self.shell.execute_line(command);
            self.persist();
        }
    }

    fn submit(&mut self, session: Option<&mut ClientSession>) {
        // `Console_Key`: one leading `/` or `\` is optional and stripped before the
        // line is completed and run.
        if let Some(line) = self.input.trim_start().strip_prefix(['/', '\\']) {
            self.input = line.to_owned();
        }
        if self.integer_cvar("cl_allowentercompletion").unwrap_or(1) != 0 {
            self.complete_command(CompletionKey::Enter);
        }
        let command = self.input.trim().to_owned();
        self.input.clear();
        self.history_index = None;
        self.rebuild_prompt();
        if command.is_empty() {
            return;
        }
        if self.history.last() != Some(&command) {
            self.history.push(command.clone());
        }
        self.execute_console_line(&command, session);
    }

    /// Change the console catcher and its existing animated presentation state.
    pub(crate) fn set_open(&mut self, open: bool) {
        if self.bool_cvar("con_autoclear").unwrap_or(true) {
            self.input.clear();
            self.rebuild_prompt();
        }
        self.open = open;
        self.history_index = None;
    }

    fn navigate_history(&mut self, direction: i32) {
        if self.history.is_empty() {
            return;
        }
        let current = self.history_index.unwrap_or(self.history.len());
        let next = if direction < 0 {
            current.saturating_sub(1)
        } else {
            (current + 1).min(self.history.len())
        };
        self.history_index = (next < self.history.len()).then_some(next);
        self.input.clear();
        if let Some(index) = self.history_index {
            self.input.push_str(&self.history[index]);
        }
        self.rebuild_prompt();
    }

    fn complete_command(&mut self, key: CompletionKey) {
        if let Some(line) = self.shell.complete_line(&self.input, key) {
            self.input = line;
            self.rebuild_prompt();
        }
    }

    fn rebuild_prompt(&mut self) {
        self.prompt.clear();
        self.prompt.push_str("] ");
        self.prompt.push_str(&self.input);
        self.prompt.push('_');
    }

    fn persist(&mut self) {
        if let Err(error) = self.shell.save() {
            self.shell
                .push_log(format!("^1Could not save config: {error}"));
        }
    }

    /// Borrow a text cvar without allocating a display string.
    pub(crate) fn text_cvar(&self, name: &str) -> Result<&str, Box<dyn Error>> {
        match self.shell.cvars.get(name).map(|cvar| &cvar.value) {
            Some(CvarValue::Text(value)) => Ok(value),
            _ => Err(invalid_cvar(name, "must be text").into()),
        }
    }

    fn positive_u32_cvar(&self, name: &str) -> Result<u32, Box<dyn Error>> {
        match self.shell.cvars.get(name).map(|cvar| &cvar.value) {
            Some(CvarValue::Integer(value)) if *value > 0 => u32::try_from(*value)
                .map_err(|_| invalid_cvar(name, "must fit in an unsigned 32-bit integer").into()),
            _ => Err(invalid_cvar(name, "must be a positive integer").into()),
        }
    }
}

impl Drop for ViewerConsole {
    fn drop(&mut self) {
        let _ = self.shell.save();
    }
}

fn invalid_cvar(name: &str, requirement: &str) -> IoError {
    IoError::new(
        ErrorKind::InvalidInput,
        format!("cvar {name:?} {requirement}"),
    )
}
