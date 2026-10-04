//! Per-gamestate cached content selection. Offline mounts never include the download cache.
use jkr_protocol::{GameState, InfoString};
use jkr_vfs::{Pk3Fingerprint, VirtualFileSystem};
use md4::{Digest, Md4};
use std::error::Error;
use std::path::Path;

/// Immutable request copied to the world-loading worker, never sampled per frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Selection {
    checksums: Vec<i32>,
    map_checksum: Option<i32>,
}

impl Selection {
    /// Parse stock's references, which sv_init.cpp publishes on both pure and unpure servers.
    pub(crate) fn from_game(game: &GameState) -> Result<Self, Box<dyn Error>> {
        Self::from_system_info(game.config_string(1).unwrap_or(b""))
    }

    fn from_system_info(raw: &[u8]) -> Result<Self, Box<dyn Error>> {
        let info = InfoString::parse(std::str::from_utf8(raw)?)?;
        let checksums = jkr_client::referenced_paks::parse(
            info.get("sv_referencedPaks").unwrap_or(""),
            info.get("sv_referencedPakNames").unwrap_or(""),
        )?
        .into_iter()
        .map(|reference| reference.checksum)
        .collect::<Vec<_>>();
        let map_checksum = info
            .get("sv_mapChecksum")
            .map(|value| {
                value
                    .parse::<i32>()
                    .or_else(|_| value.parse::<u32>().map(|v| v as i32))
            })
            .transpose()?;
        if checksums.is_empty() {
            crate::log::progress(format_args!(
                "session content: no pak references; cached downloads excluded"
            ));
        }
        Ok(Self {
            checksums,
            map_checksum,
        })
    }

    /// Build a fresh configured search path; no previous world's mounts can leak into it.
    pub(crate) fn mount(&self, game_data: &Path) -> Result<VirtualFileSystem, Box<dyn Error>> {
        self.mount_in(game_data, &super::downloads::home()?)
    }

    fn mount_in(
        &self,
        game_data: &Path,
        cache: &Path,
    ) -> Result<VirtualFileSystem, Box<dyn Error>> {
        let mut vfs = super::mount_game_data(game_data)?;
        self.mount_cache(&mut vfs, cache)?;
        Ok(vfs)
    }

    fn mount_cache(
        &self,
        vfs: &mut VirtualFileSystem,
        directory: &Path,
    ) -> Result<(), Box<dyn Error>> {
        if self.checksums.is_empty() {
            return Ok(());
        }
        let mut files = if directory.is_dir() {
            std::fs::read_dir(directory)?
                .map(|entry| entry.map(|e| e.path()))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };
        files.retain(|path| {
            path.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pk3"))
        });
        files.sort();
        let mut candidates = Vec::new();
        let installed: Vec<_> = vfs
            .mounts()
            .filter_map(|mount| {
                Pk3Fingerprint::open(mount.name.as_ref())
                    .ok()
                    .map(|p| (p.checksum(), std::path::PathBuf::from(mount.name.as_ref())))
            })
            .collect();
        for path in files {
            match Pk3Fingerprint::open(&path) {
                Ok(pak) => candidates.push((pak.checksum(), path)),
                Err(error) => crate::log::progress(format_args!(
                    "session content: ignoring unreadable cached pak {}: {error}",
                    path.display()
                )),
            }
        }
        // The server list is high-to-low search priority; VFS mounts are low-to-high.
        // Match contents, never the local cache filename or the requested map's basename.
        // Installed references also participate: a cached pak may be LOWER priority than
        // retail in the server list. Simply appending that pak would select the wrong BSP.
        for checksum in self.checksums.iter().rev() {
            if let Some((_, path)) = installed
                .iter()
                .chain(&candidates)
                .find(|(sum, _)| sum == checksum)
            {
                let canonical = std::fs::canonicalize(path)?;
                vfs.mount_pk3(&canonical)?;
                crate::log::progress(format_args!(
                    "session content: selected {} checksum={checksum}",
                    canonical.display()
                ));
            } else {
                // References describe content the server touched, not a required
                // client install manifest. Server-only/cosmetic paks may be absent.
                // Do not mount a same-name substitute; load_bsp still checks the
                // actual map bytes, and pure-server admission remains separate.
                crate::log::progress(format_args!(
                    "session content: referenced pak checksum {checksum} unavailable; using available content"
                ));
            }
        }
        Ok(())
    }

    /// Parse one selected BSP for both collision and presentation; reject advertised mismatches.
    pub(crate) fn load_bsp(
        &self,
        vfs: &VirtualFileSystem,
        path: &str,
    ) -> Result<jkr_bsp::Bsp, Box<dyn Error>> {
        let asset = vfs
            .read(path)?
            .ok_or_else(|| format!("session map missing: {path}"))?;
        let actual = checksum(&asset.bytes);
        if self.map_checksum.is_some_and(|expected| actual != expected) {
            return Err(format!(
                "session map checksum mismatch: {path} from {}; expected {:?}, got {actual}; \
                 refusing mismatched collision geometry",
                asset.source.mount_name, self.map_checksum
            )
            .into());
        }
        crate::log::progress(format_args!(
            "session map: {path} source={} checksum={actual} verified={}",
            asset.source.mount_name,
            self.map_checksum.is_some()
        ));
        Ok(jkr_bsp::Bsp::parse(&asset.bytes)?)
    }
}

/// codemp CM_LoadMap: XOR the four little-endian words of MD4 over the entire BSP file.
fn checksum(bytes: &[u8]) -> i32 {
    Md4::digest(bytes).chunks_exact(4).fold(0u32, |sum, word| {
        sum ^ u32::from_le_bytes(word.try_into().unwrap())
    }) as i32
}
