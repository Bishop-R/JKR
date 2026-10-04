//! Floating hero scoreboard, using the existing retained match data.

use super::ScoreRow;
use crate::menu_widgets::MenuCanvas;
use jkr_ui::{Color, FontWeight, Rect, TextAlign};

/// Cached match identity for presentation.
pub(super) struct MatchHeader<'a> {
    pub(super) map: &'a str,
    pub(super) mode: &'a str,
    pub(super) team_scores: [i32; 2],
    pub(super) team_game: bool,
    pub(super) local_client: u16,
}

/// Build every retained row without tinting the world.
pub(super) fn build(
    ui: &mut MenuCanvas,
    rows: &[ScoreRow],
    header: MatchHeader<'_>,
    viewport: [f32; 2],
) {
    ui.begin_transparent(viewport);
    let layout = super::layout::Layout::new(viewport);
    let s = layout.scale;
    let width = layout.table_width;
    let x = layout.table_left;
    let theme = ui.theme();
    let groups = if header.team_game { 3 } else { 2 };
    let row_height = ((viewport[1] - 226.0 * s - groups as f32 * 46.0 * s)
        / rows.len().max(1) as f32)
        .min(38.0 * s);
    // Chat owns the left column; the world remains visible behind both overlays.
    ui.accent_bar(
        Rect::new(x - 18.0 * s, 64.0 * s, 4.0 * s, 72.0 * s),
        theme.accent,
    );
    ui.text(
        header.mode,
        Rect::new(x, 54.0 * s, width, 22.0 * s),
        14.0 * s,
        theme.accent,
        FontWeight::Semibold,
        2.0 * s,
    );
    ui.text(
        header.map,
        Rect::new(x, 80.0 * s, width, 64.0 * s),
        56.0 * s,
        theme.foreground,
        FontWeight::Semibold,
        -s,
    );
    let table = Rect::new(x, 160.0 * s, width, viewport[1] - 200.0 * s);
    column_header(ui, table, s, rows.iter().any(|r| r.deaths.is_some()));
    let mut y = table.y + 26.0 * s;
    let teams: &[u8] = if header.team_game {
        &[1, 2, 3]
    } else {
        &[0, 3]
    };
    for &team in teams {
        let (label, accent) = match team {
            1 => ("RED TEAM", Color::new(0.95, 0.28, 0.30, 1.0)),
            2 => ("BLUE TEAM", Color::new(0.30, 0.60, 1.0, 1.0)),
            3 => ("SPECTATORS", theme.muted),
            _ => ("PLAYERS", theme.accent),
        };
        if team == 3 && !rows.iter().any(|row| row.team == 3) {
            continue;
        }
        ui.accent_bar(Rect::new(x, y + 8.0 * s, 4.0 * s, 26.0 * s), accent);
        ui.text(
            label,
            Rect::new(x + 16.0 * s, y, width * 0.5, 42.0 * s),
            18.0 * s,
            accent,
            FontWeight::Semibold,
            s,
        );
        if team == 1 || team == 2 {
            ui.text_fmt_aligned(
                format_args!("{}", header.team_scores[usize::from(team - 1)]),
                Rect::new(x + width * 0.5, y, width * 0.5, 42.0 * s),
                36.0 * s,
                accent,
                FontWeight::Semibold,
                0.0,
                TextAlign::End,
            );
        }
        y += 46.0 * s;
        for row in rows
            .iter()
            .filter(|row| row.team == team || (team == 0 && row.team != 3))
        {
            player_row(
                ui,
                row,
                header.local_client,
                Rect::new(x, y, width, row_height),
                s.min(row_height / 28.0),
            );
            y += row_height;
        }
    }
    if rows.is_empty() {
        ui.text(
            "Requesting scores…",
            Rect::new(x, y, width, 32.0 * s),
            22.0 * s,
            theme.muted,
            FontWeight::Regular,
            0.0,
        );
    }
}

fn column_header(ui: &mut MenuCanvas, table: Rect, scale: f32, deaths: bool) {
    let columns = columns(table, scale);
    let score = if deaths { "SCORE / DEATHS" } else { "SCORE" };
    for (index, label) in ["PLAYER", score, "PING", "TIME"].into_iter().enumerate() {
        let rect = columns[index];
        ui.text_aligned(
            label,
            Rect::new(rect.x, table.y, rect.width, 22.0 * scale),
            12.0 * scale,
            ui.theme().muted,
            FontWeight::Semibold,
            scale,
            if index == 0 {
                TextAlign::Start
            } else {
                TextAlign::End
            },
        );
    }
}

fn player_row(ui: &mut MenuCanvas, row: &ScoreRow, local: u16, rect: Rect, s: f32) {
    let theme = ui.theme();
    if u16::from(row.client_num) == local {
        ui.accent_bar(
            Rect::new(rect.x, rect.y + 3.0 * s, 4.0 * s, rect.height - 6.0 * s),
            theme.accent,
        );
    }
    let [name, score, ping, time] = columns(rect, s);
    let color = if row.team == 3 {
        theme.muted
    } else {
        theme.foreground
    };
    ui.text(&row.name, name, 22.0 * s, color, FontWeight::Regular, 0.0);
    for (value, target) in [(row.score, score), (row.ping, ping), (row.time, time)] {
        if target == score
            && let Some(deaths) = row.deaths
        {
            ui.text_fmt_aligned(
                format_args!("{value}/{deaths}"),
                target,
                22.0 * s,
                color,
                FontWeight::Regular,
                0.0,
                TextAlign::End,
            );
            continue;
        }
        ui.text_fmt_aligned(
            format_args!("{value}"),
            target,
            22.0 * s,
            color,
            FontWeight::Regular,
            0.0,
            TextAlign::End,
        );
    }
}

fn columns(table: Rect, s: f32) -> [Rect; 4] {
    let name = table.width * 0.50;
    let number = table.width * 0.166;
    [
        Rect::new(table.x + 16.0 * s, table.y, name - 32.0 * s, table.height),
        Rect::new(table.x + name, table.y, number - 12.0 * s, table.height),
        Rect::new(
            table.x + name + number,
            table.y,
            number - 12.0 * s,
            table.height,
        ),
        Rect::new(
            table.x + name + number * 2.0,
            table.y,
            number - 12.0 * s,
            table.height,
        ),
    ]
}
