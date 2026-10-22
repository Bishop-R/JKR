//! Where the console finds a config file for `exec`, and where the server keeps its
//! archived variables — the reference's search paths as far as a server's configs go.
//!
//! A name is looked up in the operator's home directory (`fs_homepath/base`), then in
//! the game data's archives (`base/*.pk3`, in the reference's order, where the stock
//! `mpdefault.cfg` lives), then loose in the game data's `base` directory. A mod
//! (`fs_game`) is searched before all of that, loose in the home and the game data's mod
//! folder, and its archives win over `base`'s; what the server writes goes to the mod's
//! folder in the home directory. Nothing is written unless a home directory was given.

use jkr_vfs::VirtualFileSystem;
use std::path::PathBuf;

/// The file `Com_WriteConfiguration` keeps archived variables in (`openjk_server.cfg`
/// in the reference), in the home directory's `base`.
pub const ARCHIVE_FILE: &str = "jkr_server.cfg";

/// The search paths.
#[derive(Default)]
pub struct ConfigFiles {
    /// Written to and read first; `None` for a server that writes nothing.
    pub home: Option<PathBuf>,
    /// The retail `GameData` directory.
    pub game_data: Option<PathBuf>,
    /// The game data's archives, mounted on first use.
    archives: Option<VirtualFileSystem>,
    /// The game directory written to: the mod's (`fs_game`), else `base`.
    game: String,
}

impl ConfigFiles {
    /// Search paths over an optional home and game-data directory.
    pub fn new(home: Option<PathBuf>, game_data: Option<PathBuf>, game: &str) -> Self {
        let game = crate::map::game_directories(game)
            .last()
            .copied()
            .unwrap_or("base")
            .to_owned();
        Self {
            home,
            game_data,
            archives: None,
            game,
        }
    }

    /// A file loose in `directory` of `root` (the home or the game data).
    fn loose(root: Option<&PathBuf>, directory: &str, name: &str) -> Option<Vec<u8>> {
        std::fs::read(root?.join(directory).join(name)).ok()
    }

    /// Whether a mod is searched before `base`.
    fn modded(&self) -> bool {
        self.game != "base"
    }

    /// A file's bytes by its game path, or `None` where no search path has it.
    pub fn read(&mut self, name: &str) -> Option<Vec<u8>> {
        let name = name.trim_start_matches(['/', '\\']);
        if self.modded()
            && let Some(bytes) = Self::loose(self.home.as_ref(), &self.game, name)
                .or_else(|| Self::loose(self.game_data.as_ref(), &self.game, name))
        {
            return Some(bytes);
        }
        if let Some(bytes) = Self::loose(self.home.as_ref(), "base", name) {
            return Some(bytes);
        }
        let game_data = self.game_data.clone()?;
        if self.archives.is_none() {
            let mut files = VirtualFileSystem::new();
            for directory in crate::map::game_directories(&self.game) {
                let path = game_data.join(directory);
                if directory != "base" && !path.is_dir() {
                    continue;
                }
                let _ = files.mount_pk3_directory_with_warnings(path, |archive, error| {
                    eprintln!("skipping {}: {error}", archive.display());
                });
            }
            self.archives = Some(files);
        }
        if let Some(Ok(Some(asset))) = self.archives.as_ref().map(|files| files.read(name)) {
            return Some(asset.bytes.to_vec());
        }
        std::fs::read(game_data.join("base").join(name)).ok()
    }

    /// Whether a search path has the file (`FS_Open` > 0), without reading it. The
    /// archives are those an earlier [`Self::read`] or [`Self::list_files`] mounted.
    pub fn has(&self, name: &str) -> bool {
        let name = name.trim_start_matches(['/', '\\']);
        let loose = |root: Option<&PathBuf>, directory: &str| {
            root.is_some_and(|root| root.join(directory).join(name).is_file())
        };
        if (self.modded()
            && (loose(self.home.as_ref(), &self.game)
                || loose(self.game_data.as_ref(), &self.game)))
            || loose(self.home.as_ref(), "base")
        {
            return true;
        }
        let archived = self
            .archives
            .as_ref()
            .is_some_and(|files| files.contains(name).unwrap_or(false));
        archived
            || self
                .game_data
                .as_ref()
                .is_some_and(|game_data| game_data.join("base").join(name).is_file())
    }

    /// `FS_GetFileList` of `directory` in the game data's archives: the names with
    /// `extension`, in the archives' order.
    pub fn list_files(&mut self, directory: &str, extension: &str) -> Vec<String> {
        self.read("");
        self.archives
            .as_ref()
            .map(|files| files.list_files(directory, extension))
            .unwrap_or_default()
    }

    /// A file of the server's own in the home directory's `base` (`FS_SV_FOpenFileRead`,
    /// as the ban file is read); `None` without a home or the file.
    pub fn read_home(&self, name: &str) -> Option<Vec<u8>> {
        if !stays_inside(name) {
            return None;
        }
        std::fs::read(self.home.as_ref()?.join(&self.game).join(name)).ok()
    }

    /// Write a file of the server's own in the home directory's `base`
    /// (`FS_SV_FOpenFileWrite`); nothing without a home.
    pub fn write_home(&self, name: &str, text: &[u8]) -> std::io::Result<()> {
        let Some(home) = &self.home else {
            return Ok(());
        };
        if !stays_inside(name) {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
        let directory = home.join(&self.game);
        std::fs::create_dir_all(&directory)?;
        std::fs::write(directory.join(name), text)
    }

    /// A file of the server's own in the home directory's `base`, opened for appending
    /// (`FS_APPEND`, created if missing); `None` without a home or where it cannot be.
    pub fn append_home(&self, name: &str) -> Option<std::fs::File> {
        if !stays_inside(name) {
            return None;
        }
        let directory = self.home.as_ref()?.join(&self.game);
        std::fs::create_dir_all(&directory).ok()?;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join(name))
            .ok()
    }

    /// Append `bytes` to a file of the server's own in the home directory's `base`
    /// (`FS_SV_FOpenFileAppend`); `true` without a home, where nothing is kept, and
    /// `false` where it cannot be opened.
    pub fn append_record(&self, name: &str, bytes: &[u8]) -> bool {
        if self.home.is_none() {
            return true;
        }
        self.append_home(name)
            .is_some_and(|mut file| std::io::Write::write_all(&mut file, bytes).is_ok())
    }

    /// A file of the server's own in the home directory's `base`, created afresh with
    /// its directories (`FS_FOpenFileWrite`); `None` without a home or where it cannot be.
    pub fn create_home(&self, name: &str) -> Option<std::fs::File> {
        if !stays_inside(name) {
            return None;
        }
        let path = self.home.as_ref()?.join(&self.game).join(name);
        std::fs::create_dir_all(path.parent()?).ok()?;
        std::fs::File::create(path).ok()
    }

    /// Whether a file of the server's own exists in the home directory's `base`.
    pub fn home_has(&self, name: &str) -> bool {
        stays_inside(name)
            && self
                .home
                .as_ref()
                .is_some_and(|home| home.join(&self.game).join(name).exists())
    }

    /// `FS_Rename` of a file of the server's own in the home directory's `base`: nothing
    /// without a home, or where either name climbs out of it or the file is missing.
    pub fn rename_home(&self, from: &str, to: &str) {
        let Some(home) = &self.home else { return };
        if !stays_inside(from) || !stays_inside(to) {
            return;
        }
        let base = home.join(&self.game);
        let target = base.join(to);
        if let Some(parent) = target.parent()
            && std::fs::create_dir_all(parent).is_ok()
        {
            let _ = std::fs::rename(base.join(from), target);
        }
    }

    /// `sv_autoDemoMaxMaps`: only the `keep` newest maps' folders of automatic demos
    /// stay in the home directory's `base/demos/autorecord`; nothing without a home.
    pub fn prune_auto_demos(&self, keep: usize) {
        if let Some(home) = &self.home {
            jkr_network::legacy_prune_auto_demos(&mut HomeFolders(home.join(&self.game)), keep);
        }
    }

    /// `Com_WriteConfigToFile`: the archived variables' `seta` lines under the
    /// reference's header. Nothing happens without a home directory.
    pub fn write_archive(&self, lines: &[u8]) -> std::io::Result<()> {
        self.write_home(
            ARCHIVE_FILE,
            &[&b"// generated by JKR, do not modify\n"[..], lines].concat(),
        )
    }
}

/// `FS_FOpenFileRead`'s and `FS_SV_FOpenFileRead`'s guard: nothing that could leave the
/// search path, and no absolute path.
fn stays_inside(name: &str) -> bool {
    !name.contains("..") && !name.contains("::") && !name.starts_with(['/', '\\'])
}

/// `gethostbyname`: a host name's first IPv4 address.
pub fn resolve_host(name: &str) -> Option<std::net::Ipv4Addr> {
    std::net::ToSocketAddrs::to_socket_addrs(&(name, 0))
        .ok()?
        .find_map(|address| match address {
            std::net::SocketAddr::V4(address) => Some(*address.ip()),
            std::net::SocketAddr::V6(_) => None,
        })
}

/// The home directory's `base` as automatic demos' folders are listed and removed in it.
struct HomeFolders(PathBuf);

impl jkr_network::LegacyDemoFolders for HomeFolders {
    fn folders(&self, path: &str) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(self.0.join(path)) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
    fn has_files(&self, path: &str) -> bool {
        std::fs::read_dir(self.0.join(path)).is_ok_and(|mut entries| {
            entries.any(|entry| {
                entry.is_ok_and(|entry| entry.file_type().is_ok_and(|kind| !kind.is_dir()))
            })
        })
    }
    fn remove(&mut self, path: &str, recursive: bool) {
        if !stays_inside(path) {
            return;
        }
        let path = self.0.join(path);
        // What cannot be removed stays, as `FS_Rmdir` leaves it.
        let _ = if recursive {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_dir(path)
        };
    }
}

#[cfg(test)]
mod mod_tests {
    use super::*;

    /// A home and a game data directory under the system's temporary one, removed after.
    struct Dirs(PathBuf);

    impl Dirs {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("jkr-{name}-{}", std::process::id()));
            for directory in ["home/base", "home/japro", "data/base", "data/japro"] {
                std::fs::create_dir_all(root.join(directory)).unwrap();
            }
            Self(root)
        }
        fn write(&self, path: &str, text: &str) {
            std::fs::write(self.0.join(path), text).unwrap();
        }
    }

    impl Drop for Dirs {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_mod_is_searched_before_base_and_written_to() {
        let dirs = Dirs::new("fsgame");
        dirs.write("home/base/server.cfg", "base home");
        dirs.write("data/japro/server.cfg", "mod data");
        dirs.write("data/base/motd.cfg", "base data");
        let mut files = ConfigFiles::new(
            Some(dirs.0.join("home")),
            Some(dirs.0.join("data")),
            "japro",
        );
        assert_eq!(files.read("server.cfg").unwrap(), b"mod data");
        assert_eq!(files.read("motd.cfg").unwrap(), b"base data");
        assert!(files.has("server.cfg"));
        files.write_home("games.log", b"x").unwrap();
        assert!(dirs.0.join("home/japro/games.log").is_file());
        assert!(!dirs.0.join("home/base/games.log").exists());
        // Without the mod, base's own.
        let mut plain = ConfigFiles::new(Some(dirs.0.join("home")), Some(dirs.0.join("data")), "");
        assert_eq!(plain.read("server.cfg").unwrap(), b"base home");
    }

    #[test]
    fn only_a_safe_mod_name_is_searched() {
        use crate::map::game_directories;
        assert_eq!(game_directories(""), ["base"]);
        assert_eq!(game_directories("BASE"), ["base"]);
        assert_eq!(game_directories("japlus"), ["base", "japlus"]);
        assert_eq!(game_directories("../etc"), ["base"]);
        assert_eq!(game_directories("a/b"), ["base"]);
    }
}
