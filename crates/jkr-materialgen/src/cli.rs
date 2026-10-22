//! Command-line parsing and help text.

use crate::generate::Settings;
use crate::mount::{DEFAULT_FILE_NAME, default_output, find_game_data, is_inside};
use crate::run::{Options, normalize_map};
use std::path::{Path, PathBuf};

/// `--help` output.
pub const USAGE: &str = "\
jkr-materialgen: generate rend2-style material maps for your installed Jedi Academy maps

USAGE:
    jkr-materialgen [OPTIONS]

OPTIONS:
    --game-data DIR   the game's GameData directory (default: JKR_GAME_DATA, the JKR
                      config's fs_gameData, or the usual Steam location)
    --fs-game NAME    also mount GameData/NAME after base, like fs_game
    --maps LIST       only textures of these maps, comma-separated (mp/ffa3,mp/duel1);
                      default: every installed map
    --limit N         only the N most-used textures
    --strength F      multiply every class's normal strength (default 1.0)
    --max-size N      halve textures larger than N texels before generating
                      (default: keep the source resolution)
    --dry-run         list what would be generated and what is skipped; write nothing
    --out FILE        output pk3 (default: <JKR user data>/generated/zzz_jkr_materials.pk3,
                      i.e. %APPDATA%\\jkr\\generated on Windows); never inside GameData
    -h, --help        print this help

For each world texture that installed maps draw on lightmapped surfaces, the tool
writes a normal map (<texture>_nh with height for parallax on stone, tiles and
ground, <texture>_n otherwise) and a packed <texture>_rmo map (roughness,
metalness, occlusion) into one pk3, plus jkr-materialgen/manifest.json listing
every source, output, skipped shader and setting. Skies, fog, liquids, system and
interface images, effects, glowing, animated and alpha-tested foliage stages, and
textures that already have rend2 maps get none. Game data is only read.

To use the maps, either point the client at the output directory
(JKR_CONTENT=<directory>) or copy the pk3 into GameData/base yourself, then enable
r_normalMapping, r_specularMapping and optionally r_parallaxMapping.

The generated images are derived from your retail textures. Keep them on your
machine: do not share, upload or commit them.
";

/// What the command line asks for.
#[derive(Debug)]
pub enum Command {
    Help,
    Run(Options),
}

/// Parse `arguments` (without the program name).
pub fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut arguments = arguments.into_iter();
    let mut game_data = None;
    let mut fs_game = None;
    let mut maps = None;
    let mut limit = None;
    let mut settings = Settings::default();
    let mut dry_run = false;
    let mut out = None;
    while let Some(argument) = arguments.next() {
        let mut value = |name: &str| {
            arguments
                .next()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match argument.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--game-data" => game_data = Some(PathBuf::from(value("--game-data")?)),
            "--fs-game" => fs_game = Some(value("--fs-game")?),
            "--maps" => {
                let list: Vec<String> = value("--maps")?
                    .split(',')
                    .map(str::trim)
                    .filter(|map| !map.is_empty())
                    .map(normalize_map)
                    .collect();
                if list.is_empty() {
                    return Err("--maps needs at least one map name".into());
                }
                maps = Some(list);
            }
            "--limit" => {
                limit = Some(
                    value("--limit")?
                        .parse::<usize>()
                        .map_err(|_| "--limit needs a whole number")?,
                )
            }
            "--strength" => {
                let strength = value("--strength")?
                    .parse::<f32>()
                    .ok()
                    .filter(|s| s.is_finite() && *s > 0.0 && *s <= 10.0)
                    .ok_or("--strength needs a number above 0 and at most 10")?;
                settings.strength = strength;
            }
            "--max-size" => {
                let size = value("--max-size")?
                    .parse::<u32>()
                    .ok()
                    .filter(|size| *size >= 16)
                    .ok_or("--max-size needs a whole number of at least 16")?;
                settings.max_size = Some(size);
            }
            "--dry-run" => dry_run = true,
            "--out" => out = Some(PathBuf::from(value("--out")?)),
            other => return Err(format!("unknown argument {other:?}; see --help")),
        }
    }
    let game_data = find_game_data(game_data.as_deref())?;
    let out = match out {
        Some(out) if out.is_dir() => out.join(DEFAULT_FILE_NAME),
        Some(out) => out,
        None => default_output()?,
    };
    check_output(&out, &game_data)?;
    Ok(Command::Run(Options {
        game_data,
        fs_game,
        maps,
        limit,
        settings,
        dry_run,
        out,
    }))
}

/// The output must be a `.pk3` outside the game installation.
fn check_output(out: &Path, game_data: &Path) -> Result<(), String> {
    let is_pk3 = out
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pk3"));
    if !is_pk3 {
        return Err(format!(
            "--out must name a .pk3 file, not {}",
            out.display()
        ));
    }
    let install = game_data.parent().unwrap_or(game_data);
    if is_inside(out, game_data) || is_inside(out, install) {
        return Err(format!(
            "refusing to write into the game installation ({}): write elsewhere and copy \
             the pk3 into base yourself",
            install.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_must_be_a_pk3_outside_the_game() {
        let root = std::env::temp_dir().join("jkr-materialgen-cli");
        let game_data = root.join("Jedi Academy/GameData");
        assert!(check_output(&root.join("out/x.pk3"), &game_data).is_ok());
        assert!(check_output(&root.join("out/x.zip"), &game_data).is_err());
        assert!(check_output(&game_data.join("base/zzz.pk3"), &game_data).is_err());
        assert!(check_output(&root.join("Jedi Academy/zzz.pk3"), &game_data).is_err());
    }

    #[test]
    fn help_and_bad_arguments() {
        assert!(matches!(parse(["--help".to_owned()]), Ok(Command::Help)));
        assert!(parse(["--bogus".to_owned()]).is_err());
        assert!(parse(["--strength".to_owned(), "0".to_owned()]).is_err());
        assert!(parse(["--limit".to_owned()]).is_err());
    }

    #[test]
    fn usage_carries_the_notice() {
        assert!(USAGE.contains("do not share, upload or commit them"));
    }
}
