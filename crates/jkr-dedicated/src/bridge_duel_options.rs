//! jaPRO's duel options on this server: the duellists' start health and armour, the
//! distance limit (kept on by default, as in stock), FFA duellists respawning where they
//! fell, and the duels' own Force regeneration times.

use super::*;
use jkr_game_jka::duel::DuelRules;
use jkr_game_jka::means_of_death::{MOD_SUICIDE, MOD_TEAM_CHANGE};

const START_HEALTH: &[u8] = b"g_duelStartHealth";
const START_ARMOR: &[u8] = b"g_duelStartArmor";
const DISTANCE_LIMIT: &[u8] = b"g_duelDistanceLimit";
const RESPAWN: &[u8] = b"g_duelRespawn";
pub(super) const ALLOW_GUN_DUEL: &[u8] = b"g_allowGunDuel";
/// jaPRO's regeneration times in saber and Force duels; -1, this server's default, follows
/// `g_forceRegenTime` as stock does (jaPRO's own default is a fixed 200).
pub(super) const SABER_DUEL_REGEN: &[u8] = b"g_saberDuelForceRegenTime";
pub(super) const FORCE_DUEL_REGEN: &[u8] = b"g_forceDuelForceRegenTime";

impl NativeGame {
    /// The duel options, archived and announced: jaPRO's defaults but the distance
    /// limit's, which stays on as in stock (jaPRO's own default, 0, lifts it).
    pub(super) fn register_duel_options(&mut self) {
        let options: [(&[u8], &[u8], &[u8]); 7] = [
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
            (
                ALLOW_GUN_DUEL,
                b"1",
                b"jaPRO: engage_gunduel [weapon] challenges to a duel with one weapon (or all)",
            ),
            (
                SABER_DUEL_REGEN,
                b"-1",
                b"jaPRO: ms a Force point takes to come back in a saber duel (-1: g_forceRegenTime)",
            ),
            (
                FORCE_DUEL_REGEN,
                b"-1",
                b"jaPRO: ms a Force point takes to come back in a Force duel (-1: g_forceRegenTime)",
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
            protection: self.cvars.integer(b"g_spawnInvulnerability"),
        }
    }

    /// The regeneration pace of player `client`'s duel (`w_force.c:6130-6135` of jaPRO): a
    /// saber duel's or a Force duel's own time, one at least; `None` outside a duel, in a
    /// gun duel, or where the time follows `g_forceRegenTime` (-1).
    pub(super) fn duel_regen_pace(&self, client: usize) -> Option<f64> {
        let peer = self.peer(client)?;
        if !peer.state.duel_in_progress() {
            return None;
        }
        let name = match peer.force.duel_kind {
            jkr_game_jka::duel_kind::DuelKind::Saber => SABER_DUEL_REGEN,
            jkr_game_jka::duel_kind::DuelKind::Force => FORCE_DUEL_REGEN,
            jkr_game_jka::duel_kind::DuelKind::Gun(_) => return None,
        };
        let time = self.cvars.integer(name);
        (time >= 0).then(|| f64::from(time.max(1)))
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

#[cfg(test)]
mod tests {
    use super::super::{Identity, NativeGame};
    use super::{FORCE_DUEL_REGEN, SABER_DUEL_REGEN};
    use jkr_game_jka::duel_kind::DuelKind;
    use jkr_network::LegacyGameHost;
    use jkr_protocol::UserCommand;

    /// `ps.duelInProgress`.
    const PS_DUEL_IN_PROGRESS: usize = 119;

    /// jaPRO's duel regeneration times (`w_force.c:6130-6135` of jaPRO): none outside a
    /// duel, in a gun duel, or at -1 (this server's default, following
    /// `g_forceRegenTime`); else the saber or Force duel's own, one at least.
    #[test]
    fn duels_regenerate_at_their_own_pace_when_set() {
        let identity = Identity {
            hostname: b"test".to_vec(),
            mapname: b"mp/ffa3".to_vec(),
        };
        let cvars = crate::cvars::Cvars::server();
        let mut game = NativeGame::with_cvars(identity, None, 4, 4, cvars).unwrap();
        game.register_duel_options();
        let userinfo = b"\\name\\a\\rate\\25000\\snaps\\40\\model\\kyle/default\\forcepowers\\7-1-032330000000001333\\color1\\4\\color2\\4\\handicap\\100\\cg_predictItems\\1\\saber1\\single_1\\saber2\\none\\char_color_red\\255\\char_color_green\\255\\char_color_blue\\255\\ip\\127.0.0.1:1";
        game.client_connect(0, userinfo).unwrap();
        game.enter_world(0, &UserCommand::default(), 100);
        assert_eq!(game.duel_regen_pace(0), None);
        game.peer_mut(0)
            .unwrap()
            .state
            .set_raw_field(PS_DUEL_IN_PROGRESS, 1);
        assert_eq!(game.duel_regen_pace(0), None);
        game.cvars.set(SABER_DUEL_REGEN, b"500");
        game.cvars.set(FORCE_DUEL_REGEN, b"0");
        assert_eq!(game.duel_regen_pace(0), Some(500.0));
        game.peer_mut(0).unwrap().force.duel_kind = DuelKind::Force;
        assert_eq!(game.duel_regen_pace(0), Some(1.0));
        game.peer_mut(0).unwrap().force.duel_kind = DuelKind::Gun(13);
        assert_eq!(game.duel_regen_pace(0), None);
    }
}
