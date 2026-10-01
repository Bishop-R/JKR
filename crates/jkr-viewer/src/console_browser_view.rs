//! Drawing of the console browser: hero header, filter tabs, search field and a
//! two-line row per entry (name, value and default; then its description).

use super::pointer::{ACTIVATE_TOKEN, RESET_TOKEN, ROW_BASE, SCROLLBAR_TOKEN, SEARCH_TOKEN};
use super::{Browser, Entry, Kind, TABS};
use crate::menu_widgets::{BACK_TOKEN, FormLayout, Scrim};
use crate::text::{TextVertex, UiFont};
use jkr_shell::CommandSource;
use jkr_ui::{Color, FontWeight, Rect, TextAlign};

impl Browser {
    /// Draw the browser over the whole frame. Overlay text draws above every overlay's
    /// shapes, so the text other overlays appended earlier this frame (menus, HUD,
    /// chat) is dropped rather than shown through the browser.
    pub(crate) fn append(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
    ) {
        vertices.clear();

        let mut layout = FormLayout::new(viewport);
        let s = layout.scale;
        layout.margin = (viewport[0] * 0.06).max(48.0 * s);
        layout.column_width = viewport[0] - layout.margin * 2.0;
        let search = Rect::new(
            layout.margin,
            layout.tabs_y() + 42.0 * s,
            layout.column_width.min(560.0 * s),
            36.0 * s,
        );
        layout.rows_y = search.bottom() + 18.0 * s;
        layout.row_height = 58.0 * s;
        let rows_bottom = viewport[1] - 84.0 * s;
        self.rows = (((rows_bottom - layout.rows_y) / layout.row_height).floor()).max(1.0) as usize;
        self.first = self.first.min(self.visible.len().saturating_sub(self.rows));

        self.ui.begin_hero(viewport, 1.0, Scrim::Wide);
        self.ui.form_header(
            &layout,
            "JKR   /   CONSOLE",
            "COMMANDS & CVARS",
            &self.summary,
        );
        self.ui.form_tabs(&layout, &TABS, self.tab);
        self.search_view(search, s);

        let shown = self.first..self.visible.len().min(self.first + self.rows);
        for (slot, position) in shown.enumerate() {
            let rect = Rect::new(
                layout.margin,
                layout.rows_y + slot as f32 * layout.row_height,
                layout.column_width,
                layout.row_height,
            );
            self.row_view(rect, slot, position, s);
        }
        if self.visible.is_empty() {
            let muted = self.ui.theme().muted;
            self.ui.text(
                "No command or cvar matches the search.",
                Rect::new(
                    layout.margin,
                    layout.rows_y + 12.0 * s,
                    layout.column_width,
                    22.0 * s,
                ),
                16.0 * s,
                muted,
                FontWeight::Regular,
                0.2 * s,
            );
        } else if self.visible.len() > self.rows {
            self.ui.scrollbar(
                SCROLLBAR_TOKEN,
                Rect::new(
                    layout.margin + layout.column_width + 14.0 * s,
                    layout.rows_y,
                    5.0 * s,
                    self.rows as f32 * layout.row_height,
                ),
                self.first,
                self.rows,
                self.visible.len(),
            );
        }

        let editing = self.editing.is_some();
        self.ui.form_footer_actions(
            &layout,
            &[
                (
                    "ENTER",
                    if editing { "Apply" } else { "Edit / insert" },
                    ACTIVATE_TOKEN,
                ),
                ("DEL", "Default", RESET_TOKEN),
                ("TAB", "Filter", 0),
                ("ESC", if editing { "Cancel" } else { "Close" }, BACK_TOKEN),
            ],
        );
        if !self.status.is_empty() {
            let theme = self.ui.theme();
            self.ui.text_aligned(
                &self.status,
                Rect::new(
                    layout.margin,
                    viewport[1] - 62.0 * s,
                    layout.column_width,
                    20.0 * s,
                ),
                14.0 * s,
                if self.status_error {
                    theme.critical
                } else {
                    theme.muted
                },
                FontWeight::Regular,
                0.2 * s,
                TextAlign::End,
            );
        }
        self.ui.end_hero();
        let selected = self.selected.saturating_sub(self.first);
        self.ui
            .finish(ROW_BASE + selected.min(usize::from(u16::MAX - ROW_BASE)) as u16);
        self.ui.append_text(vertices, font, viewport);
    }

    /// The search field: the filter and a caret while it has the keyboard, else a prompt.
    fn search_view(&mut self, rect: Rect, s: f32) {
        let active = self.editing.is_none();
        self.ui.text_field(rect, active);
        self.ui.hit_region(SEARCH_TOKEN, rect);
        let theme = self.ui.theme();
        let text = Rect::new(
            rect.x + 12.0 * s,
            rect.y + 9.0 * s,
            rect.width - 24.0 * s,
            20.0 * s,
        );
        if self.filter.is_empty() && !active {
            self.ui.text(
                "Search names and descriptions",
                text,
                15.0 * s,
                theme.muted,
                FontWeight::Regular,
                0.2 * s,
            );
        } else if self.filter.is_empty() {
            self.ui.text(
                "Type to search names and descriptions_",
                text,
                15.0 * s,
                theme.muted,
                FontWeight::Regular,
                0.2 * s,
            );
        } else {
            self.ui.text_fmt_aligned(
                format_args!("{}{}", self.filter, if active { "_" } else { "" }),
                text,
                15.0 * s,
                theme.foreground,
                FontWeight::Regular,
                0.2 * s,
                TextAlign::Start,
            );
        }
    }

    /// One entry: name, then value and default on the first line; kind and
    /// description on the second.
    fn row_view(&mut self, rect: Rect, slot: usize, position: usize, s: f32) {
        let selected = position == self.selected;
        let entry = &self.entries[self.visible[position]];
        self.ui
            .form_row_frame(rect, ROW_BASE + slot as u16, selected, s);
        let theme = self.ui.theme();
        let line = |x: f32, width: f32, second: bool| {
            Rect::new(
                rect.x + x * rect.width,
                rect.y + if second { 32.0 } else { 9.0 } * s,
                width * rect.width,
                if second { 18.0 } else { 22.0 } * s,
            )
        };
        self.ui.text(
            &entry.name,
            line(0.0, 0.40, false),
            17.0 * s,
            if selected {
                theme.foreground
            } else {
                Color::new(0.82, 0.88, 0.94, 0.86)
            },
            if selected {
                FontWeight::Semibold
            } else {
                FontWeight::Regular
            },
            0.2 * s,
        );
        let value_rect = line(0.42, 0.30, false);
        match &entry.kind {
            Kind::Cvar { value, default, .. } => {
                if let Some(edit) = self.editing.as_deref().filter(|_| selected) {
                    self.ui.text_fmt_aligned(
                        format_args!("{edit}_"),
                        value_rect,
                        16.0 * s,
                        theme.accent,
                        FontWeight::Semibold,
                        0.2 * s,
                        TextAlign::Start,
                    );
                    self.ui.edit_underline(value_rect, theme.accent, s);
                } else {
                    self.ui.text(
                        value,
                        value_rect,
                        16.0 * s,
                        if selected {
                            theme.accent
                        } else if entry.changed() {
                            theme.foreground
                        } else {
                            theme.muted
                        },
                        FontWeight::Semibold,
                        0.2 * s,
                    );
                }
                self.ui.text_fmt_aligned(
                    format_args!("default  {default}"),
                    line(0.74, 0.26, false),
                    14.0 * s,
                    theme.muted,
                    FontWeight::Regular,
                    0.2 * s,
                    TextAlign::End,
                );
            }
            Kind::Command(_) => {}
        }
        self.ui.text_fmt_aligned(
            format_args!("{}{}", tag(entry), entry.description),
            line(0.0, 1.0, true),
            13.0 * s,
            Color::new(0.70, 0.78, 0.86, 0.72),
            FontWeight::Regular,
            0.2 * s,
            TextAlign::Start,
        );
    }
}

/// Kind label at the start of an entry's description line.
fn tag(entry: &Entry) -> &'static str {
    match entry.kind {
        Kind::Command(CommandSource::External) => "SERVER COMMAND   ",
        Kind::Command(_) => "COMMAND   ",
        Kind::Cvar {
            read_only: true, ..
        } => "CVAR, READ-ONLY   ",
        Kind::Cvar { archived: true, .. } => "CVAR, SAVED   ",
        Kind::Cvar { .. } => "CVAR   ",
    }
}
