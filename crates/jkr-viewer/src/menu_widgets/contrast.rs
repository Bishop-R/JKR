//! Menu text contrast over the live map (`ui_menuContrast`).
//!
//! Hero screens draw their text straight over the world, so how readable it is
//! depends on the map behind it. Each level names the brightest backdrop it is
//! tuned for and darkens the text column just enough that the weakest text
//! style still reaches the WCAG 2 AA body-text ratio (4.5:1) over it:
//!
//! - `off`: the original scrims, unchanged.
//! - `standard` (default): theme body text (the muted colour and the dimmed
//!   labels) over a backdrop of relative luminance 0.5, about sRGB `#bcbcbc`.
//! - `strong`: all enabled text, the accent included, over pure white.
//!
//! UI colours are linear values blended into an sRGB or float target, so
//! relative luminance is computed from them directly. The glyph drop shadow
//! and the scrim's top and bottom bands are ignored, which only makes the
//! figures conservative.

use super::MenuCanvas;
use jkr_ui::{Color, Theme};
use std::sync::atomic::{AtomicU8, Ordering};

/// WCAG 2 AA minimum contrast ratio for body text.
pub(crate) const BODY_TEXT_RATIO: f32 = 4.5;

/// Relative luminance of the menus' scrim ink.
const INK_LUMINANCE: f32 = 0.2126 * 0.005 + 0.7152 * 0.010 + 0.0722 * 0.018;

/// The darkest backing a level may ask for, so the world never disappears.
const MAX_COVERAGE: f32 = 0.95;

/// Text drawn below this opacity is the disabled style, which WCAG exempts;
/// it keeps its dimmed look.
const DISABLED_ALPHA: f32 = 0.5;

/// The level shared by every menu canvas, published from the cvar once per
/// frame; a plain atomic so the frame path neither locks nor allocates.
static CURRENT: AtomicU8 = AtomicU8::new(MenuContrast::Standard as u8);

/// How strongly menus darken the world behind their text.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum MenuContrast {
    /// The original scrims.
    Off = 0,
    /// Body text reaches AA over bright surfaces (luminance 0.5).
    #[default]
    Standard = 1,
    /// All enabled text, accent included, reaches AA over white.
    Strong = 2,
}

impl MenuContrast {
    /// Archived cvar holding the level by name (`off`, `standard`, `strong`)
    /// or number (0 to 2).
    pub(crate) const CVAR: &'static str = "ui_menuContrast";

    /// Parse a level name or number; `None` for anything else.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "off" | "0" => Some(Self::Off),
            "standard" | "1" => Some(Self::Standard),
            "strong" | "2" => Some(Self::Strong),
            _ => None,
        }
    }

    /// The level a cvar value selects; the default when absent or malformed.
    pub(crate) fn from_cvar(value: Option<&str>) -> Self {
        value.and_then(Self::parse).unwrap_or_default()
    }

    /// Make this the level every menu canvas uses from its next frame on.
    pub(crate) fn publish(self) {
        CURRENT.store(self as u8, Ordering::Relaxed);
    }

    /// The level last published.
    pub(crate) fn current() -> Self {
        match CURRENT.load(Ordering::Relaxed) {
            0 => Self::Off,
            2 => Self::Strong,
            _ => Self::Standard,
        }
    }

    /// Relative luminance of the brightest backdrop this level is tuned for.
    const fn reference_backdrop(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Standard => 0.5,
            Self::Strong => 1.0,
        }
    }

    /// Total ink coverage (0 to 1) a text column needs at this level for
    /// `theme`'s colours; 0 when off.
    pub(crate) fn column_coverage(self, theme: &Theme) -> f32 {
        let weakest = match self {
            Self::Off => return 0.0,
            Self::Standard => luminance(theme.muted),
            Self::Strong => luminance(theme.muted).min(luminance(theme.accent)),
        };
        required_coverage(weakest, self.reference_backdrop())
    }

    /// Luminance behind text on a backing of total `coverage` over this
    /// level's reference backdrop.
    pub(crate) fn backing_luminance(self, coverage: f32) -> f32 {
        let backdrop = self.reference_backdrop();
        backdrop + (INK_LUMINANCE - backdrop) * coverage
    }
}

/// WCAG relative luminance of `color`'s RGB, which the UI treats as linear.
pub(crate) fn luminance(color: Color) -> f32 {
    0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b
}

/// Ink coverage that puts opaque text of luminance `text` at
/// [`BODY_TEXT_RATIO`] over a backdrop of luminance `backdrop`, capped so the
/// world stays visible.
pub(crate) fn required_coverage(text: f32, backdrop: f32) -> f32 {
    let darkest_backing = (text + 0.05) / BODY_TEXT_RATIO - 0.05;
    if backdrop <= darkest_backing {
        return 0.0;
    }
    ((backdrop - darkest_backing) / (backdrop - INK_LUMINANCE).max(f32::EPSILON))
        .clamp(0.0, MAX_COVERAGE)
}

/// Alpha of one layer that, stacked over a layer of alpha `base`, brings the
/// total coverage to `total`.
pub(crate) fn layer_alpha(total: f32, base: f32) -> f32 {
    if total <= base {
        return 0.0;
    }
    (1.0 - (1.0 - total) / (1.0 - base).max(f32::EPSILON)).clamp(0.0, 1.0)
}

/// `color` with just enough opacity to reach [`BODY_TEXT_RATIO`] over a
/// backing of luminance `under`. Opaque, disabled-style and dark text is
/// returned unchanged, so hierarchy by colour and weight survives.
pub(crate) fn legible(color: Color, under: f32) -> Color {
    let text = luminance(color);
    if color.a >= 1.0 || color.a < DISABLED_ALPHA || text <= under {
        return color;
    }
    let needed = (BODY_TEXT_RATIO * (under + 0.05) - 0.05 - under) / (text - under);
    Color::new(color.r, color.g, color.b, needed.clamp(color.a, 1.0))
}

/// One horizontal stretch of a scrim fade: x range and ink alpha at its ends.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct FadeSegment {
    pub(crate) x0: f32,
    pub(crate) x1: f32,
    pub(crate) a0: f32,
    pub(crate) a1: f32,
}

impl FadeSegment {
    const fn new(x0: f32, x1: f32, a0: f32, a1: f32) -> Self {
        Self { x0, x1, a0, a1 }
    }
}

/// A scrim's left-to-right fade from `start` at x = 0 to `end` at `width`
/// (nothing beyond), held at no less than `floor` up to `hold` and eased back
/// into the original fade over `feather`. Returns the segments and how many
/// are used; with a floor the fade already meets, the original single
/// segment comes back unchanged.
pub(crate) fn held_fade(
    width: f32,
    start: f32,
    end: f32,
    hold: f32,
    floor: f32,
    feather: f32,
) -> ([FadeSegment; 3], usize) {
    let original = |x: f32| {
        if x <= width {
            start + (end - start) * (x / width.max(f32::EPSILON))
        } else {
            0.0
        }
    };
    let mut segments = [FadeSegment::default(); 3];
    if hold <= 0.0 || floor <= original(hold) {
        segments[0] = FadeSegment::new(0.0, width, start, end);
        return (segments, 1);
    }
    segments[0] = FadeSegment::new(0.0, hold, start.max(floor), floor);
    let eased = hold + feather.max(0.0);
    if eased < width {
        segments[1] = FadeSegment::new(hold, eased, floor, original(eased));
        segments[2] = FadeSegment::new(eased, width, original(eased), end);
        (segments, 3)
    } else {
        segments[1] = FadeSegment::new(hold, eased, floor, 0.0);
        (segments, 2)
    }
}

impl MenuCanvas {
    /// Total ink coverage this canvas's text backings use this frame; 0 when
    /// `ui_menuContrast` is off.
    pub(crate) fn readability_coverage(&self) -> f32 {
        self.contrast.column_coverage(&self.theme)
    }

    /// Note that text from here on sits on a backing of total `coverage`,
    /// so dimmed labels are raised to stay legible on it.
    pub(super) fn mark_backing(&mut self, coverage: f32) {
        if coverage > 0.0 {
            self.backing = Some(self.contrast.backing_luminance(coverage));
        }
    }

    /// `color` as this frame's text should draw it (see [`legible`]).
    pub(super) fn legible_text(&self, color: Color) -> Color {
        self.backing.map_or(color, |under| legible(color, under))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG contrast ratio between two relative luminances.
    fn contrast_ratio(a: f32, b: f32) -> f32 {
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// Ink alpha the segments draw at `x` (0 outside them).
    fn sample(segments: &[FadeSegment], x: f32) -> f32 {
        segments
            .iter()
            .find(|s| x >= s.x0 && x <= s.x1 && s.x1 > s.x0)
            .map_or(0.0, |s| s.a0 + (s.a1 - s.a0) * (x - s.x0) / (s.x1 - s.x0))
    }

    fn total(base: f32, layer: f32) -> f32 {
        1.0 - (1.0 - base) * (1.0 - layer)
    }

    #[test]
    fn levels_parse_by_name_and_number() {
        assert_eq!(MenuContrast::parse("OFF"), Some(MenuContrast::Off));
        assert_eq!(MenuContrast::parse(" 2 "), Some(MenuContrast::Strong));
        assert_eq!(
            MenuContrast::parse("standard"),
            Some(MenuContrast::Standard)
        );
        assert_eq!(MenuContrast::parse("3"), None);
        assert_eq!(MenuContrast::from_cvar(None), MenuContrast::Standard);
        assert_eq!(MenuContrast::from_cvar(Some("x")), MenuContrast::Standard);
    }

    #[test]
    fn off_leaves_the_world_alone() {
        assert_eq!(MenuContrast::Off.column_coverage(&Theme::default()), 0.0);
    }

    #[test]
    fn standard_column_gives_muted_text_aa_over_bright_surfaces() {
        let theme = Theme::default();
        let level = MenuContrast::Standard;
        let coverage = level.column_coverage(&theme);
        let under = level.backing_luminance(coverage);
        let ratio = contrast_ratio(luminance(theme.muted), under);
        assert!(ratio >= BODY_TEXT_RATIO - 1e-3, "muted {ratio}");
        assert!(contrast_ratio(luminance(theme.foreground), under) > 6.0);
        // The default accent still clears the 3:1 large-text/UI floor.
        assert!(contrast_ratio(luminance(theme.accent), under) >= 3.0);
    }

    #[test]
    fn strong_column_gives_accent_and_muted_text_aa_over_white() {
        let theme = Theme::default();
        let level = MenuContrast::Strong;
        let coverage = level.column_coverage(&theme);
        let under = level.backing_luminance(coverage);
        for color in [theme.muted, theme.accent, theme.foreground] {
            let ratio = contrast_ratio(luminance(color), under);
            assert!(ratio >= BODY_TEXT_RATIO - 1e-3, "{color:?} {ratio}");
        }
        assert!(coverage > MenuContrast::Standard.column_coverage(&theme));
    }

    #[test]
    fn dark_custom_accents_are_capped() {
        let theme = Theme {
            accent: Color::new(0.0, 0.0, 0.5, 1.0),
            ..Theme::default()
        };
        assert_eq!(MenuContrast::Strong.column_coverage(&theme), MAX_COVERAGE);
    }

    #[test]
    fn dimmed_labels_gain_just_enough_opacity() {
        let level = MenuContrast::Standard;
        let under = level.backing_luminance(level.column_coverage(&Theme::default()));
        let label = Color::new(0.82, 0.88, 0.94, 0.55);
        let raised = legible(label, under);
        assert!(raised.a > label.a && raised.a < 1.0);
        let blended = raised.a * luminance(label) + (1.0 - raised.a) * under;
        assert!((contrast_ratio(blended, under) - BODY_TEXT_RATIO).abs() < 1e-3);
        // Already legible, opaque and disabled-style text keep their alpha.
        let bright = Color::new(0.82, 0.88, 0.94, 0.95);
        assert_eq!(legible(bright, under), bright);
        let disabled = Color::new(0.82, 0.88, 0.94, 0.30);
        assert_eq!(legible(disabled, under), disabled);
    }

    #[test]
    fn held_fade_meets_the_floor_and_never_lightens() {
        let (width, start, end) = (1229.0, 0.94, 0.0);
        let floor = layer_alpha(0.79, 0.30);
        let (hold, feather) = (824.0, 230.0);
        let (segments, count) = held_fade(width, start, end, hold, floor, feather);
        let segments = &segments[..count];
        assert_eq!(count, 3);
        for step in 0..=200 {
            let x = width * step as f32 / 200.0;
            let original = start + (end - start) * x / width;
            let drawn = sample(segments, x);
            assert!(drawn >= original - 1e-4, "x {x}: {drawn} < {original}");
            if x <= hold {
                assert!(total(0.30, drawn) >= 0.79 - 1e-4, "x {x}");
            }
        }
        for pair in segments.windows(2) {
            assert_eq!(pair[0].x1, pair[1].x0);
            assert!((pair[0].a1 - pair[1].a0).abs() < 1e-6);
        }
        assert_eq!(segments[2].x1, width);
        assert_eq!(segments[2].a1, end);
    }

    #[test]
    fn held_fade_is_the_original_when_off_or_already_dark() {
        let original = FadeSegment::new(0.0, 1000.0, 0.95, 0.0);
        let (segments, count) = held_fade(1000.0, 0.95, 0.0, 400.0, 0.0, 100.0);
        assert_eq!((segments[0], count), (original, 1));
        let (segments, count) = held_fade(1000.0, 0.95, 0.0, 400.0, 0.5, 100.0);
        assert_eq!((segments[0], count), (original, 1));
    }

    #[test]
    fn held_fade_past_the_original_eases_to_nothing() {
        let (segments, count) = held_fade(500.0, 0.95, 0.0, 450.0, 0.8, 100.0);
        assert_eq!(count, 2);
        assert_eq!(segments[1], FadeSegment::new(450.0, 550.0, 0.8, 0.0));
    }

    #[test]
    fn layer_alpha_composes_to_the_total() {
        let layer = layer_alpha(0.79, 0.30);
        assert!((total(0.30, layer) - 0.79).abs() < 1e-5);
        assert_eq!(layer_alpha(0.2, 0.30), 0.0);
    }
}
