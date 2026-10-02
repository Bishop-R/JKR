//! The `ui_menuStyle` setting: which layout the main menu uses.
//!
//! `modern` is the native hero layout ([`super::main_view`]); `classic`
//! follows the original Jedi Academy multiplayer menus ([`super::classic`]).
//! Screens without a classic version yet use their modern layout in both.

use super::ClientMenu;

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
