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

/// Load with the frozen startup policy, logging each discarded definition once per catalogue.
pub(crate) fn load(vfs: &VirtualFileSystem, protect: bool) -> ShaderCatalog {
    let warning = |path: &jkr_vfs::VirtualPath, error: &jkr_shader::ShaderError| {
        crate::log::progress(format_args!("warning: shader recovery in {path}: {error}"));
    };
    if !protect {
        return ShaderCatalog::load_with_warnings(vfs, warning);
    }
    ShaderCatalog::load_with_authority(vfs, authoritative, warning, |name, winner, loser| {
        crate::log::progress(format_args!(
            "shader authority: suppressed {name}; winner={} [{}]; loser={} [{}]",
            winner.asset.mount_name, winner.script, loser.asset.mount_name, loser.script,
        ));
    })
}
