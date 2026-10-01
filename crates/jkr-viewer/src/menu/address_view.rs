//! Direct-connect modal presentation.

use crate::menu_widgets::MenuCanvas;
use jkr_ui::{Color, FontWeight, Rect};

/// Append the shared direct-connect modal over an already-started menu canvas.
pub(crate) fn append(ui: &mut MenuCanvas, viewport: [f32; 2], input: &str, error: &str) {
    ui.hit_region(40, Rect::new(0.0, 0.0, viewport[0], viewport[1]));
    let card = ui.centered_card(540.0, 264.0, 32.0);
    ui.panel(card.panel);
    let y = ui.card_heading(
        card,
        "DIRECT CONNECT",
        "Connect to address",
        "IPv4 or hostname  /  default port 29070",
    );
    let field = Rect::new(card.content.x, y, card.content.width, 46.0);
    ui.text_field(field, true);
    ui.hit_region(41, field);
    ui.text(
        if input.is_empty() {
            "server.example.org:29070"
        } else {
            input
        },
        Rect::new(field.x + 14.0, field.y, field.width - 28.0, field.height),
        15.0,
        if input.is_empty() {
            ui.theme().muted
        } else {
            ui.theme().foreground
        },
        FontWeight::Regular,
        0.0,
    );
    ui.text(
        error,
        Rect::new(
            card.content.x,
            field.bottom() + 8.0,
            card.content.width,
            20.0,
        ),
        12.0,
        Color::new(1.0, 0.36, 0.32, 1.0),
        FontWeight::Semibold,
        0.0,
    );
    let buttons_y = card.content.bottom() - 42.0;
    ui.button(
        42,
        "Cancel",
        Rect::new(card.content.x, buttons_y, 120.0, 42.0),
        false,
    );
    ui.button(
        43,
        "Connect",
        Rect::new(card.content.right() - 142.0, buttons_y, 142.0, 42.0),
        false,
    );
}
