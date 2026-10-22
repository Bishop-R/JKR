//! A pure server's pk3 files (`sv_pure`, `SV_SpawnServer`'s pure branch,
//! `sv_init.cpp:590-722`): every pk3 a map was loaded with, its checksum and its pure
//! checksum under the map's checksum feed, and the pk3 the game's cgame and ui come from.
//!
//! The systeminfo lists them (`sv_paks`, `sv_pakNames`) so that a pure client loads from
//! nothing else, and the endpoint checks each client's `cp` against them
//! ([`jkr_network::LegacyPureManifest`]). Built as a map is loaded, from the files the map
//! was loaded from, and only while `sv_pure` is set.

use jkr_network::LegacyPureManifest;
use jkr_vfs::{Pk3Fingerprint, VirtualFileSystem};
use std::path::Path;
use std::sync::Arc;

/// The game's cgame and ui modules, whose pk3 a pure client must have loaded them from
/// (`SV_VerifyPaks_f`: "dlls are valid too now").
const CGAME: &str = "cgamex86.dll";
const UI: &str = "uix86.dll";

/// One loaded pk3, as a pure server lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pak {
    /// Where it was mounted from (the mount's name).
    path: Arc<str>,
    /// `pakBasename`: its file name without `.pk3`.
    pub name: String,
    /// `<game directory>/<name>`, as `FS_ReferencedPakNames` lists it.
    pub game_name: String,
    /// `FS_LoadZipFile`'s checksum of its entries, and with the feed folded in.
    pub checksum: i32,
    pub pure_checksum: i32,
}

/// The pk3 files a map was loaded with, highest priority first (`fs_searchpaths`), and
/// what a pure server checks its clients against.
#[derive(Clone, Debug, Default)]
pub struct PureFiles {
    pub paks: Vec<Pak>,
    pub manifest: LegacyPureManifest,
    /// Which of them the cgame comes from.
    cgame: Option<usize>,
}

impl Pak {
    /// Whether it is the pk3 at `path`.
    pub fn is_at(&self, path: &Path) -> bool {
        Path::new(&*self.path) == path
    }
}

impl PureFiles {
    /// The pk3 files `files` mounts, under the checksum feed `feed`. A pk3 that cannot be
    /// read again is left out with a warning.
    pub fn new(files: &VirtualFileSystem, feed: i32) -> Self {
        let paks: Vec<Pak> = files
            .mounts()
            .rev()
            .filter_map(|mount| {
                let path = Path::new(&*mount.name);
                let pk3 = path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("pk3"));
                if !pk3 {
                    return None;
                }
                let print = match Pk3Fingerprint::open(path) {
                    Ok(print) => print,
                    Err(error) => {
                        eprintln!("sv_pure: leaving out {}: {error}", path.display());
                        return None;
                    }
                };
                let name = path.file_stem()?.to_string_lossy().into_owned();
                let directory = path.parent()?.file_name()?.to_string_lossy();
                Some(Pak {
                    path: Arc::clone(&mount.name),
                    game_name: format!("{directory}/{name}"),
                    name,
                    checksum: print.checksum(),
                    pure_checksum: print.pure_checksum(feed),
                })
            })
            .collect();
        // `FS_FileIsInPAK`: the first pk3 in search order that holds the file.
        let pak_of = |file: &str| {
            let mount = files.source_of(file).ok()??;
            paks.iter().position(|pak| pak.path == mount.name)
        };
        let (cgame, ui) = (pak_of(CGAME), pak_of(UI));
        let manifest = LegacyPureManifest {
            checksum_feed: feed,
            loaded: paks.iter().map(|pak| pak.pure_checksum).collect(),
            cgame: cgame.map(|index| paks[index].pure_checksum),
            ui: ui.map(|index| paks[index].pure_checksum),
        };
        if paks.is_empty() {
            eprintln!("WARNING: sv_pure set but no PK3 files loaded");
        }
        Self {
            paks,
            manifest,
            cgame,
        }
    }

    /// `FS_LoadedPakChecksums` (`sv_paks`): each checksum followed by a space.
    pub fn checksums(&self) -> String {
        self.paks
            .iter()
            .map(|pak| format!("{} ", pak.checksum))
            .collect()
    }

    /// `FS_LoadedPakNames` (`sv_pakNames`): the names, a space between each.
    pub fn names(&self) -> String {
        let names: Vec<&str> = self.paks.iter().map(|pak| pak.name.as_str()).collect();
        names.join(" ")
    }

    /// The pk3 the cgame comes from, which a dedicated pure server touches so that it is
    /// referenced (`SV_TouchCGame`).
    pub fn cgame(&self) -> Option<&Pak> {
        self.paks.get(self.cgame?)
    }
}

/// A map's checksum feed (`SV_SpawnServer`: `(rand() << 16 ^ rand()) ^ Com_Milliseconds()`):
/// a number no client can guess, drawn from the clock, which keys its movement packets and
/// its pure checksums.
pub fn checksum_feed() -> i32 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    // Fold every bit of the clock in; the low ones change fastest.
    (nanos as u64 ^ (nanos >> 64) as u64)
        .to_le_bytes()
        .chunks_exact(4)
        .map(|word| i32::from_le_bytes(word.try_into().expect("four bytes")))
        .fold(0x5a5a_5a5a, |feed, word| feed.rotate_left(13) ^ word)
}
