//! Lazy, mount-scoped compatibility catalogue for player-profile assets.
//!
//! Construction enumerates and reads the mounted VFS and is therefore kept on
//! an explicit worker. A loader instance belongs to one immutable VFS mount
//! set, so a completed result is reused until the caller replaces the loader.

use crate::character_catalog::{LegacyCharacter, LegacySpecies, legacy_character_catalog};
use crate::player_profile::SaberColor;
use crate::saber_definitions::{LegacySaberDefinition, legacy_saber_definitions};
use crate::string_table;
use jkr_vfs::VirtualFileSystem;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

/// The six stable BaseJKA `saber_colors_t` values (`q_shared.h:349-358`).
pub const LEGACY_SABER_COLORS: [SaberColor; 6] = [
    SaberColor::Red,
    SaberColor::Orange,
    SaberColor::Yellow,
    SaberColor::Green,
    SaberColor::Blue,
    SaberColor::Purple,
];

/// Fully parsed compatibility catalogue used by player-profile screens.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyAssetCatalog {
    /// Icon-backed ordinary player model/skin choices.
    pub characters: Vec<LegacyCharacter>,
    /// Multipart customisable species choices.
    pub species: Vec<LegacySpecies>,
    /// Saber definitions accepted by `UI_SaberValidForPlayerInMP`.
    pub saber_hilts: Vec<LegacySaberDefinition>,
    /// Definitions excluded because their raw `notInMP` value is non-zero.
    pub excluded_saber_hilts: usize,
    /// The six numeric saber-colour choices sent in `color1` and `color2`.
    pub saber_colors: [SaberColor; 6],
}

/// Builds a catalogue synchronously.
///
/// UI callers should normally use [`LegacyAssetCatalogLoader`] so VFS reads do
/// not block the event thread. This entry point exists for workers and tools.
pub fn legacy_asset_catalog(
    vfs: &VirtualFileSystem,
) -> Result<LegacyAssetCatalog, Box<dyn std::error::Error + Send + Sync>> {
    let characters = legacy_character_catalog(vfs)?;
    let mut all_sabers =
        legacy_saber_definitions(vfs).map_err(|error| std::io::Error::other(error.to_string()))?;
    // `.sab` names are menu string-table references (`@MENUS_SINGLE_HILT1`).
    let strings = string_table::load_referenced(vfs, &["strings/english/menus.str"]);
    for definition in all_sabers.values_mut() {
        let name = string_table::resolve(&strings, &definition.display_name).to_owned();
        definition.display_name = name;
    }
    let excluded_saber_hilts = all_sabers
        .values()
        .filter(|definition| definition.not_in_mp)
        .count();
    let saber_hilts = all_sabers
        .into_values()
        .filter(|definition| !definition.not_in_mp)
        .collect();
    Ok(LegacyAssetCatalog {
        characters: characters.characters,
        species: characters.species,
        saber_hilts,
        excluded_saber_hilts,
        saber_colors: LEGACY_SABER_COLORS,
    })
}

/// State of a mount-scoped asynchronous catalogue request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyCatalogStatus {
    /// No request has been made; no worker exists and no VFS read has occurred.
    Idle,
    /// The worker is enumerating and parsing the mounted assets.
    Loading,
    /// The parsed catalogue is cached and available through `catalog`.
    Ready,
    /// The worker failed; the stable diagnostic is available through `error`.
    Failed,
}

/// One lazy catalogue cache bound to a single VFS mount set.
pub struct LegacyAssetCatalogLoader {
    vfs: Arc<VirtualFileSystem>,
    receiver: Option<Receiver<Result<LegacyAssetCatalog, String>>>,
    catalog: Option<Arc<LegacyAssetCatalog>>,
    error: Option<String>,
    status: LegacyCatalogStatus,
}

impl LegacyAssetCatalogLoader {
    /// Creates an idle loader without enumerating or reading the VFS.
    pub fn new(vfs: Arc<VirtualFileSystem>) -> Self {
        Self {
            vfs,
            receiver: None,
            catalog: None,
            error: None,
            status: LegacyCatalogStatus::Idle,
        }
    }

    /// Starts exactly one background build; ready and loading requests are no-ops.
    pub fn request(&mut self) {
        if self.status != LegacyCatalogStatus::Idle {
            return;
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let vfs = Arc::clone(&self.vfs);
        std::thread::spawn(move || {
            let result = legacy_asset_catalog(&vfs).map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
        self.receiver = Some(receiver);
        self.status = LegacyCatalogStatus::Loading;
    }

    /// Polls the worker without blocking and caches a completed result.
    pub fn poll(&mut self) -> LegacyCatalogStatus {
        if self.status != LegacyCatalogStatus::Loading {
            return self.status;
        }
        let Some(receiver) = self.receiver.as_ref() else {
            return self.status;
        };
        match receiver.try_recv() {
            Ok(Ok(catalog)) => {
                self.catalog = Some(Arc::new(catalog));
                self.receiver = None;
                self.status = LegacyCatalogStatus::Ready;
            }
            Ok(Err(error)) => {
                self.error = Some(error);
                self.receiver = None;
                self.status = LegacyCatalogStatus::Failed;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.error = Some("catalogue worker disconnected".to_owned());
                self.receiver = None;
                self.status = LegacyCatalogStatus::Failed;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        self.status
    }

    /// Returns the current state without polling or allocating.
    pub fn status(&self) -> LegacyCatalogStatus {
        self.status
    }

    /// Returns the cached catalogue after the loader reaches `Ready`.
    pub fn catalog(&self) -> Option<&Arc<LegacyAssetCatalog>> {
        self.catalog.as_ref()
    }

    /// Returns the stable worker diagnostic after the loader reaches `Failed`.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}
