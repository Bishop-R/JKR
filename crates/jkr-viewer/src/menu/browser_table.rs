//! The server table of the browser screen: one measured column grid shared
//! verbatim by the sortable header cells and the box-free rows below them.

use crate::menu_widgets::{FormLayout, MenuCanvas};
use crate::server_browser::{ServerBrowser, SortColumn};
use jkr_client::CompatProfile;
use jkr_ui::{FontWeight, Rect, TextAlign};

/// Height (at scale 1) of the column header line.
pub(crate) const HEADER_HEIGHT: f32 = 28.0;
/// Height (at scale 1) of one server row.
pub(crate) const ROW_HEIGHT: f32 = 36.0;
/// Wheel target under the rows.
pub(crate) const TABLE_TOKEN: u16 = 16;
/// Draggable track beside the rows.
pub(crate) const SCROLLBAR_TOKEN: u16 = 14;
/// Header cells `SERVER`..`MODE` are `HEADER_TOKEN + column`.
pub(crate) const HEADER_TOKEN: u16 = 100;
/// Row `n` is `ROW_TOKEN + n`.
pub(crate) const ROW_TOKEN: u16 = 1_000;

/// Column headers left to right with the sort column each one drives.
const COLUMNS: [(&str, Option<SortColumn>); 6] = [
    ("", None),
    ("SERVER", Some(SortColumn::Name)),
    ("MAP", Some(SortColumn::Map)),
    ("PLAYERS", Some(SortColumn::Players)),
    ("PING", Some(SortColumn::Ping)),
    ("MODE", Some(SortColumn::Gametype)),
];

/// Where the table, its rows and the details column sit on a hero form.
pub(crate) struct TableLayout {
    /// The column header line.
    pub(crate) header: Rect,
    /// The scrolling rows under it.
    pub(crate) rows: Rect,
    /// The details column to the right of the table.
    pub(crate) details: Rect,
}

impl TableLayout {
    /// The table runs from the left margin to the details column and from
    /// the form's first row down to just above the footer.
    pub(crate) fn new(layout: &FormLayout) -> Self {
        let s = layout.scale;
        let content_width = layout.viewport[0] - layout.margin * 2.0;
        let details_width = (content_width * 0.26).clamp(300.0 * s, 440.0 * s);
        let gap = 64.0 * s;
        let table_width = content_width - details_width - gap;
        let header = Rect::new(
            layout.margin,
            layout.rows_y + 34.0 * s,
            table_width,
            HEADER_HEIGHT * s,
        );
        let bottom = layout.viewport[1] - 84.0 * s;
        Self {
            header,
            rows: Rect::new(
                header.x,
                header.bottom(),
                table_width,
                bottom - header.bottom(),
            ),
            details: Rect::new(
                header.right() + gap,
                header.y,
                details_width,
                bottom - header.y,
            ),
        }
    }
}

/// Fixed column geometry shared verbatim by browser headers and cells.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BrowserGrid {
    pub(crate) cells: [Rect; 6],
    scale: f32,
}

impl BrowserGrid {
    pub(crate) fn new(rect: Rect, scale: f32) -> Self {
        let mark = 26.0 * scale;
        let data = rect.width - mark;
        let widths = [
            mark,
            data * 0.36,
            data * 0.17,
            data * 0.10,
            data * 0.09,
            data * 0.28,
        ];
        let mut x = rect.x;
        let cells = std::array::from_fn(|index| {
            let cell = Rect::new(x, rect.y, widths[index], rect.height);
            x += widths[index];
            cell
        });
        Self { cells, scale }
    }

    pub(crate) fn at_y(self, y: f32, height: f32) -> Self {
        Self {
            cells: self
                .cells
                .map(|cell| Rect::new(cell.x, y, cell.width, height)),
            scale: self.scale,
        }
    }

    /// The text area of cell `index`, inset from the column edges.
    pub(crate) fn inset(&self, index: usize) -> Rect {
        let cell = self.cells[index];
        let left = if index == 0 { 0.0 } else { 10.0 * self.scale };
        let right = if index == 0 { 0.0 } else { 12.0 * self.scale };
        Rect::new(
            cell.x + left,
            cell.y,
            (cell.width - left - right).max(1.0),
            cell.height,
        )
    }
}

/// Numeric columns are right-aligned so their digits line up.
fn align(index: usize) -> TextAlign {
    if matches!(index, 3 | 4) {
        TextAlign::End
    } else {
        TextAlign::Start
    }
}

/// The sortable column header line; returns the grid its rows reuse.
pub(crate) fn append_header(
    ui: &mut MenuCanvas,
    rect: Rect,
    browser: &ServerBrowser,
    s: f32,
) -> BrowserGrid {
    let grid = BrowserGrid::new(rect, s);
    let theme = ui.theme();
    let (sort, descending) = browser.sort_state();
    for (index, (label, column)) in COLUMNS.into_iter().enumerate() {
        let active = column.is_some() && column == Some(sort);
        let suffix = if !active {
            ""
        } else if descending {
            "  v"
        } else {
            "  ^"
        };
        let cell = grid.inset(index);
        ui.text_fmt_aligned(
            format_args!("{label}{suffix}"),
            Rect::new(cell.x, cell.y + 6.0 * s, cell.width, 16.0 * s),
            11.0 * s,
            if active { theme.accent } else { theme.muted },
            FontWeight::Semibold,
            1.6 * s,
            align(index),
        );
        if index > 0 {
            ui.hit_region(HEADER_TOKEN + index as u16 - 1, grid.cells[index]);
        }
    }
    ui.separator_line(Rect::new(rect.x, rect.bottom() - 1.0, rect.width, 1.0));
    grid
}

/// The visible window of server rows inside `rows_rect`, plus the empty-list
/// caption and, once the list outgrows the window, the scrollbar beside them.
pub(crate) fn append_rows(
    ui: &mut MenuCanvas,
    rows_rect: Rect,
    grid: BrowserGrid,
    browser: &mut ServerBrowser,
    s: f32,
) {
    let row_height = ROW_HEIGHT * s;
    let visible_rows = (rows_rect.height / row_height).max(1.0).floor() as usize;
    browser.set_page(visible_rows);
    let start = browser.scroll();
    let count = browser
        .visible_len()
        .saturating_sub(start)
        .min(visible_rows);
    ui.scroll_region(TABLE_TOKEN, rows_rect);
    for visible in 0..count {
        let row = start + visible;
        let Some(entry) = browser.visible_entry(row) else {
            continue;
        };
        let rect = Rect::new(
            rows_rect.x,
            rows_rect.y + visible as f32 * row_height,
            rows_rect.width,
            row_height,
        );
        let selected = row == browser.selected();
        ui.form_row_frame(rect, ROW_TOKEN + row as u16, selected, s);
        let theme = ui.theme();
        let color = if selected {
            theme.foreground
        } else {
            theme.muted
        };
        let cells = grid.at_y(rect.y, rect.height);
        if browser.is_favorite(entry.address) {
            ui.text(
                "*",
                cells.inset(0),
                16.0 * s,
                theme.accent,
                FontWeight::Semibold,
                0.0,
            );
        }
        ui.text(
            &entry.name,
            cells.inset(1),
            15.0 * s,
            color,
            FontWeight::Semibold,
            0.0,
        );
        ui.text(
            &entry.map,
            cells.inset(2),
            13.0 * s,
            color,
            FontWeight::Regular,
            0.0,
        );
        ui.text_fmt_aligned(
            format_args!("{}/{}", entry.players, entry.capacity),
            cells.inset(3),
            13.0 * s,
            color,
            FontWeight::Regular,
            0.0,
            align(3),
        );
        ui.text_fmt_aligned(
            format_args!("{} ms", entry.ping_millis),
            cells.inset(4),
            13.0 * s,
            color,
            FontWeight::Regular,
            0.0,
            align(4),
        );
        ui.text_fmt_aligned(
            format_args!(
                "{}  {}{}",
                entry.gametype,
                profile_label(&entry.profile),
                if entry.password { "  LOCKED" } else { "" }
            ),
            cells.inset(5),
            12.0 * s,
            color,
            FontWeight::Semibold,
            0.6 * s,
            align(5),
        );
    }
    if count == 0 {
        ui.text(
            if browser.favorites_only() {
                "No favourites yet.  Press F on a server to keep it here."
            } else if browser.is_refreshing() {
                "Waiting for the first server to answer..."
            } else {
                "No servers match this filter."
            },
            Rect::new(
                rows_rect.x,
                rows_rect.y + 14.0 * s,
                rows_rect.width,
                24.0 * s,
            ),
            15.0 * s,
            ui.theme().muted,
            FontWeight::Regular,
            0.1 * s,
        );
    }
    if browser.visible_len() <= visible_rows {
        return;
    }
    ui.scrollbar(
        SCROLLBAR_TOKEN,
        Rect::new(
            rows_rect.right() + 14.0 * s,
            rows_rect.y + 4.0 * s,
            4.0 * s,
            (rows_rect.height - 8.0 * s).max(1.0),
        ),
        start,
        visible_rows,
        browser.visible_len(),
    );
}

fn profile_label(profile: &CompatProfile) -> &'static str {
    match profile {
        CompatProfile::BaseJka => "BASE",
        CompatProfile::JaPlus { .. } => "JA+",
        CompatProfile::TaystJk => "TAYST",
        CompatProfile::Unknown(_) => "MOD",
    }
}
