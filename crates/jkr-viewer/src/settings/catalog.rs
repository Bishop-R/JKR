//! Declarative settings catalog shared by the settings screen.
//!

//! Every row here is backed by a consumer: a cvar that nothing reads yet is
//! not offered. Player identity
//! (name, model, sabers) lives in the Player menu, not here.

pub(super) const TABS: [&str; 7] = [
    "VIDEO", "AUDIO", "HUD", "CONTROLS", "GAME", "NETWORK", "HUD+",
];
/// The tab whose last row opens the key-binding editor.
pub(super) const KEYBINDS_TAB: usize = 3;
/// The tab whose last row opens the renderer settings ([`RENDERER_TABS`]).
pub(super) const RENDERER_TAB: usize = 0;
/// Tabs of the renderer settings, JKR's own `jkr_*` rendering cvars, reached
/// from the last row of [`VIDEO`] as JoF EJK reaches its advanced renderer page
/// from Video.
pub(super) const RENDERER_TABS: [&str; 3] = ["IMAGE", "LIGHTING", "SHADOWS"];

#[derive(Clone, Copy)]
pub(super) enum ValueKind {
    Bool,
    Integer { min: i64, max: i64, step: i64 },
    Float { min: f64, max: f64, step: f64 },
    Choice(&'static [&'static str]),
    Text,
}

#[derive(Clone, Copy)]
pub(super) struct Setting {
    pub(super) label: &'static str,
    pub(super) cvar: &'static str,
    pub(super) kind: ValueKind,
}

pub(crate) const RESOLUTIONS: &[&str] = &[
    "1280x720",
    "1600x900",
    "1920x1080",
    "2560x1440",
    "3840x2160",
];
pub(super) const VIDEO: &[Setting] = &[
    Setting {
        label: "Resolution",
        cvar: "r_resolution",
        kind: ValueKind::Choice(RESOLUTIONS),
    },
    Setting {
        label: "Display mode",
        cvar: "r_fullscreen",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Vertical sync",
        cvar: "r_vsync",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "FPS cap (0 = uncapped)",
        cvar: "com_maxfps",
        kind: ValueKind::Integer {
            min: 0,
            max: 2000,
            step: 25,
        },
    },
    Setting {
        label: "Field of view",
        cvar: "cg_fov",
        kind: ValueKind::Float {
            min: 70.0,
            max: 130.0,
            step: 5.0,
        },
    },
    Setting {
        label: "Impact marks",
        cvar: "cg_marks",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Player shadows",
        cvar: "cg_shadows",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "First-person weapon",
        cvar: "cg_drawGun",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "FPS / frame-time readout",
        cvar: "cg_drawFps",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Display gamma",
        cvar: "r_gamma",
        kind: ValueKind::Float {
            min: 0.5,
            max: 3.0,
            step: 0.1,
        },
    },
];
pub(super) const AUDIO: &[Setting] = &[
    Setting {
        label: "Effects volume",
        cvar: "s_volume",
        kind: ValueKind::Float {
            min: 0.0,
            max: 1.0,
            step: 0.05,
        },
    },
    Setting {
        label: "Music volume",
        cvar: "s_musicVolume",
        kind: ValueKind::Float {
            min: 0.0,
            max: 1.0,
            step: 0.05,
        },
    },
    Setting {
        label: "Voice volume",
        cvar: "s_volumeVoice",
        kind: ValueKind::Float {
            min: 0.0,
            max: 1.0,
            step: 0.05,
        },
    },
    Setting {
        label: "Doppler",
        cvar: "s_doppler",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Footsteps",
        cvar: "cg_footsteps",
        kind: ValueKind::Bool,
    },
];
pub(super) const HUD_OPTIONS: &[Setting] = &[
    Setting {
        label: "Crosshair size",
        cvar: "cg_crosshairSize",
        kind: ValueKind::Float {
            min: 0.0,
            max: 96.0,
            step: 4.0,
        },
    },
    Setting {
        label: "Team status (0/1)",
        cvar: "cg_drawTeamOverlay",
        kind: ValueKind::Integer {
            min: 0,
            max: 1,
            step: 1,
        },
    },
    Setting {
        label: "Speed readout (0/1)",
        cvar: "cg_speedometer",
        kind: ValueKind::Integer {
            min: 0,
            max: 1,
            step: 1,
        },
    },
];

pub(super) const HUD: &[Setting] = &[
    Setting {
        label: "HUD",
        cvar: "cg_drawHud",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "HUD scale",
        cvar: "cg_hudScale",
        kind: ValueKind::Float {
            min: 0.5,
            max: 1.5,
            step: 0.05,
        },
    },
    Setting {
        label: "Status (health / armour / force)",
        cvar: "cg_drawStatus",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Weapon bar",
        cvar: "cg_drawWeapon",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Crosshair",
        cvar: "cg_crosshair",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Crosshair names",
        cvar: "cg_drawCrosshairNames",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Match timer",
        cvar: "cg_drawTimer",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Lagometer",
        cvar: "cg_lagometer",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Chat",
        cvar: "cg_drawChat",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Classic HUD font",
        cvar: "cg_classicHudFont",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Ground HUD (third person)",
        cvar: crate::ground_hud::CVAR,
        kind: ValueKind::Bool,
    },
];
pub(super) const CONTROLS: &[Setting] = &[
    Setting {
        label: "Mouse sensitivity",
        cvar: "sensitivity",
        kind: ValueKind::Float {
            min: 0.1,
            max: 20.0,
            step: 0.25,
        },
    },
    Setting {
        label: "Invert mouse",
        cvar: "m_invert",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Raw mouse input",
        cvar: "in_raw",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Always run",
        cvar: "cl_run",
        kind: ValueKind::Bool,
    },
];
pub(super) const GAME: &[Setting] = &[
    Setting {
        label: "Simple pickup icons",
        cvar: "cg_simpleItems",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Force my player model",
        cvar: "cg_forceModel",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Saber trail",
        cvar: "cg_saberTrail",
        kind: ValueKind::Choice(&["0", "1", "2"]),
    },
    Setting {
        label: "Force Seeing aura",
        cvar: "cg_auraShell",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Third-person camera damping",
        cvar: "cg_thirdPersonCameraDamp",
        kind: ValueKind::Float {
            min: 0.0,
            max: 1.0,
            step: 0.05,
        },
    },
    Setting {
        label: "Third-person target damping",
        cvar: "cg_thirdPersonTargetDamp",
        kind: ValueKind::Float {
            min: 0.0,
            max: 1.0,
            step: 0.05,
        },
    },
    Setting {
        label: "Prediction error smoothing (ms)",
        cvar: "cg_errorDecay",
        kind: ValueKind::Float {
            min: 0.0,
            max: 500.0,
            step: 25.0,
        },
    },
    Setting {
        label: "Menu accent",
        cvar: "ui_accent",
        kind: ValueKind::Choice(&["ember", "amber", "blue", "green", "violet", "neutral"]),
    },
];
pub(super) const NETWORK: &[Setting] = &[
    Setting {
        label: "Master server",
        cvar: "cl_master",
        kind: ValueKind::Text,
    },
    Setting {
        label: "Rate",
        cvar: "rate",
        kind: ValueKind::Integer {
            min: 1000,
            max: 100000,
            step: 1000,
        },
    },
    Setting {
        label: "Snapshot rate",
        cvar: "snaps",
        kind: ValueKind::Integer {
            min: 10,
            max: 60,
            step: 5,
        },
    },
];

// Renderer settings. "(restart)" marks cvars the renderer reads only at startup
// (their registrations print a restart notice on change); "(next map)" those read
// when a map loads. The rest apply immediately. Ranges follow each consumer's clamp.

pub(super) const RENDER_IMAGE: &[Setting] = &[
    Setting {
        label: "HDR scene (restart)",
        cvar: "jkr_hdr",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "HDR exposure (restart)",
        cvar: "jkr_hdrExposure",
        kind: ValueKind::Float {
            min: 0.25,
            max: 4.0,
            step: 0.05,
        },
    },
    Setting {
        label: "Filmic tone curve",
        cvar: "jkr_tonemap",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Bloom",
        cvar: "jkr_bloom",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "FXAA (restart)",
        cvar: "jkr_fxaa",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Supersampling, 1 off (restart)",
        cvar: "jkr_renderScale",
        kind: ValueKind::Integer {
            min: 1,
            max: 3,
            step: 1,
        },
    },
    Setting {
        label: "Soft particles",
        cvar: "jkr_softParticles",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Per-pixel model lighting",
        cvar: "jkr_modelDiffusePixels",
        kind: ValueKind::Bool,
    },
];

pub(super) const RENDER_LIGHTING: &[Setting] = &[
    Setting {
        label: "Sun and sky (restart)",
        cvar: "jkr_dayNight",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Live lighting 0-2 (next map)",
        cvar: "jkr_realtime",
        kind: ValueKind::Integer {
            min: 0,
            max: 2,
            step: 1,
        },
    },
    Setting {
        label: "Time of day (hour)",
        cvar: "jkr_dayHour",
        kind: ValueKind::Float {
            min: 0.0,
            max: 24.0,
            step: 0.5,
        },
    },
    Setting {
        label: "Day length, min (0 holds)",
        cvar: "jkr_dayMinutes",
        kind: ValueKind::Float {
            min: 0.0,
            max: 1440.0,
            step: 5.0,
        },
    },
    Setting {
        label: "Sunlight brightness",
        cvar: "jkr_dayBrightness",
        kind: ValueKind::Float {
            min: 0.1,
            max: 10.0,
            step: 0.1,
        },
    },
    Setting {
        label: "Ambient fill",
        cvar: "jkr_ambientFill",
        kind: ValueKind::Float {
            min: 0.0,
            max: 0.2,
            step: 0.005,
        },
    },
    Setting {
        label: "Ambient fill corner shading",
        cvar: "jkr_ambientFillOcclusion",
        kind: ValueKind::Float {
            min: 0.0,
            max: 1.0,
            step: 0.05,
        },
    },
    Setting {
        label: "Indirect light boost",
        cvar: "jkr_indirectBoost",
        kind: ValueKind::Float {
            min: 0.0,
            max: 4.0,
            step: 0.1,
        },
    },
    Setting {
        label: "Light shafts 0-3 (restart)",
        cvar: "jkr_volumetrics",
        kind: ValueKind::Integer {
            min: 0,
            max: 3,
            step: 1,
        },
    },
    Setting {
        label: "Light shaft clarity",
        cvar: "jkr_volumetricClarity",
        kind: ValueKind::Float {
            min: 0.0,
            max: 1.0,
            step: 0.05,
        },
    },
];

pub(super) const RENDER_SHADOWS: &[Setting] = &[
    Setting {
        label: "World sun shadows (restart)",
        cvar: "jkr_worldSunShadows",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Character sun shadows (restart)",
        cvar: "jkr_sunShadows",
        kind: ValueKind::Bool,
    },
    Setting {
        label: "Shadow resolution (restart)",
        cvar: "jkr_shadowResolution",
        kind: ValueKind::Integer {
            min: 512,
            max: 4096,
            step: 512,
        },
    },
    Setting {
        label: "Sharp shadow distance (restart)",
        cvar: "jkr_shadowDistance",
        kind: ValueKind::Integer {
            min: 128,
            max: 4096,
            step: 128,
        },
    },
    Setting {
        label: "Close shadow distance (restart)",
        cvar: "jkr_shadowNear",
        kind: ValueKind::Integer {
            min: 64,
            max: 1024,
            step: 64,
        },
    },
    Setting {
        label: "Shadow filter taps (restart)",
        cvar: "jkr_shadowTaps",
        kind: ValueKind::Integer {
            min: 4,
            max: 32,
            step: 4,
        },
    },
    Setting {
        label: "Close shadow slits (units)",
        cvar: "jkr_shadowGapClose",
        kind: ValueKind::Float {
            min: 0.0,
            max: 64.0,
            step: 1.0,
        },
    },
    Setting {
        label: "Contact shadows",
        cvar: "jkr_contactShadows",
        kind: ValueKind::Bool,
    },
];
