//! Shared compatibility parser for retail and community `.sab` definitions.
//!
//! `WP_SetSaber` fills `saber_t` from `ext_data/sabers/*.sab`; both visible
//! hilt presentation and the codemp saber-hum loop must therefore consume the
//! same override-aware definition catalog.

use crate::catalog_tokens::tokenize;
use crate::player_profile::SaberColor;
use jkr_protocol::GameState;
use jkr_vfs::VirtualFileSystem;
use std::collections::BTreeMap;
use std::error::Error;

/// Compatibility fields consumed by current presentation and audio adapters.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacySaberDefinition {
    /// Definition token referenced by clientinfo `st`/`st2`.
    pub name: String,
    /// Menu-facing name from the definition's `name` keyword.
    pub display_name: String,
    /// Geometry family used by the multiplayer hilt chooser.
    pub saber_type: LegacySaberType,
    /// Hilt model selected by `saberModel`.
    pub model: String,
    /// Primary blade length in legacy units.
    pub length: f32,
    /// Primary blade render radius.
    pub radius: f32,
    /// Hum asset selected by `soundLoop`.
    pub sound_loop: String,
    /// Optional model animation spin-sound override.
    pub sound_spin: Option<String>,
    /// Three authored animation swing variants (`swingSound1` through `3`).
    pub sound_swing: [Option<String>; 3],
    /// Number of active blade records (one through the codemp maximum of eight).
    pub num_blades: u8,
    /// Per-blade maximum lengths after the global and numbered overrides.
    pub blade_lengths: [f32; 8],
    /// Per-blade render radii after the global and numbered overrides.
    pub blade_radii: [f32; 8],
    /// First blade index that uses the secondary presentation style.
    pub blade_style2_start: u8,
    /// Primary blade trail style: normal, sword blur, or disabled.
    pub trail_style: u8,
    /// Secondary blade trail style selected by `blade_style2_start`.
    pub trail_style2: u8,
    /// Authored `noDlight` disables this hilt's presentation light.
    pub no_dlight: bool,
    /// Suppress world-contact marks and sparks (`noWallMarks`).
    pub no_wall_marks: bool,
    /// Definition-default blade colour.
    pub default_color: LegacySaberColor,
    /// Whether the raw `notInMP` keyword excludes this definition from UI lists.
    pub not_in_mp: bool,
    /// Whether `noRolls 1` sets `SFL_NO_ROLLS` for this hilt.
    pub no_rolls: bool,
    /// Authored BG_AdjustClientSpeed multiplier, defaulting to one.
    pub move_speed_scale: f32,
    /// `animSpeedScale`: a saber attack's animation speed (`BG_SaberStartTransAnim`).
    pub anim_speed_scale: f32,
    /// Generic retail icon selected for this hilt family.
    pub icon: String,
}

/// Saber geometry families understood by BaseJKA definitions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacySaberType {
    /// A one-handed hilt (`SABER_SINGLE`).
    Single,
    /// A two-handed staff hilt (`SABER_STAFF`).
    Staff,
    /// A gameplay-defined extended family retained for community compatibility.
    Other(String),
}

/// A fixed or definition-randomized saber colour.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacySaberColor {
    /// One of the six `saber_colors_t` values.
    Fixed(SaberColor),
    /// `saberColor random`, chosen by gameplay when instantiated.
    Random,
}

/// Load the visible VFS union so mod PK3 definitions override retail files.
pub fn legacy_saber_definitions(
    vfs: &VirtualFileSystem,
) -> Result<BTreeMap<String, LegacySaberDefinition>, Box<dyn Error>> {
    let mut definitions = BTreeMap::new();
    for path in vfs.paths().into_iter().filter(|path| {
        path.as_str().starts_with("ext_data/sabers/") && path.as_str().ends_with(".sab")
    }) {
        let Some(asset) = vfs.read(path.as_str())? else {
            continue;
        };
        for definition in parse(&String::from_utf8_lossy(&asset.bytes)) {
            definitions.insert(definition.name.to_ascii_lowercase(), definition);
        }
    }
    Ok(definitions)
}

/// Resolve `SFL_NO_ROLLS` across both equipped sabers for one client.
///
/// `PM_TryRoll` rejects either flagged hilt (`bg_pmove.c:3594-3607`); the
/// keyword is parsed by `Saber_ParseNoRolls` (`bg_saberLoad.c:1245-1252`).
pub fn legacy_sabers_forbid_rolls(
    vfs: &VirtualFileSystem,
    game_state: &GameState,
    client_num: u16,
) -> bool {
    legacy_saber_movement(vfs, game_state, client_num).0
}

/// Resolve roll restrictions and both hands' moveSpeedScale and animSpeedScale values
/// off the frame path: BG_AdjustClientSpeed, bg_pmove.c:8505-8517;
/// BG_SaberStartTransAnim, bg_panimate.c:2693-2711.
pub fn legacy_saber_movement(
    vfs: &VirtualFileSystem,
    game_state: &GameState,
    client_num: u16,
) -> (bool, [f32; 2], [f32; 2]) {
    let names = crate::player_identity::legacy_client_saber_names(game_state, client_num);
    let Ok(definitions) = legacy_saber_definitions(vfs) else {
        return (false, [1.0; 2], [1.0; 2]);
    };
    let mut no_rolls = false;
    let mut scales = [1.0; 2];
    let mut anim_scales = [1.0; 2];
    for (index, name) in names.into_iter().enumerate() {
        if let Some(definition) = name.and_then(|name| definitions.get(&name.to_ascii_lowercase()))
        {
            no_rolls |= definition.no_rolls;
            scales[index] = definition.move_speed_scale;
            anim_scales[index] = definition.anim_speed_scale;
        }
    }
    (no_rolls, scales, anim_scales)
}

fn parse(source: &str) -> Vec<LegacySaberDefinition> {
    let tokens = tokenize(source);
    let mut definitions = Vec::new();
    let mut index = 0;
    while index + 1 < tokens.len() {
        let name = tokens[index].clone();
        index += 1;
        if tokens.get(index).map(String::as_str) != Some("{") {
            index += 1;
            continue;
        }
        index += 1;
        // `WP_SaberSetDefaults`, bg_saberLoad.c:402-420.
        let mut display_name = "lightsaber".to_owned();
        let mut saber_type = LegacySaberType::Single;
        let mut model = "models/weapons2/saber/saber_w.glm".to_owned();
        let mut blade_lengths = [32.0_f32; 8];
        let mut blade_radii = [3.0_f32; 8];
        let mut sound_spin = None;
        let mut sound_swing = [None, None, None];
        let mut sound_loop = "sound/weapons/saber/saberhum3.wav".to_owned();
        let mut num_blades = 1_u8;
        let mut blade_style2_start = 0_u8;
        let mut trail_style = 0_u8;
        let mut trail_style2 = 0_u8;
        let mut no_dlight = false;
        let mut no_wall_marks = false;
        let mut default_color = LegacySaberColor::Fixed(SaberColor::Red);
        let mut not_in_mp = false;
        let mut no_rolls = false;
        let mut move_speed_scale = 1.0;
        let mut anim_speed_scale = 1.0;
        while let Some(key) = tokens.get(index) {
            if key == "}" {
                index += 1;
                break;
            }
            let Some(value) = tokens.get(index + 1) else {
                break;
            };
            match key.to_ascii_lowercase().as_str() {
                "name" => display_name.clone_from(value),
                "sabertype" => saber_type = parse_saber_type(value),
                "sabermodel" => model.clone_from(value),
                "numblades" => {
                    if let Ok(value) = value.parse::<u8>() {
                        if (1..=8).contains(&value) {
                            num_blades = value;
                        }
                    }
                }
                "sabercolor" => default_color = parse_color(value),
                "saberlength" => {
                    let length = value.parse::<f32>().unwrap_or(blade_lengths[0]).max(4.0);
                    blade_lengths.fill(length);
                }
                "saberradius" => {
                    let radius = value.parse::<f32>().unwrap_or(blade_radii[0]);
                    blade_radii.fill(radius.max(0.25));
                }
                "bladestyle2start" => {
                    blade_style2_start = value.parse::<u8>().unwrap_or(0).min(8);
                }
                "trailstyle" => trail_style = value.parse::<u8>().unwrap_or(0),
                "trailstyle2" => trail_style2 = value.parse::<u8>().unwrap_or(0),
                "nowallmarks" => no_wall_marks |= value.parse::<i32>().unwrap_or(0) != 0,
                "nodlight" => no_dlight |= value.parse::<i32>().unwrap_or(0) != 0,
                "soundloop" => sound_loop.clone_from(value),
                "spinsound" => sound_spin = Some(value.to_ascii_lowercase()),
                "swingsound1" => sound_swing[0] = Some(value.to_ascii_lowercase()),
                "swingsound2" => sound_swing[1] = Some(value.to_ascii_lowercase()),
                "swingsound3" => sound_swing[2] = Some(value.to_ascii_lowercase()),
                "notinmp" => not_in_mp = value.parse::<i32>().unwrap_or(0) != 0,
                // `Saber_ParseNoRolls`, codemp/game/bg_saberLoad.c:1245-1252.
                "norolls" => no_rolls = value.parse::<i32>().unwrap_or(0) != 0,
                "movespeedscale" => move_speed_scale = value.parse::<f32>().unwrap_or(1.0),
                "animspeedscale" => anim_speed_scale = value.parse::<f32>().unwrap_or(1.0),
                numbered if numbered.starts_with("saberlength") => {
                    if let Ok(blade) = numbered[11..].parse::<usize>() {
                        if (2..=8).contains(&blade) {
                            blade_lengths[blade - 1] = value
                                .parse::<f32>()
                                .unwrap_or(blade_lengths[blade - 1])
                                .max(4.0);
                        }
                    }
                }
                numbered if numbered.starts_with("saberradius") => {
                    if let Ok(blade) = numbered[11..].parse::<usize>() {
                        if (2..=8).contains(&blade) {
                            blade_radii[blade - 1] = value
                                .parse::<f32>()
                                .unwrap_or(blade_radii[blade - 1])
                                .max(0.25);
                        }
                    }
                }
                _ => {}
            }
            index += 2;
        }
        definitions.push(LegacySaberDefinition {
            name,
            display_name,
            icon: icon_for_type(&saber_type).to_owned(),
            saber_type,
            model,
            length: blade_lengths[0],
            radius: blade_radii[0],
            sound_loop,
            sound_spin,
            sound_swing,
            num_blades,
            blade_lengths,
            blade_radii,
            blade_style2_start,
            trail_style,
            trail_style2,
            no_dlight,
            no_wall_marks,
            default_color,
            not_in_mp,
            no_rolls,
            move_speed_scale,
            anim_speed_scale,
        });
    }
    definitions
}

fn parse_saber_type(value: &str) -> LegacySaberType {
    match value.to_ascii_lowercase().as_str() {
        "saber_single" => LegacySaberType::Single,
        "saber_staff" => LegacySaberType::Staff,
        _ => LegacySaberType::Other(value.to_owned()),
    }
}

fn parse_color(value: &str) -> LegacySaberColor {
    if value.eq_ignore_ascii_case("random") {
        return LegacySaberColor::Random;
    }
    let color = match value.to_ascii_lowercase().as_str() {
        "red" => SaberColor::Red,
        "orange" => SaberColor::Orange,
        "yellow" => SaberColor::Yellow,
        "green" => SaberColor::Green,
        "purple" => SaberColor::Purple,
        _ => SaberColor::Blue,
    };
    LegacySaberColor::Fixed(color)
}

fn icon_for_type(saber_type: &LegacySaberType) -> &'static str {
    match saber_type {
        LegacySaberType::Staff => "gfx/hud/w_icon_saberstaff.tga",
        LegacySaberType::Single | LegacySaberType::Other(_) => "gfx/hud/w_icon_lightsaber.tga",
    }
}
