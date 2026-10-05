//! Multiplayer `CS_SHADERSTATE` and `remapShader` compatibility state.
use jkr_protocol::GameState;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// The multiplayer shader-state configstring (`codemp/game/bg_public.h`).
pub const SHADER_STATE_CONFIG: usize = 24;
const MAX_REMAPS: usize = 1024;

/// Persistent, one-hop renderer aliases. Removing an entry from a configstring
/// does not undo it: stock cgame applies entries, and self-remaps undo aliases.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderRemaps {
    identity: u64,
    revision: u64,
    tables: [ShaderRemapTable; 2],
}
impl Default for ShaderRemaps {
    fn default() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            identity: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            revision: 0,
            tables: Default::default(),
        }
    }
}
impl ShaderRemaps {
    /// Seed the renderer state of a new gamestate.
    pub fn from_game_state(game: &GameState) -> Self {
        let mut state = Self::default();
        state.apply_config(game.config_string(SHADER_STATE_CONFIG).unwrap_or_default());
        state
    }
    /// Session identity and revision, safe when a resident map is reused on reconnect.
    pub fn stamp(&self) -> (u64, u64) {
        (self.identity, self.revision)
    }
    /// Monotonic change counter within this session, including map resets.
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// Reset aliases for a replacement gamestate, then apply its initial state.
    pub(crate) fn reset(&mut self, game: &GameState) {
        let revision = self.revision.wrapping_add(1);
        *self = Self::from_game_state(game);
        self.revision = revision;
    }
    /// Tayst mode 1 excludes player-texture configstring remaps; mode 2 accepts
    /// all. Reliable remapShader commands are accepted by either nonzero mode.
    pub fn table(&self, mode: i64) -> Option<&ShaderRemapTable> {
        (mode != 0).then(|| &self.tables[usize::from(mode != 1)])
    }
    /// Apply complete `old=new:offset@` entries; a truncated tail is ignored.
    pub fn apply_config(&mut self, bytes: &[u8]) {
        let Ok(mut rest) = std::str::from_utf8(bytes) else {
            return;
        };
        while let Some((original, tail)) = rest.split_once('=') {
            let Some((target, tail)) = tail.split_once(':') else {
                break;
            };
            let Some((offset, tail)) = tail.split_once('@') else {
                break;
            };
            self.apply(original, target, offset, false);
            rest = tail;
        }
    }
    /// EternalJK's `clearRemaps` drops every alias until later entries arrive.
    /// Like its renderer, destination time offsets stay with their shaders.
    pub fn clear(&mut self) {
        let changed = self.tables.iter().any(|table| !table.aliases.is_empty());
        for table in &mut self.tables {
            table.aliases.clear();
        }
        self.revision = self.revision.wrapping_add(u64::from(changed));
    }
    /// Consume the already tokenized reliable cgame command (no wire changes).
    pub(crate) fn command(&mut self, arguments: &[Vec<u8>]) -> bool {
        if arguments
            .first()
            .is_none_or(|a| !a.eq_ignore_ascii_case(b"remapShader"))
        {
            return false;
        }
        if arguments.len() == 4 {
            if let (Ok(old), Ok(new), Ok(offset)) = (
                std::str::from_utf8(&arguments[1]),
                std::str::from_utf8(&arguments[2]),
                std::str::from_utf8(&arguments[3]),
            ) {
                self.apply(old, new, offset, true);
            }
        }
        true
    }
    fn apply(&mut self, original: &str, target: &str, offset: &str, command: bool) {
        let (Some(original), Some(target)) = (shader_name(original), shader_name(target)) else {
            return;
        };
        let offset = offset
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
            .unwrap_or(0.);
        let mut changed = self.tables[1].apply(&original, &target, offset);
        if command || !original.starts_with("models/players/") {
            changed |= self.tables[0].apply(&original, &target, offset);
        }
        self.revision = self.revision.wrapping_add(u64::from(changed));
    }
}

/// Resolved remaps for one client policy. This is material data, not wire state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShaderRemapTable {
    aliases: BTreeMap<String, String>,
    offsets: BTreeMap<String, f32>,
}
impl ShaderRemapTable {
    /// Shader used for this original name. Aliases deliberately do not chain.
    pub fn destination<'a>(&'a self, original: &'a str) -> &'a str {
        self.aliases.get(original).map_or(original, String::as_str)
    }
    /// Stock stores timeOffset on the destination shader, shared by its users.
    pub fn time_offset(&self, destination: &str) -> f32 {
        self.offsets.get(destination).copied().unwrap_or(0.)
    }
    /// Whether this state has an explicit alias or clock update for a name.
    pub fn affects(&self, original: &str) -> bool {
        self.aliases.contains_key(original) || self.offsets.contains_key(original)
    }
    /// Original and replacement names, for the console's listRemaps command.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.aliases.iter().map(|(a, b)| (a.as_str(), b.as_str()))
    }
    fn apply(&mut self, original: &str, target: &str, offset: f32) -> bool {
        if (!self.aliases.contains_key(original) && self.aliases.len() >= MAX_REMAPS)
            || (!self.offsets.contains_key(target) && self.offsets.len() >= MAX_REMAPS * 2)
        {
            return false;
        }
        if self.aliases.get(original).is_some_and(|v| v == target)
            && self.time_offset(target) == offset
        {
            return false;
        }
        self.aliases.insert(original.to_owned(), target.to_owned());
        self.offsets.insert(target.to_owned(), offset);
        true
    }
}

/// Renderer shader identity: case-insensitive virtual path without its extension.
pub fn shader_name(name: &str) -> Option<String> {
    if name.is_empty()
        || name.len() >= 64
        || name
            .bytes()
            .any(|b| b < 32 || matches!(b, b'=' | b':' | b'@'))
    {
        return None;
    }
    let mut name = name.replace('\\', "/").to_ascii_lowercase();
    let start = name.rfind('/').map_or(0, |i| i + 1);
    if let Some(dot) = name[start..].rfind('.') {
        name.truncate(start + dot);
    }
    (!name.is_empty() && !name.split('/').any(|part| part == "..")).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_drops_aliases_until_the_server_sends_them_again() {
        let mut remaps = ShaderRemaps::default();
        let state = b"textures/a=textures/b:1.5@models/players/x/y=textures/c:0@";
        remaps.apply_config(state);
        let (identity, revision) = remaps.stamp();
        remaps.clear();
        assert_eq!(remaps.stamp(), (identity, revision + 1));
        for mode in [1, 2] {
            let table = remaps.table(mode).unwrap();
            assert_eq!(table.entries().count(), 0);
            assert_eq!(table.destination("textures/a"), "textures/a");
            assert_eq!(table.time_offset("textures/b"), 1.5);
        }
        remaps.clear();
        assert_eq!(remaps.revision(), revision + 1);
        remaps.apply_config(state);
        let all = remaps.table(2).unwrap();
        assert_eq!(all.destination("textures/a"), "textures/b");
        assert_eq!(all.entries().count(), 2);
        assert_eq!(remaps.table(1).unwrap().entries().count(), 1);
    }
}
