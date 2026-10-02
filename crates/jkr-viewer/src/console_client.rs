//! Client command catalogue and bounded handoff to the existing viewer services.
use super::*;

impl ViewerConsole {
    /// Coalesce `scl` until the ordinary viewer service handoff.
    pub(crate) fn request_siege_class(&mut self) {
        self.client_commands.siege_class = true;
    }
}
use std::collections::VecDeque;
use std::sync::mpsc::Receiver;

#[path = "console_hud_commands.rs"]
mod hud_commands;
#[path = "console_identity.rs"]
mod identity;
#[path = "console_info.rs"]
mod info;
#[path = "console_queries.rs"]
mod queries;
#[path = "console_restarts.rs"]
mod restarts;

pub(super) use identity::color_chat;

/// Completion/help inventory for commands owned by viewer services.
pub(super) const COMMANDS: &[(&str, &str)] = &[
    ("speedometer", "Configure supported speedometer flags"),
    ("strafehelper", "Configure supported airborne CGAZ flags"),
    ("play", "Play local sound files"),
    ("music", "Start intro and repeating music"),
    ("stopmusic", "Stop background music"),
    ("soundstop", "Stop sounds and music"),
    ("soundlist", "List registered sound assets"),
    ("soundinfo", "Describe audio output"),
    (
        "s_dynamic",
        "Select dynamic music state (untimed transitions)",
    ),
    ("messagemode", "Compose global chat"),
    ("messagemode2", "Compose team chat"),
    (
        "messagemode3",
        "Compose crosshair-client chat when a target is available",
    ),
    (
        "messagemode4",
        "Compose last-attacker chat when tracking is available",
    ),
    ("toggleconsole", "Toggle the console"),
    ("togglemenu", "Toggle the in-game menu"),
    ("cmd", "Forward arguments as a reliable server command"),
    ("clientinfo", "Print client state and userinfo"),
    ("userinfo", "Print userinfo"),
    ("model", "Set player model and optional skin"),
    ("forcepowers", "Set the player force profile"),
    ("configstrings", "Print non-empty indexed configstrings"),
    ("showip", "List local interface addresses"),
    ("fs_openedList", "Print mounted package names"),
    ("fs_referencedList", "Print pure-proof package references"),
    ("modelist", "List video modes"),
    ("minimize", "Minimize the window"),
    ("ping", "Query server info and round-trip time"),
    ("serverstatus", "Query server status"),
    ("globalservers", "Query master server addresses"),
    ("localservers", "Discover LAN servers"),
    ("addFavorite", "Add an address to browser favorites"),
    ("rcon", "Execute a remote console command"),
    ("afk", "Toggle the AFK name prefix"),
    ("colorname", "Apply selected name colors"),
    ("colorstring", "Configure outgoing chat colors"),
    ("vid_restart", "Reapply video settings (device is retained)"),
    (
        "snd_restart",
        "Recreate audio output and reload sound assets",
    ),
    ("in_restart", "Reset held input and pointer capture"),
];

/// Queue and worker replies exist only for explicitly requested commands.
#[derive(Default)]
pub(super) struct Commands {
    /// Coalesced server request; never forwarded back to the server.
    siege_class: bool,
    last_demo: Option<String>,
    /// Bounded main-thread service requests.
    pub pending: VecDeque<Vec<String>>,
    /// Bounded worker reply channels.
    pub jobs: Vec<Receiver<Vec<String>>>,
    /// Consecutive frames beyond the connected-server timeout.
    pub timeout_frames: u8,
    /// Name-edit cooldown shared with the userinfo callback.
    pub name_clock: identity::NameClock,
}

/// Register command metadata and only the settings consumed by these services.
pub(super) fn register(shell: &mut Shell, commands: &Commands) -> Result<(), Box<dyn Error>> {
    for &(name, help) in COMMANDS {
        if !shell.commands.contains(name) && shell.cvars.get(name).is_none() {
            shell.commands.register(name, help, |_| {
                Err(jkr_shell::CommandError::Handler(
                    "Viewer command dispatcher required".into(),
                ))
            })?;
        }
    }
    queries::register(&mut shell.cvars)?;
    restarts::register(&mut shell.cvars)?;
    identity::register(&mut shell.cvars, &commands.name_clock)?;
    for (name, value, flags, help) in [
        (
            "cg_chatBeep",
            true,
            CvarFlags::ARCHIVE,
            "Play global chat notifications",
        ),
        (
            "cg_teamChatBeep",
            true,
            CvarFlags::ARCHIVE,
            "Play team chat notifications",
        ),
        (
            "s_allowDynamicMusic",
            true,
            CvarFlags::ARCHIVE,
            "Enable dynamic music selection",
        ),
        (
            "s_initsound",
            true,
            CvarFlags::ARCHIVE,
            "Open audio output on startup/restart",
        ),
        (
            "s_show",
            false,
            CvarFlags::NONE,
            "Print started sounds (diagnostic)",
        ),
        (
            "cg_duelMusic",
            true,
            CvarFlags::ARCHIVE,
            "Play private duel music",
        ),
    ] {
        shell
            .cvars
            .register(CvarDefinition::new(name, value, flags, help))?;
    }
    shell.cvars.register(CvarDefinition::new(
        "s_separation",
        0.5,
        CvarFlags::ARCHIVE,
        "Stereo separation",
    ))?;
    Ok(())
}

impl Commands {
    /// Retain a recognized service command, preserving rcon's raw argument text.
    pub fn queue(
        &mut self,
        command: &str,
        tokens: &[String],
    ) -> Option<Result<Vec<String>, String>> {
        if !COMMANDS
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(&tokens[0]))
        {
            return None;
        }
        if self.pending.len() == 64 {
            return Some(Err("Client command queue is full".into()));
        }
        if tokens[0].eq_ignore_ascii_case("rcon") {
            let raw = command
                .split_once(char::is_whitespace)
                .map_or("", |(_, args)| args);
            self.pending
                .push_back(vec![tokens[0].clone(), raw.to_owned()]);
        } else {
            self.pending.push_back(tokens.to_vec());
        }
        Some(Ok(Vec::new()))
    }
}

impl ViewerConsole {
    /// Last successfully opened console demo, retained after playback stops.
    pub(crate) fn last_console_demo(&self) -> Option<&str> {
        self.client_commands.last_demo.as_deref()
    }

    /// Remember a demo after its header and gamestate have loaded successfully.
    pub(crate) fn remember_console_demo(&mut self, name: &str) {
        self.client_commands.last_demo = Some(name.to_owned());
    }
}

impl crate::GpuState {
    /// Open stock targeted chat using authoritative damage and retained crosshair state.
    pub(crate) fn targeted_chat(&mut self, attacker: bool) {
        if let Some(session) = &self.live_session {
            let snapshot = session.latest_snapshot();
            let slot = if !attacker {
                self.crosshair_scan.chat_client(snapshot.server_time)
            } else {
                self.damage_feedback
                    .has_attacker()
                    .then(|| u16::try_from(snapshot.player.persistent[6]).ok())
                    .flatten()
            };
            self.chat.update_roster(session.game_state());
            if self.chat.whisper_to(slot) {
                self.gameplay_input.release_keys();
                if let Some(console) = &mut self.console {
                    console.set_open(false);
                }
                self.sync_cursor_policy();
            }
        }
    }
    /// Consume explicit service requests and nonblocking query replies each frame.
    pub(crate) fn run_client_commands(&mut self, audio: &mut Option<crate::GameAudio>) {
        if self.live_presentation_ready()
            && self
                .console
                .as_mut()
                .is_some_and(|c| std::mem::take(&mut c.client_commands.siege_class))
        {
            self.open_siege_classes();
        }
        if let (Some(audio), Some(console)) = (audio.as_mut(), self.console.as_mut()) {
            audio.print_sound_starts(console);
        }
        if let (Some(session), Some(console)) = (&mut self.live_session, &mut self.console) {
            while let Some(text) = session.pop_server_print() {
                console.push_log(text);
            }
            let limit = console.float_cvar("cl_timeout").unwrap_or(200.0).max(0.0);
            let timed_out = session
                .packet_silence()
                .is_some_and(|age| age.as_secs_f64() > limit);
            console.client_commands.timeout_frames = if timed_out {
                console.client_commands.timeout_frames.saturating_add(1)
            } else {
                0
            };
            if console.client_commands.timeout_frames > 5 {
                self.session_disconnected("Server connection timed out".into());
            }
        }
        loop {
            let command = self
                .console
                .as_mut()
                .and_then(|console| console.client_commands.pending.pop_front());
            let Some(tokens) = command else { break };
            let result = self.client_command(&tokens, audio);
            if let Some(console) = &mut self.console {
                match result {
                    Ok(lines) => {
                        for line in lines {
                            console.push_log(line);
                        }
                    }
                    Err(error) => console.push_log(format!("^1{error}")),
                }
            }
        }
        if let Some(console) = &mut self.console {
            let mut index = 0;
            while index < console.client_commands.jobs.len() {
                match console.client_commands.jobs[index].try_recv() {
                    Ok(lines) => {
                        console.client_commands.jobs.swap_remove(index);
                        for line in lines {
                            console.push_log(line);
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => index += 1,
                    Err(_) => {
                        console.client_commands.jobs.swap_remove(index);
                        console.push_log("^1Query worker stopped");
                    }
                }
            }
        }
    }

    fn client_command(
        &mut self,
        tokens: &[String],
        audio: &mut Option<crate::GameAudio>,
    ) -> Result<Vec<String>, String> {
        let name = tokens[0].to_ascii_lowercase();
        let args = &tokens[1..];
        match name.as_str() {
            "speedometer" | "strafehelper" => {
                return hud_commands::execute(
                    self.console.as_mut().ok_or("Console unavailable")?,
                    &name,
                    args,
                );
            }
            "play" | "music" | "stopmusic" | "soundstop" | "soundlist" | "soundinfo"
            | "s_dynamic" => {
                let Some(audio) = audio else {
                    if name == "soundinfo" {
                        return Ok(vec!["Audio device closed/unavailable".into()]);
                    }
                    return Err("Audio is not initialized".into());
                };
                audio.sync_gains(self.console.as_ref());
                return audio.sound_command(&name, args, self.vfs.as_deref());
            }
            "messagemode" | "messagemode2" => {
                self.apply_input_action(Some(crate::input::InputAction::MessageMode(
                    name == "messagemode2",
                )));
            }
            "messagemode3" | "messagemode4" => {
                self.targeted_chat(name == "messagemode4");
            }
            "toggleconsole" => {
                if let Some(console) = &mut self.console {
                    console.set_open(!console.is_open());
                }
                self.sync_cursor_policy();
            }
            "togglemenu" => {
                if self.game_menu {
                    self.game_menu = false;
                    self.sync_cursor_policy();
                } else if self.live_session.is_some() {
                    self.release_pointer();
                    self.game_menu = true;
                    self.game_menu_page = crate::ingame_menu::Page::Main;
                    self.game_menu_row = 0;
                } else {
                    return Err("No game session; the client menu is already available".into());
                }
            }
            "minimize" => {
                self.window
                    .as_ref()
                    .ok_or("No window to minimize")?
                    .set_minimized(true);
            }
            "vid_restart" | "snd_restart" | "in_restart" | "modelist" => {
                return self.restart_command(&name, audio);
            }
            "ping" | "serverstatus" | "globalservers" | "localservers" | "rcon" | "showip" => {
                let server = self.live_session.as_ref().map(|session| session.server());
                let console = self.console.as_mut().ok_or("Console unavailable")?;
                queries::start(console, &name, args, server)?;
            }
            "addfavorite" => {
                let address = match args {
                    [address] => queries::address(address, 29070)?,
                    [] => self
                        .live_session
                        .as_ref()
                        .ok_or("Not connected to a server.")?
                        .server(),
                    _ => return Err("usage: addFavorite [address]".into()),
                };
                let menu = self
                    .client_menu
                    .as_mut()
                    .ok_or("Server browser unavailable")?;
                menu.add_favorite(address)?;
                return Ok(vec![format!("Added favorite {address}")]);
            }
            _ => {
                let console = self.console.as_mut().ok_or("Console unavailable")?;
                return console.info_command(&name, args, self.live_session.as_mut());
            }
        }
        Ok(Vec::new())
    }
}
