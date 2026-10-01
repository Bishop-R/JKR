//! Optional source authority, independent of any game's archive naming conventions.
use super::*;
use jkr_vfs::AssetSource;
use std::collections::HashMap;

/// Script and mount supplying a shader definition.
#[derive(Clone, Debug)]
pub struct DefinitionSource {
    /// Normalized virtual script path.
    pub script: VirtualPath,
    /// Mounted asset origin.
    pub asset: AssetSource,
}

impl ShaderCatalog {
    /// Prefers definitions from sources classified as authoritative by the application.
    ///
    /// Authoritative scripts are read even when another mount hides the same script path.
    /// Between authoritative sources normal script/mount order wins. Visible non-authoritative
    /// scripts retain legacy order and may add unique definitions, but cannot replace preferred
    /// ones. Each discarded definition reports its name, winner, and loser to `suppressed`.
    /// Use `load_with_warnings` instead for exactly the legacy visible-file, last-wins policy.
    pub fn load_with_authority(
        vfs: &VirtualFileSystem,
        authoritative: impl Fn(&AssetSource) -> bool,
        mut warning: impl FnMut(&VirtualPath, &ShaderError),
        mut suppressed: impl FnMut(&str, &DefinitionSource, &DefinitionSource),
    ) -> Self {
        let mut catalog = Self::default();
        let mut origins: HashMap<String, DefinitionSource> = HashMap::new();
        let paths: Vec<_> = shader_paths(vfs).collect();
        for path in &paths {
            for mount in vfs.mounts() {
                let source = AssetSource {
                    mount_id: mount.id,
                    mount_name: mount.name,
                };
                if !authoritative(&source) {
                    continue;
                }
                match vfs.read_from_mount(source.mount_id, path.as_str()) {
                    Ok(Some(asset)) => {
                        for definition in
                            crate::recovery::parse_recovering(&asset.bytes, path, |error| {
                                warning(path, &error)
                            })
                        {
                            let name = normalize_name(&definition.name);
                            origins.insert(
                                name.clone(),
                                DefinitionSource {
                                    script: path.clone(),
                                    asset: source.clone(),
                                },
                            );
                            catalog.definitions.insert(name, definition);
                        }
                    }
                    Ok(None) => {}
                    Err(error) => warning(path, &ShaderError::Vfs(error)),
                }
            }
        }
        let mut reported = std::collections::HashSet::new();
        for path in &paths {
            match vfs.read(path.as_str()) {
                Ok(Some(asset)) => {
                    if authoritative(&asset.source) {
                        continue;
                    }
                    let source = DefinitionSource {
                        script: path.clone(),
                        asset: asset.source,
                    };
                    for definition in
                        crate::recovery::parse_recovering(&asset.bytes, path, |error| {
                            warning(path, &error)
                        })
                    {
                        let name = normalize_name(&definition.name);
                        if let Some(winner) = origins.get(&name) {
                            let conflict = (name.clone(), source.asset.mount_id, path.clone());
                            if reported.insert(conflict) {
                                suppressed(&name, winner, &source);
                            }
                        } else {
                            catalog.definitions.insert(name, definition);
                        }
                    }
                }
                Ok(None) => warning(path, &ShaderError::DisappearedAsset(path.clone())),
                Err(error) => warning(path, &ShaderError::Vfs(error)),
            }
        }
        catalog
    }
}
