//! Strict tooling and fault-isolated client shader catalogue loading.

use jkr_vfs::{VirtualFileSystem, VirtualPath};

use crate::{ShaderCatalog, ShaderError, normalize_name, parse::parse_script};

mod authority;
pub use authority::DefinitionSource;

impl ShaderCatalog {
    /// Adds the definitions of a shader script an engine carries itself, each only where
    /// the loaded content has none of that name, so game and mod shaders always win.
    /// Returns how many were added.
    pub fn add_fallback_script(&mut self, bytes: &[u8], name: &str) -> Result<usize, ShaderError> {
        let mut added = 0;
        for definition in crate::parse_shader_script(bytes, name)? {
            let key = normalize_name(&definition.name);
            if !self.definitions.contains_key(&key) {
                self.definitions.insert(key, definition);
                added += 1;
            }
        }
        Ok(added)
    }

    /// Loads every shader script, returning the first read or parse error.
    /// Useful for validation tools which must detect broken content.
    pub fn load(vfs: &VirtualFileSystem) -> Result<Self, ShaderError> {
        let mut catalog = Self::default();
        for path in shader_paths(vfs) {
            catalog.load_script(vfs, &path)?;
        }
        Ok(catalog)
    }

    /// Loads usable definitions, reporting each malformed shader independently.
    /// Unreadable files are reported once. Valid definitions retain normal override order.
    pub fn load_with_warnings(
        vfs: &VirtualFileSystem,
        mut warning: impl FnMut(&VirtualPath, &ShaderError),
    ) -> Self {
        let mut catalog = Self::default();
        for path in shader_paths(vfs) {
            match vfs.read(path.as_str()) {
                Ok(Some(asset)) => {
                    let definitions =
                        crate::recovery::parse_recovering(&asset.bytes, &path, |error| {
                            warning(&path, &error)
                        });
                    for definition in definitions {
                        catalog
                            .definitions
                            .insert(normalize_name(&definition.name), definition);
                    }
                }
                Ok(None) => warning(&path, &ShaderError::DisappearedAsset(path.clone())),
                Err(error) => warning(&path, &ShaderError::Vfs(error)),
            }
        }
        catalog
    }

    fn load_script(
        &mut self,
        vfs: &VirtualFileSystem,
        path: &VirtualPath,
    ) -> Result<(), ShaderError> {
        let asset = vfs
            .read(path.as_str())?
            .ok_or_else(|| ShaderError::DisappearedAsset(path.clone()))?;
        for definition in parse_script(&asset.bytes, path)? {
            self.definitions
                .insert(normalize_name(&definition.name), definition);
        }
        Ok(())
    }
}

fn shader_paths(vfs: &VirtualFileSystem) -> impl Iterator<Item = VirtualPath> {
    vfs.paths()
        .into_iter()
        .filter(|path| path.as_str().starts_with("shaders/") && path.as_str().ends_with(".shader"))
}

#[cfg(test)]
mod fallback_tests {
    use super::*;

    #[test]
    fn a_fallback_script_fills_only_missing_names() {
        let mut catalog = ShaderCatalog::default();
        catalog
            .add_fallback_script(b"gfx/a\n{\n{\nmap gfx/one\n}\n}\n", "first.shader")
            .unwrap();
        let added = catalog
            .add_fallback_script(
                b"gfx/a\n{\n{\nmap gfx/two\n}\n}\ngfx/b\n{\n{\nmap gfx/two\n}\n}\n",
                "second.shader",
            )
            .unwrap();
        assert_eq!(added, 1);
        assert_eq!(catalog.get("gfx/a").unwrap().stages[0].images[0], "gfx/one");
        assert!(catalog.get("gfx/b").is_some());
    }
}

