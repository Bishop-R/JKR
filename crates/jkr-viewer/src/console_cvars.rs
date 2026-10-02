//! Daily-use cvar definitions kept separate from console interaction logic.

use jkr_shell::{CvarDefinition, CvarFlags, CvarRegistry};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

/// Change-callback cache for integer values; frame reads never look up a name.
pub(super) struct IntegerSetting(Arc<AtomicI64>);

impl IntegerSetting {
    /// Bind before config loading so archived values apply.
    pub(super) fn bind(
        cvars: &mut CvarRegistry,
        name: &str,
        default: i64,
    ) -> Result<Self, jkr_shell::CvarError> {
        let value = Arc::new(AtomicI64::new(default));
        let changed = Arc::clone(&value);
        cvars.on_change(name, move |change| {
            if let jkr_shell::CvarValue::Integer(value) = change.current {
                changed.store(value, Ordering::Relaxed);
            }
        })?;
        Ok(Self(value))
    }

    /// Read the nonzero state without allocation or locking.
    pub(super) fn enabled(&self) -> bool {
        self.value() != 0
    }

    /// Read the integer value without allocation or locking.
    pub(super) fn value(&self) -> i64 {
        self.0.load(Ordering::Relaxed)
    }
}

pub(super) fn register_daily_cvars(cvars: &mut CvarRegistry) -> Result<(), jkr_shell::CvarError> {
    crate::frame_target::aa::register(cvars)?;
    crate::frame_target::scale::register(cvars)?;
    crate::world_materials::filtering::register(cvars)?;
    crate::assets::search_paths::register(cvars)?;
    super::client_options::register(cvars)?;
    super::ui_options::register(cvars)?;
    crate::hud::icons::register(cvars)?;
    let archive = CvarFlags::ARCHIVE;
    let definitions = [
        CvarDefinition::new(
            "cg_forceEnemyModel",
            "none",
            archive,
            "Enemy model with cg_forceModel",
        ),
        CvarDefinition::new(
            "cg_forceAllyModel",
            "none",
            archive,
            "Ally model with cg_forceModel",
        ),
        CvarDefinition::new(
            "cg_forceOwnSaber",
            "none",
            CvarFlags::NONE,
            "Local presentation hilt override(s)",
        ),
        CvarDefinition::new(
            "cg_scorePlums",
            1_i64,
            archive,
            "Server-authored floating numbers",
        ),
        CvarDefinition::new(
            "cg_killMessage",
            1_i64,
            archive,
            "Personal kill announcement mode",
        ),
        CvarDefinition::new(
            "cg_duelSounds",
            1_i64,
            archive,
            "Duel: 0 off, 1 both, 2 sound, 3 text",
        ),
        CvarDefinition::new(
            "cg_scoreDeaths",
            1_i64,
            archive,
            "Deaths: 1 plugin, 2/3 observed",
        ),
        CvarDefinition::new(
            "cg_hitsounds",
            0_i64,
            archive,
            "Hit cues 1-4; saber variants 5-6",
        ),
        CvarDefinition::new(
            "cg_oldPainSounds",
            false,
            archive,
            "Local pain from health loss",
        ),
        CvarDefinition::new(
            "cg_killSounds",
            2_i64,
            archive,
            "Kill cue: 0 off, 1 normal, 2 midair",
        ),
        CvarDefinition::new(
            "cg_jumpSounds",
            0_i64,
            archive,
            "Jump voices: 0 off, 1 all, 2 others, 3 self",
        ),
        CvarDefinition::new(
            "cg_rollSounds",
            1_i64,
            archive,
            "Roll voices: 0 off, 1 all, 2 others, 3 self",
        ),
        CvarDefinition::new(
            "cl_allowDownload",
            1_i64,
            archive,
            "Allow verified UDP pak downloads",
        ),
        CvarDefinition::new("developer", 0_i64, CvarFlags::NONE, "Debug server commands"),
        CvarDefinition::new(
            "pmove_fixed",
            0_i64,
            CvarFlags::READ_ONLY,
            "Server fixed-step movement",
        ),
        CvarDefinition::new(
            "pmove_msec",
            8_i64,
            CvarFlags::READ_ONLY,
            "Server movement step (8..33 ms)",
        ),
        CvarDefinition::new(
            "pmove_float",
            0_i64,
            CvarFlags::READ_ONLY,
            "Server unsnapped velocity",
        ),
        CvarDefinition::new(
            "g_stepSlideFix",
            0_i64,
            CvarFlags::READ_ONLY,
            "Server step-slide correction",
        ),
        CvarDefinition::new("r_resolution", "1280x720", archive, "Window resolution"),
        CvarDefinition::new("r_fullscreen", false, archive, "Borderless fullscreen"),
        CvarDefinition::new(
            "r_swapInterval",
            false,
            archive,
            "Synchronize presentation (0/1)",
        ),
        CvarDefinition::new(
            "ui_accent",
            "ember",
            archive,
            "Menu accent: ember, amber, blue, green, violet, neutral, or RRGGBB hex",
        ),
        CvarDefinition::new(
            "r_gamma",
            1.0_f64,
            archive,
            "Display gamma (0.5–3.0, neutral 1); applies immediately, including HUD",
        ),
        CvarDefinition::new("cg_fov", 90.0_f64, archive, "Horizontal field of view"),
        CvarDefinition::new(
            "cg_fovAspectAdjust",
            true,
            archive,
            "Widen the field of view for widescreen displays",
        ),
        CvarDefinition::new(
            "cl_showTimeDelta",
            // Default off: this was raised to 1 during the step-74 netcode investigation and
            // never lowered, so every connected session spammed the owner's console with a
            // `nettiming:` line every five seconds. Set it to 1 to bring the windows back.
            0_i64,
            CvarFlags::NONE,
            "Log aggregate snapshot and prediction timing every five seconds while connected",
        ),
        CvarDefinition::new(
            "cg_smoothClients",
            0_i64,
            archive,
            "Extrapolate remote players and NPCs from snapshot velocity",
        ),
        CvarDefinition::new(
            "cg_errorDecay",
            100.0_f64,
            archive,
            "Milliseconds over which a prediction miss is smoothed out of the view; 0 snaps",
        ),
        CvarDefinition::new(
            "cg_hudScale",
            0.7_f64,
            archive,
            "Size multiplier for the modern HUD widgets (0.25 to 2)",
        ),
        CvarDefinition::new(
            "cg_thirdPersonRange",
            80.0,
            archive,
            "Third-person camera distance behind the player",
        ),
        CvarDefinition::new(
            "cg_thirdPersonVertOffset",
            16.0,
            archive,
            "Third-person camera height above the player",
        ),
        CvarDefinition::new(
            "cg_thirdPersonAngle",
            0.0,
            archive,
            "Third-person camera yaw offset in degrees",
        ),
        CvarDefinition::new(
            "cg_thirdPersonPitchOffset",
            0.0,
            archive,
            "Third-person camera pitch offset in degrees",
        ),
        CvarDefinition::new(
            "cg_thirdPersonCameraDamp",
            0.3_f64,
            archive,
            "Third-person camera easing per 50 ms; 1 or more snaps to the ideal position",
        ),
        CvarDefinition::new(
            "cg_thirdPersonTargetDamp",
            0.5_f64,
            archive,
            "Third-person look-target easing per 50 ms; 1 or more snaps",
        ),
        CvarDefinition::new(
            "com_maxfps",
            1000_i64,
            archive,
            "Maximum rendered frames per second; zero is uncapped",
        ),
        CvarDefinition::new("cg_drawFPS", false, archive, "Display FPS and frame time"),
        CvarDefinition::new(
            "cg_classicHudFont",
            false,
            archive,
            "Use the retail bitmap font for the in-game status HUD only",
        ),
        CvarDefinition::new(
            crate::ground_hud::CVAR,
            false,
            archive,
            "Third-person ground HUD: health, shield, Force and stance around the feet",
        ),
        // Retail names and defaults: OpenJK codemp client/snd_dma.cpp S_Init
        // (s_volume "0.5", s_musicvolume "0.25", s_volumeVoice "1.0",
        // s_khz "44"); the retail sound menu labels s_volume "Effects Volume".
        CvarDefinition::new("s_volume", 0.5_f64, archive, "Effects volume"),
        CvarDefinition::new("s_musicVolume", 0.25_f64, archive, "Music volume"),
        CvarDefinition::new(
            "s_volumeVoice",
            1.0_f64,
            archive,
            "Saved voice gain; mixer bus integration pending",
        ),
        CvarDefinition::new(
            "s_khz",
            44_i64,
            archive,
            "Retail sample-rate preset (11/22/44); JKR always mixes at 44.1 kHz",
        ),
        CvarDefinition::new("s_doppler", true, archive, "Legacy looping-sound Doppler"),
        CvarDefinition::new("cg_footsteps", true, archive, "Play footstep sounds"),
        // Stock defaults and units: `sensitivity` 5 scaling `m_yaw`/`m_pitch`
        // degrees per mouse count (`cl_main.cpp:2774,2802-2803`, applied in
        // `cl_input.cpp:1161-1180`). Matching them is what lets a player carry
        // a sensitivity value over from JKA and keep their aim.
        CvarDefinition::new("sensitivity", 5.0_f64, archive, "Mouse sensitivity"),
        CvarDefinition::new(
            "m_yaw",
            0.022_f64,
            archive,
            "Degrees turned per horizontal mouse count",
        ),
        CvarDefinition::new(
            "m_pitch",
            0.022_f64,
            archive,
            "Degrees turned per vertical mouse count",
        ),
        CvarDefinition::new("m_invert", false, archive, "Invert mouse pitch"),
        CvarDefinition::new("in_raw", true, archive, "Saved raw-input preference"),
        CvarDefinition::new(
            "cl_run",
            true,
            archive,
            "Run by default; the walk key walks",
        ),
        // Retail defaults: `codemp/cgame/cg_xcvar.h` cg_marks 1, cg_shadows 1,
        // cg_drawGun 1.
        CvarDefinition::new(
            "cg_saberContact",
            true,
            archive,
            "Saber contact marks and sparks",
        ),
        CvarDefinition::new("cg_marks", true, archive, "Leave impact marks on the world"),
        CvarDefinition::new("cg_shadows", true, archive, "Draw player shadows"),
        CvarDefinition::new("cg_drawGun", true, archive, "Draw the first-person weapon"),
        CvarDefinition::new("cg_drawCrosshair", true, archive, "Display crosshair"),
        // Stock defaults from `codemp/cgame/cg_xcvar.h:60,72,102`.
        CvarDefinition::new(
            "cg_drawCrosshairNames",
            true,
            archive,
            "Display the name under the crosshair",
        ),
        CvarDefinition::new("cg_drawTimer", false, archive, "Display elapsed match time"),
        CvarDefinition::new(
            "cg_lagometer",
            false,
            archive,
            "Display frame interpolation and snapshot latency samples",
        ),
        CvarDefinition::new(
            "cg_saberTrail",
            1_i64,
            archive,
            "Saber motion trails; mode 2 currently uses the normal trail",
        ),
        // Stock default "1" (`codemp/cgame/cg_xcvar.h`).
        CvarDefinition::new(
            "cg_speedTrail",
            1_i64,
            archive,
            "Draw Force Speed afterimages behind players",
        ),
        CvarDefinition::new(
            "cg_auraShell",
            1_i64,
            archive,
            "Display the Force Seeing aura shell on other players",
        ),
        CvarDefinition::new(
            "cl_maxpackets",
            63_i64,
            archive,
            "Saved packet-rate preference; scheduler integration pending",
        ),
        // Stock default "1", clamped 0..5 (`codemp/client/cl_input.cpp:1539`).
        CvarDefinition::new(
            "cl_packetdup",
            1_i64,
            archive,
            "Repeat the previous N usercmds in every packet (0 to 5) so lost packets drop no move",
        ),
        CvarDefinition::new(
            "cl_timeNudge",
            0_i64,
            archive,
            "Live presentation latency in milliseconds (-30 to 30); positive adds buffer",
        ),
        CvarDefinition::new("cg_drawHud", true, archive, "Display the in-game HUD"),
        CvarDefinition::new("cg_draw2D", true, archive, "Display 2D game overlays"),
        CvarDefinition::new(
            "cg_simpleItems",
            false,
            archive,
            "Draw pickup icons instead of models",
        ),
        CvarDefinition::new(
            "cg_forceModel",
            false,
            archive,
            "Use your model for other players",
        ),
        CvarDefinition::new(
            "cg_crosshairSize",
            24.0_f64,
            archive,
            "Crosshair size in 640x480 units",
        ),
        CvarDefinition::new(
            "cg_drawTeamOverlay",
            0_i64,
            archive,
            "Team status: 0 off, 1 on",
        ),
        CvarDefinition::new(
            "cg_speedometer",
            0_i64,
            archive,
            "Speed bits: 1 on, 256 kph, 512 mph, 32768 XYZ",
        ),
        CvarDefinition::new(
            "cl_consoleKeys",
            // `cl_main.cpp:2826` ships "~ ` 0x7e 0x60 0xb2"; 0xb0 (the shifted
            // German key) is added so shift+that key opens the console there.
            "~ ` 0x7e 0x60 0xb2 0xb0",
            archive,
            "Characters that toggle the console",
        ),
        CvarDefinition::new(
            "cg_drawStatus",
            true,
            archive,
            "Display health, armor, and force status",
        ),
        CvarDefinition::new(
            "cg_drawWeapon",
            true,
            archive,
            "Display current weapon and ammunition",
        ),
        CvarDefinition::new(
            "cg_drawScores",
            true,
            archive,
            "Display the bound scoreboard",
        ),
        CvarDefinition::new(
            "cg_drawChat",
            true,
            archive,
            "Display incoming chat and prints",
        ),
        CvarDefinition::new(
            "jkr_sensitivityScaleVersion",
            0_i64,
            archive,
            "Internal migration marker for the move to stock sensitivity units",
        ),
        CvarDefinition::new(
            "jkr_bindDefaultsVersion",
            0_i64,
            archive,
            "Internal stock-bind migration version",
        ),
    ];
    for definition in definitions {
        cvars.register(definition)?;
    }
    cvars.register_alias("r_vsync", "r_swapInterval")?;
    crate::cgame_options::register(cvars)
}
