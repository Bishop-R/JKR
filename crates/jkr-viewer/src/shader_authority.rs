//! JKA archive authority at the adapter boundary; not a cryptographic trust claim.
use jkr_shader::ShaderCatalog;
use jkr_vfs::{AssetSource, VirtualFileSystem};
use std::path::Path;

fn authoritative(source: &AssetSource) -> bool {
    let path = Path::new(source.mount_name.as_ref());
    let base = path
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name.eq_ignore_ascii_case("base"));
    base && path.file_name().is_some_and(|name| {
        ["assets0.pk3", "assets1.pk3", "assets2.pk3", "assets3.pk3"]
            .iter()
            .any(|expected| name.eq_ignore_ascii_case(expected))
    })
}

/// The shaders JKR carries for client options the retail data lacks (`assets/shaders`).
const CARRIED: [(&str, &[u8]); 1] = [(
    "jkr/eternaljk/japro_players.shader",
    include_bytes!("../assets/shaders/eternaljk/japro_players.shader"),
)];

/// Load with the frozen startup policy, logging each discarded definition once per catalogue,
/// then the carried shaders where the content has none of their names.
pub(crate) fn load(vfs: &VirtualFileSystem, protect: bool) -> ShaderCatalog {
    let warning = |path: &jkr_vfs::VirtualPath, error: &jkr_shader::ShaderError| {
        crate::log::progress(format_args!("warning: shader recovery in {path}: {error}"));
    };
    let mut catalog = if protect {
        ShaderCatalog::load_with_authority(vfs, authoritative, warning, |name, winner, loser| {
            crate::log::progress(format_args!(
                "shader authority: suppressed {name}; winner={} [{}]; loser={} [{}]",
                winner.asset.mount_name, winner.script, loser.asset.mount_name, loser.script,
            ));
        })
    } else {
        ShaderCatalog::load_with_warnings(vfs, warning)
    };
    for (name, script) in CARRIED {
        if let Err(error) = catalog.add_fallback_script(script, name) {
            crate::log::progress(format_args!("warning: carried shaders {name}: {error}"));
        }
    }
    catalog
}

#[cfg(test)]
mod carried_tests {
    use super::*;

    #[test]
    fn the_carried_shaders_parse() {
        let mut catalog = ShaderCatalog::default();
        for (name, script) in CARRIED {
            assert!(catalog.add_fallback_script(script, name).unwrap() > 0, "{name}");
        }
        let duel = catalog.get("gfx/effects/duelShader").expect("duel shader");
        assert_eq!(duel.stages.len(), 3);
        assert_eq!(catalog.get("gfx/effects/raceShader").unwrap().stages.len(), 1);
    }
}
