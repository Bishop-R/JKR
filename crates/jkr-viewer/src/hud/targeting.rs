//! Sampled crosshair policy and retained position telemetry; no second target scan.
use super::*;
use jkr_shell::{CvarDefinition, CvarFlags, CvarRegistry};
use jkr_ui::{DrawCommand, FontWeight, Rect, TextAlign, TextOverflow};
use std::fmt::Write;

/// Register supported options and the old crosshair spelling before config restoration.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), jkr_shell::CvarError> {
    super::identification::register(cvars)?;
    super::tints::register(cvars)?;
    super::enemy_info::register(cvars)?;
    cvars.register(CvarDefinition::new(
        "cg_dynamicCrosshair",
        1_i64,
        CvarFlags::ARCHIVE,
        "On-foot muzzle crosshair: 0 static, 1 dynamic, 2 selective; box traces only",
    ))?;
    cvars.register_alias("cg_crosshair", "cg_drawCrosshair")?;
    for (name, value, help) in [
        (
            "cg_crosshairX",
            0.0,
            "Crosshair horizontal offset in virtual units",
        ),
        (
            "cg_crosshairY",
            0.0,
            "Crosshair vertical offset in virtual units",
        ),
        (
            "cg_lagometerX",
            48.0,
            "Lagometer left edge measured from the right",
        ),
        (
            "cg_lagometerY",
            144.0,
            "Lagometer top edge measured from the bottom",
        ),
    ] {
        cvars.register(CvarDefinition::new(name, value, CvarFlags::ARCHIVE, help))?;
    }
    for (name, value, help) in [
        (
            "cg_crosshairIdentifyTarget",
            true,
            "Classify the traced entity by team and ownership",
        ),
        (
            "cg_crosshairSizeScale",
            true,
            "Scale crosshair with the viewport",
        ),
        (
            "cg_crosshairHealth",
            false,
            "Color crosshair using local health and armor",
        ),
        (
            "cg_crosshairSaberStyleColor",
            false,
            "Color crosshair using the drawn saber style",
        ),
    ] {
        cvars.register(CvarDefinition::new(name, value, CvarFlags::ARCHIVE, help))?;
    }
    cvars.register(CvarDefinition::new(
        "cg_showpos",
        false,
        CvarFlags::NONE,
        "Show position, angles and three-dimensional speed",
    ))?;
    Ok(())
}

/// Scalar settings sampled once during the normal HUD update.
#[derive(Clone, Copy)]
pub(crate) struct Policy {
    offset: [f32; 2],
    scaled: bool,
    health: bool,
    identify: bool,
    style: bool,
    showpos: bool,
    lagometer: [f32; 2],
    color: [f32; 4],
}

impl Policy {
    fn color_for(self, health: i32, armor: i32, weapon: u8, style: u32) -> [f32; 4] {
        if self.health {
            health_color(health, armor)
        } else if self.style && weapon == 3 {
            style_color(style, self.color[3])
        } else {
            self.color
        }
    }

    fn read(console: Option<&ViewerConsole>) -> Self {
        let f = |name, fallback| crate::cgame_options::scalar(console, name, fallback);
        let b = |name, fallback| console.and_then(|c| c.bool_cvar(name)).unwrap_or(fallback);
        Self {
            offset: [f("cg_crosshairx", 0.0), f("cg_crosshairy", 0.0)],
            scaled: b("cg_crosshairsizescale", true),
            health: b("cg_crosshairhealth", false),
            identify: b("cg_crosshairidentifytarget", true),
            style: b("cg_crosshairsaberstylecolor", false),
            showpos: b("cg_showpos", false),
            lagometer: [f("cg_lagometerx", 48.0), f("cg_lagometery", 144.0)],
            color: crate::cgame_options::crosshair_color(console),
        }
    }

    /// Shader offset in normalized screen coordinates and size-coordinate dimensions.
    pub(crate) fn parameters(self, viewport: [f32; 2]) -> [f32; 4] {
        let dimensions = if self.scaled {
            [viewport[0] * 480.0 / viewport[1].max(1.0), 480.0]
        } else {
            viewport
        };
        [
            self.offset[0] / 640.0,
            // Full-screen triangle UVs increase upward; stock Y increases downward.
            -self.offset[1] / 480.0,
            dimensions[0],
            dimensions[1],
        ]
    }

    /// Preserve the modern graph's dimensions and the reference's right/bottom anchor.
    pub(super) fn lagometer_rect(self, rect: Rect, viewport: [f32; 2]) -> Rect {
        // TaystJK cg_draw.c:7175-7176; horizontal virtual units are aspect corrected.
        let scale = viewport[1] / 480.0;
        Rect::new(
            viewport[0] - self.lagometer[0] * scale - (rect.width - 48.0 * scale).max(0.0),
            viewport[1] - self.lagometer[1] * scale,
            rect.width,
            rect.height,
        )
    }
}

/// Retained text and color, updated from the existing player-state projection.
pub(crate) struct State {
    /// Options sampled by the normal HUD update.
    pub(super) policy: Policy,
    /// Bounded, preallocated position/angle/velocity label.
    pub(super) position: String,
    /// Final shader color, with stock health-before-style precedence.
    pub(crate) color: [f32; 4],

    /// Projected muzzle trace offset, or zero for the static crosshair.
    pub(crate) dynamic_offset: Option<[f32; 2]>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            policy: Policy::read(None),
            position: String::with_capacity(384),
            color: [0.92, 0.98, 1.0, 1.0],

            dynamic_offset: None,
        }
    }
}

impl State {
    /// Apply the existing trace classification after local-health/style precedence.
    pub(crate) fn classify(&mut self, color: Option<[f32; 4]>, weapon: u8) {
        if self.policy.identify && !self.policy.health && !(self.policy.style && weapon == 3) {
            self.color = color.unwrap_or([1.0; 4]);
        }
    }
    pub(super) fn update(&mut self, console: Option<&ViewerConsole>, player: &PlayerState) {
        self.policy = Policy::read(console);
        self.color = self.policy.color_for(
            player.health(),
            player.armor(),
            player.weapon(),
            player.raw_field(25).unwrap_or(0),
        );
        self.position.clear();
        if self.policy.showpos {
            let origin = player.origin();
            let angles = player.view_angles();
            let velocity = player.velocity();
            // Bound hostile float formatting to fit the retained capacity.
            let f = |value: f32| value.clamp(-9999999.0, 9999999.0);
            let speed = velocity.iter().map(|v| v * v).sum::<f32>().sqrt();
            let _ = write!(
                self.position,
                "POS {:.2} {:.2} {:.2}  ANG {:.2} {:.2}  VEL {:.2}",
                f(origin[0]),
                f(origin[1]),
                f(origin[2]),
                f(angles[0]),
                f(angles[1]),
                f(speed)
            );
        }
    }

    /// The GPU consumes this cached policy without additional cvar lookup or formatting.
    pub(crate) fn parameters(&self, viewport: [f32; 2]) -> [f32; 4] {
        let mut parameters = self.policy.parameters(viewport);
        if let Some(offset) = self.dynamic_offset {
            parameters[0] = offset[0];
            parameters[1] = offset[1];
        }
        parameters
    }

    pub(super) fn emit(&self, list: &mut DrawList, viewport: [f32; 2], theme: Theme) {
        if !self.policy.showpos {
            return;
        }
        let scale = crate::ui_scale::height_scale(viewport[1]);
        let _ = list.push(DrawCommand::Text {
            rect: Rect::new(
                32.0 * scale,
                28.0 * scale,
                viewport[0] - 64.0 * scale,
                30.0 * scale,
            ),
            text: TextId(317),
            size: 20.0 * scale,
            color: theme.foreground,
            align: TextAlign::Start,
            overflow: TextOverflow::Ellipsis,
            weight: FontWeight::Regular,
            letter_spacing: 0.0,
        });
    }
}

// TaystJK cg_drawtools.c:52-97. Armor protection is 0.5 in MP bg_public.h.
fn health_color(health: i32, armor: i32) -> [f32; 4] {
    if health <= 0 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let health = health.saturating_add(armor.min(health)) as f32;
    [
        1.0,
        ((health - 30.0) / 30.0).clamp(0.0, 1.0),
        ((health - 66.0) / 33.0).clamp(0.0, 1.0),
        1.0,
    ]
}

// TaystJK cg_draw.c:7897-7925.
fn style_color(style: u32, alpha: f32) -> [f32; 4] {
    match style {
        1 | 5 => [0.0, 0.0, 1.0, alpha],
        2 | 6 | 7 => [1.0, 1.0, 0.0, alpha],
        3 | 4 => [1.0, 0.0, 0.0, alpha],
        _ => [1.0, 1.0, 1.0, alpha],
    }
}
