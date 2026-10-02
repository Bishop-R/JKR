//! Full-bleed "hero" widgets for screens drawn over the live map: a
//! readability scrim, box-free list entries, and keyboard key caps.

use super::contrast::{self, FadeSegment};
use super::{FormLayout, MenuCanvas};
use jkr_ui::{Color, DrawCommand, FontWeight, Gradient, Rect, TextAlign};

const INK: [f32; 3] = [0.005, 0.010, 0.018];

/// Width, as a fraction of the viewport, over which a readability-held scrim
/// eases back into its original fade past the text column.
const BACKING_FEATHER: f32 = 0.12;

/// Right edge of the widest hero text column (the form layout's).
fn text_column_right(viewport: [f32; 2]) -> f32 {
    let form = FormLayout::new(viewport);
    let hero = HeroColumn::new(viewport);
    (form.margin + form.column_width).max(hero.margin + hero.column_width)
}

fn ink(alpha: f32) -> Color {
    Color::new(INK[0], INK[1], INK[2], alpha)
}

fn horizontal(rect: Rect, start: Color, end: Color) -> DrawCommand {
    DrawCommand::GradientRect {
        rect,
        radius: 0.0,
        gradient: Gradient {
            start,
            end,
            vertical: false,
        },
    }
}

fn vertical(rect: Rect, start: Color, end: Color) -> DrawCommand {
    DrawCommand::GradientRect {
        rect,
        radius: 0.0,
        gradient: Gradient {
            start,
            end,
            vertical: true,
        },
    }
}

/// Geometry of a hero screen's left column for one viewport: everything
/// scales with viewport height so 1080p, ultrawide and 4K keep the same
/// proportions.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HeroColumn {
    pub(crate) scale: f32,
    pub(crate) margin: f32,
    pub(crate) column_width: f32,
}

impl HeroColumn {
    pub(crate) fn new(viewport: [f32; 2]) -> Self {
        let scale = (viewport[1] / 1_080.0).clamp(0.6, 2.5);
        let margin = (viewport[0] * 0.075).max(72.0 * scale);
        Self {
            scale,
            margin,
            column_width: (viewport[0] * 0.42).clamp(360.0 * scale, 560.0 * scale),
        }
    }

    /// Pointer target of list row `row` when the list starts at `top` with
    /// `row_height`-tall entries.
    pub(crate) fn row_rect(&self, top: f32, row: usize, row_height: f32) -> Rect {
        Rect::new(
            self.margin,
            top + row as f32 * row_height,
            self.column_width,
            row_height,
        )
    }
}

/// How much of the live world a hero screen darkens.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Scrim {
    /// Light full-frame tint, a heavy fade on the left column and top/bottom
    /// bands: the main menu, where the world is scenery.
    Full,
    /// Only the left column fades, and it stops at the middle of the frame:
    /// screens that show something in the world (the player, the sabers)
    /// keep it at its real brightness.
    Column,
    /// The left fade reaches across the whole frame: data-dense screens
    /// (the server browser) whose columns run past the middle.
    Wide,
}

impl MenuCanvas {
    /// Scale the existing translucent column, without changing its geometry.
    pub(crate) fn column_tint_opacity(&mut self, rect: Rect, opacity: f32) {
        let _ = self.draw.push(horizontal(
            Rect::new(rect.x, rect.y, rect.width * 0.54, rect.height),
            ink(0.95 * opacity.clamp(0.0, 1.0)),
            ink(0.0),
        ));
    }

    /// Feathered readability behind floating text: no border or hard panel edge.
    pub(crate) fn floating_scrim(&mut self, rect: Rect, opacity: f32) {
        const BANDS: usize = 32;
        let band = rect.height / BANDS as f32;
        for i in 0..BANDS {
            let t = (i as f32 + 0.5) / BANDS as f32;
            let edge = (t * 6.0).min((1.0 - t) * 6.0).min(1.0);
            let alpha = opacity * edge * edge * (3.0 - 2.0 * edge);
            let _ = self.draw.push(horizontal(
                Rect::new(rect.x, rect.y + i as f32 * band, rect.width, band),
                ink(alpha),
                ink(0.0),
            ));
        }
    }

    /// Begin a screen over the live world with `scrim` so the chrome stays
    /// readable over any map lighting. Everything up to [`Self::end_hero`]
    /// is drawn at `opacity`, which lets a screen fade in as the backdrop
    /// camera arrives on its shot.
    pub(crate) fn begin_hero(&mut self, viewport: [f32; 2], opacity: f32, scrim: Scrim) {
        self.begin_transparent(viewport);
        self.push_opacity(opacity);
        let [width, height] = viewport;
        let column = text_column_right(viewport);
        if scrim == Scrim::Column {
            self.held_fade(viewport, width * 0.54, 0.95, 0.0, 0.0, column);
            return;
        }
        let _ = self.draw.push(DrawCommand::SolidRect {
            rect: Rect::new(0.0, 0.0, width, height),
            color: ink(0.30),
        });
        if scrim == Scrim::Wide {
            self.held_fade(viewport, width, 0.94, 0.50, 0.30, width);
        } else {
            self.held_fade(viewport, width * 0.64, 0.94, 0.0, 0.30, column);
        }
        let _ = self.draw.push(vertical(
            Rect::new(0.0, 0.0, width, height * 0.18),
            ink(0.62),
            ink(0.0),
        ));
        let _ = self.draw.push(vertical(
            Rect::new(0.0, height * 0.70, width, height * 0.30),
            ink(0.0),
            ink(0.84),
        ));
    }

    /// The column scrim of [`Scrim::Column`] for screens drawn over the live
    /// match without one (the in-game menu), only while `ui_menuContrast`
    /// is on; with it off the match stays untinted.
    pub(crate) fn readability_column(&mut self, viewport: [f32; 2]) {
        if self.readability_coverage() > 0.0 {
            let column = text_column_right(viewport);
            self.held_fade(viewport, viewport[0] * 0.54, 0.95, 0.0, 0.0, column);
        }
    }

    /// Rounded backing at the `ui_menuContrast` floor behind text that sits
    /// outside the text column; nothing while the setting is off.
    pub(crate) fn text_backing(&mut self, rect: Rect) {
        let coverage = self.readability_coverage();
        if coverage > 0.0 {
            let _ = self.draw.push(DrawCommand::RoundedRect {
                rect,
                radius: self.theme.radii.lg,
                color: ink(coverage),
            });
        }
    }

    /// A scrim's full-height left fade from `start` to `end` over `width`,
    /// stacked on a uniform `base` tint and held at the `ui_menuContrast`
    /// floor up to `hold`, the right edge of the text it backs.
    fn held_fade(
        &mut self,
        viewport: [f32; 2],
        width: f32,
        start: f32,
        end: f32,
        base: f32,
        hold: f32,
    ) {
        let coverage = self.readability_coverage();
        let floor = contrast::layer_alpha(coverage, base);
        let feather = viewport[0] * BACKING_FEATHER;
        let (segments, count) = contrast::held_fade(width, start, end, hold, floor, feather);
        for FadeSegment { x0, x1, a0, a1 } in &segments[..count] {
            let rect = Rect::new(*x0, 0.0, x1 - x0, viewport[1]);
            let _ = self.draw.push(horizontal(rect, ink(*a0), ink(*a1)));
        }
        self.mark_backing(coverage);
    }

    /// Close the opacity group opened by [`Self::begin_hero`].
    pub(crate) fn end_hero(&mut self) {
        self.pop_opacity();
    }

    /// Box-free list entry: label, an accent sweep and a one-line hint while
    /// selected, a softer sweep while hovered. `rect` is the pointer target.
    pub(crate) fn hero_item(
        &mut self,
        token: u16,
        label: &str,
        hint: &str,
        rect: Rect,
        selected: bool,
        scale: f32,
    ) -> Rect {
        self.hero_entry(token, label, hint, rect, selected, true, scale)
    }

    /// [`Self::hero_item`] that can be disabled: a dimmed entry that keeps
    /// its pointer target but is neither focusable nor highlighted.
    pub(crate) fn hero_entry(
        &mut self,
        token: u16,
        label: &str,
        hint: &str,
        rect: Rect,
        selected: bool,
        enabled: bool,
        scale: f32,
    ) -> Rect {
        if !enabled {
            self.text(
                label,
                Rect::new(rect.x, rect.y + 8.0 * scale, rect.width, 40.0 * scale),
                34.0 * scale,
                Color::new(0.82, 0.88, 0.94, 0.30),
                FontWeight::Regular,
                0.4 * scale,
            );
            self.interactive(token, rect, false, false);
            return rect;
        }
        let hovered = self.token_hovered(token) || self.token_pressed(token);
        if selected || hovered {
            self.accent_sweep(rect, if selected { 0.17 } else { 0.08 }, 28.0 * scale);
        }
        let foreground = if selected {
            self.theme.foreground
        } else if hovered {
            Color::new(0.94, 0.97, 1.0, 0.92)
        } else {
            Color::new(0.82, 0.88, 0.94, 0.70)
        };
        self.text(
            label,
            Rect::new(rect.x, rect.y + 8.0 * scale, rect.width, 40.0 * scale),
            34.0 * scale,
            foreground,
            if selected {
                FontWeight::Semibold
            } else {
                FontWeight::Regular
            },
            0.4 * scale,
        );
        if selected {
            self.text(
                hint,
                Rect::new(rect.x, rect.y + 48.0 * scale, rect.width, 20.0 * scale),
                15.0 * scale,
                self.theme.muted,
                FontWeight::Regular,
                0.1 * scale,
            );
        }
        self.interactive(token, rect, true, false);
        rect
    }

    /// Text-only hero entry with a narrow accent line, never a tinted backplate.
    pub(crate) fn hero_line_entry(
        &mut self,
        token: u16,
        label: &str,
        rect: Rect,
        selected: bool,
        scale: f32,
    ) {
        let theme = self.theme();
        let active = selected || self.token_hovered(token) || self.token_pressed(token);
        if active {
            let _ = self.draw_list_mut().push(DrawCommand::SolidRect {
                rect: Rect::new(
                    rect.x - 18.0 * scale,
                    rect.y + 8.0 * scale,
                    3.0 * scale,
                    34.0 * scale,
                ),
                color: theme.accent,
            });
        }
        self.text(
            label,
            Rect::new(rect.x, rect.y + 8.0 * scale, rect.width, 40.0 * scale),
            34.0 * scale,
            if active {
                theme.foreground
            } else {
                theme.muted
            },
            if active {
                FontWeight::Semibold
            } else {
                FontWeight::Regular
            },
            0.4 * scale,
        );
        self.interactive(token, rect, true, false);
    }

    /// Accent gradient fading out to the right, starting `inset` left of
    /// `rect`: the selection/hover mark of box-free rows.
    pub(crate) fn accent_sweep(&mut self, rect: Rect, strength: f32, inset: f32) {
        let accent = self.theme.accent;
        let _ = self.draw.push(horizontal(
            Rect::new(rect.x - inset, rect.y, rect.width + inset, rect.height),
            Color::new(accent.r, accent.g, accent.b, strength),
            Color::new(accent.r, accent.g, accent.b, 0.0),
        ));
    }

    /// Faint hairline between box-free rows.
    pub(crate) fn separator_line(&mut self, rect: Rect) {
        let _ = self.draw.push(DrawCommand::SolidRect {
            rect,
            color: Color::new(1.0, 1.0, 1.0, 0.07),
        });
    }

    /// One keyboard key cap followed by its action; returns the next free x.
    pub(crate) fn key_hint(
        &mut self,
        key: &str,
        action: &str,
        origin: [f32; 2],
        scale: f32,
    ) -> f32 {
        let height = 22.0 * scale;
        let cap_width = (16.0 + 8.4 * key.len() as f32) * scale;
        let cap = Rect::new(origin[0], origin[1], cap_width, height);
        let _ = self.draw.push(DrawCommand::RoundedRect {
            rect: cap,
            radius: 4.0 * scale,
            color: Color::new(1.0, 1.0, 1.0, 0.09),
        });
        let _ = self.draw.push(DrawCommand::Border {
            rect: cap,
            radius: 4.0 * scale,
            width: 1.0,
            color: Color::new(1.0, 1.0, 1.0, 0.16),
        });
        self.text_aligned(
            key,
            Rect::new(cap.x, cap.y + 3.5 * scale, cap.width, 14.0 * scale),
            12.0 * scale,
            Color::new(0.90, 0.94, 0.98, 0.92),
            FontWeight::Semibold,
            0.6 * scale,
            TextAlign::Center,
        );
        let label_x = cap.right() + 10.0 * scale;
        let label_width = (7.2 * action.len() as f32 + 8.0) * scale;
        self.text(
            action,
            Rect::new(label_x, cap.y + 3.0 * scale, label_width, 16.0 * scale),
            13.0 * scale,
            self.theme.muted,
            FontWeight::Regular,
            0.2 * scale,
        );
        label_x + label_width + 22.0 * scale
    }
}
