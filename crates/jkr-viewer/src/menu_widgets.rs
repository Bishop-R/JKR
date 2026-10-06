//! Shared modern menu widgets built on renderer-neutral `jkr-ui` primitives.

mod contrast;
mod hero;
mod input;
mod layout;
mod text;
mod vote;
pub(crate) use contrast::MenuContrast;
pub(crate) use hero::{HeroColumn, Scrim};
pub(crate) use vote::VoteLayout;
mod controls;
mod form;
pub(crate) mod numeric;

pub(crate) use form::{BACK_TOKEN, FormLayout, TAB_BASE, cycler_direction, palette_index};

use jkr_ui::{
    Color, DrawCommand, DrawList, FontWeight, InputRouter, Rect, TextAlign, Theme, WidgetId,
    WidgetTree,
};

const MAX_TEXT: usize = 160;
const MAX_WIDGETS: usize = 96;
const MAX_DRAW: usize = 512;

/// Visual state shared by ordinary, team-accented, and disabled buttons.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ButtonStyle<'a> {
    pub(crate) selected: bool,
    pub(crate) enabled: bool,
    pub(crate) accent: Option<Color>,
    pub(crate) badge: Option<&'a str>,
}

impl ButtonStyle<'_> {
    pub(crate) const fn plain(selected: bool) -> Self {
        Self {
            selected,
            enabled: true,
            accent: None,
            badge: None,
        }
    }
}

/// Exact centered-card geometry. `content_height` excludes the two padding bands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ContentCard {
    pub(crate) panel: Rect,
    pub(crate) content: Rect,
    pub(crate) padding: f32,
}

/// Semantic action attached to one focusable screen widget.
pub(crate) type MenuToken = u16;

/// Reusable fixed-storage screen canvas and input router.
pub(crate) struct MenuCanvas {
    theme: Theme,
    draw: DrawList,
    tree: WidgetTree,
    rects: Vec<Rect>,
    tokens: Vec<MenuToken>,
    text: [String; MAX_TEXT],
    text_len: usize,
    input: InputRouter,
    hovered_token: Option<MenuToken>,
    pressed_token: Option<MenuToken>,
    viewport: [f32; 2],
    contrast: MenuContrast,
    /// Luminance behind text once this frame has drawn a readability backing.
    backing: Option<f32>,
}

impl MenuCanvas {
    /// Create process-lifetime storage for one menu layer.
    pub(crate) fn new() -> Self {
        Self::with_text_capacity(128)
    }

    /// Reserve screen-specific text storage before entering the frame loop.
    pub(crate) fn with_text_capacity(text_bytes: usize) -> Self {
        Self {
            theme: Theme::default(),
            draw: DrawList::new(MAX_DRAW),
            tree: WidgetTree::new(MAX_WIDGETS),
            rects: Vec::with_capacity(MAX_WIDGETS),
            tokens: Vec::with_capacity(MAX_WIDGETS),
            text: std::array::from_fn(|_| String::with_capacity(text_bytes)),
            text_len: 0,
            input: InputRouter::new(MAX_WIDGETS),
            hovered_token: None,
            pressed_token: None,
            viewport: [1.0, 1.0],
            contrast: MenuContrast::Off,
            backing: None,
        }
    }

    /// Where along a slider row's rail the pointer at `x` is, at the form
    /// scale of the viewport this canvas was last begun with.
    pub(crate) fn slider_ratio(&self, rect: Rect, x: f32) -> f32 {
        form::slider_ratio(rect, x, FormLayout::new(self.viewport).scale)
    }

    /// Reset retained scratch without drawing a full-screen background.
    pub(crate) fn begin_transparent(&mut self, viewport: [f32; 2]) {
        self.hovered_token = self
            .input
            .hovered()
            .and_then(|id| self.tokens.get(id.0 as usize).copied());
        self.pressed_token = self
            .input
            .pressed()
            .and_then(|id| self.tokens.get(id.0 as usize).copied());
        self.viewport = viewport;
        self.contrast = MenuContrast::current();
        self.backing = None;
        self.draw.clear();
        self.tree.clear();
        self.rects.clear();
        self.tokens.clear();
        self.text_len = 0;
    }

    /// Draw a modern backplate with the same contrast treatment as the HUD,
    /// darkened to the `ui_menuContrast` floor when that is higher.
    pub(crate) fn panel(&mut self, rect: Rect) {
        let alpha = self.readability_coverage().max(0.55);
        let _ = self.draw.push(DrawCommand::RoundedRect {
            rect,
            radius: self.theme.radii.lg,
            color: Color::new(0.0, 0.0, 0.0, alpha),
        });
        self.mark_backing(self.readability_coverage());
        let _ = self.draw.push(DrawCommand::Border {
            rect,
            radius: self.theme.radii.lg,
            width: 1.0,
            color: Color::new(1.0, 1.0, 1.0, 0.10),
        });
    }

    /// Draw a recessed text field with an optional active-focus outline.
    pub(crate) fn text_field(&mut self, rect: Rect, active: bool) {
        let _ = self.draw.push(DrawCommand::RoundedRect {
            rect,
            radius: self.theme.radii.md,
            color: Color::new(0.015, 0.028, 0.043, 0.92),
        });
        let outline = if active {
            self.theme.accent
        } else {
            Color::new(1.0, 1.0, 1.0, 0.12)
        };
        let _ = self.draw.push(DrawCommand::Border {
            rect,
            radius: self.theme.radii.md,
            width: if active { 2.0 } else { 1.0 },
            color: outline,
        });
    }

    /// Draw a one-pixel visual divider.
    pub(crate) fn separator(&mut self, rect: Rect) {
        let _ = self.draw.push(DrawCommand::SolidRect {
            rect,
            color: Color::new(1.0, 1.0, 1.0, 0.09),
        });
    }

    /// Draw a draggable scrollbar and register its entire track for pointer input.
    pub(crate) fn scrollbar(
        &mut self,
        token: MenuToken,
        track: Rect,
        first: usize,
        visible: usize,
        total: usize,
    ) {
        let _ = self.draw.push(DrawCommand::RoundedRect {
            rect: track,
            radius: track.width * 0.5,
            color: Color::new(1.0, 1.0, 1.0, 0.08),
        });
        let fraction = (visible as f32 / total.max(visible) as f32).clamp(0.08, 1.0);
        let thumb_height = track.height * fraction;
        let travel = track.height - thumb_height;
        let denominator = total.saturating_sub(visible).max(1) as f32;
        let thumb_y = track.y + travel * (first as f32 / denominator).clamp(0.0, 1.0);
        let active = self.token_hovered(token) || self.token_pressed(token);
        let _ = self.draw.push(DrawCommand::RoundedRect {
            rect: Rect::new(track.x, thumb_y, track.width, thumb_height),
            radius: track.width * 0.5,
            color: if active {
                self.theme.accent
            } else {
                Color::new(0.42, 0.56, 0.66, 0.72)
            },
        });
        self.interactive(token, track, false, true);
    }

    /// Draw a semantic accent bar supplied by the calling screen.
    pub(crate) fn accent_bar(&mut self, rect: Rect, color: Color) {
        let _ = self.draw.push(DrawCommand::SolidRect { rect, color });
    }

    /// Add a focusable button and return its physical rectangle.
    pub(crate) fn button(
        &mut self,
        token: MenuToken,
        label: &str,
        rect: Rect,
        selected: bool,
    ) -> Rect {
        self.button_styled(token, label, rect, ButtonStyle::plain(selected))
    }

    /// Add a button with optional accent, status badge, and disabled semantics.
    pub(crate) fn button_styled(
        &mut self,
        token: MenuToken,
        label: &str,
        rect: Rect,
        style: ButtonStyle<'_>,
    ) -> Rect {
        let pressed = self.token_pressed(token);
        let hovered = self.token_hovered(token);
        let color = if !style.enabled {
            Color::new(0.025, 0.038, 0.052, 0.54)
        } else if pressed {
            Color::new(0.018, 0.15, 0.22, 1.0)
        } else if style.selected || hovered {
            Color::new(0.035, 0.24, 0.34, 0.96)
        } else {
            Color::new(0.035, 0.055, 0.075, 0.74)
        };
        let _ = self.draw.push(DrawCommand::RoundedRect {
            rect,
            radius: self.theme.radii.md,
            color,
        });
        if let Some(accent) = style.accent {
            let _ = self.draw.push(DrawCommand::SolidRect {
                rect: Rect::new(rect.x, rect.y + 8.0, 3.0, rect.height - 16.0),
                color: accent,
            });
        }
        if (style.selected || hovered || pressed) && style.enabled {
            let _ = self.draw.push(DrawCommand::SolidRect {
                rect: Rect::new(rect.x, rect.y + 8.0, 3.0, rect.height - 16.0),
                color: style.accent.unwrap_or(self.theme.accent),
            });
            let _ = self.draw.push(DrawCommand::Border {
                rect,
                radius: self.theme.radii.md,
                width: 2.0,
                color: Color::new(
                    self.theme.accent.r,
                    self.theme.accent.g,
                    self.theme.accent.b,
                    0.72,
                ),
            });
        }
        let foreground = if style.enabled {
            self.theme.foreground
        } else {
            Color::new(0.48, 0.54, 0.60, 0.82)
        };
        let badge_width = if style.badge.is_some() { 84.0 } else { 0.0 };
        self.text(
            label,
            Rect::new(
                rect.x + 20.0,
                rect.y + 1.0,
                rect.width - 40.0 - badge_width,
                rect.height - 2.0,
            ),
            17.0,
            foreground,
            if style.selected {
                FontWeight::Semibold
            } else {
                FontWeight::Regular
            },
            0.2,
        );
        if let Some(badge) = style.badge {
            let badge_rect =
                Rect::new(rect.right() - 86.0, rect.y + 11.0, 68.0, rect.height - 22.0);
            let _ = self.draw.push(DrawCommand::RoundedRect {
                rect: badge_rect,
                radius: self.theme.radii.sm,
                color: Color::new(1.0, 1.0, 1.0, 0.08),
            });
            self.text_aligned(
                badge,
                badge_rect,
                10.0,
                foreground,
                FontWeight::Semibold,
                1.2,
                TextAlign::Center,
            );
        }
        if style.enabled {
            self.interactive(token, rect, true, false);
        }
        rect
    }

    /// Finish focus ordering and restore semantic selection.
    pub(crate) fn finish(&mut self, selected_token: MenuToken) {
        self.input.begin_frame(&self.tree);
        if let Some(index) = self
            .tokens
            .iter()
            .position(|token| *token == selected_token)
        {
            let _ = self.input.focus(WidgetId(index as u32));
        }
    }

    pub(crate) fn draw_list(&self) -> &DrawList {
        &self.draw
    }

    /// The frame's command list, for screens that push commands the widget
    /// vocabulary has no word for (textured tiles, say).
    pub(crate) fn draw_list_mut(&mut self) -> &mut DrawList {
        &mut self.draw
    }

    /// Multiply the opacity of everything drawn until [`Self::pop_opacity`].
    pub(crate) fn push_opacity(&mut self, opacity: f32) {
        let _ = self.draw.push(DrawCommand::PushOpacity(opacity));
    }

    pub(crate) fn pop_opacity(&mut self) {
        let _ = self.draw.push(DrawCommand::PopOpacity);
    }

    pub(crate) fn theme(&self) -> Theme {
        self.theme
    }

    /// Override the theme accent (the player's `ui_accent`).
    pub(crate) fn set_accent(&mut self, accent: Color) {
        self.theme.accent = accent;
    }
}
