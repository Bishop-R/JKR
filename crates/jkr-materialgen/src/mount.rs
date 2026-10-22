//! Finding and mounting the player's game data the way the client does, and
//! the default output location.

use jkr_vfs::{VfsError, VirtualFileSystem, pk3_search_order};
use std::env;
use std::path::{Path, PathBuf};

/// File name of the generated archive: `zzz_` sorts after the retail
/// `assets*.pk3`, so the maps load after (and never replace) retail content.
pub const DEFAULT_FILE_NAME: &str = "zzz_jkr_materials.pk3";

/// Whether `path` looks like a Jedi Academy `GameData` directory.
pub fn is_game_data(path: &Path) -> bool {
    path.join("base/assets0.pk3").is_file() && path.join("base/assets3.pk3").is_file()
}

/// The `GameData` directory: `explicit`, then `JKR_GAME_DATA`, then
/// `fs_gameData` from the JKR config file, then the usual install locations.
pub fn find_game_data(explicit: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        return if is_game_data(path) {
            Ok(path.to_owned())
        } else {
            Err(format!(
                "{} is not a Jedi Academy GameData directory (no base/assets0.pk3)",
                path.display()
            ))
        };
    }
    let mut candidates = Vec::new();
    candidates.extend(env::var_os("JKR_GAME_DATA").map(PathBuf::from));
    if let Some(config) = user_data_root().map(|root| root.join("config.cfg"))
        && let Ok(text) = std::fs::read_to_string(config)
    {
        candidates.extend(config_game_data(&text));
    }
    if let Ok(current) = env::current_dir() {
        candidates.push(current.join("GameData"));
    }
    if cfg!(windows) {
        for root in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(root) = env::var_os(root) {
                let mut path = PathBuf::from(root);
                path.extend(["Steam", "steamapps", "common", "Jedi Academy", "GameData"]);
                candidates.push(path);
            }
        }
    } else if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
        candidates.push(home.join(".local/share/Steam/steamapps/common/Jedi Academy/GameData"));
        candidates.push(home.join(".steam/steam/steamapps/common/Jedi Academy/GameData"));
        candidates.push(home.join("Games/Jedi Academy/GameData"));
    }
    candidates
        .into_iter()
        .find(|path| is_game_data(path))
        .ok_or_else(|| {
            "Jedi Academy GameData was not found: pass --game-data, or set JKR_GAME_DATA".into()
        })
}

/// `fs_gameData` from `set`/`seta` lines of a config file.
fn config_game_data(text: &str) -> Option<PathBuf> {
    text.lines().rev().find_map(|line| {
        let mut words = line.split_whitespace();
        let command = words.next()?;
        if !command.eq_ignore_ascii_case("set") && !command.eq_ignore_ascii_case("seta") {
            return None;
        }
        if !words.next()?.eq_ignore_ascii_case("fs_gameData") {
            return None;
        }
        // The value is the rest of the line: quoted paths may contain spaces.
        let start = line.to_ascii_lowercase().find("fs_gamedata")? + "fs_gameData".len();
        let value = line[start..].trim().trim_matches('"');
        (!value.is_empty()).then(|| PathBuf::from(value))
    })
}

/// JKR's per-user directory: `%APPDATA%\jkr` on Windows, `~/Library/Application
/// Support/jkr` on macOS, `$XDG_CONFIG_HOME/jkr` or `~/.config/jkr` elsewhere
/// (where the client keeps `config.cfg`).
pub fn user_data_root() -> Option<PathBuf> {
    let variable = |name: &str| {
        env::var_os(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    let root = if cfg!(windows) {
        variable("APPDATA")?
    } else if cfg!(target_os = "macos") {
        variable("HOME")?.join("Library/Application Support")
    } else if let Some(path) = variable("XDG_CONFIG_HOME") {
        path
    } else {
        variable("HOME")?.join(".config")
    };
    Some(root.join("jkr"))
}

/// `<user data>/generated/zzz_jkr_materials.pk3`.
pub fn default_output() -> Result<PathBuf, String> {
    user_data_root()
        .map(|root| root.join("generated").join(DEFAULT_FILE_NAME))
        .ok_or_else(|| "no per-user data directory (APPDATA/HOME unset): pass --out".into())
}

/// Whether `path` is `directory` or inside it, after resolving both as far as
/// they exist.
pub fn is_inside(path: &Path, directory: &Path) -> bool {
    let resolve = |path: &Path| {
        let mut existing = path.to_owned();
        let mut rest = Vec::new();
        while !existing.exists() {
            match (existing.parent(), existing.file_name()) {
                (Some(parent), Some(name)) => {
                    rest.push(name.to_owned());
                    existing = parent.to_owned();
                }
                _ => break,
            }
        }
        let mut resolved = existing.canonicalize().unwrap_or(existing);
        for name in rest.iter().rev() {
            resolved.push(name);
        }
        resolved
    };
    let lower = |path: PathBuf| {
        path.to_string_lossy()
            .to_ascii_lowercase()
            .replace('\\', "/")
    };
    let (path, directory) = (lower(resolve(path)), lower(resolve(directory)));
    let directory = directory.trim_end_matches('/');
    path == directory || path.starts_with(&format!("{directory}/"))
}

/// Mount `GameData/base`, then `GameData/<fs_game>`, then every `JKR_CONTENT`
/// directory, as the client does (loose files below each directory's pk3s,
/// pk3s in Quake 3 order; `JKR_CONTENT` pk3s below its loose files). Archives
/// named `exclude` (the tool's own earlier output) are left out and returned.
pub fn mount_game_data(
    game_data: &Path,
    fs_game: Option<&str>,
    exclude: &str,
) -> Result<(VirtualFileSystem, Vec<PathBuf>), VfsError> {
    let mut vfs = VirtualFileSystem::new();
    let mut excluded = Vec::new();
    let mut mount_pk3s = |vfs: &mut VirtualFileSystem, directory: &Path| -> Result<(), VfsError> {
        for archive in pk3_search_order(directory)? {
            let name = archive
                .file_name()
                .map(|name| name.to_string_lossy().to_ascii_lowercase());
            if name.as_deref() == Some(exclude.to_ascii_lowercase().as_str()) {
                excluded.push(archive);
                continue;
            }
            if let Err(error) = vfs.mount_pk3(&archive) {
                eprintln!("warning: skipping {}: {error}", archive.display());
            }
        }
        Ok(())
    };
    for game in std::iter::once("base").chain(fs_game.filter(|game| !game.is_empty())) {
        let directory = game_data.join(game);
        if !directory.is_dir() {
            continue;
        }
        vfs.mount_directory(&directory)?;
        mount_pk3s(&mut vfs, &directory)?;
    }
    for directory in env::var_os("JKR_CONTENT")
        .iter()
        .flat_map(env::split_paths)
        .filter(|directory| directory.is_dir())
    {
        mount_pk3s(&mut vfs, &directory)?;
        vfs.mount_directory(&directory)?;
    }
    Ok((vfs, excluded))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_game_data_is_read() {
        let text = "seta r_fullscreen \"1\"\nseta fs_gameData \"C:\\Games\\JA\\GameData\"\n";
        assert_eq!(
            config_game_data(text),
            Some(PathBuf::from("C:\\Games\\JA\\GameData"))
        );
        assert_eq!(config_game_data("seta fs_game \"x\"\n"), None);
    }

    #[test]
    fn inside_checks_are_case_and_separator_blind() {
        let base = env::temp_dir().join("jkr-materialgen-inside");
        assert!(is_inside(&base.join("Base/zzz.pk3"), &base));
        assert!(is_inside(&base, &base));
        assert!(!is_inside(
            &env::temp_dir().join("jkr-materialgen-elsewhere"),
            &base
        ));
    }
}
