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
    Setting {
        label: "Filmic scene (0 off / 1 on)",
        cvar: "jkr_tonemap",
        kind: ValueKind::Integer {
            min: 0,
            max: 1,
            step: 1,
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
        label: "Sunbeam dust (0 off)",
        cvar: crate::dust_motes::CVAR,
        kind: ValueKind::Float {
            min: 0.0,
            max: 1.0,
            step: 0.1,
        },
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
