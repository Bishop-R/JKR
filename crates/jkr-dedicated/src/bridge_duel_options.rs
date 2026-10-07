//! jaPRO's duel options on this server: the duellists' start health and armour, the
//! distance limit (kept on by default, as in stock), and FFA duellists respawning where
//! they fell.

use super::*;
use jkr_game_jka::duel::DuelRules;
use jkr_game_jka::means_of_death::{MOD_SUICIDE, MOD_TEAM_CHANGE};

const START_HEALTH: &[u8] = b"g_duelStartHealth";
const START_ARMOR: &[u8] = b"g_duelStartArmor";
const DISTANCE_LIMIT: &[u8] = b"g_duelDistanceLimit";
const RESPAWN: &[u8] = b"g_duelRespawn";

impl NativeGame {
    /// The duel options, archived and announced: jaPRO's defaults but the distance
    /// limit's, which stays on as in stock (jaPRO's own default, 0, lifts it).
    pub(super) fn register_duel_options(&mut self) {
        let options: [(&[u8], &[u8], &[u8]); 4] = [
            (
                START_HEALTH,
                b"0",
                b"jaPRO: the duellists' health once a duel starts (0: as they are)",
            ),
            (
                START_ARMOR,
                b"0",
                b"jaPRO: their armour then, with a start health",
            ),
            (
                DISTANCE_LIMIT,
                b"1",
                b"jaPRO: a duel ends 1024 apart (0 lifts the limit)",
            ),
            (
                RESPAWN,
                b"0",
                b"jaPRO: in FFA, one killed in a duel respawns where it fell",
            ),
        ];
        for (name, value, about) in options {
            self.cvars
                .get_announced(name, value, crate::cvars::CVAR_ARCHIVE, Some(about));
        }
    }

    /// The duel options as they stand.
    pub(super) fn duel_rules(&self) -> DuelRules {
        DuelRules {
            start_health: self.cvars.integer(START_HEALTH),
            start_armor: self.cvars.integer(START_ARMOR),
            distance_limit: self.cvars.integer(DISTANCE_LIMIT) != 0,
        }
    }

    /// `player_die`'s note for jaPRO's `g_duelRespawn`: one killed duelling in FFA (not
    /// by its own hand nor by a team change) comes back where it fell, facing as it
    /// faced; any other death forgets the place.
    pub(super) fn note_duel_death(&mut self, client: usize, means: u32) {
        let respawn = self.cvars.integer(RESPAWN) != 0 && self.gametype == GAMETYPE_FFA;
        if let Some(peer) = self.peer_mut(client) {
            let duelled = respawn
                && peer.state.duel_in_progress()
                && means != MOD_SUICIDE
                && means != MOD_TEAM_CHANGE;
            peer.duel_respawn = duelled.then(|| (peer.state.origin(), peer.state.view_angles()[1]));
        }
    }
}
