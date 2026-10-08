//! The siege classes' looks this client dresses players in ([`jkr_client::SiegeClassLooks`]):
//! every class file under `ext_data/Siege/Classes` read once a siege level is shown
//! (`CG_InitSiegeMode`), and nothing outside siege.

use jkr_client::SiegeClassLooks;
use jkr_protocol::GameState;
use jkr_vfs::VirtualFileSystem;
use std::sync::Arc;

const GT_SIEGE: i32 = 7;
const CLASSES: &str = "ext_data/Siege/Classes";

/// The looks of the level last asked about, by its `mapname`.
#[derive(Default)]
pub(crate) struct SiegeLooksCache {
    map: Vec<u8>,
    looks: Arc<SiegeClassLooks>,
}

impl SiegeLooksCache {
    /// The looks for the level `game_state` names, the class files read again only when
    /// the level or its game type changes.
    pub(crate) fn for_level(
        &mut self,
        game_state: &GameState,
        vfs: Option<&VirtualFileSystem>,
    ) -> &Arc<SiegeClassLooks> {
        let info = game_state
            .config_string(0)
            .map(jkr_client::LegacyClientInfo::new);
        let siege = info.and_then(|info| info.integer("g_gametype")) == Some(GT_SIEGE);
        let map = info
            .and_then(|info| info.bytes("mapname"))
            .unwrap_or_default();
        // An empty key marks a level that is not siege, so that a siege game on the same
        // map reads the classes.
        let key = if siege { map } else { &[] };
        if self.map != key {
            self.map = key.to_vec();
            self.looks = Arc::new(match vfs.filter(|_| siege) {
                Some(vfs) => read(vfs),
                None => SiegeClassLooks::default(),
            });
        }
        &self.looks
    }
}

impl SiegeLooksCache {
    /// The looks last read, for a caller that cannot ask about a level.
    pub(crate) fn current(&self) -> &Arc<SiegeClassLooks> {
        &self.looks
    }
}

/// Every class file's looks (`BG_SiegeLoadClasses`).
pub(crate) fn read(vfs: &VirtualFileSystem) -> SiegeClassLooks {
    let classes: Vec<_> = vfs
        .list_files(CLASSES, ".scl")
        .into_iter()
        .filter_map(|file| {
            let asset = vfs.read(&format!("{CLASSES}/{file}")).ok().flatten()?;
            jkr_game_jka::siege_class::parse(&String::from_utf8_lossy(&asset.bytes)).ok()
        })
        .collect();
    SiegeClassLooks::new(&classes)
}
