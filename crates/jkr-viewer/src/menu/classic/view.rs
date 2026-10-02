//! Drawing of the classic main menu: the retail page composition (logo,
//! page title, gold entries with a glow behind the focused one, the
//! description line underneath) on the 640x480 canvas fitted into the
//! window, over a dimmed live map. Only JKR's own vector shapes and text
//! are drawn; no retail artwork is needed.

use super::ClassicMain;
use super::layout::{CANVAS, HINT_Y, LOGO, Page, Placement};
use crate::menu_widgets::MenuCanvas;
use jkr_ui::{Color, DrawCommand, FontWeight, Gradient, Rect, TextAlign};

/// Retail entry colour (`forecolor 1 .682 0`).
const GOLD: Color = Color::new(1.0, 0.682, 0.0, 1.0);
/// Retail focus colour (`focusColor 1 1 1 1`).
const FOCUS: Color = Color::new(1.0, 1.0, 1.0, 1.0);
/// Retail page-title colour (`forecolor .695 .760 .861`).
const TITLE: Color = Color::new(0.695, 0.760, 0.861, 1.0);
/// Retail description colour (`descColor 1 .682 0 .8`).
const HINT: Color = Color::new(1.0, 0.682, 0.0, 0.8);
const INK: [f32; 3] = [0.004, 0.008, 0.020];

fn ink(alpha: f32) -> Color {
    Color::new(INK[0], INK[1], INK[2], alpha)
}

fn gold(alpha: f32) -> Color {
    Color::new(GOLD.r, GOLD.g, GOLD.b, alpha)
}

/// Build the classic main menu into `canvas` at `reveal` opacity.
pub(crate) fn build(canvas: &mut MenuCanvas, viewport: [f32; 2], menu: &ClassicMain, reveal: f32) {
    let place = Placement::new(viewport);
    let s = place.scale;
    canvas.begin_transparent(viewport);
    canvas.push_opacity(reveal);
    backdrop(canvas, viewport, &place);
    logo(canvas, &place);
    let page = menu.page();
    let (title, title_y) = page.title();
    canvas.text_aligned(
        title,
        place.centered([CANVAS[0] * 0.5, title_y], 400.0, 20.0),
        16.0 * s,
        TITLE,
        FontWeight::Semibold,
        3.0 * s,
        TextAlign::Center,
    );
    if page == Page::Quit {
        canvas.text_aligned(
            "Quit to the desktop?",
            place.centered([CANVAS[0] * 0.5, 260.0], 450.0, 24.0),
            18.0 * s,
            FOCUS,
            FontWeight::Regular,
            0.4 * s,
            TextAlign::Center,
        );
    }
    let mut described = menu.selection();
    for (index, slot) in menu.slots().iter().enumerate() {
        let token = index as u16;
        let target = place.rect(slot.target());
        let hovered = canvas.token_hovered(token);
        if hovered {
            described = index;
        }
        let active = index == menu.selection() || hovered;
        if active {
            glow(canvas, target, s);
        }
        let size = slot.size.text();
        canvas.text_aligned(
            slot.label,
            place.centered(slot.center, slot.width + 40.0, size * 1.2),
            size * s,
            if active { FOCUS } else { GOLD },
            FontWeight::Semibold,
            1.2 * s,
            TextAlign::Center,
        );
        canvas.hit_region(token, target);
    }
    if let Some(slot) = menu.slots().get(described) {
        canvas.text_aligned(
            slot.hint,
            place.centered([CANVAS[0] * 0.5, HINT_Y], 560.0, 18.0),
            13.0 * s,
            HINT,
            FontWeight::Regular,
            0.3 * s,
            TextAlign::Center,
        );
    }
    let muted = canvas.theme().muted;
    canvas.text_aligned(
        super::super::main_view::VERSION_LINE,
        Rect::new(
            viewport[0] - 336.0 * s,
            viewport[1] - 26.0 * s,
            320.0 * s,
            16.0 * s,
        ),
        6.0 * s,
        muted,
        FontWeight::Regular,
        0.5 * s,
        TextAlign::End,
    );
    canvas.pop_opacity();
    canvas.finish(menu.selection() as u16);
}

/// Dim the live map so the page reads as one surface (retail drew an
/// opaque background here), darker at the top and bottom bands.
fn backdrop(canvas: &mut MenuCanvas, viewport: [f32; 2], place: &Placement) {
    let [width, height] = viewport;
    let draw = canvas.draw_list_mut();
    let _ = draw.push(DrawCommand::SolidRect {
        rect: Rect::new(0.0, 0.0, width, height),
        color: ink(0.58),
    });
    let _ = draw.push(DrawCommand::GradientRect {
        rect: Rect::new(0.0, 0.0, width, height * 0.30),
        radius: 0.0,
        gradient: Gradient {
            start: ink(0.55),
            end: ink(0.0),
            vertical: true,
        },
    });
    let _ = draw.push(DrawCommand::GradientRect {
        rect: Rect::new(0.0, height * 0.78, width, height * 0.22),
        radius: 0.0,
        gradient: Gradient {
            start: ink(0.0),
            end: ink(0.60),
            vertical: true,
        },
    });
    // Hairlines framing the button area, where retail drew its window art.
    for y in [180.0, 360.0] {
        let _ = draw.push(DrawCommand::GradientRect {
            rect: place.rect([16.0, y, 304.0, 1.0]),
            radius: 0.0,
            gradient: Gradient {
                start: gold(0.0),
                end: gold(0.22),
                vertical: false,
            },
        });
        let _ = draw.push(DrawCommand::GradientRect {
            rect: place.rect([320.0, y, 304.0, 1.0]),
            radius: 0.0,
            gradient: Gradient {
                start: gold(0.22),
                end: gold(0.0),
                vertical: false,
            },
        });
    }
}

/// Game title where the retail logo sits.
fn logo(canvas: &mut MenuCanvas, place: &Placement) {
    let s = place.scale;
    let [x, y, width, height] = LOGO;
    let center = x + width * 0.5;
    canvas.text_aligned(
        "JEDI KNIGHT",
        place.centered([center, y + height * 0.30], width, 16.0),
        13.0 * s,
        GOLD,
        FontWeight::Semibold,
        6.0 * s,
        TextAlign::Center,
    );
    canvas.text_aligned(
        "JEDI ACADEMY",
        place.centered([center, y + height * 0.62], width, 46.0),
        40.0 * s,
        FOCUS,
        FontWeight::Semibold,
        4.0 * s,
        TextAlign::Center,
    );
}

/// The retail `menu_buttonback` glow behind a focused entry: a soft gold
/// band, brightest in the middle.
fn glow(canvas: &mut MenuCanvas, target: Rect, scale: f32) {
    let half = Rect::new(target.x, target.y, target.width * 0.5, target.height);
    let draw = canvas.draw_list_mut();
    let _ = draw.push(DrawCommand::GradientRect {
        rect: half,
        radius: 0.0,
        gradient: Gradient {
            start: gold(0.0),
            end: gold(0.26),
            vertical: false,
        },
    });
    let _ = draw.push(DrawCommand::GradientRect {
        rect: Rect::new(
            half.right(),
            target.y,
            target.width - half.width,
            target.height,
        ),
        radius: 0.0,
        gradient: Gradient {
            start: gold(0.26),
            end: gold(0.0),
            vertical: false,
        },
    });
    let _ = draw.push(DrawCommand::SolidRect {
        rect: Rect::new(
            target.x + target.width * 0.2,
            target.bottom() - 1.5 * scale,
            target.width * 0.6,
            1.5 * scale,
        ),
        color: gold(0.7),
    });
}
