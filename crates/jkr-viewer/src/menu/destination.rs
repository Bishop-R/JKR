//! Screens the main menu opens, shared by every menu style so the modern
//! and classic layouts lead to the same places.

use super::*;

/// Where a main-menu entry leads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MainDestination {
    /// The server browser, refreshed when its rows are stale.
    Browser,
    /// The Create game screen.
    CreateGame,
    /// The Player screen (name, model, saber, Force).
    Player,
    /// The settings screen, on tab `tab`.
    Settings { tab: usize },
    /// Exit to the desktop.
    Quit,
}

impl ClientMenu {
    /// Open `destination` from the main menu.
    pub(super) fn open_main_destination(
        &mut self,
        destination: MainDestination,
        console: &mut ViewerConsole,
    ) -> MenuAction {
        match destination {
            MainDestination::Browser => {
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
            MainDestination::CreateGame => {
                self.open_create_game(console);
                MenuAction::None
            }
            MainDestination::Player => {
                self.open_player(console, ReturnTarget::MainMenu);
                MenuAction::None
            }
            MainDestination::Settings { tab } => {
                self.open_settings_from(console, ReturnTarget::MainMenu, tab);
                MenuAction::None
            }
            MainDestination::Quit => MenuAction::Quit,
        }
    }
}
