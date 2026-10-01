//! Connecting / connection-failed notices in the hero design: box-free text
//! in the left column over the untouched world (the gate opening behind it
//! is the whole show), with a single hero entry as the only action.

use crate::menu_widgets::{HeroColumn, MenuCanvas};
use jkr_ui::{FontWeight, Rect};

/// What a network notice says; the one action is always token 0.
pub(crate) struct NetworkNotice<'a> {
    /// Small accent kicker above the title, e.g. `NETWORK`.
    pub(crate) kicker: &'a str,
    /// The state, e.g. `Loading`.
    pub(crate) title: &'a str,
    /// Live status line from the client state machine.
    pub(crate) status: &'a str,
    /// One sentence of context under the status.
    pub(crate) body: &'a str,
    /// Label and hint of the single entry.
    pub(crate) action: (&'a str, &'a str),
}

/// Build `notice` into `canvas`; shared by the live client and evidence.
pub(crate) fn build(canvas: &mut MenuCanvas, viewport: [f32; 2], notice: &NetworkNotice) {
    let column = HeroColumn::new(viewport);
    let (s, x, width) = (column.scale, column.margin, column.column_width);
    let title_y = viewport[1] * 0.50;
    canvas.begin_transparent(viewport);
    let theme = canvas.theme();
    canvas.text(
        notice.kicker,
        Rect::new(x, title_y - 30.0 * s, width, 18.0 * s),
        14.0 * s,
        theme.accent,
        FontWeight::Semibold,
        3.2 * s,
    );
    canvas.text(
        notice.title,
        Rect::new(x, title_y, width, 60.0 * s),
        52.0 * s,
        theme.foreground,
        FontWeight::Semibold,
        -0.5 * s,
    );
    canvas.text(
        notice.status,
        Rect::new(x, title_y + 74.0 * s, width, 24.0 * s),
        17.0 * s,
        theme.foreground,
        FontWeight::Regular,
        0.0,
    );
    canvas.text(
        notice.body,
        Rect::new(x, title_y + 102.0 * s, width, 22.0 * s),
        15.0 * s,
        theme.muted,
        FontWeight::Regular,
        0.1 * s,
    );
    let (label, hint) = notice.action;
    canvas.hero_item(0, label, hint, action_rect(viewport), true, s);
    canvas.finish(0);
}

/// Pointer target of the notice's single entry at `viewport`.
pub(crate) fn action_rect(viewport: [f32; 2]) -> Rect {
    let column = HeroColumn::new(viewport);
    let s = column.scale;
    column.row_rect(viewport[1] * 0.50 + 150.0 * s, 0, 72.0 * s)
}
