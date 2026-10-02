//! Classic menu style (`ui_menuStyle classic`): screens laid out close to
//! the retail Jedi Academy multiplayer menus so long-time players find
//! things where they expect them. Not a port of the `.menu` scripts: pages
//! and entries are declared in [`layout`], drawn with the shared menu
//! widgets in [`view`], and lead to the same screens as the modern style
//! through [`MainDestination`].
//!
//! Implemented: the main menu with its Play (multiplayer), Controls, Setup
//! and quit pages, and the in-game menu ([`crate::ingame_menu`]). The
//! screens these pages open (server browser, Create game, Player, settings,
//! key bindings) are still the modern ones; the follow-up plan is kept in
//! `docs/client.md`.

pub(crate) mod layout;
mod pages;
pub(crate) mod view;

use super::{ClientMenu, MenuAction};
use crate::console::ViewerConsole;
use crate::menu::destination::MainDestination;
use crate::settings::SettingsMenu;
use jkr_ui::AbstractAction;
use layout::{Outcome, Page, Slot};

/// Page and focused entry of the classic main menu.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ClassicMain {
    page: Page,
    selection: usize,
}

impl ClassicMain {
    pub(crate) const fn new() -> Self {
        Self {
            page: Page::Main,
            selection: 0,
        }
    }

    /// Back to the opening page, focus on its first entry.
    pub(crate) fn reset(&mut self) {
        *self = Self::new();
    }

    pub(crate) fn page(&self) -> Page {
        self.page
    }

    pub(crate) fn selection(&self) -> usize {
        self.selection
    }

    /// The page's entries in focus order.
    pub(crate) fn slots(&self) -> &'static [Slot] {
        self.page.slots()
    }

    /// Focus entry `index` of the current page; out-of-range indices (a
    /// pointer token of another screen) are ignored.
    pub(crate) fn select(&mut self, index: usize) {
        if index < self.slots().len() {
            self.selection = index;
        }
    }

    /// Step focus forward or back in the page's entry order, wrapping.
    fn step(&mut self, forward: bool) {
        let count = self.slots().len();
        self.selection = if forward {
            (self.selection + 1) % count
        } else {
            self.selection.checked_sub(1).unwrap_or(count - 1)
        };
    }

    /// Show `page` with its initial entry focused.
    pub(crate) fn show(&mut self, page: Page) {
        self.page = page;
        self.selection = page.initial_selection();
    }

    /// What activating the focused entry does.
    fn outcome(&self) -> Option<Outcome> {
        Some(self.slots().get(self.selection)?.entry.outcome())
    }
}

impl ClientMenu {
    /// Move focus on the classic main menu. Focus follows the page's entry
    /// order, as retail Up/Down/Tab did.
    pub(super) fn navigate_classic(&mut self, action: AbstractAction) {
        let count = self.classic.slots().len();
        match self
            .ui
            .action(action)
            .map(usize::from)
            .filter(|row| *row < count)
        {
            Some(row) => self.classic.select(row),
            None => self.classic.step(action != AbstractAction::Previous),
        }
    }

    /// Activate the focused classic entry: change page, or open the screen
    /// it leads to.
    pub(super) fn activate_classic(&mut self, console: &mut ViewerConsole) -> MenuAction {
        match self.classic.outcome() {
            Some(Outcome::Page(page)) => {
                self.classic.show(page);
                MenuAction::None
            }
            Some(Outcome::Open(destination)) => self.open_main_destination(destination, console),
            Some(Outcome::Settings(caption)) => {
                let tab = SettingsMenu::tab_index(caption).unwrap_or(0);
                self.open_main_destination(MainDestination::Settings { tab }, console)
            }
            Some(Outcome::Keybinds(category)) => self.open_main_destination(
                MainDestination::Keybinds {
                    category: category as usize,
                },
                console,
            ),
            Some(Outcome::Unavailable) | None => MenuAction::None,
        }
    }

    /// Escape on the classic main menu: the opening page asks to quit, the
    /// others return to it.
    pub(super) fn cancel_classic(&mut self) {
        // Routed through the canvas for its "back" sound cue.
        let _ = self.ui.action(AbstractAction::Cancel);
        let page = self.classic.page().escape();
        self.classic.show(page);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keybind_editor::Category;
    use layout::Entry;

    fn focus(menu: &mut ClassicMain, entry: Entry) {
        let index = menu.page().index_of(entry).expect("entry on page");
        menu.select(index);
    }

    #[test]
    fn exit_asks_before_quitting() {
        let mut menu = ClassicMain::new();
        focus(&mut menu, Entry::Exit);
        assert_eq!(menu.outcome(), Some(Outcome::Page(Page::Quit)));
        menu.show(Page::Quit);
        // The quit page opens on No, so a second Enter does not quit.
        assert_eq!(menu.outcome(), Some(Outcome::Page(Page::Main)));
        focus(&mut menu, Entry::Yes);
        assert_eq!(menu.outcome(), Some(Outcome::Open(MainDestination::Quit)));
    }

    #[test]
    fn play_opens_the_multiplayer_page() {
        let mut menu = ClassicMain::new();
        assert_eq!(menu.outcome(), Some(Outcome::Page(Page::Play)));
        menu.show(Page::Play);
        // Retail focuses Solo Game, a local match with bots.
        assert_eq!(
            menu.outcome(),
            Some(Outcome::Open(MainDestination::CreateGame))
        );
        focus(&mut menu, Entry::JoinServer);
        assert_eq!(
            menu.outcome(),
            Some(Outcome::Open(MainDestination::Browser))
        );
        focus(&mut menu, Entry::PlayDemo);
        assert_eq!(menu.outcome(), Some(Outcome::Unavailable));
    }

    #[test]
    fn controls_pages_open_the_key_binding_tabs() {
        let mut menu = ClassicMain::new();
        focus(&mut menu, Entry::Controls);
        assert_eq!(menu.outcome(), Some(Outcome::Page(Page::Controls)));
        menu.show(Page::Controls);
        assert_eq!(menu.outcome(), Some(Outcome::Keybinds(Category::Movement)));
        focus(&mut menu, Entry::ForcePowers2);
        assert_eq!(menu.outcome(), Some(Outcome::Keybinds(Category::Force)));
        focus(&mut menu, Entry::MouseJoystick);
        assert_eq!(menu.outcome(), Some(Outcome::Settings("CONTROLS")));
    }

    #[test]
    fn setup_pages_open_settings_tabs() {
        let mut menu = ClassicMain::new();
        menu.show(Page::Setup);
        assert_eq!(menu.outcome(), Some(Outcome::Settings("VIDEO")));
        focus(&mut menu, Entry::Sound);
        assert_eq!(menu.outcome(), Some(Outcome::Settings("AUDIO")));
        focus(&mut menu, Entry::Mods);
        assert_eq!(menu.outcome(), Some(Outcome::Unavailable));
    }

    #[test]
    fn focus_wraps_and_ignores_foreign_tokens() {
        let mut menu = ClassicMain::new();
        menu.step(false);
        assert_eq!(menu.selection(), menu.slots().len() - 1);
        menu.step(true);
        assert_eq!(menu.selection(), 0);
        menu.select(500);
        assert_eq!(menu.selection(), 0);
        menu.show(Page::Quit);
        menu.reset();
        assert_eq!(menu, ClassicMain::new());
    }
}
