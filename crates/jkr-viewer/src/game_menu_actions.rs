//! In-game menu actions kept out of the renderer/event-loop module.

use super::*;

impl GpuState {
    pub(crate) fn select_team(&mut self, team: jkr_client::LegacyTeamChoice) {
        let command = jkr_client::legacy_team_command(team);
        if let Some(console) = &mut self.console {
            console.note_team_choice(team);
        }
        if let Some(session) = &mut self.live_session {
            match session.send_reliable_command(command) {
                Ok(()) => {
                    self.game_menu = false;
                    self.capture_pointer();
                }
                Err(error) => eprintln!("failed to select team: {error}"),
            }
        }
    }

    pub(crate) fn game_menu_row_count(&self) -> usize {
        self.in_game_menu
            .row_count(self.game_menu_page, self.is_team_game(), self.vote_active())
    }

    pub(crate) fn back_or_close_game_menu(&mut self) {
        if matches!(self.game_menu_page, GameMenuPage::Main | GameMenuPage::Shot) {
            self.game_menu = false;
            self.capture_pointer();
        } else if self.game_menu_page.is_vote_page()
            && self.game_menu_page != GameMenuPage::CallVote
        {
            self.game_menu_page = GameMenuPage::CallVote;
            self.game_menu_row = 0;
        } else {
            // The classic bar keeps focus on the button whose pop-up closed.
            let tab = ingame_menu::classic::Tab::of_page(self.game_menu_page)
                .filter(|_| self.in_game_menu.is_classic());
            self.game_menu_page = GameMenuPage::Main;
            self.game_menu_row = tab.map_or(0, ingame_menu::classic::Tab::index);
        }
    }

    pub(crate) fn activate_game_menu_row(&mut self) {
        if self.in_game_menu.is_classic() && self.activate_classic_row() {
            return;
        }
        match self.game_menu_page {
            GameMenuPage::Main => self.activate_main_menu_row(),
            GameMenuPage::Team => self.activate_team_row(),
            GameMenuPage::Siege => match self.in_game_menu.siege_action(self.game_menu_row) {
                ingame_menu::CallVoteAction::Send(command) => {
                    let unchanged = self.live_session.as_ref().is_some_and(|session| {
                        ingame_menu::siege_data::unchanged_choice(session.game_state(), &command)
                    });
                    if !unchanged {
                        self.send_menu_reliable(&command);
                    }
                    self.game_menu = false;
                    self.capture_pointer();
                }
                ingame_menu::CallVoteAction::Back => self.back_or_close_game_menu(),
                _ => self.game_menu_row = 0,
            },
            GameMenuPage::About => self.open_game_menu_page(GameMenuPage::Main),
            GameMenuPage::Leave => self.activate_leave_row(),
            page if page.is_vote_page() => self.activate_callvote_row(page),
            _ => {}
        }
    }

    fn activate_main_menu_row(&mut self) {
        let active = self.vote_active();
        match self.game_menu_row {
            0 => self.back_or_close_game_menu(),
            1 => {
                if !self.open_siege_classes() {
                    self.open_game_menu_page(GameMenuPage::Team);
                }
            }
            2 => self.open_browser_from_game(),
            3 => self.open_player_menu_from_game(),
            4 => {
                let game_state = self
                    .live_session
                    .as_ref()
                    .map(jkr_client::ClientSession::game_state);
                self.in_game_menu
                    .refresh_about(game_state, self.last_connect_address.as_deref());
                self.open_game_menu_page(GameMenuPage::About);
            }
            5 => self.open_settings_from_game(0),
            6 => self.open_settings_from_game(crate::settings::SettingsMenu::keybinds_tab()),
            7 => {
                let game_state = self
                    .live_session
                    .as_ref()
                    .map(jkr_client::ClientSession::game_state);
                self.in_game_menu
                    .refresh_callvote(game_state, self.vfs.as_deref());
                self.open_game_menu_page(GameMenuPage::CallVote);
            }
            8 if active => self.send_vote(true),
            9 if active => self.send_vote(false),
            row if row == if active { 10 } else { 8 } => self.open_shot_panel(),
            row if row == if active { 11 } else { 9 } => {
                self.open_game_menu_page(GameMenuPage::Leave)
            }
            _ => {}
        }
    }

    /// The leave page doubles as the confirmation: each entry is the action.
    fn activate_leave_row(&mut self) {
        match self.game_menu_row {
            0 => self.disconnect_to_menu(),
            1 => self.quit_requested = true,
            _ => self.open_game_menu_page(GameMenuPage::Main),
        }
    }

    fn activate_team_row(&mut self) {
        let team = if self.is_team_game() {
            [
                Some(jkr_client::LegacyTeamChoice::Auto),
                Some(jkr_client::LegacyTeamChoice::Red),
                Some(jkr_client::LegacyTeamChoice::Blue),
                Some(jkr_client::LegacyTeamChoice::Spectator),
                None,
            ]
            .get(self.game_menu_row)
            .copied()
            .flatten()
        } else {
            [
                Some(jkr_client::LegacyTeamChoice::Free),
                Some(jkr_client::LegacyTeamChoice::Spectator),
                None,
            ]
            .get(self.game_menu_row)
            .copied()
            .flatten()
        };
        if let Some(team) = team {
            self.select_team(team);
        } else {
            self.open_game_menu_page(GameMenuPage::Main);
        }
    }

    fn activate_callvote_row(&mut self, page: GameMenuPage) {
        match self.in_game_menu.callvote_action(page, self.game_menu_row) {
            ingame_menu::CallVoteAction::Open(page) => self.open_game_menu_page(page),
            ingame_menu::CallVoteAction::Back => self.back_or_close_game_menu(),
            ingame_menu::CallVoteAction::Send(command) => {
                self.send_menu_reliable(&command);
                self.open_game_menu_page(GameMenuPage::Main);
            }
            ingame_menu::CallVoteAction::None => {}
        }
    }

    /// Hand over to the shell's settings screen on `tab`; it returns to
    /// the game menu when closed.
    pub(crate) fn open_settings_from_game(&mut self, tab: usize) {
        if let (Some(menu), Some(console)) = (&mut self.client_menu, &self.console) {
            menu.open_settings_from(console, crate::player_menu::ReturnTarget::InGame, tab);
            self.game_menu = false;
        }
    }

    /// Hand over to the shell's server browser (japro's in-game entry); it
    /// returns to the game menu when closed, and a join leaves this server.
    fn open_browser_from_game(&mut self) {
        if let (Some(menu), Some(console)) = (&mut self.client_menu, &self.console) {
            menu.open_browser_from_game(console);
            menu.refresh_servers_if_stale();
            self.game_menu = false;
        }
    }

    pub(crate) fn is_team_game(&self) -> bool {
        self.live_session
            .as_ref()
            .and_then(|session| session.game_state().config_string(0))
            .and_then(|info| std::str::from_utf8(info).ok())
            .and_then(|info| InfoString::parse(info).ok())
            .and_then(|info| info.get_i32("g_gametype"))
            .is_some_and(|gametype| gametype >= 6)
    }

    pub(crate) fn vote_active(&self) -> bool {
        let Some(session) = &self.live_session else {
            return false;
        };
        let time = session.latest_snapshot().server_time;
        jkr_client::legacy_global_vote(session.game_state(), time).active
            || jkr_client::legacy_team_vote(
                session.game_state(),
                session.latest_snapshot().player.team(),
                time,
            )
            .is_some_and(|vote| vote.active)
    }

    pub(crate) fn send_vote(&mut self, yes: bool) {
        self.send_menu_reliable(if yes { "vote yes" } else { "vote no" });
    }

    fn send_menu_reliable(&mut self, command: &str) {
        if let Some(session) = &mut self.live_session
            && let Err(error) = session.send_reliable_command(command.as_bytes())
        {
            eprintln!("failed to send menu command: {error}");
        }
    }

    pub(crate) fn open_game_menu_page(&mut self, page: GameMenuPage) {
        self.game_menu_page = page;
        self.game_menu_row = 0;
    }

    /// Open the map's selector only in Siege; absent data is reported, never fabricated.
    pub(crate) fn open_siege_classes(&mut self) -> bool {
        let game = self.live_session.as_ref().map(ClientSession::game_state);
        if !ingame_menu::siege_data::is_siege(game) {
            return false;
        }
        let result = match (game, self.vfs.as_deref()) {
            (Some(game), Some(vfs)) => self.in_game_menu.refresh_siege(
                game,
                vfs,
                ingame_menu::siege_data::local_team(game),
            ),
            _ => Err("Siege assets are not available yet".into()),
        };
        match result {
            Ok(()) => {
                self.release_pointer();
                self.game_menu = true;
                self.open_game_menu_page(GameMenuPage::Siege);
            }
            Err(error) => {
                crate::log::progress(format_args!("Siege class selection: {error}"));
                if let Some(console) = &mut self.console {
                    console.push_log(error);
                }
            }
        }
        true
    }
}
