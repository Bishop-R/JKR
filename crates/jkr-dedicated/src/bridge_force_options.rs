//! jaPRO's Force options on this server ([`jkr_game_jka::force_powers::ForceRules`]):
//! `g_teamHealScale`, `g_teamEnergizeScale` and `g_teamAbsorbScale` (1, stock's amounts)
//! and `g_forcePowerDisableFFA` (0, no power withheld), archived and announced; the cvar
//! hook keeps them for the players' and the NPCs' powers.

use super::*;

impl NativeGame {
    /// The four options, at jaPRO's defaults, which are stock's.
    pub(super) fn register_force_options(&mut self) {
        let options: [(&[u8], &[u8], &[u8]); 4] = [
            (
                b"g_teamHealScale",
                b"1",
                b"jaPRO: team heal gives this many times stock's 50, 33 or 25",
            ),
            (
                b"g_teamEnergizeScale",
                b"1",
                b"jaPRO: team energize gives this many times stock's 50, 33 or 25",
            ),
            (
                b"g_teamAbsorbScale",
                b"1",
                b"jaPRO: absorb takes this many times stock's share of a teammate's power",
            ),
            (
                b"g_forcePowerDisableFFA",
                b"0",
                b"jaPRO: Force powers (a bit each) no one may use outside a duel",
            ),
        ];
        for (name, value, about) in options {
            self.cvars
                .get_announced(name, value, crate::cvars::CVAR_ARCHIVE, Some(about));
        }
    }
}
