//! Entries and geometry of the classic profile pages, from the retail
//! `ui/jamp` menus: `player.menu`, `player2.menu` and `saber.menu` (full
//! screen), `ingame_player.menu`, `ingame_player2.menu` and
//! `ingame_saber.menu` (windows over a match). Rectangles are the retail
//! item rectangles on the 640x480 canvas; in-game ones are given relative to
//! their window, as retail wrote them, and offset by [`window`].

use super::{ClassicPage, Frame};

/// One selectable entry of a classic profile page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Item {
    NavPlay,
    NavProfile,
    NavControls,
    NavSetup,
    Name,
    Team,
    /// The head grid (`FEEDER_Q3HEADS`).
    Models,
    /// The Custom button leading to character creation.
    Custom,
    /// The in-game profile's Saber button.
    SaberButton,
    Species,
    /// Skin tint swatches.
    Tints,
    PartHead,
    PartTorso,
    PartLegs,
    /// The list of the selected part's variants.
    Parts,
    Single,
    Dual,
    Staff,
    Hilts,
    Hilts2,
    Blades,
    Blades2,
    Exit,
    Back,
    /// APPLY: on to the next page, or back to the match in game.
    Apply,
    /// The saber page's second Apply: back to the main menu.
    ApplyMain,
}

impl Item {
    pub(crate) fn is_nav(self) -> bool {
        matches!(
            self,
            Self::NavPlay | Self::NavProfile | Self::NavControls | Self::NavSetup
        )
    }

    /// The part list a part button selects.
    pub(crate) fn part_axis(self) -> Option<usize> {
        match self {
            Self::PartHead => Some(0),
            Self::PartTorso => Some(1),
            Self::PartLegs => Some(2),
            _ => None,
        }
    }

    /// Retail label (`strings/english/menus.str`); empty for lists and
    /// image buttons, which draw their own content.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::NavPlay => "PLAY",
            Self::NavProfile => "PROFILE",
            Self::NavControls => "CONTROLS",
            Self::NavSetup => "SETUP",
            Self::Name => "Name:",
            Self::Team => "Team Color:",
            Self::PartHead => "HEAD",
            Self::PartTorso => "TORSO",
            Self::PartLegs => "LEGS",
            Self::Single => "Standard Saber",
            Self::Dual => "Dual Sabers",
            Self::Staff => "Two-Handed Saber",
            Self::Exit => "EXIT",
            Self::Back => "Back",
            Self::Apply => "APPLY",
            Self::ApplyMain => "Apply",
            Self::Models
            | Self::Custom
            | Self::SaberButton
            | Self::Species
            | Self::Tints
            | Self::Parts
            | Self::Hilts
            | Self::Hilts2
            | Self::Blades
            | Self::Blades2 => "",
        }
    }

    /// Retail description line (`descText`).
    pub(crate) fn hint(self) -> &'static str {
        match self {
            Self::NavPlay => "Start playing now!",
            Self::NavProfile => "Configure character settings.",
            Self::NavControls => "Configure game controls.",
            Self::NavSetup => "Configure game settings.",
            Self::Name => "Enter your name here.",
            Self::Team => "Choose the color for your model's skin.",
            Self::Models => "Choose the model for your character.",
            Self::Custom => "Create your own character.",
            Self::SaberButton => "Configure your lightsaber.",
            Self::Species => "Choose a species.",
            Self::Tints => "Choose the color for your model's skin.",
            Self::PartHead => "Select a head style.",
            Self::PartTorso => "Select a torso style.",
            Self::PartLegs => "Select a legs style.",
            Self::Parts => "Choose your character's look.",
            Self::Single => "Use one one-handed saber, choice of 3 fighting styles.",
            Self::Dual => "Use 2 one-handed sabers, only one fighting style.",
            Self::Staff => "Use one two-handed saber, only one fighting style.",
            Self::Hilts => "Select a hilt.",
            Self::Hilts2 => "Select a second hilt.",
            Self::Blades => "Select a blade color.",
            Self::Blades2 => "Select a second blade color.",
            Self::Exit => "Leave Jedi Academy.",
            Self::Back => "Back to profile menu.",
            Self::Apply => "Apply changes to player and go to saber selection.",
            Self::ApplyMain => "Apply saber changes and return to Main Menu.",
        }
    }
}

const PLAYER_FULL: [Item; 10] = [
    Item::NavPlay,
    Item::NavProfile,
    Item::NavControls,
    Item::NavSetup,
    Item::Name,
    Item::Team,
    Item::Models,
    Item::Custom,
    Item::Exit,
    Item::Apply,
];
const PLAYER_IN_GAME: [Item; 6] = [
    Item::Name,
    Item::Team,
    Item::Models,
    Item::Custom,
    Item::SaberButton,
    Item::Apply,
];
const CHARACTER_FULL: [Item; 12] = [
    Item::NavPlay,
    Item::NavProfile,
    Item::NavControls,
    Item::NavSetup,
    Item::Species,
    Item::Tints,
    Item::PartHead,
    Item::PartTorso,
    Item::PartLegs,
    Item::Parts,
    Item::Back,
    Item::Apply,
];
const CHARACTER_IN_GAME: [Item; 8] = [
    Item::Species,
    Item::Tints,
    Item::PartHead,
    Item::PartTorso,
    Item::PartLegs,
    Item::Parts,
    Item::Back,
    Item::Apply,
];
const SABER_FULL: [Item; 12] = [
    Item::NavPlay,
    Item::NavProfile,
    Item::NavControls,
    Item::NavSetup,
    Item::Single,
    Item::Dual,
    Item::Staff,
    Item::Hilts,
    Item::Blades,
    Item::Exit,
    Item::Apply,
    Item::ApplyMain,
];
const SABER_FULL_DUAL: [Item; 14] = [
    Item::NavPlay,
    Item::NavProfile,
    Item::NavControls,
    Item::NavSetup,
    Item::Single,
    Item::Dual,
    Item::Staff,
    Item::Hilts,
    Item::Hilts2,
    Item::Blades,
    Item::Blades2,
    Item::Exit,
    Item::Apply,
    Item::ApplyMain,
];
const SABER_IN_GAME: [Item; 6] = [
    Item::Single,
    Item::Dual,
    Item::Staff,
    Item::Hilts,
    Item::Blades,
    Item::Apply,
];
const SABER_IN_GAME_DUAL: [Item; 8] = [
    Item::Single,
    Item::Dual,
    Item::Staff,
    Item::Hilts,
    Item::Hilts2,
    Item::Blades,
    Item::Blades2,
    Item::Apply,
];

/// The page's entries in focus order (retail item order).
pub(crate) fn items(page: ClassicPage, frame: Frame, dual: bool) -> &'static [Item] {
    match (page, frame, dual) {
        (ClassicPage::Player, Frame::Full, _) => &PLAYER_FULL,
        (ClassicPage::Player, Frame::InGame, _) => &PLAYER_IN_GAME,
        (ClassicPage::Character, Frame::Full, _) => &CHARACTER_FULL,
        (ClassicPage::Character, Frame::InGame, _) => &CHARACTER_IN_GAME,
        (ClassicPage::Saber, Frame::Full, false) => &SABER_FULL,
        (ClassicPage::Saber, Frame::Full, true) => &SABER_FULL_DUAL,
        (ClassicPage::Saber, Frame::InGame, false) => &SABER_IN_GAME,
        (ClassicPage::Saber, Frame::InGame, true) => &SABER_IN_GAME_DUAL,
    }
}

/// The retail window of an in-game page on the 640x480 canvas
/// (`ingame_player` 20 25 600 440, the others 105 40 430 425); the full
/// screen for the main menu's pages.
pub(crate) fn window(page: ClassicPage, frame: Frame) -> [f32; 4] {
    match (frame, page) {
        (Frame::Full, _) => [0.0, 0.0, 640.0, 480.0],
        (Frame::InGame, ClassicPage::Player) => [20.0, 25.0, 600.0, 440.0],
        (Frame::InGame, _) => [105.0, 40.0, 430.0, 425.0],
    }
}

/// Offset a window-relative retail rectangle onto the canvas.
pub(crate) fn place(page: ClassicPage, frame: Frame, [x, y, w, h]: [f32; 4]) -> [f32; 4] {
    let [wx, wy, _, _] = window(page, frame);
    [wx + x, wy + y, w, h]
}

/// Navigation row: `player.menu` puts it under the logo at y 126, the
/// creation pages along the top at y 16.
fn nav(page: ClassicPage, index: usize) -> [f32; 4] {
    let y = if page == ClassicPage::Player {
        126.0
    } else {
        16.0
    };
    [[7.0, 170.0, 340.0, 502.0][index], y, 130.0, 24.0]
}

/// The item's pointer target on the canvas (window offset applied).
pub(crate) fn rect(item: Item, page: ClassicPage, frame: Frame, dual: bool) -> [f32; 4] {
    use ClassicPage::{Character, Player, Saber};
    use Frame::{Full, InGame};
    let local = match (frame, page, item) {
        (_, _, Item::NavPlay) => nav(page, 0),
        (_, _, Item::NavProfile) => nav(page, 1),
        (_, _, Item::NavControls) => nav(page, 2),
        (_, _, Item::NavSetup) => nav(page, 3),
        (_, _, Item::Exit) => [59.0, 444.0, 130.0, 24.0],
        // player.menu / ingame_player.menu
        (Full, Player, Item::Name) => [15.0, 163.0, 300.0, 28.0],
        (InGame, Player, Item::Name) => [20.0, 31.0, 300.0, 22.0],
        (Full, Player, Item::Team) => [50.0, 205.0, 160.0, 19.0],
        (InGame, Player, Item::Team) => [50.0, 77.0, 160.0, 13.0],
        (Full, Player, Item::Models) => [30.0, 224.0, 404.0, 194.0],
        (InGame, Player, Item::Models) => [20.0, 90.0, 404.0, 194.0],
        (Full, Player, Item::Custom) => [480.0, 280.0, 96.0, 96.0],
        (InGame, Player, Item::Custom) => [465.0, 160.0, 75.0, 75.0],
        (_, Player, Item::SaberButton) => [465.0, 322.0, 75.0, 75.0],
        (Full, Player, Item::Apply) => [455.0, 444.0, 130.0, 24.0],
        (InGame, Player, Item::Apply) => [5.0, 412.0, 105.0, 28.0],
        // player2.menu / ingame_player2.menu
        (Full, Character, Item::Species) => [176.0, 92.0, 150.0, 16.0],
        (InGame, Character, Item::Species) => [161.0, 52.0, 150.0, 16.0],
        (Full, Character, Item::Tints) => [30.0, 168.0, 292.0, 48.0],
        (InGame, Character, Item::Tints) => [15.0, 104.0, 292.0, 48.0],
        (Full, Character, Item::PartHead) => [30.0, 280.0, 90.0, 16.0],
        (Full, Character, Item::PartTorso) => [126.0, 280.0, 90.0, 16.0],
        (Full, Character, Item::PartLegs) => [224.0, 280.0, 90.0, 16.0],
        (InGame, Character, Item::PartHead) => [15.0, 184.0, 90.0, 16.0],
        (InGame, Character, Item::PartTorso) => [111.0, 184.0, 90.0, 16.0],
        (InGame, Character, Item::PartLegs) => [209.0, 184.0, 90.0, 16.0],
        (Full, Character, Item::Parts) => [30.0, 306.0, 292.0, 93.0],
        (InGame, Character, Item::Parts) => [15.0, 206.0, 292.0, 93.0],
        (Full, Character, Item::Back) => [59.0, 444.0, 130.0, 24.0],
        (InGame, Character, Item::Back) => [30.0, 370.0, 110.0, 32.0],
        (Full, Character, Item::Apply) => [455.0, 444.0, 130.0, 24.0],
        (InGame, Character, Item::Apply) => [290.0, 370.0, 110.0, 32.0],
        // saber.menu / ingame_saber.menu
        (Full, Saber, Item::Single) => [32.0, 132.0, 180.0, 16.0],
        (Full, Saber, Item::Dual) => [32.0, 152.0, 180.0, 16.0],
        (Full, Saber, Item::Staff) => [32.0, 172.0, 180.0, 16.0],
        (InGame, Saber, Item::Single) => [15.0, 72.0, 180.0, 16.0],
        (InGame, Saber, Item::Dual) => [15.0, 88.0, 180.0, 16.0],
        (InGame, Saber, Item::Staff) => [15.0, 104.0, 180.0, 16.0],
        (Full, Saber, Item::Hilts) if dual => [240.0, 95.0, 160.0, 54.0],
        (Full, Saber, Item::Hilts) => [240.0, 95.0, 160.0, 120.0],
        (Full, Saber, Item::Hilts2) => [240.0, 165.0, 160.0, 54.0],
        (InGame, Saber, Item::Hilts) if dual => [200.0, 50.0, 160.0, 55.0],
        (InGame, Saber, Item::Hilts) => [200.0, 56.0, 160.0, 120.0],
        (InGame, Saber, Item::Hilts2) => [200.0, 120.0, 160.0, 55.0],
        (Full, Saber, Item::Blades) => [446.0, 124.0, 159.0, 24.0],
        (Full, Saber, Item::Blades2) => [446.0, 170.0, 159.0, 24.0],
        (InGame, Saber, Item::Blades) => [15.0, 197.0, 149.0, 24.0],
        (InGame, Saber, Item::Blades2) => [270.0, 197.0, 149.0, 24.0],
        (Full, Saber, Item::Apply) => [255.0, 444.0, 130.0, 24.0],
        (Full, Saber, Item::ApplyMain) => [455.0, 444.0, 130.0, 24.0],
        (InGame, Saber, Item::Apply) => [160.0, 360.0, 110.0, 32.0],
        _ => [0.0, 0.0, 0.0, 0.0],
    };
    if item.is_nav() || item == Item::Exit {
        return local;
    }
    place(page, frame, local)
}

/// Tiles per row of the head grid: 64-unit cells in its 404-unit width.
pub(crate) const GRID_COLUMNS: usize = 6;
/// Cell side of the head grid, on the canvas.
pub(crate) const GRID_CELL: f32 = 64.0;
/// Cell side of the part lists (`elementwidth 72`).
pub(crate) const PART_CELL: f32 = 72.0;
/// Cell side of the tint list (`elementwidth 32`).
pub(crate) const TINT_CELL: f32 = 32.0;
/// Row height of the hilt lists (`elementheight 16`).
pub(crate) const HILT_ROW: f32 = 16.0;
/// Side of a blade colour swatch, and the step between swatches.
pub(crate) const SWATCH: f32 = 24.0;
pub(crate) fn swatch_step(frame: Frame) -> f32 {
    match frame {
        Frame::Full => 27.0,
        Frame::InGame => 25.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGES: [ClassicPage; 3] = [
        ClassicPage::Player,
        ClassicPage::Character,
        ClassicPage::Saber,
    ];

    fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
        a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
    }

    #[test]
    fn every_entry_has_a_place_inside_its_window_and_none_overlap() {
        for page in PAGES {
            for frame in [Frame::Full, Frame::InGame] {
                for dual in [false, true] {
                    let list = items(page, frame, dual);
                    let [wx, wy, ww, wh] = window(page, frame);
                    for (index, item) in list.iter().enumerate() {
                        let r = rect(*item, page, frame, dual);
                        assert!(r[2] > 0.0 && r[3] > 0.0, "{page:?} {frame:?} {item:?}");
                        assert!(r[0] >= wx && r[1] >= wy, "{page:?} {frame:?} {item:?}");
                        assert!(r[0] + r[2] <= wx + ww + 0.5 && r[1] + r[3] <= wy + wh + 0.5);
                        for other in &list[index + 1..] {
                            let o = rect(*other, page, frame, dual);
                            assert!(!overlaps(r, o), "{page:?} {frame:?}: {item:?} / {other:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn grid_holds_the_retail_cells() {
        let [_, _, width, height] = rect(Item::Models, ClassicPage::Player, Frame::Full, false);
        assert_eq!((width / GRID_CELL) as usize, GRID_COLUMNS);
        assert_eq!((height / GRID_CELL) as usize, 3);
    }
}
