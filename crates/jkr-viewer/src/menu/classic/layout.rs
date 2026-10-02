//! Pages, entries and geometry of the classic main menu, taken from the
//! retail multiplayer menus (`ui/jamp/main.menu`, `multiplayer.menu`,
//! `quit.menu`): which entries each page has, in which order, where they sit
//! on the original 640x480 menu canvas, and where each one leads.
//!
//! Positions are the retail item rectangles reduced to a text centre; labels
//! are the retail words. Hints and titles are JKR's own text.

use crate::menu::destination::MainDestination;
use jkr_ui::Rect;

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
    /// Retail "multiplayer" menu behind Play: join or create a server.
    Play,
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
    JoinServer,
    CreateServer,
    Back,
    No,
    Yes,
}

/// Label size class: the main page's big buttons, or the smaller ones of
/// the navigation row and lists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Size {
    Large,
    Medium,
}

impl Size {
    /// Label height on the 640x480 canvas.
    pub(crate) fn text(self) -> f32 {
        match self {
            Self::Large => 22.0,
            Self::Medium => 17.0,
        }
    }
}

/// One entry placed on a page.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Slot {
    pub(crate) entry: Entry,
    pub(crate) label: &'static str,
    /// The description line shown while the entry has focus.
    pub(crate) hint: &'static str,
    /// Centre of the label on the 640x480 canvas.
    pub(crate) center: [f32; 2],
    /// Width of the pointer target and focus glow on the canvas: the
    /// retail item width (130 for buttons, 190 for list entries).
    pub(crate) width: f32,
    pub(crate) size: Size,
}

impl Slot {
    /// Pointer target and focus glow on the 640x480 canvas.
    pub(crate) fn target(&self) -> [f32; 4] {
        let width = self.width;
        let height = 30.0;
        [
            self.center[0] - width * 0.5,
            self.center[1] - height * 0.5,
            width,
            height,
        ]
    }
}

const fn slot(
    entry: Entry,
    label: &'static str,
    hint: &'static str,
    center: [f32; 2],
    width: f32,
    size: Size,
) -> Slot {
    Slot {
        entry,
        label,
        hint,
        center,
        width,
        size,
    }
}

const PLAY_HINT: &str = "Join a server or start your own";
const PROFILE_HINT: &str = "Name, model, saber and Force";
const CONTROLS_HINT: &str = "Mouse and key bindings";
const SETUP_HINT: &str = "Video, audio, HUD and game options";
const EXIT_HINT: &str = "Leave the game";

/// Retail `main.menu`: two columns either side of the centre window, Exit
/// below. Order is the retail item order, which keyboard focus follows.
const MAIN: [Slot; 5] = [
    slot(
        Entry::Play,
        "PLAY",
        PLAY_HINT,
        [101.0, 224.0],
        190.0,
        Size::Large,
    ),
    slot(
        Entry::Profile,
        "PROFILE",
        PROFILE_HINT,
        [101.0, 322.0],
        190.0,
        Size::Large,
    ),
    slot(
        Entry::Controls,
        "CONTROLS",
        CONTROLS_HINT,
        [521.0, 224.0],
        190.0,
        Size::Large,
    ),
    slot(
        Entry::Setup,
        "SETUP",
        SETUP_HINT,
        [521.0, 322.0],
        190.0,
        Size::Large,
    ),
    slot(
        Entry::Exit,
        "EXIT",
        EXIT_HINT,
        [320.0, 456.0],
        190.0,
        Size::Large,
    ),
];

/// The navigation row every retail sub-menu repeats along its top.
const fn nav_row() -> [Slot; 4] {
    [
        slot(
            Entry::Play,
            "PLAY",
            PLAY_HINT,
            [72.0, 138.0],
            130.0,
            Size::Medium,
        ),
        slot(
            Entry::Profile,
            "PROFILE",
            PROFILE_HINT,
            [235.0, 138.0],
            130.0,
            Size::Medium,
        ),
        slot(
            Entry::Controls,
            "CONTROLS",
            CONTROLS_HINT,
            [405.0, 138.0],
            130.0,
            Size::Medium,
        ),
        slot(
            Entry::Setup,
            "SETUP",
            SETUP_HINT,
            [567.0, 138.0],
            130.0,
            Size::Medium,
        ),
    ]
}

/// Retail `multiplayer.menu`, reduced to the entries JKR has a screen for:
/// Join Server and Create Server in the centre list, Back and Exit below.
const PLAY: [Slot; 8] = {
    let [play, profile, controls, setup] = nav_row();
    [
        play,
        profile,
        controls,
        setup,
        slot(
            Entry::JoinServer,
            "JOIN SERVER",
            "Browse servers and join a game",
            [320.0, 209.0],
            190.0,
            Size::Medium,
        ),
        slot(
            Entry::CreateServer,
            "CREATE SERVER",
            "Host a match with bots on this machine",
            [320.0, 244.0],
            190.0,
            Size::Medium,
        ),
        slot(
            Entry::Back,
            "BACK",
            "Return to the main menu",
            [124.0, 456.0],
            130.0,
            Size::Medium,
        ),
        slot(
            Entry::Exit,
            "EXIT",
            EXIT_HINT,
            [320.0, 456.0],
            130.0,
            Size::Medium,
        ),
    ]
};

/// Retail `quit.menu`: No bottom left, Yes bottom right.
const QUIT: [Slot; 6] = {
    let [play, profile, controls, setup] = nav_row();
    [
        play,
        profile,
        controls,
        setup,
        slot(
            Entry::No,
            "NO",
            "Return to the main menu",
            [124.0, 456.0],
            130.0,
            Size::Medium,
        ),
        slot(
            Entry::Yes,
            "YES",
            "Exit to the desktop",
            [519.0, 456.0],
            130.0,
            Size::Medium,
        ),
    ]
};

impl Page {
    /// The page's entries in focus order.
    pub(crate) fn slots(self) -> &'static [Slot] {
        match self {
            Self::Main => &MAIN,
            Self::Play => &PLAY,
            Self::Quit => &QUIT,
        }
    }

    /// Entry focused when the page opens: retail focuses the first list
    /// entry of the multiplayer menu; the quit page starts on No.
    pub(crate) fn initial_selection(self) -> usize {
        let entry = match self {
            Self::Main => Entry::Play,
            Self::Play => Entry::JoinServer,
            Self::Quit => Entry::No,
        };
        self.index_of(entry).unwrap_or(0)
    }

    /// Position of `entry` on this page.
    pub(crate) fn index_of(self, entry: Entry) -> Option<usize> {
        self.slots().iter().position(|slot| slot.entry == entry)
    }

    /// Page heading under the logo, if the page has one.
    pub(crate) fn title(self) -> (&'static str, f32) {
        match self {
            Self::Main => ("MULTIPLAYER", 132.0),
            Self::Play => ("START PLAYING", 172.0),
            Self::Quit => ("QUIT", 172.0),
        }
    }

    /// Where Escape leads: the main page asks to quit, as retail does;
    /// every other page returns to the main page.
    pub(crate) fn escape(self) -> Page {
        match self {
            Self::Main => Self::Quit,
            Self::Play | Self::Quit => Self::Main,
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
}

impl Entry {
    /// What activating this entry does; `controls_tab` is the settings tab
    /// holding the mouse options and the key-bindings editor.
    pub(crate) fn outcome(self, controls_tab: usize) -> Outcome {
        match self {
            Self::Play => Outcome::Page(Page::Play),
            Self::Exit => Outcome::Page(Page::Quit),
            Self::Back | Self::No => Outcome::Page(Page::Main),
            Self::Profile => Outcome::Open(MainDestination::Player),
            Self::Controls => Outcome::Open(MainDestination::Settings { tab: controls_tab }),
            Self::Setup => Outcome::Open(MainDestination::Settings { tab: 0 }),
            Self::JoinServer => Outcome::Open(MainDestination::Browser),
            Self::CreateServer => Outcome::Open(MainDestination::CreateGame),
            Self::Yes => Outcome::Open(MainDestination::Quit),
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

    const PAGES: [Page; 3] = [Page::Main, Page::Play, Page::Quit];

    fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
        a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
    }

    #[test]
    fn main_page_keeps_the_retail_entry_order() {
        let entries: Vec<_> = Page::Main.slots().iter().map(|slot| slot.entry).collect();
        assert_eq!(
            entries,
            [
                Entry::Play,
                Entry::Profile,
                Entry::Controls,
                Entry::Setup,
                Entry::Exit
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
    fn pages_open_on_a_valid_entry() {
        for page in PAGES {
            assert!(page.initial_selection() < page.slots().len());
        }
        assert_eq!(
            Page::Play.slots()[Page::Play.initial_selection()].entry,
            Entry::JoinServer
        );
        assert_eq!(
            Page::Quit.slots()[Page::Quit.initial_selection()].entry,
            Entry::No
        );
    }

    #[test]
    fn only_yes_quits() {
        for page in PAGES {
            for slot in page.slots() {
                let quits = slot.entry.outcome(3) == Outcome::Open(MainDestination::Quit);
                assert_eq!(quits, slot.entry == Entry::Yes, "{:?}", slot.entry);
            }
        }
        assert_eq!(Page::Main.escape(), Page::Quit);
        assert_eq!(Page::Quit.escape(), Page::Main);
        assert_eq!(Page::Play.escape(), Page::Main);
    }

    #[test]
    fn every_main_destination_except_quit_is_one_page_away() {
        let reachable: Vec<_> = Page::Main
            .slots()
            .iter()
            .chain(Page::Play.slots())
            .filter_map(|slot| match slot.entry.outcome(3) {
                Outcome::Open(destination) => Some(destination),
                Outcome::Page(_) => None,
            })
            .collect();
        for destination in [
            MainDestination::Browser,
            MainDestination::CreateGame,
            MainDestination::Player,
            MainDestination::Settings { tab: 0 },
            MainDestination::Settings { tab: 3 },
        ] {
            assert!(reachable.contains(&destination), "{destination:?}");
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
