//! The classic main menu's page tables: every page's entries in retail item
//! order (which keyboard focus follows), with their retail positions on the
//! 640x480 canvas. Types and geometry are in [`super::layout`].

use super::layout::{Entry, Page, Size, Slot};
use jkr_ui::TextAlign;

/// A centred button `width` canvas units wide and 30 high.
const fn button(
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
        height: 30.0,
        size,
        align: TextAlign::Center,
    }
}

/// One row of the Controls and Setup option lists: retail rect `80 y 170
/// 24`, its label set against the right edge.
const fn list_row(entry: Entry, label: &'static str, hint: &'static str, y: f32) -> Slot {
    Slot {
        entry,
        label,
        hint,
        center: [165.0, y + 12.0],
        width: 170.0,
        height: 24.0,
        size: Size::List,
        align: TextAlign::End,
    }
}

/// One entry of the start-playing centre list: retail rect `225 y 190 36`.
const fn centre_row(entry: Entry, label: &'static str, hint: &'static str, y: f32) -> Slot {
    Slot {
        entry,
        label,
        hint,
        center: [320.0, y + 18.0],
        width: 190.0,
        height: 34.0,
        size: Size::Medium,
        align: TextAlign::Center,
    }
}

const PLAY_HINT: &str = "Solo game, join a server or start your own";
const PROFILE_HINT: &str = "Name, model, saber and Force";
const CONTROLS_HINT: &str = "Key bindings and mouse";
const SETUP_HINT: &str = "Video, sound and game options";
const EXIT_HINT: &str = "Leave the game";
const BACK_HINT: &str = "Return to the main menu";

/// Retail `main.menu`: two columns either side of the centre window, Exit
/// below.
const MAIN: [Slot; 5] = [
    button(
        Entry::Play,
        "PLAY",
        PLAY_HINT,
        [101.0, 224.0],
        190.0,
        Size::Large,
    ),
    button(
        Entry::Profile,
        "PROFILE",
        PROFILE_HINT,
        [101.0, 322.0],
        190.0,
        Size::Large,
    ),
    button(
        Entry::Controls,
        "CONTROLS",
        CONTROLS_HINT,
        [521.0, 224.0],
        190.0,
        Size::Large,
    ),
    button(
        Entry::Setup,
        "SETUP",
        SETUP_HINT,
        [521.0, 322.0],
        190.0,
        Size::Large,
    ),
    button(
        Entry::Exit,
        "EXIT",
        EXIT_HINT,
        [320.0, 456.0],
        190.0,
        Size::Large,
    ),
];

/// The navigation row every retail sub-menu repeats along its top (retail
/// rects `7 126`, `170 126`, `340 126`, `502 126`, 130 by 24).
const fn nav_row() -> [Slot; 4] {
    [
        button(
            Entry::Play,
            "PLAY",
            PLAY_HINT,
            [72.0, 138.0],
            130.0,
            Size::Medium,
        ),
        button(
            Entry::Profile,
            "PROFILE",
            PROFILE_HINT,
            [235.0, 138.0],
            130.0,
            Size::Medium,
        ),
        button(
            Entry::Controls,
            "CONTROLS",
            CONTROLS_HINT,
            [405.0, 138.0],
            130.0,
            Size::Medium,
        ),
        button(
            Entry::Setup,
            "SETUP",
            SETUP_HINT,
            [567.0, 138.0],
            130.0,
            Size::Medium,
        ),
    ]
}

/// Back and Exit along the bottom of every sub-page (retail rects `59 444`
/// and `255 444`).
const fn back_exit() -> [Slot; 2] {
    [
        button(
            Entry::Back,
            "BACK",
            BACK_HINT,
            [124.0, 456.0],
            130.0,
            Size::Medium,
        ),
        button(
            Entry::Exit,
            "EXIT",
            EXIT_HINT,
            [320.0, 456.0],
            130.0,
            Size::Medium,
        ),
    ]
}

/// Retail `multiplayer.menu`: the start-playing list in the centre.
const PLAY: [Slot; 11] = {
    let [play, profile, controls, setup] = nav_row();
    let [back, exit] = back_exit();
    [
        play,
        profile,
        controls,
        setup,
        centre_row(
            Entry::SoloGame,
            "SOLO GAME",
            "A local match with bots, set up in Create game",
            191.0,
        ),
        centre_row(
            Entry::JoinServer,
            "JOIN SERVER",
            "Browse servers and join a game",
            226.0,
        ),
        centre_row(
            Entry::CreateServer,
            "CREATE SERVER",
            "Host a match with bots on this machine",
            261.0,
        ),
        centre_row(
            Entry::PlayDemo,
            "PLAY DEMO",
            "Not in JKR yet: use the demo console command",
            296.0,
        ),
        centre_row(
            Entry::Rules,
            "RULES",
            "Not in JKR yet: no rules pages",
            331.0,
        ),
        back,
        exit,
    ]
};

/// Retail `controls.menu`: the binding pages down the left. Each opens the
/// key-binding editor on its tab; Mouse/Joystick opens the mouse options.
const CONTROLS: [Slot; 13] = {
    let [play, profile, controls, setup] = nav_row();
    let [back, exit] = back_exit();
    [
        play,
        profile,
        controls,
        setup,
        list_row(
            Entry::Movement,
            "MOVEMENT",
            "Key bindings: moving, jumping, turning and looking",
            185.0,
        ),
        list_row(
            Entry::Interaction,
            "INTERACTION",
            "Key bindings: attacks, saber, use and items",
            209.0,
        ),
        list_row(
            Entry::Weapons,
            "WEAPONS",
            "Key bindings: weapon selection",
            233.0,
        ),
        list_row(
            Entry::ForcePowers1,
            "FORCE POWERS 1",
            "Key bindings: Force powers",
            257.0,
        ),
        list_row(
            Entry::ForcePowers2,
            "FORCE POWERS 2",
            "Key bindings: Force powers (one page in JKR)",
            281.0,
        ),
        list_row(
            Entry::MouseJoystick,
            "MOUSE/JOYSTICK",
            "Mouse sensitivity, inversion and always run",
            305.0,
        ),
        list_row(
            Entry::OtherControls,
            "OTHER",
            "Key bindings: chat, scores, votes and emotes",
            329.0,
        ),
        back,
        exit,
    ]
};

/// Retail `setup.menu`: the option pages down the left, then the settings
/// JKR adds (HUD, network) after them.
const SETUP: [Slot; 15] = {
    let [play, profile, controls, setup] = nav_row();
    let [back, exit] = back_exit();
    [
        play,
        profile,
        controls,
        setup,
        list_row(
            Entry::Video,
            "VIDEO",
            "Resolution, display mode, sync and frame rate",
            185.0,
        ),
        list_row(
            Entry::MoreVideo,
            "MORE VIDEO",
            "Gamma, marks and shadows (on the video settings)",
            209.0,
        ),
        list_row(
            Entry::Sound,
            "SOUND",
            "Effects, music and voice volume",
            233.0,
        ),
        list_row(
            Entry::GameOptions,
            "GAME OPTIONS",
            "Pickups, saber trail, camera and menu options",
            257.0,
        ),
        list_row(
            Entry::Mods,
            "MODS",
            "Not in JKR yet: set fs_game and restart",
            281.0,
        ),
        list_row(
            Entry::Defaults,
            "DEFAULTS",
            "Not in JKR yet: the key-binding editor resets keys with R",
            305.0,
        ),
        list_row(
            Entry::Hud,
            "HUD",
            "HUD elements, scale and crosshair",
            329.0,
        ),
        list_row(
            Entry::MoreHud,
            "MORE HUD",
            "Crosshair size, team status and speed readout",
            353.0,
        ),
        list_row(
            Entry::Network,
            "NETWORK",
            "Master server and connection rates",
            377.0,
        ),
        back,
        exit,
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
        button(
            Entry::No,
            "NO",
            BACK_HINT,
            [124.0, 456.0],
            130.0,
            Size::Medium,
        ),
        button(
            Entry::Yes,
            "YES",
            "Exit to the desktop",
            [519.0, 456.0],
            130.0,
            Size::Medium,
        ),
    ]
};

/// The entries of `page`, in focus order.
pub(super) fn slots(page: Page) -> &'static [Slot] {
    match page {
        Page::Main => &MAIN,
        Page::Play => &PLAY,
        Page::Controls => &CONTROLS,
        Page::Setup => &SETUP,
        Page::Quit => &QUIT,
    }
}
