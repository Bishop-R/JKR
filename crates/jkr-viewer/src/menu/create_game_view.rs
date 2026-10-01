//! Create game presentation: the shared hero form over the live map — a
//! left column of box-free rows (cyclers, a name field, a LAN toggle), the
//! Start action and one status line; nothing behind the text.

use super::create_game::{CreateGameMenu, ROWS, Row};
use super::create_game_catalog::{MODES, ScoreLimit};
use super::map_picker_view;
use crate::menu_widgets::{FormLayout, MenuCanvas, Scrim};
use crate::{TextVertex, UiFont};
use jkr_ui::{FontWeight, Rect};

/// Footer caps; only the back cap, which doubles as the pointer's way out.
const KEY_HINTS: [(&str, &str); 1] = [("ESC", "Back")];

/// The stock game's names for bot skills 1 to 5.
const SKILL_NAMES: [&str; 5] = ["Initiate", "Padawan", "Jedi", "Jedi Knight", "Jedi Master"];

impl CreateGameMenu {
    /// Build the screen at `reveal` opacity and append its text.
    pub(crate) fn append(
        &mut self,
        vertices: &mut Vec<TextVertex>,
        font: &UiFont,
        viewport: [f32; 2],
        reveal: f32,
    ) {
        if self.picker.is_open() {
            self.append_picker(vertices, font, viewport, reveal);
            return;
        }
        let layout = FormLayout::new(viewport);
        self.ui.begin_hero(viewport, reveal, Scrim::Full);
        self.ui.form_header(
            &layout,
            "JKR   /   HOST",
            "Create game",
            if self.draft.allow_lan {
                "A match on this machine. Players on your network can join."
            } else {
                "A match on this machine, for you and the bots only."
            },
        );
        for (row, kind) in ROWS.iter().enumerate() {
            self.row_view(&layout, row, *kind);
        }
        self.form_preview(&layout);
        let s = layout.scale;
        let status_y = layout.row_rect(ROWS.len()).y + 14.0 * s;
        let theme = self.ui.theme();
        self.ui.text(
            &self.status,
            Rect::new(layout.margin, status_y, layout.column_width, 20.0 * s),
            15.0 * s,
            theme.muted,
            FontWeight::Regular,
            0.2 * s,
        );
        self.ui.form_footer(&layout, &KEY_HINTS);
        self.ui.end_hero();
        self.ui.finish(self.selected as u16);
        self.ui.append_text(vertices, font, viewport);
    }

    /// One row: frame, label and value control.
    fn row_view(&mut self, layout: &FormLayout, row: usize, kind: Row) {
        let selected = row == self.selected;
        if kind == Row::Start {
            let action = if self.hosting() {
                "STARTING..."
            } else {
                "START  >"
            };
            self.ui
                .form_action_row(layout, row, selected, "Start", action);
            return;
        }
        let s = layout.scale;
        let rect = layout.row_rect(row);
        let mode = MODES[self.draft.mode];
        let label = match (kind, mode.score) {
            (Row::ScoreLimit, ScoreLimit::Captures) => "Capture limit",
            (Row::ScoreLimit, ScoreLimit::Frags) => "Frag limit",
            _ => label(kind),
        };
        self.ui.form_row_frame(rect, row as u16, selected, s);
        self.ui.form_label(rect, label, selected, s);
        let zone = layout.value_zone(rect);
        let color = self.ui.form_value_color(selected);
        let draft = &self.draft;
        match kind {
            Row::Mode => self.ui.form_cycler(zone, mode.label, None, color, s),
            Row::Map => {
                // Field by field, so the title is borrowed, not copied.
                let title = self
                    .catalogue
                    .map(draft.mode, &draft.map)
                    .map_or(draft.map.as_str(), |entry| entry.title.as_str());
                self.ui.form_cycler(zone, title, None, color, s);
            }
            Row::Bots if draft.bots == 0 => self.ui.form_cycler(zone, "None", None, color, s),
            Row::Bots => {
                self.ui
                    .form_cycler_fmt(zone, format_args!("{}", draft.bots), None, color, s)
            }
            Row::BotSkill => {
                let name = SKILL_NAMES[usize::from(draft.bot_skill.clamp(1, 5)) - 1];
                self.ui.form_cycler(zone, name, None, color, s);
            }
            Row::ScoreLimit => {
                let limit = match mode.score {
                    ScoreLimit::Frags => draft.fraglimit,
                    ScoreLimit::Captures => draft.capturelimit,
                    ScoreLimit::None => {
                        self.ui.form_value("Objectives", zone, color, s);
                        return;
                    }
                };
                if limit == 0 {
                    self.ui.form_cycler(zone, "None", None, color, s);
                } else {
                    self.ui
                        .form_cycler_fmt(zone, format_args!("{limit}"), None, color, s);
                }
            }
            Row::TimeLimit if draft.timelimit == 0 => {
                self.ui.form_cycler(zone, "None", None, color, s)
            }
            Row::TimeLimit => {
                self.ui.form_cycler_fmt(
                    zone,
                    format_args!("{} min", draft.timelimit),
                    None,
                    color,
                    s,
                );
            }
            Row::Hostname => self.name_field(zone, rect, color, s),
            Row::Lan => lan_toggle(&mut self.ui, zone, rect, draft.allow_lan, color, s),
            Row::Start => {}
        }
    }

    /// The chosen map's levelshot, small, right of the Map row; a click on
    /// it opens the map list like the row does.
    fn form_preview(&mut self, layout: &FormLayout) {
        let s = layout.scale;
        let Some(row) = ROWS.iter().position(|kind| *kind == Row::Map) else {
            return;
        };
        let x = layout.margin + layout.column_width + 40.0 * s;
        let width = (240.0 * s).min(layout.viewport[0] - x - layout.margin);
        if width < 96.0 * s {
            return;
        }
        let rect = Rect::new(x, layout.row_rect(row).y, width, width * 0.75);
        let preview = self.levelshots.preview(self.preview_map());
        map_picker_view::draw_preview(&mut self.ui, rect, preview, s);
        self.ui.hit_region(row as u16, rect);
    }

    /// The server name, or the name being typed with its underline.
    fn name_field(&mut self, zone: Rect, rect: Rect, color: jkr_ui::Color, s: f32) {
        match &self.editing {
            Some(buffer) => {
                self.ui.form_value(buffer, zone, color, s);
                let field = Rect::new(zone.x, rect.y + 6.0 * s, zone.width, rect.height - 12.0 * s);
                let accent = self.ui.theme().accent;
                self.ui.edit_underline(field, accent, s);
            }
            None => self.ui.form_value(&self.draft.hostname, zone, color, s),
        }
    }
}

/// The LAN row: a toggle pill and what it means.
fn lan_toggle(ui: &mut MenuCanvas, zone: Rect, rect: Rect, on: bool, color: jkr_ui::Color, s: f32) {
    let accent = ui.theme().accent;
    let pill = Rect::new(
        zone.right() - 44.0 * s,
        rect.y + 15.0 * s,
        44.0 * s,
        22.0 * s,
    );
    ui.toggle_pill(pill, on, accent);
    let label = Rect::new(zone.x, rect.y, zone.width - 58.0 * s, rect.height);
    ui.form_value(if on { "On" } else { "This PC only" }, label, color, s);
}

/// A row's label when it does not depend on the mode.
fn label(kind: Row) -> &'static str {
    match kind {
        Row::Mode => "Game type",
        Row::Map => "Map",
        Row::Bots => "Bots",
        Row::BotSkill => "Bot skill",
        Row::ScoreLimit => "Score limit",
        Row::TimeLimit => "Time limit",
        Row::Hostname => "Server name",
        Row::Lan => "Allow LAN players",
        Row::Start => "Start",
    }
}
