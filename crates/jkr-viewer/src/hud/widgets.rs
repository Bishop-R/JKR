//! Data-bound HUD widget emission into the renderer-neutral draw list.

use super::{HudLayout, WidgetData, text_id};
use jkr_ui::{
    Color, DrawCommand, DrawList, FontWeight, Gradient, HudDataSource, HudWidget, HudWidgetKind,
    Insets, Rect, TextAlign, TextId, TextOverflow, TextureId, Theme,
};

pub(super) struct EmitContext<'a> {
    pub(super) data: &'a WidgetData<'a>,
    /// Live presentation switches.
    pub(super) family: super::family::Policy,
    /// Sampled crosshair and telemetry placement.
    pub(super) targeting: super::targeting::Policy,
    /// Retained icon handles and sampled tint/size policy.
    pub(super) icons: &'a super::icons::Icons,
    /// Physical viewport for obituary anchoring.
    pub(super) viewport: [f32; 2],
    pub(super) dpi_scale: f32,
    /// Hero overlay scale, independent of the compact HUD's historical clamp.
    pub(super) hero_scale: f32,
    pub(super) low_health: bool,
    pub(super) low_ammo: bool,
    pub(super) pulse: f32,
    pub(super) team_side: u8,
    pub(super) team_len: usize,
    pub(super) kill_len: usize,
    pub(super) kill_alpha: f32,
    pub(super) crosshair_alpha: f32,
    pub(super) crosshair_teammate: bool,
    pub(super) lagometer: &'a jkr_client::LagometerSamples,
}

pub(super) fn emit(
    draw_list: &mut DrawList,
    theme: Theme,
    widget: &HudWidget,
    rect: Rect,
    context: &EmitContext<'_>,
    output: &mut HudLayout,
) {
    let binding = widget.binding.as_deref();
    let mut foreground = widget.style.foreground.unwrap_or(theme.foreground);
    if (matches!(binding, Some("health_value" | "health_ratio")) && context.low_health)
        || (matches!(binding, Some("ammo_value")) && context.low_ammo)
    {
        foreground = theme.critical;
        foreground.a *= context.pulse;
    }
    if binding == Some("weapon_value") {
        foreground.a *= context.data.weapon_alpha;
    }
    let radius = widget.style.radius.unwrap_or(theme.radii.sm);
    match widget.kind {
        HudWidgetKind::Panel => emit_panel(draw_list, theme, widget, rect, radius),
        HudWidgetKind::Text if binding == Some("crosshair_name") => {
            emit_crosshair_name(draw_list, theme, widget, rect, context)
        }
        HudWidgetKind::Text => {
            let Some(text) = text_id(binding) else { return };
            let align = if matches!(
                binding,
                Some(
                    "weapon_text"
                        | "ammo_text"
                        | "weapon_value"
                        | "ammo_value"
                        | "match_timer"
                        | "warmup_text"
                        | "connection_interrupted"
                )
            ) {
                TextAlign::Center
            } else if binding == Some("style_value") {
                TextAlign::End
            } else {
                TextAlign::Start
            };
            let _ = draw_list.push(DrawCommand::Text {
                rect,
                text,
                size: theme.typography.value
                    * widget.style.type_scale.unwrap_or(1.0)
                    * context.dpi_scale,
                color: foreground,
                align,
                overflow: TextOverflow::Ellipsis,
                weight: widget.style.weight.unwrap_or(FontWeight::Regular),
                letter_spacing: widget.style.letter_spacing.unwrap_or(0.0),
            });
        }
        HudWidgetKind::Meter => {
            let ratio = context
                .data
                .number(binding.unwrap_or_default())
                .unwrap_or(0.0);
            let _ = draw_list.push(DrawCommand::RoundedRect {
                rect,
                radius,
                color: widget
                    .style
                    .background
                    .unwrap_or(Color::new(1.0, 1.0, 1.0, 0.20)),
            });
            let _ = draw_list.push(DrawCommand::RoundedRect {
                rect: Rect::new(
                    rect.x,
                    rect.y,
                    rect.width * ratio.clamp(0.0, 1.0),
                    rect.height,
                ),
                radius,
                color: foreground,
            });
            let packed = [rect.x, rect.y, rect.right(), rect.bottom()];
            match binding {
                Some("health_ratio") => output.health_bar = packed,
                Some("armor_ratio") => output.armor_bar = packed,
                Some("force_ratio") => output.force_bar = packed,
                _ => {}
            }
        }
        HudWidgetKind::Repeater if binding == Some("vote_panel") => {
            super::vote::emit(draw_list, theme, widget, rect, context)
        }
        HudWidgetKind::Repeater if binding == Some("kill_feed") => {
            emit_kill_feed(draw_list, theme, widget, rect, context)
        }
        HudWidgetKind::Repeater if binding == Some("lagometer") => {
            let rect = context.targeting.lagometer_rect(rect, context.viewport);
            emit_lagometer(draw_list, theme, widget, rect, context)
        }
        HudWidgetKind::Repeater => emit_team(draw_list, theme, widget, rect, context),
        HudWidgetKind::Image => {
            let _ = draw_list.push(DrawCommand::TexturedQuad {
                rect,
                texture: TextureId(0),
                color: foreground,
            });
        }
    }
}

/// Line height of the crosshair name at the HUD's 1080-line reference, in
/// physical pixels. `CG_DrawCrosshairNames` (`cg_draw.c`) draws the name with
/// `CG_DrawProportionalString`: `FONT_MEDIUM` (`ergoec`, point size 20) at
/// scale 1.0 in the 640x480 virtual screen, so 20 * 1080 / 480 = 45 px at 1080p.
const CROSSHAIR_NAME_LINE: f32 = 20.0 * 1_080.0 / 480.0;

/// Crosshair name line height for a layout's `type_scale` and the HUD's
/// resolution and `cg_hudScale` factor, so it scales like the other HUD text.
fn crosshair_name_size(type_scale: f32, dpi_scale: f32) -> f32 {
    CROSSHAIR_NAME_LINE * type_scale * dpi_scale
}

fn emit_crosshair_name(
    draw_list: &mut DrawList,
    theme: Theme,
    widget: &HudWidget,
    rect: Rect,
    context: &EmitContext<'_>,
) {
    let mut color = if !context.family.name_colors {
        theme.foreground
    } else if context.crosshair_teammate {
        Color::new(0.38, 1.0, 0.58, 1.0)
    } else {
        Color::new(1.0, 0.38, 0.34, 1.0)
    };
    color.a = context.crosshair_alpha * context.family.name_alpha;
    let _ = draw_list.push(DrawCommand::Text {
        rect,
        text: TextId(310),
        size: crosshair_name_size(widget.style.type_scale.unwrap_or(1.0), context.dpi_scale),
        color,
        align: TextAlign::Center,
        overflow: TextOverflow::Ellipsis,
        weight: FontWeight::Semibold,
        letter_spacing: 0.0,
    });
}

/// One fading obituary line, no backplate: the console keeps the history.
fn emit_kill_feed(
    draw_list: &mut DrawList,
    theme: Theme,
    widget: &HudWidget,
    rect: Rect,
    context: &EmitContext<'_>,
) {
    if context.kill_len == 0 {
        return;
    }
    let mut color = widget.style.foreground.unwrap_or(theme.foreground);
    color.a *= context.kill_alpha;
    let (x, align) = match context.family.kill_align {
        1 => (24.0 * context.dpi_scale, TextAlign::Start),
        2 => ((context.viewport[0] - rect.width) * 0.5, TextAlign::Center),
        _ => (
            context.viewport[0] - rect.width - 24.0 * context.dpi_scale,
            TextAlign::End,
        ),
    };
    let mut text_rect = Rect::new(
        x + context.family.kill_geometry[0] * context.viewport[0] / 640.0
            * if context.family.kill_align == 0 {
                -1.0
            } else {
                1.0
            },
        rect.y + context.family.kill_geometry[1] * context.viewport[1] / 480.0,
        rect.width,
        rect.height * context.family.kill_geometry[2],
    );
    context.icons.feed(
        draw_list,
        &mut text_rect,
        context.kill_alpha,
        (context.viewport[1] / 480.0).clamp(0.6, 5.0),
    );
    let _ = draw_list.push(DrawCommand::Text {
        rect: text_rect,
        text: TextId(300),
        size: theme.typography.body
            * widget.style.type_scale.unwrap_or(1.0)
            * context.dpi_scale
            * context.family.kill_geometry[2],
        color,
        align,
        overflow: TextOverflow::Ellipsis,
        weight: FontWeight::Semibold,
        letter_spacing: 0.0,
    });
}

fn emit_lagometer(
    draw_list: &mut DrawList,
    theme: Theme,
    widget: &HudWidget,
    rect: Rect,
    context: &EmitContext<'_>,
) {
    emit_panel(
        draw_list,
        theme,
        widget,
        rect,
        widget.style.radius.unwrap_or(theme.radii.sm),
    );
    let width = rect.width.floor().clamp(1.0, 64.0) as usize;
    let bar_width = rect.width / width as f32;
    let frame_range = rect.height / 3.0;
    let middle = rect.y + frame_range;
    let ping_range = rect.height / 2.0;
    for offset in 0..width {
        let x = rect.right() - (offset + 1) as f32 * bar_width;
        let frame = context.lagometer.frame_newest(offset) as f32;
        let height = (frame.abs() * frame_range / 300.0).min(frame_range);
        if height > 0.0 {
            let (y, color) = if frame > 0.0 {
                (middle - height, Color::new(1.0, 0.82, 0.20, 0.9))
            } else {
                (middle, Color::new(0.25, 0.55, 1.0, 0.9))
            };
            let _ = draw_list.push(DrawCommand::SolidRect {
                rect: Rect::new(x, y, bar_width.max(1.0), height),
                color,
            });
        }
        let sample = context.lagometer.snapshot_newest(offset);
        let (height, color) = if sample.ping < 0 {
            (ping_range, Color::new(1.0, 0.22, 0.18, 0.9))
        } else {
            let color = if sample.flags & 1 != 0 {
                Color::new(1.0, 0.82, 0.20, 0.9)
            } else {
                Color::new(0.28, 0.95, 0.48, 0.9)
            };
            (
                (sample.ping as f32 * ping_range / 900.0).min(ping_range),
                color,
            )
        };
        if height > 0.0 {
            let _ = draw_list.push(DrawCommand::SolidRect {
                rect: Rect::new(x, rect.bottom() - height, bar_width.max(1.0), height),
                color,
            });
        }
    }
}

fn emit_panel(draw_list: &mut DrawList, theme: Theme, widget: &HudWidget, rect: Rect, radius: f32) {
    let start = widget.style.background.unwrap_or(theme.surface);
    let _ = draw_list.push(match widget.style.background_end {
        Some(end) => DrawCommand::GradientRect {
            rect,
            radius,
            gradient: Gradient {
                start,
                end,
                vertical: true,
            },
        },
        None => DrawCommand::RoundedRect {
            rect,
            radius,
            color: start,
        },
    });
    if let Some(color) = widget.style.border {
        let _ = draw_list.push(DrawCommand::Border {
            rect,
            radius,
            width: widget.style.border_width.unwrap_or(1.0),
            color,
        });
    }
}

fn emit_team(
    draw_list: &mut DrawList,
    theme: Theme,
    _widget: &HudWidget,
    rect: Rect,
    context: &EmitContext<'_>,
) {
    let s = context.hero_scale * context.family.team[2];
    let rect = Rect::new(rect.right() - 640.0 * s, rect.y, 640.0 * s, 580.0 * s);
    let _ = draw_list.push(DrawCommand::Text {
        rect: rect.inset(Insets::all(theme.spacing.sm)),
        text: TextId(100),
        size: 26.0 * s,
        color: theme.muted,
        align: TextAlign::Start,
        overflow: TextOverflow::Clip,
        weight: FontWeight::Semibold,
        letter_spacing: 1.2,
    });
    let accent = if context.team_side == 1 {
        Color::new(0.98, 0.25, 0.20, 1.0)
    } else {
        Color::new(0.20, 0.55, 1.0, 1.0)
    };
    for row in 0..context.team_len {
        let row_y = rect.y + 44.0 * s + row as f32 * 66.0 * s;
        let _ = draw_list.push(DrawCommand::RoundedRect {
            rect: Rect::new(rect.x + 8.0 * s, row_y, 3.0 * s, 56.0 * s),
            radius: 1.5,
            color: accent,
        });
        emit_team_text(
            draw_list,
            Rect::new(rect.x + 20.0 * s, row_y, rect.width * 0.65, 28.0 * s),
            TextId(101 + row as u32),
            26.0 * s,
            theme.foreground,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        emit_team_text(
            draw_list,
            Rect::new(
                rect.x + 20.0 * s,
                row_y + 28.0 * s,
                rect.width - 28.0 * s,
                18.0 * s,
            ),
            TextId(111 + row as u32),
            18.0 * s,
            theme.muted,
            FontWeight::Regular,
            TextAlign::Start,
        );
        emit_team_text(
            draw_list,
            Rect::new(
                rect.x + 20.0 * s,
                row_y + 46.0 * s,
                rect.width - 28.0 * s,
                18.0 * s,
            ),
            TextId(131 + row as u32),
            18.0 * s,
            theme.muted,
            FontWeight::Regular,
            TextAlign::Start,
        );
        emit_team_text(
            draw_list,
            Rect::new(rect.right() - 180.0 * s, row_y, 172.0 * s, 24.0 * s),
            TextId(121 + row as u32),
            24.0 * s,
            theme.muted,
            FontWeight::Semibold,
            TextAlign::End,
        );
    }
}

fn emit_team_text(
    draw_list: &mut DrawList,
    rect: Rect,
    text: TextId,
    size: f32,
    color: Color,
    weight: FontWeight,
    align: TextAlign,
) {
    let _ = draw_list.push(DrawCommand::Text {
        rect,
        text,
        size,
        color,
        align,
        overflow: TextOverflow::Ellipsis,
        weight,
        letter_spacing: 0.0,
    });
}

#[cfg(test)]
mod tests {
    use super::crosshair_name_size;

    /// The HUD's resolution factor before `cg_hudScale`, as `HudOverlay::layout`.
    fn dpi(height: f32) -> f32 {
        (height / 1_080.0).clamp(2.0 / 3.0, 4.0 / 3.0)
    }

    #[test]
    fn crosshair_name_matches_stock_virtual_screen_size() {
        // Stock: ergoec point size 20 in the 480-line virtual screen.
        for height in [720.0_f32, 1_080.0, 1_440.0] {
            let stock = 20.0 * height / 480.0;
            assert!((crosshair_name_size(1.0, dpi(height)) - stock).abs() < 1e-3);
        }
    }

    #[test]
    fn crosshair_name_follows_layout_and_hud_scale() {
        assert!((crosshair_name_size(0.9, 1.0) - 40.5).abs() < 1e-3);
        assert!((crosshair_name_size(1.0, 1.0 * 1.5) - 67.5).abs() < 1e-3);
    }
}
