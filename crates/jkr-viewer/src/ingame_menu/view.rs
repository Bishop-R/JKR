//! Hero presentation of the in-game menu: the main menu's left column drawn
//! over the live match, so pausing keeps the shell's look instead of
//! dropping a boxed card in the middle of the frame.

use super::{Page, VOTE_SCROLL_TOKEN, View};
use crate::menu_widgets::{HeroColumn, MenuCanvas};
use jkr_ui::{Color, DrawCommand, FontWeight, Rect};

const RED_TEAM: Color = Color::new(0.92, 0.20, 0.22, 1.0);
const BLUE_TEAM: Color = Color::new(0.18, 0.48, 0.96, 1.0);
/// Pages with more entries than this use the compact row size.
const COMPACT_ROWS: usize = 10;

/// The prepared entries of one page.
pub(super) struct Rows<'a> {
    pub(super) labels: &'a [String],
    pub(super) enabled: &'a [bool],
    /// `(first visible, total)` of a scrolling vote list.
    pub(super) scroll: Option<(usize, usize)>,
    /// Read-only lines shown above the entries.
    pub(super) info: &'a [String],
}

/// Height of one read-only info line.
const INFO_LINE: f32 = 30.0;

/// Entry list geometry for `rows` entries under `info` lines:
/// `(list top, row height, item scale)`.
fn list_metrics(viewport: [f32; 2], scale: f32, rows: usize, info: usize) -> (f32, f32, f32) {
    let top = viewport[1] * 0.13 + 90.0 * scale + info as f32 * INFO_LINE * scale;
    if rows > COMPACT_ROWS {
        (top, 42.0 * scale, scale * 0.6)
    } else {
        (top, 58.0 * scale, scale * 0.8)
    }
}

/// Draw `view`'s page into `canvas`; `kicker` is the context line above the
/// page title and `hint` the one-line description of each entry.
pub(super) fn build(
    canvas: &mut MenuCanvas,
    view: &View<'_>,
    kicker: &str,
    rows: Rows<'_>,
    hint: impl Fn(usize) -> &'static str,
    viewport: [f32; 2],
) {
    let column = HeroColumn::new(viewport);
    let (s, x, width) = (column.scale, column.margin, column.column_width);
    let count = rows.labels.len();
    let (list_top, row_height, item_scale) = list_metrics(viewport, s, count, rows.info.len());
    // The match stays untinted apart from the `ui_menuContrast` column
    // behind the entries; hover accents use a sweep.
    canvas.begin_transparent(viewport);
    canvas.readability_column(viewport);
    let theme = canvas.theme();
    let title_y = viewport[1] * 0.13;
    canvas.text(
        kicker,
        Rect::new(x, title_y - 30.0 * s, width, 18.0 * s),
        14.0 * s,
        theme.accent,
        FontWeight::Semibold,
        3.2 * s,
    );
    canvas.text(
        page_title(view.page),
        Rect::new(x - 2.0 * s, title_y, width, 64.0 * s),
        56.0 * s,
        theme.foreground,
        FontWeight::Semibold,
        -1.0 * s,
    );
    let info_top = viewport[1] * 0.13 + 84.0 * s;
    for (index, line) in rows.info.iter().enumerate() {
        canvas.text(
            line,
            Rect::new(
                x,
                info_top + index as f32 * INFO_LINE * s,
                width * 1.4,
                26.0 * s,
            ),
            20.0 * s,
            Color::new(0.82, 0.88, 0.94, 0.80),
            FontWeight::Regular,
            0.2 * s,
        );
    }
    for (row, label) in rows.labels.iter().enumerate() {
        let rect = column.row_rect(list_top, row, row_height);
        if let Some(color) = team_mark(view, row) {
            let _ = canvas.draw_list_mut().push(DrawCommand::SolidRect {
                rect: Rect::new(
                    x - 18.0 * s,
                    rect.y + 10.0 * s,
                    4.0 * s,
                    rect.height - 20.0 * s,
                ),
                color,
            });
        }
        let selected = row == view.selected_row;
        let enabled = rows.enabled[row];
        if view.page == Page::Siege {
            canvas.hero_line_entry(row as u16, label, rect, selected, item_scale);
        } else {
            canvas.hero_entry(
                row as u16,
                label,
                hint(row),
                rect,
                selected,
                enabled,
                item_scale,
            );
        }
    }
    if let Some((first, total)) = rows.scroll {
        let track_height = (count as f32 * row_height - 8.0 * s).max(24.0);
        canvas.scrollbar(
            VOTE_SCROLL_TOKEN,
            Rect::new(
                x + width + 16.0 * s,
                list_top + 4.0 * s,
                6.0 * s,
                track_height,
            ),
            first,
            super::callvote::PAGE_ITEMS,
            total,
        );
    }
    canvas.finish(view.selected_row as u16);
}

/// Colour bar beside a team entry on the join page of a team game.
fn team_mark(view: &View<'_>, row: usize) -> Option<Color> {
    match (view.page, view.team_game, row) {
        (Page::Team, true, 1) => Some(RED_TEAM),
        (Page::Team, true, 2) => Some(BLUE_TEAM),
        _ => None,
    }
}

fn page_title(page: Page) -> &'static str {
    match page {
        Page::Main => "Game menu",
        Page::Shot => "Shot controls",
        Page::Team => "Join / team",
        Page::Siege => "Choose your class",
        Page::About => "Server info",
        Page::Leave => "Leave",
        Page::CallVote => "Call vote",
        Page::VoteMap => "Choose map",
        Page::VoteGameType => "Choose game type",
        Page::VoteKick => "Kick player",
        Page::VoteClientKick => "Kick client number",
        Page::VoteWarmup => "Warmup",
        Page::VoteTimeLimit => "Time limit",
        Page::VoteFragLimit => "Frag limit",
    }
}
