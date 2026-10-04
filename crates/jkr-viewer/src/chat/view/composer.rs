//! Unboxed composer, channel choices, and a compact player-action popover.

use super::*;
use crate::chat::interaction::{GLOBAL, LATEST, MUTE, TEAM, WHISPER};

impl ChatOverlay {
    pub(in crate::chat) fn build_composer(&mut self, font: &UiFont, g: &Geometry, ms: u64) {
        let Some(input) = &self.input else {
            return;
        };
        let alpha = Tween::new(0.0, 1.0, self.opened_ms, ENTER_MS, Easing::EaseOutCubic).sample(ms);
        let y = g.bottom + 18.0 * g.scale;
        self.ui.push_opacity(alpha);
        for (token, label, channel, offset) in [
            (GLOBAL, "All", Channel::Global, 0.0),
            (TEAM, "Team", Channel::Team, 58.0),
        ] {
            let rect = Rect::new(g.left + offset * g.scale, y, 50.0 * g.scale, 26.0 * g.scale);
            let selected = input.channel == channel;
            self.ui.hit_region(token, rect);
            self.ui.text(
                label,
                rect,
                14.0 * g.scale,
                if selected || self.ui.token_hovered(token) {
                    tint(channel, 1.0)
                } else {
                    Color::new(0.58, 0.65, 0.72, 1.0)
                },
                FontWeight::Semibold,
                0.0,
            );
            if selected {
                self.ui.accent_bar(
                    Rect::new(rect.x, rect.bottom(), 20.0 * g.scale, 2.0 * g.scale),
                    tint(channel, 1.0),
                );
            }
        }
        if input.channel == Channel::Whisper {
            let name = input
                .recipient
                .and_then(|target| self.roster.name(target))
                .unwrap_or("Player unavailable");
            let end = layout::fitting_end(name, font, g.width - 245.0 * g.scale, 14.0 * g.scale);
            self.ui.text_fmt_aligned(
                format_args!("To {}", &name[..end]),
                Rect::new(
                    g.left + 120.0 * g.scale,
                    y,
                    g.width - 240.0 * g.scale,
                    26.0 * g.scale,
                ),
                14.0 * g.scale,
                tint(Channel::Whisper, 1.0),
                FontWeight::Semibold,
                0.0,
                TextAlign::Start,
            );
        }
        if self.scroll > 0 {
            let rect = Rect::new(
                g.left + g.width - 116.0 * g.scale,
                y,
                116.0 * g.scale,
                26.0 * g.scale,
            );
            self.ui.hit_region(LATEST, rect);
            self.ui.text_fmt_aligned(
                format_args!("Latest +{}", self.unread),
                rect,
                12.0 * g.scale,
                tint(Channel::Global, 1.0),
                FontWeight::Semibold,
                0.0,
                TextAlign::End,
            );
        }
        let input_y = y + 42.0 * g.scale;
        // Horizontally scroll the draft around its caret, without allocating.
        let size = 21.0 * g.scale;
        let width = g.width - 20.0 * g.scale;
        let mut start = input.cursor;
        let mut used = 0.0;
        for (index, c) in input.text[..input.cursor].char_indices().rev() {
            used += visible_text_width_face(
                font,
                &input.text[index..index + c.len_utf8()],
                size / font.height,
                TextFace::Regular,
            );
            if used > width {
                break;
            }
            start = index;
        }
        let end = start + layout::fitting_end(&input.text[start..], font, width, size);
        let draft = &input.text[start..end];
        self.ui.text(
            if input.text.is_empty() {
                "Say something..."
            } else {
                draft
            },
            Rect::new(g.left, input_y, g.width, 34.0 * g.scale),
            size,
            if input.text.is_empty() {
                Color::new(0.62, 0.68, 0.74, 0.8)
            } else {
                self.ui.theme().foreground
            },
            FontWeight::Regular,
            0.0,
        );
        let caret = visible_text_width_face(
            font,
            &input.text[start..input.cursor],
            size / font.height,
            TextFace::Regular,
        );
        let blink = Tween::pulse(0.35, 1.0, ms.saturating_sub(self.opened_ms), 550);
        self.ui.accent_bar(
            Rect::new(
                g.left + caret + 2.0 * g.scale,
                input_y + 2.0 * g.scale,
                2.0 * g.scale,
                23.0 * g.scale,
            ),
            tint(input.channel, blink),
        );
        self.ui.accent_bar(
            Rect::new(g.left, input_y + 38.0 * g.scale, 32.0 * g.scale, g.scale),
            tint(input.channel, 0.55),
        );
        // No standing key hints; only a transient notice
        // such as a failed whisper target appears under the draft.
        if !self.notice.is_empty() {
            self.ui.text(
                self.notice,
                Rect::new(g.left, input_y + 52.0 * g.scale, g.width, 20.0 * g.scale),
                11.0 * g.scale,
                Color::new(0.66, 0.73, 0.79, 0.85),
                FontWeight::Regular,
                0.0,
            );
        }
        self.ui.pop_opacity();
    }

    pub(in crate::chat) fn build_player_menu(
        &mut self,
        g: &Geometry,
        viewport: [f32; 2],
        font: &UiFont,
    ) {
        let Some(menu) = self.player_menu else {
            return;
        };
        let Some(name) = self.roster.name(menu.target) else {
            self.player_menu = None;
            return;
        };
        let width = 240.0 * g.scale;
        let x = if self.scoreboard_layout {
            g.left
        } else {
            (g.left + g.width + 28.0 * g.scale).min(viewport[0] - width - 16.0 * g.scale)
        };
        let y = menu.origin[1].clamp(g.top, viewport[1] - 240.0 * g.scale);
        let end = layout::fitting_end(name, font, width, 16.0 * g.scale);
        self.ui.floating_scrim(
            Rect::new(
                x - 18.0 * g.scale,
                y - 32.0 * g.scale,
                width + 80.0 * g.scale,
                164.0 * g.scale,
            ),
            0.9,
        );
        self.ui.text(
            &name[..end],
            Rect::new(x, y, width, 26.0 * g.scale),
            16.0 * g.scale,
            self.ui.theme().foreground,
            FontWeight::Semibold,
            0.0,
        );
        for (token, label, row) in [
            (WHISPER, "Whisper", 0.0),
            (
                MUTE,
                if self.muted.contains(&menu.target) {
                    "Unmute player"
                } else {
                    "Mute player"
                },
                1.0,
            ),
        ] {
            let rect = Rect::new(x, y + (37.0 + row * 36.0) * g.scale, width, 32.0 * g.scale);
            self.ui.hit_region(token, rect);
            self.ui.text(
                label,
                rect,
                16.0 * g.scale,
                if self.ui.token_hovered(token) || menu.selected == token {
                    self.ui.theme().accent
                } else {
                    self.ui.theme().foreground
                },
                FontWeight::Regular,
                0.0,
            );
        }
    }
}
