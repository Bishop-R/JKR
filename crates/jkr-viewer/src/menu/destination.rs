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
    /// The key-binding editor on category tab `category`, closing straight
    /// back to the main menu.
    Keybinds { category: usize },
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
            MainDestination::Keybinds { category } => {
                self.settings_return = ReturnTarget::MainMenu;
                self.keybinds.open_category(console, category);
                self.keybinds_direct = true;
                self.state.open_keybinds();
                MenuAction::None
            }
            MainDestination::Quit => MenuAction::Quit,
        }
    }

    /// Leave the key-binding editor: back to the settings screen that hosts
    /// it, or straight to wherever settings return when the editor was
    /// opened directly.
    pub(super) fn close_keybinds(&mut self, console: &ViewerConsole) -> MenuAction {
        if std::mem::take(&mut self.keybinds_direct) {
            return self.close_settings();
        }
        self.settings.open(console);
        self.state.open_settings();
        MenuAction::None
    }
}
