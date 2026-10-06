//! Hero selectors: readable names, selected accent, no tinted backplate.
use super::HudOverlay;
use jkr_client::selection::{FORCE_ORDER, Selection, SelectionView};
use jkr_protocol::PlayerState;
use jkr_ui::{
    Color, DrawCommand, DrawList, FontWeight, Rect, TextAlign, TextId, TextOverflow, Theme,
};

const POWERS: [&str; 18] = [
    "Heal",
    "Jump",
    "Speed",
    "Push",
    "Pull",
    "Mind Trick",
    "Grip",
    "Lightning",
    "Rage",
    "Protect",
    "Absorb",
    "Team Heal",
    "Team Energize",
    "Drain",
    "Seeing",
    "Saber Offense",
    "Saber Defense",
    "Saber Throw",
];
const ITEMS: [&str; 12] = [
    "",
    "Seeker Drone",
    "Portable Shield",
    "Bacta",
    "Large Bacta",
    "Electrobinoculars",
    "Sentry Gun",
    "Jetpack",
    "Health Dispenser",
    "Ammo Dispenser",
    "E-Web",
    "Cloaking Device",
];

impl HudOverlay {
    /// Project the local selector without allocating or owning input state.
    pub(crate) fn update_selection(
        &mut self,
        selection: &Selection,
        player: &PlayerState,
        time: i32,
    ) {
        self.selector = selection.view(player, time);
    }
}

/// Resolve static selector names (cg_draw.c:92-113).
pub(super) fn text(id: TextId) -> &'static str {
    match id.0 {
        1200 => "FORCE",
        1201 => "INVENTORY",
        1000..=1017 => POWERS[(id.0 - 1000) as usize],
        1100..=1111 => ITEMS[(id.0 - 1100) as usize],
        _ => "",
    }
}

/// Emit a bounded two-column selector into the existing retained HUD draw list.
pub(super) fn emit(
    draw: &mut DrawList,
    view: Option<SelectionView>,
    theme: Theme,
    viewport: [f32; 2],
    user_scale: f32,
) {
    let Some(view) = view else { return };
    let scale = crate::ui_scale::height_scale(viewport[1]) * user_scale.clamp(0.25, 2.0);
    let x = 60.0 * scale;
    let y = viewport[1] - 180.0 * (viewport[1] / 1080.0) - 350.0 * scale;
    let base = if view.inventory { 1100 } else { 1000 };
    let fade = |mut color: Color| {
        color.a *= view.alpha;
        color
    };
    label(
        draw,
        Rect::new(x, y, 430.0 * scale, 24.0 * scale),
        TextId(1200 + u32::from(view.inventory)),
        20.0 * scale,
        fade(theme.muted),
    );
    label(
        draw,
        Rect::new(x, y + 25.0 * scale, 440.0 * scale, 42.0 * scale),
        TextId(base + view.selected as u32),
        44.0 * scale,
        fade(theme.foreground),
    );
    let mut row = 0;
    for index in 0..18 {
        let tag = if view.inventory {
            index as u8
        } else {
            FORCE_ORDER[index]
        };
        if view.available & (1 << tag) == 0 {
            continue;
        }
        let column = row / 7;
        let rect = Rect::new(
            x + column as f32 * 270.0 * scale,
            y + (82.0 + (row % 7) as f32 * 38.0) * scale,
            258.0 * scale,
            32.0 * scale,
        );
        let selected = tag == view.selected;
        if selected {
            let _ = draw.push(DrawCommand::SolidRect {
                rect: Rect::new(
                    rect.x - 10.0 * scale,
                    rect.y + 3.0 * scale,
                    3.0 * scale,
                    20.0 * scale,
                ),
                color: fade(theme.accent),
            });
        }
        label(
            draw,
            rect,
            TextId(base + tag as u32),
            26.0 * scale,
            fade(if selected { theme.accent } else { theme.muted }),
        );
        row += 1;
    }
}

fn label(draw: &mut DrawList, rect: Rect, text: TextId, size: f32, color: Color) {
    let _ = draw.push(DrawCommand::Text {
        rect,
        text,
        size,
        color,
        align: TextAlign::Start,
        overflow: TextOverflow::Ellipsis,
        weight: FontWeight::Semibold,
        letter_spacing: 0.0,
    });
}
