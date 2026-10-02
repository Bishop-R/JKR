//! The `ui_menuStyle` setting: which layout the main menu uses.
//!
//! `modern` is the native hero layout ([`super::main_view`]); `classic`
//! follows the original Jedi Academy multiplayer menus ([`super::classic`]).
//! The in-game menu follows the same setting ([`crate::ingame_menu`]).
//! Screens without a classic version yet use their modern layout in both.

use super::ClientMenu;
use super::art::{self, ArtSet};

/// Archived cvar naming the menu style.
pub(crate) const CVAR: &str = "ui_menuStyle";

/// Layout family of the main menu.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum MenuStyle {
    /// The native layout: one column of entries over the live map.
    #[default]
    Modern,
    /// Close to the retail menus in layout and flow, for players who know
    /// where things were in the original game.
    Classic,
}

impl MenuStyle {
    /// Values the settings screen offers, in [`MenuStyle`] order; the first
    /// is the default.
    pub(crate) const NAMES: [&'static str; 2] = ["modern", "classic"];

    /// Read the cvar value: `classic` (any case) or `1` selects the classic
    /// style; anything else, including a missing or mistyped value, the
    /// modern one, so a typo never leaves the player without a menu.
    pub(crate) fn from_cvar(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some(text) if text.eq_ignore_ascii_case("classic") || text == "1" => Self::Classic,
            _ => Self::Modern,
        }
    }
}

impl ClientMenu {
    /// Apply the player's `ui_menuStyle`. A change puts the main menu back on
    /// its first page and entry, since selections do not carry across
    /// layouts.
    pub(crate) fn set_menu_style(&mut self, style: MenuStyle) {
        if style != self.menu_style {
            self.menu_style = style;
            self.main_selection = 0;
            self.classic.reset();
        }
    }

    /// The retail artwork the classic pages can draw this frame.
    pub(crate) fn set_menu_art(&mut self, art: ArtSet) {
        self.art = art;
    }
}

impl crate::GpuState {
    /// Apply `ui_menuStyle` to the main and in-game menus. While the
    /// classic style is on, the player's retail menu artwork is decoded
    /// (once, on a worker) and uploaded when ready; both menus are told
    /// which pieces they can draw.
    pub(crate) fn sync_menu_style(&mut self) {
        let Some(console) = &self.console else {
            return;
        };
        let style = MenuStyle::from_cvar(console.text_value(CVAR));
        if style == MenuStyle::Classic {
            if let Some(vfs) = &self.vfs {
                art::request(vfs);
            }
            self.ui_shapes.install_menu_art(&self.device, &self.queue);
        }
        let art = self.ui_shapes.menu_art();
        if let Some(menu) = &mut self.client_menu {
            menu.set_menu_style(style);
            menu.set_menu_art(art);
        }
        self.in_game_menu.set_style(style, art);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_needs_an_explicit_value() {
        assert_eq!(MenuStyle::from_cvar(None), MenuStyle::Modern);
        assert_eq!(MenuStyle::from_cvar(Some("")), MenuStyle::Modern);
        assert_eq!(MenuStyle::from_cvar(Some("modern")), MenuStyle::Modern);
        assert_eq!(MenuStyle::from_cvar(Some("clasic")), MenuStyle::Modern);
        assert_eq!(MenuStyle::from_cvar(Some("0")), MenuStyle::Modern);
        assert_eq!(MenuStyle::from_cvar(Some("classic")), MenuStyle::Classic);
        assert_eq!(MenuStyle::from_cvar(Some(" Classic ")), MenuStyle::Classic);
        assert_eq!(MenuStyle::from_cvar(Some("1")), MenuStyle::Classic);
    }

    #[test]
    fn offered_names_parse_in_order() {
        let parsed = MenuStyle::NAMES.map(|name| MenuStyle::from_cvar(Some(name)));
        assert_eq!(parsed, [MenuStyle::Modern, MenuStyle::Classic]);
        assert_eq!(parsed[0], MenuStyle::default());
    }
}
