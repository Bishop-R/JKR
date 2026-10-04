//! Menu rendering dispatch and activation helpers.

use super::network_view::{self, NetworkNotice};
use super::*;

impl ClientMenu {
    /// Share the browser's favorites persistence with addFavorite.
    pub(crate) fn add_favorite(&mut self, address: std::net::SocketAddr) -> Result<(), String> {
        self.browser.add_favorite(address)
    }
    pub(crate) fn append_overlay(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        scale: f32,
    ) {
        match self.state.phase() {
            ClientPhase::MainMenu => self.append_main(vertices, font, viewport),
            ClientPhase::Browser => {
                let reveal = self.screen_reveal();
                self.append_browser(vertices, font, viewport, reveal);
            }
            ClientPhase::Settings => self.append_settings(vertices, font, viewport, scale),
            ClientPhase::Keybinds => {
                let reveal = self.screen_reveal();
                self.keybinds.append(vertices, font, viewport, reveal);
            }
            ClientPhase::Player => {
                let reveal = self.screen_reveal();
                self.player.append(vertices, font, viewport, reveal);
            }
            ClientPhase::CreateGame => {
                let reveal = self.screen_reveal();
                self.create_game.append(vertices, font, viewport, reveal);
            }
            ClientPhase::Connecting(_) => {
                let notice = NetworkNotice {
                    kicker: "NETWORK",
                    title: "Loading",
                    status: self.state.status(),
                    body: "Joining the server and preparing its map...",
                    action: ("Cancel", "Stop joining and return to the browser"),
                };
                network_view::build(&mut self.ui, viewport, &notice);
                self.ui.append_text(vertices, font, viewport);
            }
            ClientPhase::ConnectionError => {
                let notice = NetworkNotice {
                    kicker: "NETWORK",
                    title: "Connection failed",
                    status: self.state.status(),
                    body: "The client returned safely. Check the address or server status.",
                    action: ("Back to browser", "Pick another server or retry"),
                };
                network_view::build(&mut self.ui, viewport, &notice);
                self.ui.append_text(vertices, font, viewport);
            }
            ClientPhase::InGame => {}
        }
    }

    pub(super) fn activate_main(&mut self, console: &mut ViewerConsole) -> MenuAction {
        match self.main_selection {
            0 => {
                if let Ok(master) = console.master_server() {
                    self.browser.set_master(master);
                }
                self.open_browser();
                // Rows fetched at start-up (or moments ago) show at once; a
                // fetch is only started when there is nothing fresh to show.
                if self.browser.is_stale() {
                    self.refresh();
                } else if self.browser.is_refreshing() {
                    self.state.set_status("Refreshing master server...");
                } else {
                    self.state.set_status(format!(
                        "{} responding servers. Enter joins; R refreshes.",
                        self.browser.entries().len()
                    ));
                }
                MenuAction::None
            }
            1 => {
                self.open_create_game(console);
                MenuAction::None
            }
            2 => {
                self.open_player(console, ReturnTarget::MainMenu);
                MenuAction::None
            }
            3 => {
                self.open_settings_from(console, ReturnTarget::MainMenu, 0);
                MenuAction::None
            }
            4 => MenuAction::Quit,
            _ => MenuAction::None,
        }
    }

    pub(super) fn refresh(&mut self) {
        self.browser.refresh();
        self.state.set_status("Refreshing master server...");
    }

    pub(super) fn join_selected(&mut self) -> MenuAction {
        let Some(entry) = self.browser.visible_entry(self.browser.selected()) else {
            return MenuAction::None;
        };
        let server = entry.address;
        let map = entry.map.clone();
        if entry.password {
            self.password.clear();
            self.password_target = Some(server.to_string());
            self.destination_map = Some(map);
            return MenuAction::None;
        }
        let address = server.to_string();
        self.destination_map = Some(map);
        self.state.connecting(address.clone());
        MenuAction::Connect(address)
    }

    pub(super) fn activate_browser_focus(&mut self) -> MenuAction {
        use super::browser_view::{ADDRESS_TOKEN, BACK_TOKEN, FAVOURITE_TOKEN, REFRESH_TOKEN};
        match self.browser_focus {
            REFRESH_TOKEN => {
                self.refresh();
                MenuAction::None
            }
            FAVOURITE_TOKEN => {
                self.browser.toggle_selected_favorite();
                MenuAction::None
            }
            BACK_TOKEN => self.close_browser(),
            ADDRESS_TOKEN => {
                self.open_address_entry();
                MenuAction::None
            }
            _ => self.join_selected(),
        }
    }

    pub(super) fn open_address_entry(&mut self) {
        self.address_editing = true;
        self.address_error.clear();
    }

    pub(super) fn submit_address(&mut self) -> MenuAction {
        match jkr_client::LegacyServerAddress::parse(&self.address_input) {
            Ok(address) => {
                let address = address.into_string();
                self.address_editing = false;
                self.address_error.clear();
                self.destination_map = None;
                self.state.connecting(address.clone());
                MenuAction::Connect(address)
            }
            Err(error) => {
                self.address_error.clear();
                use std::fmt::Write as _;
                let _ = write!(self.address_error, "Bad server address: {error}");
                MenuAction::None
            }
        }
    }

    pub(crate) fn set_last_address(&mut self, address: &str) {
        self.address_input.clear();
        self.address_input.push_str(address);
    }

    pub(super) fn append_settings(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        scale: f32,
    ) {
        let reveal = self.screen_reveal();
        self.settings
            .append(vertices, font, viewport, scale, reveal);
    }

    pub(super) fn navigate_main(&mut self, action: AbstractAction) {
        if let Some(row) = self
            .ui
            .action(action)
            .map(usize::from)
            .filter(|row| *row < MAIN_ITEMS.len())
        {
            self.main_selection = row;
        } else {
            self.main_selection = if action == AbstractAction::Previous {
                self.main_selection
                    .checked_sub(1)
                    .unwrap_or(MAIN_ITEMS.len() - 1)
            } else {
                (self.main_selection + 1) % MAIN_ITEMS.len()
            };
        }
    }
}
