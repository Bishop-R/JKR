//! jaPRO's damage options on this server: `g_selfDamageScale`, the share of a blow on
//! oneself outside siege (stock's half), which also sets the concussion rifle's alt-fire
//! shove (`-400` times it), kept from the cvar hook; and `g_maxFallDmg`, the most a
//! landing costs ([`jkr_game_jka::damage::fall_damage`]).

use super::*;

/// `g_maxFallDmg`.
const MAX_FALL_DAMAGE: &[u8] = b"g_maxFallDmg";

impl NativeGame {
    /// The damage options, archived and announced, with jaPRO's defaults (stock's rules).
    pub(super) fn register_damage_options(&mut self) {
        let options: [(&[u8], &[u8], &[u8]); 2] = [
            (
                b"g_selfDamageScale",
                b"0.5",
                b"jaPRO: the share of a blow on oneself outside siege",
            ),
            (
                MAX_FALL_DAMAGE,
                b"0",
                b"jaPRO: the most a landing costs (0: no cap)",
            ),
        ];
        for (name, value, about) in options {
            self.cvars
                .get_announced(name, value, crate::cvars::CVAR_ARCHIVE, Some(about));
        }
    }

    /// What a landing of `delta` costs here: `dmflags`' `DF_NO_FALLING`, siege's long
    /// falls and jaPRO's cap applied.
    pub(super) fn landing_damage(&self, delta: i32, knocked_down: bool) -> Option<i32> {
        jkr_game_jka::damage::fall_damage(
            delta,
            self.cvars.integer(b"dmflags") as u32,
            knocked_down,
            self.gametype == GAMETYPE_SIEGE,
            self.cvars.integer(MAX_FALL_DAMAGE),
        )
    }
}
