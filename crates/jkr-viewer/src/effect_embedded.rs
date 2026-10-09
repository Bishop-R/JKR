//! Effect definitions JKR carries for client options whose files the retail data lacks:
//! EternalJK's own (GPLv2, as JKR), kept verbatim under `assets/effects/eternaljk/`. They
//! are found only where the game data has no file of the same name.

use jkr_effect::{EffectDefinition, EffectError};
use jkr_vfs::VirtualFileSystem;

/// The carried definitions, by their path under `effects/` without `.efx`, lower case.
const EMBEDDED: [(&str, &str); 1] = [(
    "mp/drainwide_japro",
    include_str!("../assets/effects/eternaljk/drainwide_japro.efx"),
)];

/// `jkr_effect::load_effect`, falling back on a carried definition.
pub(crate) fn load_effect(
    vfs: &VirtualFileSystem,
    name: &str,
) -> Result<EffectDefinition, EffectError> {
    jkr_effect::load_effect(vfs, name).or_else(|error| {
        let path = jkr_effect::effect_path(name).to_ascii_lowercase();
        let key = path.trim_start_matches("effects/").trim_end_matches(".efx");
        match EMBEDDED.iter().find(|(name, _)| *name == key) {
            Some((_, source)) => jkr_effect::parse_effect(source),
            None => Err(error),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_carried_drain_arc_parses_and_is_found_without_the_file() {
        let vfs = VirtualFileSystem::new();
        let definition = load_effect(&vfs, "mp/drainwide_japro").unwrap();
        assert!(!definition.components.is_empty());
        assert!(load_effect(&vfs, "effects/mp/DrainWide_JaPRO.efx").is_ok());
        assert!(load_effect(&vfs, "mp/drainwide").is_err());
    }
}
