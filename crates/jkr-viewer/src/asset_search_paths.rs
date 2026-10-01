//! MP startup search-path policy. Generic mount mechanics remain in jkr-vfs.
use crate::console::ViewerConsole;
use jkr_shell::{CvarDefinition, CvarFlags, CvarRegistry};
use jkr_vfs::VirtualFileSystem;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static STARTUP: OnceLock<Options> = OnceLock::new();

/// Immutable startup settings shared by world-loading workers.
pub(crate) struct Options {
    game: String,
    basegame: String,
    home: Option<PathBuf>,
    portable: bool,
    directory_first: bool,
    debug: bool,
    /// Prefer base-game shader definitions; immutable after startup.
    pub(crate) protect_shaders: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            game: String::new(),
            basegame: String::new(),
            home: None,
            portable: true,
            directory_first: false,
            debug: false,
            protect_shaders: true,
        }
    }
}

/// Register startup preferences; edits apply on next process launch, not a map reload.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), jkr_shell::CvarError> {
    for definition in [
        CvarDefinition::new(
            "fs_game",
            "",
            CvarFlags::ARCHIVE,
            "Mod directory; restart the client after changing",
        ),
        CvarDefinition::new(
            "fs_basegame",
            "",
            CvarFlags::ARCHIVE,
            "Intermediate game directory; client restart required",
        ),
        CvarDefinition::new(
            "fs_homepath",
            "",
            CvarFlags::ARCHIVE,
            "Optional explicit content home; empty disables it; restart required",
        ),
    ] {
        cvars.register(definition)?;
    }
    for (name, value, help) in [
        (
            "fs_portable",
            true,
            "Ignore optional content home; client restart required",
        ),
        (
            "fs_dirbeforepak",
            false,
            "Prefer loose files within each directory; restart required",
        ),
        (
            "fs_debug",
            false,
            "Log mounted sources and asset reads; client restart required",
        ),
        (
            "fs_protectShaders",
            true,
            "Prefer base-game shader definitions; client restart required",
        ),
    ] {
        cvars.register(CvarDefinition::new(name, value, CvarFlags::ARCHIVE, help))?;
    }
    Ok(())
}

/// Freeze settings once before the first native world load; evidence defaults stay isolated.
pub(crate) fn initialize(console: &ViewerConsole) -> Result<(), Box<dyn Error>> {
    STARTUP
        .set(Options::from_console(console)?)
        .map_err(|_| "asset startup settings already initialized")?;
    Ok(())
}

/// Snapshot shared by all subsequent mounts without locks or config reads.
pub(crate) fn startup() -> &'static Options {
    STARTUP.get_or_init(Options::default)
}

impl Options {
    fn from_console(console: &ViewerConsole) -> Result<Self, Box<dyn Error>> {
        let game = console.text_value("fs_game").unwrap_or("").to_owned();
        let basegame = console.text_value("fs_basegame").unwrap_or("").to_owned();
        validate_directory(&game)?;
        validate_directory(&basegame)?;
        let home = console
            .text_value("fs_homepath")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        Ok(Self {
            game,
            basegame,
            home,
            portable: console.bool_cvar("fs_portable").unwrap_or(true),
            directory_first: console.bool_cvar("fs_dirbeforepak").unwrap_or(false),
            debug: console.bool_cvar("fs_debug").unwrap_or(false),
            protect_shaders: console.bool_cvar("fs_protectShaders").unwrap_or(true),
        })
    }

    /// Low-to-high priority directories, matching FS_Startup's nested ordering.
    pub(crate) fn directories(&self, install: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
        let home = if self.portable {
            None
        } else {
            // Downloads are session-selected, never an implicit global content home.
            self.home.as_deref()
        };
        let mut paths = Vec::new();
        for game in ["base", self.basegame.as_str(), self.game.as_str()] {
            if game.is_empty() {
                continue;
            }
            for root in std::iter::once(install).chain(home) {
                let path = root.join(game);
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
        Ok(paths)
    }

    /// Mount existing directories; unreadable archives warn without discarding other packs.
    pub(crate) fn mount(&self, install: &Path) -> Result<VirtualFileSystem, Box<dyn Error>> {
        // Large offline imports may opt into a higher per-asset ceiling. Keep the
        // ordinary viewer default and generic VFS policy unchanged.
        let limit = std::env::var("JKR_MAX_ASSET_MIB")
            .ok()
            .map(|value| value.parse::<u64>())
            .transpose()?
            .unwrap_or(1024)
            .checked_mul(1024 * 1024)
            .filter(|&n| n > 0)
            .ok_or("JKR_MAX_ASSET_MIB must be positive and fit in u64")?;
        let mut vfs = VirtualFileSystem::with_max_asset_bytes(limit);
        vfs.set_read_diagnostics(self.debug);
        for directory in self.directories(install)? {
            if !directory.is_dir() {
                continue;
            }
            if !self.directory_first {
                vfs.mount_directory(&directory)?;
            }
            vfs.mount_pk3_directory_with_warnings(&directory, |path, error| {
                crate::log::progress(format_args!(
                    "warning: skipping PK3 {}: {error}",
                    path.display(),
                ));
            })?;
            if self.directory_first {
                vfs.mount_directory(&directory)?;
            }
        }
        // `JKR_CONTENT=<dir>[:<dir>...]`: further content directories (loose files and
        // PK3s), above the installation: locally made content that has no place in it.
        for directory in std::env::var_os("JKR_CONTENT")
            .iter()
            .flat_map(std::env::split_paths)
            .filter(|directory| directory.is_dir())
        {
            vfs.mount_pk3_directory_with_warnings(&directory, |path, error| {
                crate::log::progress(format_args!(
                    "warning: skipping PK3 {}: {error}",
                    path.display()
                ));
            })?;
            vfs.mount_directory(&directory)?;
        }
        if self.debug {
            for mount in vfs.mounts() {
                crate::log::progress(format_args!(
                    "fs mount: {} ({} entries)",
                    mount.name, mount.entries,
                ));
            }
        }
        Ok(vfs)
    }
}

fn validate_directory(name: &str) -> Result<(), Box<dyn Error>> {
    if name == "."
        || name.contains("..")
        || name.contains(['/', '\\', ':'])
        || name.chars().any(char::is_control)
        || name.trim() != name
    {
        return Err(
            format!("invalid game directory {name:?}: expected a single directory name",).into(),
        );
    }
    Ok(())
}
