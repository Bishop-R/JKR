//! Command-line compatibility and native no-argument launch resolution.

use std::env;
use std::ffi::OsString;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

/// Fully resolved startup request for the viewer application.
pub(crate) struct LaunchPlan {
    pub(crate) game_data: PathBuf,
    pub(crate) map_path: Option<String>,
    pub(crate) player_directory: Option<String>,
    pub(crate) connect_address: Option<String>,
    pub(crate) open_main_menu: bool,
}

/// Preserve the historical positional developer syntax while making an empty
/// argument list the normal native-client launch path.
pub(crate) fn resolve(
    arguments: impl Iterator<Item = OsString>,
    configured_game_data: Option<PathBuf>,
) -> Result<LaunchPlan, LaunchError> {
    let arguments = arguments.collect::<Vec<_>>();
    if arguments.is_empty() {
        return Ok(LaunchPlan {
            game_data: find_game_data(configured_game_data.as_deref())?,
            map_path: None,
            player_directory: None,
            connect_address: None,
            open_main_menu: true,
        });
    }

    let game_data = PathBuf::from(&arguments[0]);
    if !is_game_data(&game_data) {
        return Err(LaunchError::InvalidGameData(game_data));
    }
    let trailing = arguments[1..]
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let mut positionals = Vec::new();
    let mut connect_address = None;
    let mut index = 0;
    while index < trailing.len() {
        if trailing[index] == "--connect" {
            index += 1;
            connect_address = Some(
                trailing
                    .get(index)
                    .ok_or(LaunchError::MissingConnectAddress)?
                    .clone(),
            );
        } else {
            positionals.push(trailing[index].clone());
        }
        index += 1;
    }
    Ok(LaunchPlan {
        game_data,
        map_path: positionals.first().cloned(),
        player_directory: positionals.get(1).cloned(),
        connect_address,
        open_main_menu: false,
    })
}

fn find_game_data(configured: Option<&Path>) -> Result<PathBuf, LaunchError> {
    let current = env::current_dir().map_err(LaunchError::CurrentDirectory)?;
    let home = env::var_os("HOME").map(PathBuf::from);
    let environment = env::var_os("JKR_GAME_DATA").map(PathBuf::from);
    find_game_data_in(
        configured,
        environment.as_deref(),
        &current,
        home.as_deref(),
    )
    .ok_or(LaunchError::GameDataNotFound {
        searched_from: current,
    })
}

fn find_game_data_in(
    configured: Option<&Path>,
    environment: Option<&Path>,
    current: &Path,
    home: Option<&Path>,
) -> Option<PathBuf> {
    let mut candidates = Vec::with_capacity(7);
    candidates.extend(configured.map(Path::to_owned));
    candidates.extend(environment.map(Path::to_owned));
    candidates.push(current.join("Star Wars Jedi Knight - Jedi Academy/GameData"));
    candidates.push(current.join("GameData"));
    if let Some(home) = home {
        candidates.push(home.join(".local/share/Steam/steamapps/common/Jedi Academy/GameData"));
        candidates.push(home.join(".steam/steam/steamapps/common/Jedi Academy/GameData"));
        candidates.push(home.join("Games/Jedi Academy/GameData"));
    }
    candidates
        .into_iter()
        .find(|candidate| is_game_data(candidate))
}

fn is_game_data(path: &Path) -> bool {
    path.join("base/assets0.pk3").is_file() && path.join("base/assets3.pk3").is_file()
}

#[derive(Debug)]
pub(crate) enum LaunchError {
    CurrentDirectory(std::io::Error),
    GameDataNotFound { searched_from: PathBuf },
    InvalidGameData(PathBuf),
    MissingConnectAddress,
}

impl Display for LaunchError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CurrentDirectory(error) => {
                write!(formatter, "cannot resolve current directory: {error}")
            }
            Self::GameDataNotFound { searched_from } => write!(
                formatter,
                concat!(
                    "Jedi Academy GameData was not found. Set fs_gameData in the JKR config, ",
                    "set JKR_GAME_DATA, or pass the GameData directory as the first argument ",
                    "(searched from {})."
                ),
                searched_from.display()
            ),
            Self::InvalidGameData(path) => write!(
                formatter,
                concat!(
                    "{} is not a Jedi Academy GameData directory ",
                    "(base/assets0.pk3 and assets3.pk3 are required)"
                ),
                path.display()
            ),
            Self::MissingConnectAddress => {
                formatter.write_str("--connect requires server-host:port")
            }
        }
    }
}

impl std::error::Error for LaunchError {}
