//! The server-details column beside the browser table: hostname, the
//! server's published cvars, and the connected players with score and ping,
//! set in the same box-free style as the table.

use crate::menu_widgets::MenuCanvas;
use crate::server_browser::{DetailsState, DetailsView};
use jkr_ui::{FontWeight, Rect, TextAlign};

/// Line pitch (at scale 1) of the fact list.
const LINE: f32 = 24.0;
/// Row pitch (at scale 1) of the player list.
const PLAYER_ROW: f32 = 26.0;

/// Draw the details column for `state` inside `rect` at UI scale `s`.
pub(super) fn append_details(ui: &mut MenuCanvas, rect: Rect, state: DetailsState<'_>, s: f32) {
    ui.text(
        "SELECTED SERVER",
        Rect::new(rect.x, rect.y + 6.0 * s, rect.width, 16.0 * s),
        11.0 * s,
        ui.theme().accent,
        FontWeight::Semibold,
        1.6 * s,
    );
    ui.separator_line(Rect::new(rect.x, rect.y + 28.0 * s - 1.0, rect.width, 1.0));
    let body = Rect::new(
        rect.x,
        rect.y + 40.0 * s,
        rect.width,
        rect.height - 40.0 * s,
    );
    match state {
        DetailsState::Nothing => caption(ui, body, "Select a server to see who is playing", s),
        DetailsState::Querying(address) => {
            address_line(ui, body, address, s);
            caption(ui, body, "Asking the server...", s);
        }
        DetailsState::Failed(address, error) => {
            address_line(ui, body, address, s);
            caption(ui, body, "No answer to the status query", s);
            ui.text(
                error,
                Rect::new(body.x, body.y + 62.0 * s, body.width, LINE * s),
                11.0 * s,
                ui.theme().muted,
                FontWeight::Regular,
                0.0,
            );
        }
        DetailsState::Ready(view) => append_view(ui, body, view, s),
    }
}

fn address_line(ui: &mut MenuCanvas, rect: Rect, address: std::net::SocketAddr, s: f32) {
    ui.text_fmt_aligned(
        format_args!("{address}"),
        Rect::new(rect.x, rect.y, rect.width, LINE * s),
        12.0 * s,
        ui.theme().muted,
        FontWeight::Semibold,
        0.6 * s,
        TextAlign::Start,
    );
}

fn caption(ui: &mut MenuCanvas, rect: Rect, text: &str, s: f32) {
    ui.text(
        text,
        Rect::new(rect.x, rect.y + 30.0 * s, rect.width, LINE * s),
        13.0 * s,
        ui.theme().muted,
        FontWeight::Regular,
        0.1 * s,
    );
}

fn append_view(ui: &mut MenuCanvas, rect: Rect, view: &DetailsView, s: f32) {
    let theme = ui.theme();
    ui.text(
        &view.hostname,
        Rect::new(rect.x, rect.y, rect.width, 30.0 * s),
        20.0 * s,
        theme.foreground,
        FontWeight::Semibold,
        0.0,
    );
    ui.text_fmt_aligned(
        format_args!("{}    {}", view.address, view.map),
        Rect::new(rect.x, rect.y + 32.0 * s, rect.width, LINE * s),
        12.0 * s,
        theme.muted,
        FontWeight::Semibold,
        0.6 * s,
        TextAlign::Start,
    );
    let mut y = rect.y + 70.0 * s;
    let label_width = (rect.width * 0.36).min(150.0 * s);
    for fact in &view.facts {
        ui.text(
            fact.label,
            Rect::new(rect.x, y, label_width, LINE * s),
            12.0 * s,
            theme.muted,
            FontWeight::Regular,
            0.0,
        );
        ui.text(
            &fact.value,
            Rect::new(rect.x + label_width, y, rect.width - label_width, LINE * s),
            12.0 * s,
            theme.foreground,
            FontWeight::Regular,
            0.0,
        );
        y += LINE * s;
    }
    y += 10.0 * s;
    ui.separator_line(Rect::new(rect.x, y, rect.width, 1.0));
    y += 12.0 * s;
    let humans = view.players.len() - view.bots;
    ui.text_fmt_aligned(
        format_args!("PLAYERS  {humans}"),
        Rect::new(rect.x, y, rect.width * 0.6, LINE * s),
        11.0 * s,
        theme.accent,
        FontWeight::Semibold,
        1.6 * s,
        TextAlign::Start,
    );
    if view.bots > 0 {
        ui.text_fmt_aligned(
            format_args!("{} bots", view.bots),
            Rect::new(rect.x + rect.width * 0.6, y, rect.width * 0.4, LINE * s),
            11.0 * s,
            theme.muted,
            FontWeight::Semibold,
            1.0 * s,
            TextAlign::End,
        );
    }
    y += (LINE + 2.0) * s;
    append_players(
        ui,
        Rect::new(rect.x, y, rect.width, rect.bottom() - y),
        view,
        s,
    );
}

/// Score, name and ping per player; the last line notes who did not fit.
fn append_players(ui: &mut MenuCanvas, rect: Rect, view: &DetailsView, s: f32) {
    let theme = ui.theme();
    if view.players.is_empty() {
        ui.text(
            "Nobody on the server right now",
            Rect::new(rect.x, rect.y, rect.width, LINE * s),
            13.0 * s,
            theme.muted,
            FontWeight::Regular,
            0.0,
        );
        return;
    }
    let row = PLAYER_ROW * s;
    let score_width = 52.0 * s;
    let ping_width = 60.0 * s;
    let name_x = rect.x + score_width + 8.0 * s;
    let name_width = rect.width - score_width - ping_width - 16.0 * s;
    let rows = (rect.height / row).max(0.0) as usize;
    // Leave the last line for the overflow note when not everyone fits.
    let shown = if view.players.len() > rows {
        rows.saturating_sub(1)
    } else {
        rows
    };
    let mut y = rect.y;
    for player in view.players.iter().take(shown) {
        let bot = player.ping == 0;
        let color = if bot { theme.muted } else { theme.foreground };
        ui.text_fmt_aligned(
            format_args!("{}", player.score),
            Rect::new(rect.x, y, score_width, row),
            13.0 * s,
            color,
            FontWeight::Semibold,
            0.0,
            TextAlign::End,
        );
        ui.text(
            &player.name,
            Rect::new(name_x, y, name_width.max(1.0), row),
            13.0 * s,
            color,
            FontWeight::Regular,
            0.0,
        );
        let ping = Rect::new(rect.right() - ping_width, y, ping_width, row);
        if bot {
            ui.text_aligned(
                "bot",
                ping,
                12.0 * s,
                theme.muted,
                FontWeight::Regular,
                0.0,
                TextAlign::End,
            );
        } else {
            ui.text_fmt_aligned(
                format_args!("{} ms", player.ping),
                ping,
                12.0 * s,
                theme.muted,
                FontWeight::Regular,
                0.0,
                TextAlign::End,
            );
        }
        y += row;
    }
    if view.players.len() > shown {
        ui.text_fmt_aligned(
            format_args!("+ {} more", view.players.len() - shown),
            Rect::new(rect.x, rect.bottom() - LINE * s, rect.width, LINE * s),
            12.0 * s,
            theme.muted,
            FontWeight::Regular,
            0.0,
            TextAlign::Start,
        );
    }
}
