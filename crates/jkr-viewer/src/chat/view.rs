//! Floating message typography and deterministic motion on the shared canvas.

mod composer;

use super::*;
use crate::text::{TextFace, visible_text_width_face};
use jkr_ui::{Color, Easing, FontWeight, Rect, TextAlign};
use layout::Geometry;

pub(super) fn tint(channel: Channel, alpha: f32) -> Color {
    match channel {
        Channel::Global => Color::new(0.70, 0.88, 0.98, alpha),
        Channel::Team => Color::new(0.40, 0.86, 0.86, alpha),
        Channel::Whisper => Color::new(0.84, 0.68, 1.0, alpha),
    }
}

pub(super) fn visibility(age: u64, active: bool) -> f32 {
    let entering = Tween::new(0.0, 1.0, 0, ENTER_MS, Easing::EaseOutCubic).sample(age);
    let leaving = if active {
        1.0
    } else {
        Tween::new(1.0, 0.0, HOLD_MS, FADE_MS as u32, Easing::SmoothStep).sample(age)
    };
    entering * leaving
}

impl ChatOverlay {
    pub(super) fn build(&mut self, draw_feed: bool, font: &UiFont, viewport: [f32; 2], ms: u64) {
        if self.layout_viewport != viewport {
            for line in &mut self.lines {
                line.y = None;
            }
            self.layout_viewport = viewport;
            self.pressed_action = None;
        }
        self.ui.begin_transparent(viewport);
        self.visible_targets.fill(None);
        let mut g = self.options.geometry(viewport);
        if self.scoreboard_layout {
            let layout = crate::scoreboard::layout::Layout::new(viewport);
            g.scale = layout.scale * self.options.font.min(1.5);
            g.left = layout.chat_left;
            g.width = layout.chat_width;
            g.top = 150.0 * layout.scale;
            g.bottom = viewport[1] - 200.0 * g.scale;
            g.font = 18.0 * g.scale;
            g.row = 27.0 * g.scale;
        }
        if (draw_feed || self.is_typing()) && self.options.lifetime != 0 {
            self.build_feed(font, &g, ms);
        }
        if let Some((text, received)) = &self.center {
            let age = ms.saturating_sub(*received);
            if age < self.options.center_time && !self.scoreboard_layout {
                // CG_DrawCenterString (cg_draw.c:4488): every `\n` row is its own
                // centred draw with the base colour, so colour codes never bleed
                // into the next row; the block sits around cg.centerPrintY (0.30).
                let base = self.ui.theme().foreground;
                let fade = self.options.center_time.saturating_sub(age).min(200) as f32 / 200.0;
                let color = Color::new(base.r, base.g, base.b, base.a * fade);
                let size = self.options.center_size;
                let center_scale = (viewport[1] / 1080.0).clamp(0.6, 2.5);
                let row = 30.0 * center_scale * size;
                let rows = center_rows(text).count() as f32;
                let height = if self.options.center_height == 0.0 {
                    self.combat.height.unwrap_or(0.0)
                } else {
                    self.options.center_height
                };
                let mut y = viewport[1] * if height == 0.0 { 0.30 } else { height / 480.0 }
                    - rows * row * 0.5;
                for line in center_rows(text) {
                    self.ui.text_aligned(
                        line,
                        Rect::new(32.0, y, viewport[0] - 64.0, row),
                        22.0 * center_scale * size,
                        color,
                        FontWeight::Semibold,
                        0.0,
                        TextAlign::Center,
                    );
                    y += row;
                }
            } else if age >= self.options.center_time {
                self.combat.spare = self.center.take().unwrap().0;
                self.combat.height = None;
            }
        }
        if !self.scoreboard_layout {
            self.draw_plums();
        }
        if self.is_typing() {
            self.build_composer(font, &g, ms);
            self.build_player_menu(&g, viewport, font);
        }
        self.ui
            .finish(self.player_menu.map_or(u16::MAX, |menu| menu.selected));
    }

    fn build_feed(&mut self, font: &UiFont, g: &Geometry, ms: u64) {
        let active = self.is_typing() || self.options.history;
        let limit = self.options.lines;
        let mut visible = [(0_usize, 0.0_f32, 0.0_f32); MAX_VISIBLE];
        let mut count = 0;
        let mut bottom = g.bottom;
        for (index, line) in self.lines.iter_mut().enumerate().rev().skip(self.scroll) {
            let age = ms.saturating_sub(line.received_ms);
            let alpha = if active {
                visibility(age, true)
            } else {
                visibility(age, true)
                    * (self.options.lifetime.saturating_sub(age).min(2000) as f32 / 2000.0)
            };
            if (!active && line.muted) || alpha <= 0.0 {
                continue;
            }
            line.wrap
                .update(&line.body, font, g.width - 12.0 * g.scale, g.font);
            let rows = if line.muted { 1 } else { line.wrap.len };
            let height =
                rows as f32 * g.row + if line.name.is_empty() { 14.0 } else { 36.0 } * g.scale;
            let top = bottom - height;
            if top < g.top || count == limit {
                break;
            }
            visible[count] = (index, top, alpha);
            count += 1;
            bottom = top;
        }
        for (token, (index, top, alpha)) in visible[..count].iter().copied().enumerate().rev() {
            let line = &mut self.lines[index];
            let age = ms.saturating_sub(line.received_ms);
            let y = line.y.get_or_insert_with(|| Tween::settled(top));
            if (y.target() - top).abs() > 0.1 {
                y.retarget(top, ms, ENTER_MS, Easing::EaseOutCubic);
            }
            let y = y.sample(ms);
            let slide =
                Tween::new(14.0 * g.scale, 0.0, 0, ENTER_MS, Easing::EaseOutCubic).sample(age);
            let x = g.left + slide;
            let mut body_y = y;
            if !line.name.is_empty() {
                let end = layout::fitting_end(&line.name, font, g.width * 0.65, 16.0 * g.scale);
                let name = &line.name[..end];
                let width = visible_text_width_face(
                    font,
                    name,
                    16.0 * g.scale / font.height,
                    TextFace::Semibold,
                );
                let rect = Rect::new(x, y, width + 12.0 * g.scale, 24.0 * g.scale);
                let actionable = line
                    .sender
                    .filter(|target| self.roster.name(*target).is_some());
                if active && let Some(target) = actionable {
                    self.visible_targets[token] = Some(target);
                    self.ui.hit_region(token as u16, rect);
                }
                let hover = active && self.ui.token_hovered(token as u16);
                self.ui.text(
                    name,
                    rect,
                    16.0 * g.scale,
                    if hover {
                        Color::new(1.0, 1.0, 1.0, alpha)
                    } else {
                        tint(line.channel, alpha)
                    },
                    FontWeight::Semibold,
                    0.0,
                );
                if hover {
                    self.ui.accent_bar(
                        Rect::new(x, y + 22.0 * g.scale, width, g.scale),
                        tint(line.channel, alpha),
                    );
                }
                let label = if line.muted {
                    "MUTED"
                } else {
                    match line.channel {
                        Channel::Team => "TEAM",
                        Channel::Whisper => "WHISPER",
                        Channel::Global => "",
                    }
                };
                self.ui.text(
                    label,
                    Rect::new(
                        rect.right() + 6.0 * g.scale,
                        y + 4.0 * g.scale,
                        90.0 * g.scale,
                        18.0 * g.scale,
                    ),
                    10.0 * g.scale,
                    tint(line.channel, alpha * 0.75),
                    FontWeight::Semibold,
                    1.0 * g.scale,
                );
                body_y += 26.0 * g.scale;
            }
            if line.muted {
                self.ui.text(
                    "Messages hidden on this client",
                    Rect::new(x, body_y, g.width, g.row),
                    15.0 * g.scale,
                    Color::new(0.64, 0.70, 0.76, alpha * 0.7),
                    FontWeight::Regular,
                    0.0,
                );
            } else {
                for (row, range) in line.wrap.rows[..line.wrap.len].iter().enumerate() {
                    let color = Color::new(0.96, 0.97, 0.99, alpha);
                    let truncated = row + 1 == line.wrap.len && range.end < line.body.len();
                    self.ui.text_fmt_aligned(
                        format_args!(
                            "{}{}",
                            &line.body[range.clone()],
                            if truncated { "..." } else { "" }
                        ),
                        Rect::new(x, body_y + row as f32 * g.row, g.width, g.row),
                        g.font,
                        color,
                        FontWeight::Regular,
                        0.0,
                        TextAlign::Start,
                    );
                }
            }
        }
    }
}

/// CG_DrawCenterString wraps rows longer than this at whitespace ([BugFix19]).
const CENTER_WRAP_BYTES: usize = 50;

/// Rows of a centre print: explicit `\n` breaks, then whitespace wrapping of
/// over-long rows, without allocating.
pub(super) fn center_rows(text: &str) -> impl Iterator<Item = &str> {
    text.trim_end_matches('\n')
        .split('\n')
        .flat_map(|row| WrapRow { rest: row })
}

struct WrapRow<'a> {
    rest: &'a str,
}

impl<'a> Iterator for WrapRow<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        if self.rest.is_empty() {
            return None;
        }
        let mut cut = self.rest.len();
        if cut > CENTER_WRAP_BYTES {
            cut = self.rest[..CENTER_WRAP_BYTES]
                .rfind(char::is_whitespace)
                .map_or(CENTER_WRAP_BYTES, |index| index + 1);
            while !self.rest.is_char_boundary(cut) {
                cut -= 1;
            }
        }
        let (head, tail) = self.rest.split_at(cut);
        self.rest = tail.trim_start();
        Some(head.trim_end())
    }
}
