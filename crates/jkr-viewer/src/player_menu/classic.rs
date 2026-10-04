//! Classic profile pages (`ui_menuStyle classic`): the retail `player`,
//! `player2` and `saber` menus of the main menu, and their in-game windows
//! `ingame_player`, `ingame_player2` and `ingame_saber`, laid out from the
//! retail `ui/jamp` item rectangles on the 640x480 menu canvas.
//!
//! They edit the same drafts as the modern player screen and write them the
//! same way (immediately), so the two styles cannot disagree. What differs
//! is the presentation and the flow: the profile page's head grid, Custom
//! leading to character creation, and Apply leading on to lightsaber
//! creation, as retail had it. Entry geometry and focus order are in
//! [`layout`], drawing in [`view`], pointer routing in [`pointer`].
//!
//! Retail drew a live 3D model on character creation and a spinning saber on
//! lightsaber creation. Without a world behind the menu there is nothing to
//! render them into yet, so the model's portrait and a drawn blade stand in.

mod layout;
mod pointer;
mod view;

use super::controller::wrap;
use super::saber::SaberStyle;
use super::*;
use crate::menu::art::ArtSet;
use crate::menu::classic::layout::Page as MainPage;
use layout::Item;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/// The three retail profile screens.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum ClassicPage {
    /// `player` / `ingame_player`: name, team colour and the head grid.
    #[default]
    Player,
    /// `player2` / `ingame_player2`: species, skin tint and parts.
    Character,
    /// `saber` / `ingame_saber`: saber type, hilts and blade colours.
    Saber,
}

/// Where the pages are drawn: full screen from the main menu, or as the
/// retail in-game window over a match.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Frame {
    Full,
    InGame,
}

/// Retail blade colour swatches, left to right, as `color1` indices.
pub(super) const BLADE_SWATCHES: [u8; 6] = [4, 3, 1, 5, 2, 0];

/// Classic page, focus and list scroll state of the player screen.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ClassicState {
    pub(super) page: ClassicPage,
    /// Focused entry: an index into the page's [`layout::items`].
    pub(super) focus: usize,
    /// Which part list character creation shows: 0 head, 1 torso, 2 legs.
    pub(super) part_axis: usize,
    /// First visible column of the part and tint lists.
    pub(super) part_scroll: usize,
    pub(super) tint_scroll: usize,
    /// First visible row of each hilt list.
    pub(super) hilt_scroll: [usize; 2],
    /// Retail artwork that can be drawn this frame.
    pub(super) art: ArtSet,
}

impl PlayerMenu {
    /// Apply `ui_menuStyle` to this screen.
    pub(crate) fn set_style(&mut self, classic: bool, art: ArtSet) {
        self.classic.art = art;
        if classic != self.classic_style {
            self.classic_style = classic;
            self.selected = 0;
            self.name_editing = false;
            if classic {
                self.show_classic(ClassicPage::Player);
            }
        }
    }

    /// Whether the classic pages are in use.
    pub(crate) fn is_classic(&self) -> bool {
        self.classic_style
    }

    pub(super) fn frame(&self) -> Frame {
        match self.return_target {
            ReturnTarget::MainMenu => Frame::Full,
            ReturnTarget::InGame => Frame::InGame,
        }
    }

    /// Entries of the page on show, in focus order.
    pub(super) fn classic_items(&self) -> &'static [Item] {
        layout::items(
            self.classic.page,
            self.frame(),
            self.saber.style() == SaberStyle::Dual,
        )
    }

    pub(super) fn classic_focused(&self) -> Option<Item> {
        self.classic_items().get(self.classic.focus).copied()
    }

    /// Show `page` with focus on its first entry after the navigation row.
    pub(super) fn show_classic(&mut self, page: ClassicPage) {
        self.numeric = None;
        self.name_editing = false;
        self.classic.page = page;
        let items = self.classic_items();
        self.classic.focus = items.iter().position(|item| !item.is_nav()).unwrap_or(0);
        if page == ClassicPage::Character {
            self.enter_character_creation();
        }
    }

    /// Character creation edits a species model; coming from an ordinary
    /// character, the first species is put on, as retail's Custom did.
    fn enter_character_creation(&mut self) {
        if matches!(self.choice, Some(Choice::Species(_))) {
            return;
        }
        let Some(first) = self
            .catalog()
            .filter(|catalog| !catalog.species.is_empty())
            .map(|catalog| catalog.characters.len())
        else {
            return;
        };
        self.select_choice(first);
        self.classic_dirty = true;
    }

    /// The species being edited on character creation.
    pub(super) fn current_species(&self) -> Option<usize> {
        match self.choice {
            Some(Choice::Species(index)) => Some(index),
            _ => None,
        }
    }

    fn cycle_species(&mut self, direction: isize) {
        let Some((characters, count)) = self
            .catalog()
            .map(|catalog| (catalog.characters.len(), catalog.species.len()))
        else {
            return;
        };
        if count == 0 {
            return;
        }
        let next = match self.current_species() {
            Some(index) => wrap(index, direction, count),
            None => 0,
        };
        self.select_choice(characters + next);
    }

    /// Step the focused entry left or right and write the change.
    pub(super) fn classic_adjust(&mut self, console: &mut ViewerConsole, direction: isize) {
        let Some(item) = self.classic_focused() else {
            return;
        };
        match item {
            Item::Team => self.cycle_team(direction),
            Item::Models => {
                self.cycle_model(direction);
            }
            Item::Species => self.cycle_species(direction),
            Item::Tints => self.cycle_variant(3, direction),
            Item::PartHead | Item::PartTorso | Item::PartLegs => {
                self.classic.part_axis = wrap(self.classic.part_axis, direction, 3);
                self.classic.part_scroll = 0;
                return;
            }
            Item::Parts => self.cycle_variant(self.classic.part_axis, direction),
            Item::Single | Item::Dual | Item::Staff => {
                let order = [SaberStyle::Single, SaberStyle::Dual, SaberStyle::Staff];
                let index = order
                    .iter()
                    .position(|style| *style == self.saber.style())
                    .unwrap_or(0);
                let style = order[wrap(index, direction, order.len())];
                self.set_saber_style(console, style);
                return;
            }
            Item::Hilts | Item::Hilts2 => {
                let catalog = catalog_of(&self.loader);
                let row = if item == Item::Hilts {
                    super::rows::SaberRow::Hilt
                } else {
                    super::rows::SaberRow::SecondHilt
                };
                self.saber.adjust(row, direction, catalog);
                self.saber.apply(console);
                return;
            }
            Item::Blades | Item::Blades2 => {
                let second = item == Item::Blades2;
                let current = BLADE_SWATCHES
                    .iter()
                    .position(|index| *index == self.saber.color(second))
                    .unwrap_or(0);
                let next = BLADE_SWATCHES[wrap(current, direction, BLADE_SWATCHES.len())];
                self.saber.select_color(second, next);
                self.saber.apply(console);
                return;
            }
            _ => return,
        }
        self.grid_follow = true;
        self.apply(console);
    }

    /// Enter or a click on the focused entry.
    pub(super) fn classic_activate(&mut self, console: &mut ViewerConsole) -> PlayerMenuResult {
        let Some(item) = self.classic_focused() else {
            return PlayerMenuResult::None;
        };
        let frame = self.frame();
        match item {
            Item::NavPlay => PlayerMenuResult::ClassicPage(MainPage::Play),
            Item::NavControls => PlayerMenuResult::ClassicPage(MainPage::Controls),
            Item::NavSetup => PlayerMenuResult::ClassicPage(MainPage::Setup),
            Item::NavProfile => {
                if self.classic.page != ClassicPage::Player {
                    self.show_classic(ClassicPage::Player);
                }
                PlayerMenuResult::None
            }
            Item::Exit => PlayerMenuResult::ClassicPage(MainPage::Quit),
            Item::Name => {
                self.name_before_edit.clone_from(&self.draft.name);
                self.name_editing = true;
                PlayerMenuResult::None
            }
            Item::Custom => {
                self.show_classic(ClassicPage::Character);
                self.write_if_dirty(console);
                PlayerMenuResult::None
            }
            Item::SaberButton => {
                self.show_classic(ClassicPage::Saber);
                PlayerMenuResult::None
            }
            Item::PartHead | Item::PartTorso | Item::PartLegs => {
                self.classic.part_axis = item.part_axis().unwrap_or(0);
                self.classic.part_scroll = 0;
                PlayerMenuResult::None
            }
            Item::Single => self.style_result(console, SaberStyle::Single),
            Item::Dual => self.style_result(console, SaberStyle::Dual),
            Item::Staff => self.style_result(console, SaberStyle::Staff),
            Item::Back => {
                self.show_classic(ClassicPage::Player);
                PlayerMenuResult::None
            }
            // Changes are already written; Apply moves on as retail's did.
            Item::Apply => match (frame, self.classic.page) {
                (Frame::Full, ClassicPage::Player | ClassicPage::Character) => {
                    self.show_classic(ClassicPage::Saber);
                    PlayerMenuResult::None
                }
                (Frame::Full, ClassicPage::Saber) => PlayerMenuResult::None,
                (Frame::InGame, _) => PlayerMenuResult::Back(ReturnTarget::InGame),
            },
            Item::ApplyMain => PlayerMenuResult::Back(ReturnTarget::MainMenu),
            _ => {
                self.classic_adjust(console, 1);
                PlayerMenuResult::None
            }
        }
    }

    fn style_result(&mut self, console: &mut ViewerConsole, style: SaberStyle) -> PlayerMenuResult {
        self.set_saber_style(console, style);
        PlayerMenuResult::None
    }

    fn set_saber_style(&mut self, console: &mut ViewerConsole, style: SaberStyle) {
        let catalog = catalog_of(&self.loader);
        self.saber.set_style(style, catalog);
        self.saber.apply(console);
        self.classic.hilt_scroll = [0; 2];
        // The second-saber entries come and go with Dual.
        let count = self.classic_items().len();
        self.classic.focus = self.classic.focus.min(count.saturating_sub(1));
    }

    /// Write the character draft if entering a page changed it.
    fn write_if_dirty(&mut self, console: &mut ViewerConsole) {
        if std::mem::take(&mut self.classic_dirty) {
            self.apply(console);
        }
    }

    /// Escape: back to the profile page from the others; from the profile
    /// page, back to where the screen was opened.
    fn classic_escape(&mut self) -> PlayerMenuResult {
        if self.classic.page == ClassicPage::Player {
            return PlayerMenuResult::Back(self.return_target);
        }
        self.show_classic(ClassicPage::Player);
        PlayerMenuResult::None
    }

    /// Keyboard on the classic pages: Up/Down/Tab move focus in entry order,
    /// Left/Right step the focused list or chooser, Enter activates.
    pub(super) fn classic_key(
        &mut self,
        event: &KeyEvent,
        console: &mut ViewerConsole,
    ) -> PlayerMenuResult {
        if event.state != ElementState::Pressed {
            return PlayerMenuResult::None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return PlayerMenuResult::None;
        };
        if self.name_editing {
            return self.edit_name(event, key, console);
        }
        self.write_if_dirty(console);
        let count = self.classic_items().len().max(1);
        match key {
            KeyCode::Escape if !event.repeat => return self.classic_escape(),
            KeyCode::ArrowUp | KeyCode::KeyW => {
                self.classic.focus = self.classic.focus.checked_sub(1).unwrap_or(count - 1);
            }
            KeyCode::ArrowDown | KeyCode::KeyS | KeyCode::Tab => {
                self.classic.focus = (self.classic.focus + 1) % count;
            }
            KeyCode::ArrowLeft | KeyCode::KeyA => self.classic_adjust(console, -1),
            KeyCode::ArrowRight | KeyCode::KeyD => self.classic_adjust(console, 1),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space if !event.repeat => {
                return self.classic_activate(console);
            }
            _ => {}
        }
        PlayerMenuResult::None
    }
}

#[cfg(test)]
mod tests {
    use super::layout::{Item, items};
    use super::*;

    #[test]
    fn swatches_are_the_six_stock_colours_once() {
        let mut sorted = BLADE_SWATCHES;
        sorted.sort_unstable();
        assert_eq!(sorted, [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn pages_lead_on_as_retail() {
        let mut menu = PlayerMenu::new();
        menu.return_target = ReturnTarget::MainMenu;
        menu.classic_style = true;
        menu.show_classic(ClassicPage::Player);
        assert_eq!(menu.classic_focused(), Some(Item::Name));
        let full = items(ClassicPage::Player, Frame::Full, false);
        assert!(full.contains(&Item::Custom) && full.contains(&Item::Apply));
        assert!(full.contains(&Item::NavPlay) && full.contains(&Item::Exit));
        let in_game = items(ClassicPage::Player, Frame::InGame, false);
        assert!(
            !in_game
                .iter()
                .any(|item| item.is_nav() || *item == Item::Exit)
        );
        assert!(in_game.contains(&Item::SaberButton));
        assert_eq!(
            menu.classic_escape(),
            PlayerMenuResult::Back(ReturnTarget::MainMenu)
        );
        menu.show_classic(ClassicPage::Saber);
        assert_eq!(menu.classic_escape(), PlayerMenuResult::None);
        assert_eq!(menu.classic.page, ClassicPage::Player);
    }

    #[test]
    fn second_saber_entries_follow_dual() {
        let single = items(ClassicPage::Saber, Frame::Full, false);
        let dual = items(ClassicPage::Saber, Frame::Full, true);
        assert!(!single.contains(&Item::Hilts2) && !single.contains(&Item::Blades2));
        assert!(dual.contains(&Item::Hilts2) && dual.contains(&Item::Blades2));
    }
}
