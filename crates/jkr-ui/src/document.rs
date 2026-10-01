//! Serializable HUD layout documents.

use serde::{Deserialize, Serialize};

use crate::{Anchor, Color, FontWeight, SizeSpec, Vec2};

/// Serializable root of a HUD layout document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HudLayoutDocument {
    /// Schema version. Version one is currently supported.
    pub version: u32,
    /// Theme name resolved by the application.
    pub theme: String,
    /// Ordered widget declarations.
    pub widgets: Vec<HudWidget>,
}

impl HudLayoutDocument {
    /// Parse a UTF-8 JSON document.
    pub fn from_json(source: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(source)
    }

    /// Serialize a stable, pretty-printed JSON representation.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// One data-bound HUD widget declaration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HudWidget {
    /// Stable modder-facing id.
    pub id: String,
    /// Visual primitive.
    pub kind: HudWidgetKind,
    /// Optional data-source binding name.
    #[serde(default)]
    pub binding: Option<String>,
    /// Screen anchor.
    pub anchor: Anchor,
    /// Logical-pixel offset from the anchor.
    #[serde(default)]
    pub offset: Vec2,
    /// Preferred and constrained widget size.
    pub size: SizeSpec,
    /// Data/cvar visibility rule evaluated by the adapter.
    #[serde(default)]
    pub visibility: VisibilityCondition,
    /// Stable draw layer.
    #[serde(default)]
    pub layer: i16,
    /// Per-widget style token overrides.
    #[serde(default)]
    pub style: StyleOverrides,
}

/// Built-in HUD visual primitives.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudWidgetKind {
    /// A styled grouping surface with no data binding.
    Panel,
    /// A data-bound text run.
    Text,
    /// A numeric progress meter.
    Meter,
    /// A renderer-owned texture.
    Image,
    /// A repeated row supplied by the adapter.
    Repeater,
}

/// Serializable visibility expression.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum VisibilityCondition {
    /// Always visible.
    #[default]
    Always,
    /// Visible when a boolean source binding is true.
    Flag { binding: String },
    /// Visible when a numeric binding is positive.
    Positive { binding: String },
    /// Visible while a numeric binding is below a threshold.
    Below { binding: String, value: f32 },
}

impl VisibilityCondition {
    /// Evaluate against an application-owned data source.
    pub fn evaluate(&self, data: &impl crate::HudDataSource) -> bool {
        match self {
            Self::Always => true,
            Self::Flag { binding } => data.flag(binding).unwrap_or(false),
            Self::Positive { binding } => data.number(binding).is_some_and(|value| value > 0.0),
            Self::Below { binding, value } => {
                data.number(binding).is_some_and(|current| current < *value)
            }
        }
    }
}

/// Optional style values overriding the active theme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StyleOverrides {
    /// Foreground override.
    #[serde(default)]
    pub foreground: Option<Color>,
    /// Background override.
    #[serde(default)]
    pub background: Option<Color>,
    /// When set, a panel fades vertically from `background` to this colour
    /// instead of filling a solid box.
    #[serde(default)]
    pub background_end: Option<Color>,
    /// Corner-radius override in logical pixels.
    #[serde(default)]
    pub radius: Option<f32>,
    /// Typography scale multiplier.
    #[serde(default)]
    pub type_scale: Option<f32>,
    /// Optional outline colour.
    #[serde(default)]
    pub border: Option<Color>,
    /// Outline width in logical pixels.
    #[serde(default)]
    pub border_width: Option<f32>,
    /// Text weight override.
    #[serde(default)]
    pub weight: Option<FontWeight>,
    /// Additional tracking in logical pixels.
    #[serde(default)]
    pub letter_spacing: Option<f32>,
}
