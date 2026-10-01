//! Compact text-only filter strip using the browser's existing hit regions and theme.
use super::*;
use crate::menu_widgets::{FormLayout, MenuCanvas};
use crate::server_browser::filters::Filters;
use jkr_ui::{FontWeight, Rect};

/// Independent hit tokens, outside row/header/modal namespaces.
pub(super) const BASE: u16 = 50;
const NAMES: [&str; 4] = [
    "ui_browserShowEmpty",
    "ui_browserShowFull",
    "ui_browserShowPasswordProtected",
    "ui_browserFilterInvalidInfo",
];

impl ClientMenu {
    /// Sample cvars into the browser policy without rebuilding unchanged rows.
    pub(crate) fn configure_browser(&mut self, console: &mut ViewerConsole) {
        self.browser.configure(console);
    }

    /// Apply a filter-strip activation, persisting through the ordinary archived cvar path.
    pub(super) fn activate_filter(&mut self, token: u16, console: &mut ViewerConsole) -> bool {
        if !(BASE..BASE + 5).contains(&token) {
            return false;
        }
        let index = usize::from(token - BASE);
        if let Some(name) = NAMES.get(index) {
            let value = console.integer_cvar(name).unwrap_or(1) == 0;
            console.set_cvar(name, if value { "1" } else { "0" });
        } else {
            let mode = self.browser.filters().mode;
            console.set_cvar(
                "ui_actualNetGametype",
                &if mode == 9 { -1 } else { mode + 1 }.to_string(),
            );
        }
        self.configure_browser(console);
        true
    }
}

/// Draw controls above the table, without cards or per-frame string construction.
pub(crate) fn append(ui: &mut MenuCanvas, layout: &FormLayout, filters: Filters) {
    let labels = [
        if filters.empty {
            "EMPTY: SHOW"
        } else {
            "EMPTY: HIDE"
        },
        if filters.full {
            "FULL: SHOW"
        } else {
            "FULL: HIDE"
        },
        if filters.password {
            "LOCKED: SHOW"
        } else {
            "LOCKED: HIDE"
        },
        if filters.valid {
            "VALID ONLY"
        } else {
            "ALL INFO"
        },
        if filters.mode < 0 {
            "ALL MODES"
        } else {
            crate::server_browser::gametype_name(Some(filters.mode))
        },
    ];
    let s = layout.scale;
    for (index, label) in labels.into_iter().enumerate() {
        let rect = Rect::new(
            layout.margin + index as f32 * 145.0 * s,
            layout.rows_y,
            138.0 * s,
            28.0 * s,
        );
        ui.hit_region(BASE + index as u16, rect);
        ui.text(
            label,
            rect,
            12.0 * s,
            ui.theme().accent,
            FontWeight::Semibold,
            0.5 * s,
        );
    }
}
