//! Pages, entries and geometry of the classic main menu, taken from the
//! retail multiplayer menus (`ui/jamp/main.menu`, `multiplayer.menu`,
//! `controls.menu`, `setup.menu`, `quit.menu`): which entries each page has,
//! in which order, where they sit on the original 640x480 menu canvas, and
//! where each one leads. The page tables themselves are in [`super::pages`].
//!
//! Positions are the retail item rectangles reduced to a text centre; labels
//! are the retail words. Hints and titles are JKR's own text.

use crate::keybind_editor::Category;
use crate::menu::destination::MainDestination;
use jkr_ui::{Rect, TextAlign};

/// Size of the canvas the retail menus are authored on.
pub(crate) const CANVAS: [f32; 2] = [640.0, 480.0];
/// Vertical centre of the description line under the entries (retail
/// `descY` 424, plus half a line).
pub(crate) const HINT_Y: f32 = 432.0;
/// Area of the retail game logo at the top of every page.
pub(crate) const LOGO: [f32; 4] = [107.0, 8.0, 428.0, 112.0];

/// One screen of the classic main menu.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Page {
    /// The opening menu: Play, Profile, Controls, Setup and Exit.
    Main,
    /// Retail "multiplayer" menu behind Play: solo, join or create a game.
    Play,
    /// Retail "controls" menu: the key-binding pages and mouse options.
    Controls,
    /// Retail "setup" menu: video, sound and game options.
    Setup,
    /// Retail quit confirmation behind Exit and Escape.
    Quit,
}

/// One selectable entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Entry {
    Play,
    Profile,
    Controls,
    Setup,
    Exit,
    SoloGame,
    JoinServer,
    CreateServer,
    PlayDemo,
    Rules,
    Movement,
    Interaction,
    Weapons,
    ForcePowers1,
    ForcePowers2,
    MouseJoystick,
    OtherControls,
    Video,
    MoreVideo,
    Sound,
    GameOptions,
    Mods,
    Defaults,
    Hud,
    Network,
    Back,
    No,
    Yes,
}

/// Label size class: the main page's big buttons, the smaller ones of the
/// navigation row and centre lists, and the option lists of Controls and
/// Setup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Size {
    Large,
    Medium,
    List,
}

impl Size {
    /// Label height on the 640x480 canvas.
    pub(crate) fn text(self) -> f32 {
        match self {
            Self::Large => 22.0,
            Self::Medium => 17.0,
            Self::List => 14.0,
        }
    }
}

/// One entry placed on a page.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Slot {
    pub(crate) entry: Entry,
    pub(crate) label: &'static str,
    /// The description line shown while the entry has focus; for an entry
    /// JKR cannot open yet, the note saying so.
    pub(crate) hint: &'static str,
    /// Centre of the target on the 640x480 canvas.
    pub(crate) center: [f32; 2],
    /// Width of the pointer target and focus glow on the canvas: the
    /// retail item width.
    pub(crate) width: f32,
    /// Height of the pointer target and focus glow on the canvas.
    pub(crate) height: f32,
    pub(crate) size: Size,
    /// Label alignment inside the target: centred buttons, or the option
    /// lists' labels set against their right edge as retail does.
    pub(crate) align: TextAlign,
}

impl Slot {
    /// Pointer target and focus glow on the 640x480 canvas.
    pub(crate) fn target(&self) -> [f32; 4] {
        [
            self.center[0] - self.width * 0.5,
            self.center[1] - self.height * 0.5,
            self.width,
            self.height,
        ]
    }

    /// Whether activating the entry does something; the others are drawn
    /// dimmed with their note as the hint.
    pub(crate) fn enabled(&self) -> bool {
        self.entry.outcome() != Outcome::Unavailable
    }
}

impl Page {
    /// The page's entries in focus order.
    pub(crate) fn slots(self) -> &'static [Slot] {
        super::pages::slots(self)
    }

    /// Entry focused when the page opens: the first entry of the page's own
    /// list, as retail sets focus; the quit page starts on No.
    pub(crate) fn initial_selection(self) -> usize {
        let entry = match self {
            Self::Main => Entry::Play,
            Self::Play => Entry::SoloGame,
            Self::Controls => Entry::Movement,
            Self::Setup => Entry::Video,
            Self::Quit => Entry::No,
        };
        self.index_of(entry).unwrap_or(0)
    }

    /// Position of `entry` on this page.
    pub(crate) fn index_of(self, entry: Entry) -> Option<usize> {
        self.slots().iter().position(|slot| slot.entry == entry)
    }

    /// Page heading and its vertical centre on the canvas.
    pub(crate) fn title(self) -> (&'static str, f32) {
        match self {
            Self::Main => ("MULTIPLAYER", 132.0),
            Self::Play => ("START PLAYING", 172.0),
            Self::Controls => ("CONFIGURE CONTROLS", 172.0),
            Self::Setup => ("SETUP OPTIONS", 172.0),
            Self::Quit => ("QUIT", 172.0),
        }
    }

    /// Where Escape leads: the main page asks to quit, as retail does;
    /// every other page returns to the main page.
    pub(crate) fn escape(self) -> Page {
        match self {
            Self::Main => Self::Quit,
            _ => Self::Main,
        }
    }
}

/// What activating an entry does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Outcome {
    /// Show another classic page.
    Page(Page),
    /// Leave the main menu for a screen it opens.
    Open(MainDestination),
    /// Open the settings screen on the tab with this caption.
    Settings(&'static str),
    /// Open the key-binding editor on this category.
    Keybinds(Category),
    /// A retail screen JKR has no equivalent for yet: nothing happens.
    Unavailable,
}

impl Entry {
    /// What activating this entry does.
    pub(crate) fn outcome(self) -> Outcome {
        match self {
            Self::Play => Outcome::Page(Page::Play),
            Self::Controls => Outcome::Page(Page::Controls),
            Self::Setup => Outcome::Page(Page::Setup),
            Self::Exit => Outcome::Page(Page::Quit),
            Self::Back | Self::No => Outcome::Page(Page::Main),
            Self::Profile => Outcome::Open(MainDestination::Player),
            Self::JoinServer => Outcome::Open(MainDestination::Browser),
            // Retail's Solo Game is a local match with bots, which is what
            // Create game hosts.
            Self::SoloGame | Self::CreateServer => Outcome::Open(MainDestination::CreateGame),
            Self::Yes => Outcome::Open(MainDestination::Quit),
            Self::Movement => Outcome::Keybinds(Category::Movement),
            Self::Interaction => Outcome::Keybinds(Category::Interaction),
            Self::Weapons => Outcome::Keybinds(Category::Weapons),
            Self::ForcePowers1 | Self::ForcePowers2 => Outcome::Keybinds(Category::Force),
            Self::OtherControls => Outcome::Keybinds(Category::Other),
            Self::MouseJoystick => Outcome::Settings("CONTROLS"),
            Self::Video | Self::MoreVideo => Outcome::Settings("VIDEO"),
            Self::Sound => Outcome::Settings("AUDIO"),
            Self::GameOptions => Outcome::Settings("GAME"),
            Self::Hud => Outcome::Settings("HUD"),
            Self::Network => Outcome::Settings("NETWORK"),
            Self::PlayDemo | Self::Rules | Self::Mods | Self::Defaults => Outcome::Unavailable,
        }
    }
}

/// The 640x480 canvas fitted into the window: scaled to its height (or
/// width, on a portrait window) and centred, so wide screens keep the
/// retail proportions instead of stretching them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Placement {
    pub(crate) origin: [f32; 2],
    pub(crate) scale: f32,
}

impl Placement {
    pub(crate) fn new(viewport: [f32; 2]) -> Self {
        let scale = (viewport[0] / CANVAS[0]).min(viewport[1] / CANVAS[1]);
        Self {
            origin: [
                (viewport[0] - CANVAS[0] * scale) * 0.5,
                (viewport[1] - CANVAS[1] * scale) * 0.5,
            ],
            scale,
        }
    }

    /// Window rectangle of a canvas rectangle `[x, y, width, height]`.
    pub(crate) fn rect(&self, [x, y, width, height]: [f32; 4]) -> Rect {
        Rect::new(
            self.origin[0] + x * self.scale,
            self.origin[1] + y * self.scale,
            width * self.scale,
            height * self.scale,
        )
    }

    /// Window rectangle `width` canvas units wide and `height` high, centred
    /// on canvas point `center`.
    pub(crate) fn centered(&self, center: [f32; 2], width: f32, height: f32) -> Rect {
        self.rect([
            center[0] - width * 0.5,
            center[1] - height * 0.5,
            width,
            height,
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SettingsMenu;

    const PAGES: [Page; 5] = [
        Page::Main,
        Page::Play,
        Page::Controls,
        Page::Setup,
        Page::Quit,
    ];

    fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
        a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
    }

    fn entries(page: Page) -> Vec<Entry> {
        page.slots().iter().map(|slot| slot.entry).collect()
    }

    #[test]
    fn pages_keep_the_retail_entry_order() {
        assert_eq!(
            entries(Page::Main),
            [
                Entry::Play,
                Entry::Profile,
                Entry::Controls,
                Entry::Setup,
                Entry::Exit
            ]
        );
        assert_eq!(
            entries(Page::Play)[4..9],
            [
                Entry::SoloGame,
                Entry::JoinServer,
                Entry::CreateServer,
                Entry::PlayDemo,
                Entry::Rules
            ]
        );
        assert_eq!(
            entries(Page::Controls)[4..11],
            [
                Entry::Movement,
                Entry::Interaction,
                Entry::Weapons,
                Entry::ForcePowers1,
                Entry::ForcePowers2,
                Entry::MouseJoystick,
                Entry::OtherControls
            ]
        );
        assert_eq!(
            entries(Page::Setup)[4..10],
            [
                Entry::Video,
                Entry::MoreVideo,
                Entry::Sound,
                Entry::GameOptions,
                Entry::Mods,
                Entry::Defaults
            ]
        );
    }

    #[test]
    fn targets_fit_the_canvas_without_overlapping() {
        for page in PAGES {
            let slots = page.slots();
            for (index, slot) in slots.iter().enumerate() {
                let [x, y, width, height] = slot.target();
                assert!(x >= 0.0 && y >= 0.0, "{page:?} {:?}", slot.entry);
                assert!(x + width <= CANVAS[0] && y + height <= CANVAS[1]);
                for other in &slots[index + 1..] {
                    assert!(
                        !overlaps(slot.target(), other.target()),
                        "{page:?}: {:?} overlaps {:?}",
                        slot.entry,
                        other.entry
                    );
                }
            }
        }
    }

    #[test]
    fn pages_open_on_an_enabled_entry() {
        for page in PAGES {
            assert!(page.slots()[page.initial_selection()].enabled(), "{page:?}");
        }
        assert_eq!(
            Page::Quit.slots()[Page::Quit.initial_selection()].entry,
            Entry::No
        );
    }

    #[test]
    fn only_yes_quits() {
        for page in PAGES {
            for slot in page.slots() {
                let quits = slot.entry.outcome() == Outcome::Open(MainDestination::Quit);
                assert_eq!(quits, slot.entry == Entry::Yes, "{:?}", slot.entry);
            }
        }
        assert_eq!(Page::Main.escape(), Page::Quit);
        for page in [Page::Play, Page::Controls, Page::Setup, Page::Quit] {
            assert_eq!(page.escape(), Page::Main);
        }
    }

    #[test]
    fn settings_entries_name_real_tabs() {
        for page in PAGES {
            for slot in page.slots() {
                if let Outcome::Settings(caption) = slot.entry.outcome() {
                    assert!(
                        SettingsMenu::tab_index(caption).is_some(),
                        "{:?} names missing tab {caption}",
                        slot.entry
                    );
                }
            }
        }
    }

    #[test]
    fn unavailable_entries_say_so() {
        for page in PAGES {
            for slot in page.slots() {
                assert!(!slot.hint.is_empty(), "{:?}", slot.entry);
                assert_eq!(
                    !slot.enabled(),
                    slot.hint.starts_with("Not in JKR yet"),
                    "{:?}",
                    slot.entry
                );
            }
        }
    }

    #[test]
    fn main_page_reaches_every_page_and_screen() {
        let reachable: Vec<_> = PAGES
            .iter()
            .flat_map(|page| page.slots())
            .filter_map(|slot| match slot.entry.outcome() {
                Outcome::Open(destination) => Some(destination),
                _ => None,
            })
            .collect();
        for destination in [
            MainDestination::Browser,
            MainDestination::CreateGame,
            MainDestination::Player,
        ] {
            assert!(reachable.contains(&destination), "{destination:?}");
        }
        let pages: Vec<_> = Page::Main
            .slots()
            .iter()
            .filter_map(|slot| match slot.entry.outcome() {
                Outcome::Page(page) => Some(page),
                _ => None,
            })
            .collect();
        for page in [Page::Play, Page::Controls, Page::Setup, Page::Quit] {
            assert!(pages.contains(&page), "{page:?}");
        }
    }

    #[test]
    fn canvas_is_fitted_and_centred() {
        let wide = Placement::new([1920.0, 1080.0]);
        assert_eq!(wide.scale, 2.25);
        assert_eq!(wide.origin, [240.0, 0.0]);
        let tall = Placement::new([1280.0, 1024.0]);
        assert_eq!(tall.scale, 2.0);
        assert_eq!(tall.origin, [0.0, 32.0]);
        let rect = wide.rect([0.0, 0.0, 640.0, 480.0]);
        assert_eq!(
            (rect.x, rect.y, rect.width, rect.height),
            (240.0, 0.0, 1440.0, 1080.0)
        );
        let centered = wide.centered([320.0, 240.0], 40.0, 20.0);
        assert_eq!(
            (centered.x, centered.y),
            (240.0 + 300.0 * 2.25, 230.0 * 2.25)
        );
    }
}
