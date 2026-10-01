//! Hero-style server browser over the live map, in the same vocabulary as
//! the settings and player screens: kicker / title / status header, an
//! ALL SERVERS · FAVOURITES tab strip with the filter on its right, a
//! box-free sortable table, the selected server's details beside it and a
//! key-cap footer whose caps double as the pointer's buttons.

use super::ClientMenu;
use super::browser_table::{self, TableLayout};
use crate::menu_widgets::{FormLayout, Scrim, TAB_BASE};
use crate::{TextVertex, UiFont};
use jkr_ui::{FontWeight, Rect, TextAlign};

pub(super) use super::browser_table::{HEADER_TOKEN, ROW_TOKEN, SCROLLBAR_TOKEN};

/// The tab strip; tab `i` answers to `TAB_BASE + i`.
pub(super) const TABS: [&str; 2] = ["ALL SERVERS", "FAVOURITES"];
/// Footer caps with the browser token each one activates (0 = display only).
const KEY_HINTS: [(&str, &str, u16); 6] = [
    ("ENTER", "Join", JOIN_TOKEN),
    ("R", "Refresh", REFRESH_TOKEN),
    ("F", "Favourite", FAVOURITE_TOKEN),
    ("/", "Filter", FILTER_TOKEN),
    ("C", "Address", ADDRESS_TOKEN),
    ("ESC", "Back", BACK_TOKEN),
];
pub(super) const REFRESH_TOKEN: u16 = 10;
pub(super) const FAVOURITE_TOKEN: u16 = 11;
pub(super) const BACK_TOKEN: u16 = 12;
pub(super) const JOIN_TOKEN: u16 = 13;
pub(super) const ADDRESS_TOKEN: u16 = 15;
pub(super) const FILTER_TOKEN: u16 = 20;
/// Width (at scale 1) of the filter zone at the right end of the tab line.
const FILTER_WIDTH: f32 = 300.0;

/// The browser's tab index: 1 while only favourites are listed.
pub(super) fn tab_of(favorites_only: bool) -> usize {
    usize::from(favorites_only)
}

impl ClientMenu {
    pub(super) fn append_browser(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        reveal: f32,
    ) {
        let layout = FormLayout::new(viewport);
        let s = layout.scale;
        let tab = tab_of(self.browser.favorites_only());
        self.ui.begin_hero(viewport, reveal, Scrim::Wide);
        let status = if self.browser.is_refreshing() && self.browser.entries().is_empty() {
            "Querying the master server..."
        } else if self.browser.is_refreshing() {
            "Servers are answering..."
        } else {
            self.state.status()
        };
        self.ui.form_header(
            &layout,
            "JKR   /   PLAY",
            if tab == 0 { "SERVERS" } else { TABS[1] },
            status,
        );
        self.ui.form_tabs(&layout, &TABS, tab);
        super::browser_filters::append(&mut self.ui, &layout, self.browser.filters());

        let table = TableLayout::new(&layout);
        self.append_filter(
            &layout,
            Rect::new(
                table.header.right() - FILTER_WIDTH * s,
                layout.tabs_y(),
                FILTER_WIDTH * s,
                30.0 * s,
            ),
        );
        let grid = browser_table::append_header(&mut self.ui, table.header, &self.browser, s);
        browser_table::append_rows(&mut self.ui, table.rows, grid, &mut self.browser, s);
        super::browser_details::append_details(
            &mut self.ui,
            table.details,
            self.browser.details(),
            s,
        );
        self.ui.form_footer_actions(&layout, &KEY_HINTS);
        if self.password_target.is_some() {
            self.append_password_modal(viewport);
        }
        if self.address_editing {
            super::address_view::append(
                &mut self.ui,
                viewport,
                &self.address_input,
                &self.address_error,
            );
        }
        self.ui.end_hero();
        self.ui.finish(self.browser_focus);
        self.ui.append_text(vertices, font, viewport);
    }

    /// The filter at the right end of the tab line: a muted prompt while
    /// empty, the typed text otherwise, underlined in accent while editing.
    fn append_filter(&mut self, layout: &FormLayout, zone: Rect) {
        let s = layout.scale;
        let theme = self.ui.theme();
        let filter = self.browser.filter_display();
        let empty = filter == "FILTER  ALL SERVERS";
        self.ui.hit_region(FILTER_TOKEN, zone);
        self.ui.text_aligned(
            if empty && !self.filter_editing {
                "FILTER  /"
            } else {
                filter
            },
            Rect::new(zone.x, zone.y + 4.0 * s, zone.width, 18.0 * s),
            13.0 * s,
            if empty { theme.muted } else { theme.foreground },
            FontWeight::Semibold,
            2.2 * s,
            TextAlign::End,
        );
        if self.filter_editing {
            self.ui.edit_underline(zone, theme.accent, s);
        }
    }

    fn append_password_modal(&mut self, viewport: [f32; 2]) {
        self.ui
            .hit_region(33, Rect::new(0.0, 0.0, viewport[0], viewport[1]));
        let card = self.ui.centered_card(520.0, 220.0, 32.0);
        self.ui.panel(card.panel);
        let y = self.ui.card_heading(
            card,
            "PRIVATE SERVER",
            "Password required",
            "Enter the server password to continue",
        );
        let field = Rect::new(card.content.x, y, card.content.width, 44.0);
        self.ui.text_field(field, true);
        self.ui.hit_region(30, field);
        self.ui.text(
            "********",
            Rect::new(field.x + 16.0, field.y, field.width - 32.0, field.height),
            20.0,
            self.ui.theme().foreground,
            FontWeight::Semibold,
            3.0,
        );
        let buttons_y = field.bottom() + 12.0;
        self.ui.button(
            31,
            "Cancel",
            Rect::new(card.content.x, buttons_y, 120.0, 40.0),
            false,
        );
        self.ui.button(
            32,
            "Connect",
            Rect::new(card.content.right() - 140.0, buttons_y, 140.0, 40.0),
            false,
        );
    }
}

/// Which tab a tab-strip token selects, if it is one.
pub(super) fn tab_for_token(token: u16) -> Option<usize> {
    (TAB_BASE..TAB_BASE + TABS.len() as u16)
        .contains(&token)
        .then(|| usize::from(token - TAB_BASE))
}
