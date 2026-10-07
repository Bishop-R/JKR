//! jaPRO's weapon options on this server: `g_selfDamageScale`, the share of a blow on
//! oneself outside siege (stock's half), which also sets the concussion rifle's alt-fire
//! shove (`-400` times it). The option is kept from the cvar hook
//! ([`NativeGame::self_damage_scale`](super::NativeGame)).

use super::*;

impl NativeGame {
    /// `g_selfDamageScale`, archived and announced, 0.5 (jaPRO's, stock's half).
    pub(super) fn register_weapon_options(&mut self) {
        let about: &[u8] = b"jaPRO: the share of a blow on oneself outside siege";
        self.cvars.get_announced(
            b"g_selfDamageScale",
            b"0.5",
            crate::cvars::CVAR_ARCHIVE,
            Some(about),
        );
    }
}
