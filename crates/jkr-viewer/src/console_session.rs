//! Session-facing initialization and compatibility status for the console.

use super::*;
use jkr_client::{
    ForceProfileNegotiator, LegacyTeamChoice, force_rank_reply, force_rules_from_serverinfo,
};
use jkr_protocol::GameState;

impl ViewerConsole {
    /// Cached retail `r_drawfog` mode: no allocation or lock on frame reads.
    pub(crate) fn draw_fog_mode(&self) -> crate::fog_volumes::Mode {
        crate::fog_volumes::Mode::from_cvar(self.draw_fog.value())
    }

    /// Build the stock multiplayer cvars/binds and restore the user config.
    pub(crate) fn new(config_path: PathBuf) -> Result<Self, Box<dyn Error>> {
        let mut cvars = CvarRegistry::new();
        let user = CvarFlags::ARCHIVE | CvarFlags::USER_INFO;
        for definition in [
            CvarDefinition::new(
                "r_drawfog",
                2_i64,
                CvarFlags::ARCHIVE,
                "Fog: 0 off, 1 volume, 2 global EXP2",
            ),
            CvarDefinition::new("name", "Padawan", user, "Multiplayer player name"),
            CvarDefinition::new(
                "model",
                "kyle/default",
                user,
                "Multiplayer player model and skin",
            ),
            CvarDefinition::new("rate", 25_000_i64, user, "Maximum connection data rate"),
            CvarDefinition::new("snaps", 40_i64, user, "Requested server snapshot rate"),
            CvarDefinition::new(
                "forcepowers",
                "7-1-032330000000001333",
                user,
                "Force rank, side, and power allocation",
            ),
            CvarDefinition::new("color1", 4_i64, user, "Primary saber colour index"),
            CvarDefinition::new("color2", 4_i64, user, "Secondary saber colour index"),
            CvarDefinition::new(
                "cp_sbRGB1",
                0_i64,
                user,
                "Primary RGB blade tint (r | g<<8 | b<<16)",
            ),
            CvarDefinition::new(
                "cp_sbRGB2",
                0_i64,
                user,
                "Secondary RGB blade tint (r | g<<8 | b<<16)",
            ),
            CvarDefinition::new("handicap", 100_i64, user, "Starting health percentage"),
            CvarDefinition::new("sex", "male", user, "Player voice sex token"),
            CvarDefinition::new("cg_predictItems", true, user, "Predict local item pickups"),
            CvarDefinition::new("saber1", "single_1", user, "Primary saber definition"),
            CvarDefinition::new("saber2", "none", user, "Secondary saber definition"),
            CvarDefinition::new("char_color_red", 255_i64, user, "Player tint red channel"),
            CvarDefinition::new(
                "char_color_green",
                255_i64,
                user,
                "Player tint green channel",
            ),
            CvarDefinition::new("char_color_blue", 255_i64, user, "Player tint blue channel"),
            CvarDefinition::new(
                "password",
                "",
                CvarFlags::ARCHIVE | CvarFlags::USER_INFO,
                "Password used only when joining a locked server",
            ),
            CvarDefinition::new(
                "con_maxLines",
                18_i64,
                CvarFlags::ARCHIVE,
                "Maximum visible console lines",
            ),
            CvarDefinition::new(
                "fs_gameData",
                "",
                CvarFlags::ARCHIVE,
                "Jedi Academy GameData directory",
            ),
            CvarDefinition::new(
                "cl_master",
                "master.jkhub.org:29060",
                CvarFlags::ARCHIVE,
                "Legacy protocol-26 master server",
            ),
            CvarDefinition::new(
                "ui_rankChange",
                0_i64,
                CvarFlags::NONE,
                "Force rank most recently supplied by the server",
            ),
            CvarDefinition::new(
                "ui_myteam",
                0_i64,
                CvarFlags::NONE,
                "Team selected by the server's Force-profile flow",
            ),
        ] {
            cvars.register(definition)?;
        }
        register_daily_cvars(&mut cvars)?;
        let post_color = crate::frame_target::aa::color::Settings::bind(&mut cvars)?;
        let dynamic_light_settings = crate::dynamic_lights::Settings::bind(&mut cvars)?;
        let soft_particles = crate::particle_draw::settings::Settings::bind(&mut cvars)?;
        let ssao = crate::world_materials::ssao::settings::Settings::bind(&mut cvars)?;
        crate::world_materials::shadows::settings::register(&mut cvars)?;
        let window_options = window_options::Options::register(&mut cvars)?;
        chat_log::register(&mut cvars)?;
        let qcommon = qcommon::Settings::register(&mut cvars)?;
        crate::hud::targeting::register(&mut cvars)?;
        crate::input::settings::register(&mut cvars)?;
        console_options::register(&mut cvars)?;
        let geometry_controls = crate::shared_geometry::environment::Cvars::bind(&mut cvars)?;
        let show_timedelta = crate::net_timing::CvarSetting::bind(&mut cvars)?;
        let time_nudge = crate::presentation_clock::CvarSetting::bind(&mut cvars)?;
        let smooth_clients =
            console_cvars::IntegerSetting::bind(&mut cvars, "cg_smoothClients", 0)?;
        let draw_fog = console_cvars::IntegerSetting::bind(&mut cvars, "r_drawfog", 2)?;
        let packet_dup = console_cvars::IntegerSetting::bind(&mut cvars, "cl_packetdup", 1)?;
        let userinfo_dirty = Arc::new(AtomicBool::new(true));
        let overlay_dirty = Arc::clone(&userinfo_dirty);
        cvars.on_change("cg_drawTeamOverlay", move |_| {
            overlay_dirty.store(true, Ordering::Release);
        })?;
        let userinfo_names = cvars
            .iter()
            .filter(|cvar| cvar.flags.contains(CvarFlags::USER_INFO))
            .map(|cvar| cvar.name.clone())
            .collect::<Vec<_>>();
        for name in userinfo_names {
            let dirty = Arc::clone(&userinfo_dirty);
            cvars.on_change(&name, move |_| dirty.store(true, Ordering::Release))?;
        }
        let mut shell = Shell::new(cvars, keybind_editor::default_bindings());
        crate::input::settings::register_commands(&mut shell)?;
        director::register(&mut shell)?;
        let client_commands = console_client::Commands::default();
        console_client::register(&mut shell, &client_commands)?;
        shell.set_command_frame(0);
        let server_status = Arc::new(RwLock::new("Not connected".to_owned()));
        for command in ["serverinfo", "status"] {
            let status = Arc::clone(&server_status);
            shell.commands.register(
                command,
                "Show the detected server compatibility profile",
                move |_| {
                    Ok(vec![status.read().map_or_else(
                        |_| "Server status unavailable".to_owned(),
                        |value| value.clone(),
                    )])
                },
            )?;
        }
        for (command, help) in [
            ("quit", "Quit the client"),
            ("exit", "Quit the client (alias of quit)"),
            ("connect", "Connect to host[:port]"),
            ("disconnect", "Leave the current server"),
            ("reconnect", "Reconnect to the last server"),
            ("record", "Begin recording a protocol-26 demo"),
            ("stoprecord", "Finish the active demo recording"),
            ("demo", "Play a demo from the demos directory"),
            ("playdemo", "Play a demo (demo alias)"),
            ("demo_restart", "Replay the last console-opened demo"),
            (
                "deletedemo",
                "Delete a demo from the writable demo directory",
            ),
            ("screenshot", "Write the current frame as PNG"),
            ("screenshotJPEG", "Write the current frame as JPEG"),
            (
                "viewpos",
                "Print the camera origin, yaw and pitch, and copy them to the clipboard",
            ),
            (
                "mark",
                "mark <note>: viewpos, kept with the note in marks.txt beside the configuration",
            ),
        ] {
            shell.commands.register(command, help, |_| Ok(Vec::new()))?;
        }
        let config_directory = config_path
            .parent()
            .ok_or("console config path has no parent")?
            .to_owned();
        shell.set_config_path(config_path);
        shell.set_log_path(config_directory.join("qconsole.log"));
        if let Err(error) = shell.load() {
            shell.push_log(format!("^1Could not load config: {error}"));
            // Also to stderr, which is what the launcher captures. A failed load
            // now blocks every save, so settings silently stop persisting — and
            // a console-only warning is invisible to anyone reading the log
            // rather than opening the console.
            crate::log::progress(format_args!(
                "could not load config: {error}; settings will NOT be saved until it loads cleanly"
            ));
        }
        // Sensitivity used to be counts-to-radians at 0.001; it is now stock's
        // degrees-per-count through `m_yaw`/`m_pitch`. Rescale a value saved
        // under the old units once, so an existing profile keeps the aim it
        // had instead of silently turning 2.6 times slower.
        if matches!(
            shell
                .cvars
                .get("jkr_sensitivityScaleVersion")
                .map(|cvar| &cvar.value),
            Some(CvarValue::Integer(0))
        ) {
            if let Some(saved) = shell
                .cvars
                .get("sensitivity")
                .and_then(|cvar| match cvar.value {
                    CvarValue::Float(value) => Some(value),
                    CvarValue::Integer(value) => Some(value as f64),
                    _ => None,
                })
            {
                let rescaled = saved * 0.001_f64.to_degrees() / 0.022;
                let _ = shell
                    .cvars
                    .set_text("sensitivity", &format!("{rescaled:.3}"));
            }
            let _ = shell.cvars.set_text("jkr_sensitivityScaleVersion", "1");
        }
        if matches!(
            shell
                .cvars
                .get("jkr_bindDefaultsVersion")
                .map(|cvar| &cvar.value),
            Some(CvarValue::Integer(0))
        ) {
            keybind_editor::migrate_missing_defaults(&mut shell.binds);
            let _ = shell.cvars.set_text("jkr_bindDefaultsVersion", "1");
        }
        shell.push_log("^5JKR console ready. ^7Type cmdlist for commands.");
        Ok(Self {
            shell,
            open: false,
            shift: false,
            input: String::with_capacity(INPUT_LIMIT),
            history: Vec::new(),
            history_index: None,
            scroll_offset: 0,
            server_status,
            presentation: ConsolePresentation::new(),
            userinfo_dirty,
            show_timedelta,
            time_nudge,
            smooth_clients,
            draw_fog,
            packet_dup,
            post_color,
            dynamic_light_settings,
            soft_particles,
            ssao,
            geometry_controls,
            director: director::Director::default(),
            pending_connection: None,
            pending_demo: None,
            pending_screenshot: None,
            pending_viewpos: false,
            pending_mark: None,
            marks: config_directory.join("marks.txt"),
            copied: String::new(),
            pending_quit: false,
            pending_input: Vec::with_capacity(64),

            client_commands,
            script_vfs: None,
            config_directory,
            force_profile: ForceProfileNegotiator::default(),
            movement_policy_log: None,
            window_options,
            chat_log: chat_log::ChatLog::default(),
            qcommon,
            control: false,
            edit: super::line_edit::LineEdit::default(),
            selection: super::selection::Selection::new(),
        })
    }

    /// Flush an effective profile change through OpenJK's reliable userinfo command.
    pub(crate) fn flush_userinfo(&mut self, session: &mut ClientSession, now: Instant) {
        if !self.userinfo_dirty.load(Ordering::Acquire) {
            return;
        }
        let result = self
            .userinfo()
            .map_err(|error| error.to_string())
            .and_then(|userinfo| {
                session
                    .update_userinfo_options(
                        &userinfo,
                        Some(self.integer_cvar("cg_drawTeamOverlay").unwrap_or(0) > 0),
                        now,
                    )
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(jkr_client::UserinfoUpdateStatus::RateLimited) => {}
            Ok(jkr_client::UserinfoUpdateStatus::Sent)
            | Ok(jkr_client::UserinfoUpdateStatus::Unchanged) => {
                self.userinfo_dirty.store(false, Ordering::Release);
            }
            Err(error) => {
                self.userinfo_dirty.store(false, Ordering::Release);
                self.shell
                    .push_log(format!("^1Could not update userinfo: {error}"));
            }
        }
        self.poll_force_rejoin(session, now);
    }

    /// Send the queued `forcechanged` reply and bounded `team` retries.
    fn poll_force_rejoin(&mut self, session: &mut ClientSession, now: Instant) {
        let settled = !self.userinfo_dirty.load(Ordering::Acquire);
        let spectator = session.latest_snapshot().player.is_spectator();
        let output = self.force_profile.poll(now, settled, spectator);
        for command in &output.commands {
            if let Err(error) = session.send_reliable_command(command) {
                self.shell
                    .push_log(format!("^1Could not send Force-profile reply: {error}"));
            }
        }
        if let Some(notice) = output.notice {
            self.shell.push_log(notice);
        }
    }

    /// Record the team the player chose so an `nfr` reply returns them there.
    pub(crate) fn note_team_choice(&mut self, choice: LegacyTeamChoice) {
        self.force_profile.note_team_choice(choice);
    }

    /// Adopt the `forcepowers` value negotiated while joining a server.
    pub(crate) fn note_server_forcepowers(&mut self, negotiated: &str) {
        let preferred = self.text_value("forcepowers").unwrap_or_default();
        let value = (negotiated != preferred).then(|| negotiated.to_owned());
        if value.is_some() {
            self.shell.push_log(format!(
                "^5Force profile adjusted to server limits: ^7{negotiated}"
            ));
        }
        self.force_profile.set_server_forcepowers(value);
    }

    /// Refresh `serverinfo` and profile-specific completion from the active session.
    pub(crate) fn set_server_info(&mut self, session: &ClientSession) {
        // Each connection starts with the base userinfo payload, without teamoverlay.
        self.userinfo_dirty.store(true, Ordering::Release);
        self.update_server_info(
            session.game_state(),
            session.compat_profile(),
            session.server(),
        );
    }

    /// Refresh demo serverinfo through the same status and completion path as live play.
    pub(crate) fn set_demo_server_info(&mut self, game: &GameState) {
        self.update_server_info(
            game,
            &jkr_client::CompatProfile::from_game_state(game),
            "demo",
        );
    }

    /// Mirror server-owned movement cvars without resetting profile negotiation.
    pub(crate) fn refresh_movement_cvars(&mut self, game: &GameState) {
        // CS_SYSTEMINFO is server-owned (cg_xcvar.h:157-159); never serialize
        // these renderer/client cvars into userinfo or mutate wire commands.
        let mut policy = [0; 4];
        for (slot, (index, name, default)) in [
            (1, "pmove_fixed", 0),
            (1, "pmove_msec", 8),
            (1, "pmove_float", 0),
            (0, "g_stepSlideFix", 0),
        ]
        .into_iter()
        .enumerate()
        {
            let mut value = game
                .config_string(index)
                .and_then(|bytes| std::str::from_utf8(bytes).ok())
                .and_then(|text| jkr_protocol::InfoString::parse(text).ok())
                .and_then(|info| info.get_i32(name))
                .unwrap_or(default);
            if name == "pmove_msec" {
                value = value.clamp(8, 33);
            }
            let _ = self.shell.cvars.restore_text(name, &value.to_string());
            policy[slot] = value;
        }
        if self.movement_policy_log.replace(policy) != Some(policy) {
            crate::log::progress(format_args!(
                "movement policy: pmove_fixed={} pmove_msec={} pmove_float={} g_stepSlideFix={} ",
                policy[0], policy[1], policy[2], policy[3],
            ));
        }
    }

    fn update_server_info(
        &mut self,
        game: &GameState,
        profile: &jkr_client::CompatProfile,
        server: impl std::fmt::Display,
    ) {
        self.refresh_movement_cvars(game);
        let raw = String::from_utf8_lossy(game.config_string(0).unwrap_or_default());
        self.force_profile.reset();
        let value = format!("profile={profile} server={server} CS_SERVERINFO={raw}");
        if let Ok(mut status) = self.server_status.write() {
            *status = value.clone();
        }
        self.shell.replace_external_command_help(
            compat_console_commands(profile)
                .map(|command| (command.name.to_owned(), command.description.to_owned())),
        );
        self.shell.push_log(value);
    }

    /// Apply `CG_NewForceRank_f`'s two UI cvars and answer a profile request.
    ///
    /// When the server asks for the Force menu (`open_profile`), the stored
    /// profile is legalized against `CS_SERVERINFO` and the stock
    /// `forcechanged` reply is queued through [`ForceProfileNegotiator`].
    pub(crate) fn apply_force_rank(
        &mut self,
        update: ForceRankUpdate,
        game_state: &GameState,
        now: Instant,
    ) {
        let _ = self
            .shell
            .cvars
            .set_text("ui_rankChange", &update.rank.to_string());
        let _ = self
            .shell
            .cvars
            .set_text("ui_myteam", &update.team.to_string());
        self.shell.push_log(format!(
            "^5Force profile: ^7rank {} team {}{}",
            update.rank,
            update.team,
            if update.open_profile {
                " — server requested profile selection"
            } else {
                ""
            }
        ));
        if !update.open_profile {
            return;
        }
        let rules = force_rules_from_serverinfo(game_state, update.rank, update.team);
        let current = self
            .force_profile
            .server_forcepowers()
            .or_else(|| self.text_value("forcepowers"))
            .unwrap_or_default()
            .to_owned();
        let reply = force_rank_reply(&current, rules, self.force_profile.rejoin_team());
        if reply.changed {
            self.userinfo_dirty.store(true, Ordering::Release);
            self.shell.push_log(format!(
                "^5Force profile adjusted to server limits: ^7{}",
                reply.forcepowers
            ));
        }
        self.force_profile.queue(&reply, now);
    }

    /// Route `spc` to the modern shell without opening a legacy UI menu.
    /// Despite its cgame handler's name the command only marks the client as
    /// a spectator (`ui_myteam 3`); it precedes the Force-profile `nfr`.
    pub(crate) fn request_siege_profile(&mut self) {
        let _ = self.shell.cvars.set_text("ui_myteam", "3");
        self.shell
            .push_log("^5Server moved you to spectator mode pending the Force profile check");
    }
}
