//! Hero-form vocabulary shared by the settings and player screens: the
//! left-column geometry, the kicker/title/subtitle header, a tab strip, the
//! box-free rows (label on the left, value control on the right) and the
//! key-cap footer whose trailing cap doubles as the pointer's Back target.

use super::MenuCanvas;
use jkr_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};
use std::fmt::Arguments;

/// Fraction of a row where the value zone (slider, toggle, cycler) starts.
pub(crate) const VALUE_START: f32 = 0.52;

/// Token of the footer's Back cap on every hero form.
pub(crate) const BACK_TOKEN: u16 = 900;

/// First tab token; tab `i` is `TAB_BASE + i`.
pub(crate) const TAB_BASE: u16 = 500;

/// Token of the wheel target covering the row column.
pub(crate) const SCROLL_TOKEN: u16 = 901;

/// Resolved hero-form geometry for one viewport.
pub(crate) struct FormLayout {
    pub(crate) scale: f32,
    pub(crate) margin: f32,
    pub(crate) column_width: f32,
    pub(crate) row_height: f32,
    pub(crate) rows_y: f32,
    pub(crate) viewport: [f32; 2],
}

impl FormLayout {
    pub(crate) fn new(viewport: [f32; 2]) -> Self {
        let scale = (viewport[1] / 1_080.0).clamp(0.6, 2.5);
        Self {
            scale,
            margin: (viewport[0] * 0.075).max(72.0 * scale),
            column_width: (viewport[0] * 0.44).clamp(440.0 * scale, 680.0 * scale),
            row_height: 50.0 * scale,
            rows_y: viewport[1] * 0.40,
            viewport,
        }
    }

    /// Pointer target of form row `row`.
    pub(crate) fn row_rect(&self, row: usize) -> Rect {
        Rect::new(
            self.margin,
            self.rows_y + row as f32 * self.row_height,
            self.column_width,
            self.row_height,
        )
    }

    /// Where the value zone of a row starts (sliders map pointer x from here).
    pub(crate) fn value_x(&self, row: Rect) -> f32 {
        row.x + row.width * VALUE_START
    }

    /// The right-hand part of a row that holds its control.
    pub(crate) fn value_zone(&self, row: Rect) -> Rect {
        let x = self.value_x(row);
        Rect::new(x, row.y, row.right() - x, row.height)
    }

    /// Baseline of the tab strip.
    pub(crate) fn tabs_y(&self) -> f32 {
        self.viewport[1] * 0.31
    }
}

impl MenuCanvas {
    /// Kicker line, big page name and one-line subtitle above the tab strip.
    pub(crate) fn form_header(
        &mut self,
        layout: &FormLayout,
        kicker: &str,
        title: &str,
        subtitle: &str,
    ) {
        let s = layout.scale;
        let (x, width) = (layout.margin, layout.column_width);
        let theme = self.theme();
        let title_y = layout.viewport[1] * 0.17;
        // Registered before the rows so hover and clicks still hit them; the
        // wheel falls through to this region and moves the selection.
        let rows_bottom = layout.viewport[1] - 80.0 * s;
        self.scroll_region(
            SCROLL_TOKEN,
            Rect::new(x, layout.rows_y, width, rows_bottom - layout.rows_y),
        );
        self.text(
            kicker,
            Rect::new(x, title_y - 26.0 * s, width, 18.0 * s),
            13.0 * s,
            theme.accent,
            FontWeight::Semibold,
            3.0 * s,
        );
        self.text(
            title,
            Rect::new(x - 3.0 * s, title_y, width, 72.0 * s),
            64.0 * s,
            theme.foreground,
            FontWeight::Semibold,
            -1.0 * s,
        );
        self.text(
            subtitle,
            Rect::new(x, title_y + 84.0 * s, width, 20.0 * s),
            15.0 * s,
            theme.muted,
            FontWeight::Regular,
            0.2 * s,
        );
    }

    /// Horizontal strip of page tabs; tab `i` answers to token `TAB_BASE + i`.
    pub(crate) fn form_tabs(&mut self, layout: &FormLayout, tabs: &[&str], active: usize) {
        let s = layout.scale;
        let y = layout.tabs_y();
        let theme = self.theme();
        let mut tab_x = layout.margin;
        for (index, tab) in tabs.iter().enumerate() {
            let token = TAB_BASE + index as u16;
            let width = (12.0 + 9.6 * tab.len() as f32) * s;
            let rect = Rect::new(tab_x, y, width, 30.0 * s);
            let hovered = self.token_hovered(token);
            self.text(
                tab,
                Rect::new(rect.x, rect.y + 4.0 * s, rect.width, 18.0 * s),
                13.0 * s,
                if index == active {
                    theme.foreground
                } else if hovered {
                    Color::new(0.94, 0.97, 1.0, 0.9)
                } else {
                    Color::new(0.82, 0.88, 0.94, 0.55)
                },
                FontWeight::Semibold,
                2.2 * s,
            );
            if index == active {
                self.accent_bar(
                    Rect::new(
                        rect.x,
                        rect.bottom() - 2.0 * s,
                        rect.width - 12.0 * s,
                        2.0 * s,
                    ),
                    theme.accent,
                );
            }
            self.hit_region(token, rect);
            tab_x += width + 22.0 * s;
        }
    }

    /// Selection sweep, hairline and pointer target shared by every row kind.
    pub(crate) fn form_row_frame(&mut self, rect: Rect, token: u16, selected: bool, s: f32) {
        let hovered = self.token_hovered(token);
        if selected || hovered {
            self.accent_sweep(rect, if selected { 0.12 } else { 0.06 }, 28.0 * s);
        }
        self.separator_line(Rect::new(rect.x, rect.bottom() - 1.0, rect.width, 1.0));
        self.hit_region(token, rect);
    }

    /// The label on the left of a row; bright and semibold when selected.
    pub(crate) fn form_label(&mut self, rect: Rect, label: &str, selected: bool, s: f32) {
        let theme = self.theme();
        self.text(
            label,
            Rect::new(
                rect.x,
                rect.y + 15.0 * s,
                rect.width * VALUE_START,
                22.0 * s,
            ),
            17.0 * s,
            if selected {
                theme.foreground
            } else {
                Color::new(0.82, 0.88, 0.94, 0.78)
            },
            if selected {
                FontWeight::Semibold
            } else {
                FontWeight::Regular
            },
            0.2 * s,
        );
    }

    /// Right-aligned value text inside `rect`.
    pub(crate) fn form_value(&mut self, value: &str, rect: Rect, color: Color, s: f32) {
        self.text_aligned(
            value,
            Rect::new(rect.x, rect.y + 16.0 * s, rect.width, 20.0 * s),
            15.0 * s,
            color,
            FontWeight::Semibold,
            0.3 * s,
            TextAlign::End,
        );
    }

    /// Colour of a row's value: accent when selected, muted otherwise.
    pub(crate) fn form_value_color(&self, selected: bool) -> Color {
        let theme = self.theme();
        if selected { theme.accent } else { theme.muted }
    }

    /// `<  value  >` control in `zone`, optionally with a colour chip left of
    /// the value. The arrows are decoration; the pointer half of the zone
    /// decides the direction (see [`cycler_direction`]).
    pub(crate) fn form_cycler(
        &mut self,
        zone: Rect,
        value: &str,
        swatch: Option<Color>,
        color: Color,
        s: f32,
    ) {
        let label = self.cycler_frame(zone, swatch, color, s);
        self.form_value(value, label, color, s);
    }

    /// [`form_cycler`](Self::form_cycler) with a formatted value.
    pub(crate) fn form_cycler_fmt(
        &mut self,
        zone: Rect,
        value: Arguments<'_>,
        swatch: Option<Color>,
        color: Color,
        s: f32,
    ) {
        let label = self.cycler_frame(zone, swatch, color, s);
        self.text_fmt_aligned(
            value,
            Rect::new(label.x, label.y + 16.0 * s, label.width, 20.0 * s),
            15.0 * s,
            color,
            FontWeight::Semibold,
            0.3 * s,
            TextAlign::End,
        );
    }

    /// Arrows and optional colour chip of a cycler; returns the label rect.
    fn cycler_frame(&mut self, zone: Rect, swatch: Option<Color>, color: Color, s: f32) -> Rect {
        self.text(
            "<",
            Rect::new(zone.x, zone.y + 13.0 * s, 16.0 * s, 24.0 * s),
            20.0 * s,
            color,
            FontWeight::Semibold,
            0.0,
        );
        self.text_aligned(
            ">",
            Rect::new(
                zone.right() - 16.0 * s,
                zone.y + 13.0 * s,
                16.0 * s,
                24.0 * s,
            ),
            20.0 * s,
            color,
            FontWeight::Semibold,
            0.0,
            TextAlign::End,
        );
        let mut label = Rect::new(zone.x, zone.y, zone.width - 22.0 * s, zone.height);
        if let Some(swatch) = swatch {
            let chip = Rect::new(zone.x + 24.0 * s, zone.y + 17.0 * s, 30.0 * s, 16.0 * s);
            let _ = self.draw.push(DrawCommand::RoundedRect {
                rect: chip,
                radius: chip.height * 0.5,
                color: swatch,
            });
            let _ = self.draw.push(DrawCommand::Border {
                rect: chip,
                radius: chip.height * 0.5,
                width: 1.0,
                color: Color::new(1.0, 1.0, 1.0, 0.25),
            });
            label.x = chip.right();
            label.width -= chip.right() - zone.x;
        }
        label
    }

    /// Zero-through-three level indicator (three pills) without any game policy.
    pub(crate) fn pip_row(&mut self, rect: Rect, level: u8, color: Color) {
        let gap = 5.0;
        let width = (rect.width - gap * 2.0) / 3.0;
        for index in 0..3 {
            let filled = index < usize::from(level);
            let _ = self.draw.push(DrawCommand::RoundedRect {
                rect: Rect::new(
                    rect.x + index as f32 * (width + gap),
                    rect.y,
                    width,
                    rect.height,
                ),
                radius: rect.height * 0.5,
                color: if filled {
                    color
                } else {
                    Color::new(1.0, 1.0, 1.0, 0.13)
                },
            });
        }
    }

    /// A row whose value is a right-aligned action verb (`EDIT  >`).
    pub(crate) fn form_action_row(
        &mut self,
        layout: &FormLayout,
        row: usize,
        selected: bool,
        label: &str,
        action: &str,
    ) {
        let s = layout.scale;
        let rect = layout.row_rect(row);
        let theme = self.theme();
        self.form_row_frame(rect, row as u16, selected, s);
        self.text(
            label,
            Rect::new(
                rect.x,
                rect.y + 15.0 * s,
                rect.width * VALUE_START,
                22.0 * s,
            ),
            17.0 * s,
            theme.foreground,
            FontWeight::Semibold,
            0.2 * s,
        );
        let zone = layout.value_zone(rect);
        self.text_aligned(
            action,
            Rect::new(zone.x, rect.y + 16.0 * s, zone.width, 20.0 * s),
            14.0 * s,
            theme.accent,
            FontWeight::Semibold,
            2.0 * s,
            TextAlign::End,
        );
    }

    /// Key-cap footer; the last cap is registered as [`BACK_TOKEN`].
    pub(crate) fn form_footer(&mut self, layout: &FormLayout, hints: &[(&str, &str)]) {
        let s = layout.scale;
        let footer_y = layout.viewport[1] - 64.0 * s;
        let mut hint_x = layout.margin;
        let mut last_start = hint_x;
        for (key, action) in hints {
            last_start = hint_x;
            hint_x = self.key_hint(key, action, [hint_x, footer_y], s);
        }
        self.hit_region(BACK_TOKEN, footer_cap(last_start, hint_x, footer_y, s));
    }

    /// Key-cap footer whose caps are pointer targets too: each hint's token
    /// is registered over its cap and label (0 = display only).
    pub(crate) fn form_footer_actions(&mut self, layout: &FormLayout, hints: &[(&str, &str, u16)]) {
        let s = layout.scale;
        let footer_y = layout.viewport[1] - 64.0 * s;
        let mut hint_x = layout.margin;
        for (key, action, token) in hints {
            let start = hint_x;
            hint_x = self.key_hint(key, action, [hint_x, footer_y], s);
            if *token != 0 {
                self.hit_region(*token, footer_cap(start, hint_x, footer_y, s));
            }
        }
    }
}

/// Pointer target of one footer hint spanning `start..end` at `footer_y`.
fn footer_cap(start: f32, end: f32, footer_y: f32, s: f32) -> Rect {
    Rect::new(start, footer_y - 6.0 * s, end - start, 34.0 * s)
}

/// Width (at scale 1) of the value text column a slider keeps to the
/// right of its rail.
pub(crate) const SLIDER_VALUE_COLUMN: f32 = 72.0;

/// Where along a row's value zone the pointer is, 0 at the zone's left
/// edge and 1 at the row's right edge.
pub(crate) fn zone_ratio(rect: Rect, x: f32) -> f32 {
    let start = rect.x + rect.width * VALUE_START;
    ((x - start) / (rect.right() - start)).clamp(0.0, 1.0)
}

/// Where along a slider's rail the pointer is, 0 at its left end and 1 at
/// its right end (the value column past it counts as 1), so the knob lands
/// under the pointer when a slider is clicked or dragged at UI `scale`.
pub(crate) fn slider_ratio(rect: Rect, x: f32, scale: f32) -> f32 {
    let start = rect.x + rect.width * VALUE_START;
    let end = rect.right() - SLIDER_VALUE_COLUMN * scale;
    ((x - start) / (end - start).max(1.0)).clamp(0.0, 1.0)
}

/// Whether the pointer at `x` is over the value text column a slider row
/// `rect` keeps right of its rail at UI `scale` (clicking it types a value).
pub(crate) fn slider_value_hit(rect: Rect, x: f32, scale: f32) -> bool {
    x >= rect.right() - SLIDER_VALUE_COLUMN * scale && x <= rect.right()
}

/// Which of `count` equal palette cells across a row's value zone the
/// pointer at `x` is in.
pub(crate) fn palette_index(rect: Rect, x: f32, count: usize) -> usize {
    ((zone_ratio(rect, x) * count as f32) as usize).min(count.saturating_sub(1))
}

/// Which way a pointer click at `x` on a cycler row `rect` turns: the left
/// half of the value zone steps back, the right half forward.
pub(crate) fn cycler_direction(rect: Rect, x: f32) -> isize {
    let start = rect.x + rect.width * VALUE_START;
    if x < (start + rect.right()) * 0.5 {
        -1
    } else {
        1
    }
}
