//! BaseJKA multiplayer character and multipart-species catalogue.
//!
//! Ordinary model enumeration mirrors `UI_BuildQ3Model_List` in OpenJK
//! `codemp/ui/ui_main.c:9427-9524`: the suffix after the first underscore is
//! the skin name, an icon is required, and duplicate model/skin pairs collapse.
//! Multipart species mirror `UI_BuildPlayerModel_List` at lines 9636-9792 and
//! `UI_ParseColorData` at lines 9561-9607. Model cvar strings use the exact
//! `model/head|torso|lower` format from `UI_UpdateCharacterCvars`, lines
//! 4981-5006.

use crate::catalog_tokens::tokenize;
use jkr_vfs::VirtualFileSystem;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;

/// One selectable ordinary multiplayer model skin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyCharacter {
    /// Directory component below `models/players`.
    pub model: String,
    /// Skin suffix used in the `model` userinfo cvar.
    pub skin: String,
    /// Exact userinfo value (`model/skin`).
    pub cvar_value: String,
    /// Existing icon selected using the retail JPG, PNG, TGA priority.
    pub icon: String,
}

/// One colour choice parsed from a species `playerchoice.txt` block.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacySpeciesColor {
    /// Shader used by the retail menu swatch.
    pub shader: String,
    /// RGB values assigned by the block's `setcvar` actions.
    pub rgb: [u8; 3],
}

/// Customisable multipart player species.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacySpecies {
    /// Directory component below `models/players`.
    pub model: String,
    /// Icon-backed head skin names, including their `head_` prefix.
    pub heads: Vec<String>,
    /// Icon-backed torso skin names, including their `torso_` prefix.
    pub torsos: Vec<String>,
    /// Icon-backed lower-body skin names, including their `lower_` prefix.
    pub legs: Vec<String>,
    /// Colour actions offered by `playerchoice.txt`.
    pub colors: Vec<LegacySpeciesColor>,
}

impl LegacySpecies {
    /// Builds the byte-exact multipart `model` cvar value used by OpenJK UI.
    pub fn cvar_value(&self, head: &str, torso: &str, legs: &str) -> String {
        format!("{}/{head}|{torso}|{legs}", self.model)
    }
}

/// Character-related half of the compatibility asset catalogue.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LegacyCharacterCatalog {
    /// Ordinary model/skin choices.
    pub characters: Vec<LegacyCharacter>,
    /// Multipart customisable species.
    pub species: Vec<LegacySpecies>,
}

/// Enumerates player models from the visible VFS union.
pub fn legacy_character_catalog(
    vfs: &VirtualFileSystem,
) -> Result<LegacyCharacterCatalog, Box<dyn Error + Send + Sync>> {
    let paths = vfs.paths();
    let model_dirs = model_directories(&paths);
    let mut characters = BTreeMap::new();
    let mut species = Vec::new();

    for model in model_dirs {
        let choice = format!("models/players/{model}/playerchoice.txt");
        if let Some(asset) = vfs.read(&choice)? {
            if let Some(parsed) = parse_species(vfs, &model, &asset.bytes, &paths)? {
                species.push(parsed);
            }
        }
        collect_ordinary_skins(vfs, &model, &paths, &mut characters)?;
    }

    Ok(LegacyCharacterCatalog {
        characters: characters.into_values().collect(),
        species,
    })
}

fn model_directories(paths: &[jkr_vfs::VirtualPath]) -> BTreeSet<String> {
    paths
        .iter()
        .filter_map(|path| {
            path.as_str()
                .strip_prefix("models/players/")?
                .strip_suffix("/model.glm")
                .filter(|model| !model.is_empty() && !model.contains('/'))
                .map(str::to_owned)
        })
        .collect()
}

fn collect_ordinary_skins(
    vfs: &VirtualFileSystem,
    model: &str,
    paths: &[jkr_vfs::VirtualPath],
    output: &mut BTreeMap<String, LegacyCharacter>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let prefix = format!("models/players/{model}/");
    for path in paths {
        let Some(file) = path.as_str().strip_prefix(&prefix) else {
            continue;
        };
        let Some(stem) = file.strip_suffix(".skin") else {
            continue;
        };
        let Some((_, skin)) = stem.split_once('_') else {
            continue;
        };
        if skin.is_empty() {
            continue;
        }
        let Some(icon) = icon_path(vfs, model, skin)? else {
            continue;
        };
        let cvar_value = format!("{model}/{skin}");
        output.entry(cvar_value.clone()).or_insert(LegacyCharacter {
            model: model.to_owned(),
            skin: skin.to_owned(),
            cvar_value,
            icon,
        });
    }
    Ok(())
}

fn parse_species(
    vfs: &VirtualFileSystem,
    model: &str,
    bytes: &[u8],
    paths: &[jkr_vfs::VirtualPath],
) -> Result<Option<LegacySpecies>, Box<dyn Error + Send + Sync>> {
    let prefix = format!("models/players/{model}/");
    let mut heads = Vec::new();
    let mut torsos = Vec::new();
    let mut legs = Vec::new();
    for path in paths {
        let Some(stem) = path
            .as_str()
            .strip_prefix(&prefix)
            .and_then(|file| file.strip_suffix(".skin"))
        else {
            continue;
        };
        if icon_path(vfs, model, stem)?.is_none() {
            continue;
        }
        if stem.starts_with("head_") {
            heads.push(stem.to_owned());
        } else if stem.starts_with("torso_") {
            torsos.push(stem.to_owned());
        } else if stem.starts_with("lower_") {
            legs.push(stem.to_owned());
        }
    }
    if heads.is_empty() || torsos.is_empty() || legs.is_empty() {
        return Ok(None);
    }
    Ok(Some(LegacySpecies {
        model: model.to_owned(),
        heads,
        torsos,
        legs,
        colors: parse_colors(&String::from_utf8_lossy(bytes)),
    }))
}

fn parse_colors(source: &str) -> Vec<LegacySpeciesColor> {
    let tokens = tokenize(source);
    let mut colors = Vec::new();
    let mut cursor = 0;
    while cursor + 1 < tokens.len() {
        let shader = tokens[cursor].clone();
        cursor += 1;
        if tokens.get(cursor).map(String::as_str) != Some("{") {
            cursor += 1;
            continue;
        }
        cursor += 1;
        let mut rgb = [255_u8; 3];
        while cursor < tokens.len() && tokens[cursor] != "}" {
            if tokens[cursor].eq_ignore_ascii_case("setcvar") && cursor + 2 < tokens.len() {
                let value = tokens[cursor + 2].parse::<u8>().unwrap_or(255);
                match tokens[cursor + 1].to_ascii_lowercase().as_str() {
                    "ui_char_color_red" => rgb[0] = value,
                    "ui_char_color_green" => rgb[1] = value,
                    "ui_char_color_blue" => rgb[2] = value,
                    _ => {}
                }
                cursor += 3;
            } else {
                cursor += 1;
            }
        }
        cursor += usize::from(cursor < tokens.len());
        colors.push(LegacySpeciesColor { shader, rgb });
    }
    colors
}

fn icon_path(
    vfs: &VirtualFileSystem,
    model: &str,
    skin: &str,
) -> Result<Option<String>, jkr_vfs::VfsError> {
    for extension in ["jpg", "png", "tga"] {
        let path = format!("models/players/{model}/icon_{skin}.{extension}");
        if vfs.contains(&path)? {
            return Ok(Some(path));
        }
    }
    Ok(None)
}
