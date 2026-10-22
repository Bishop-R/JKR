//! jaPRO's team spotting on this server (`g_allowSpotting`, `spot`): a player in a team
//! game marks the first enemy in sight within 15 degrees of its aim (anyone, with 2), and
//! every teammate gets a score plum over the one spotted and a red "!" (`Cmd_Spot_f`,
//! `SpotIcon`).

use super::bridge_cheats::{ALIVE, NO_INTERMISSION};
use super::*;
use jkr_game_jka::pmove::MovementCollision;

const ALLOW_SPOTTING: &[u8] = b"g_allowSpotting";
/// `EV_SCOREPLUM` and its fields: whose it is (`otherEntityNum`) and its number (`time`).
const EV_SCOREPLUM: u32 = 98;
const ES_OTHER_ENTITY_NUM: usize = 59;
const ES_TIME: usize = 65;
const TEAM_SPECTATOR: i32 = 3;
const GT_TEAM: i32 = 6;
/// `MASK_SOLID`.
const MASK_SOLID: u32 = 1;
/// How far a spot reaches.
const RANGE: f32 = 8_192.0;

impl NativeGame {
    /// `g_allowSpotting`, archived and announced, 0 (jaPRO's).
    pub(super) fn register_spotting(&mut self) {
        let about: &[u8] = b"jaPRO: spot marks an enemy for the team (2: anyone)";
        self.cvars.get_announced(
            ALLOW_SPOTTING,
            b"0",
            crate::cvars::CVAR_ARCHIVE,
            Some(about),
        );
    }

    /// `spot` from `client` (`CMD_NOINTERMISSION|CMD_ALIVE`): whether `command` was it.
    /// At most every quarter second (a second after a hit), the first player in the
    /// standings (`level.sortedClients`) who is no teammate, was not spotted in the last
    /// three seconds, stands within 8192 in plain sight of the spotter's eyes and within
    /// 15 degrees of its aim.
    pub(super) fn spot_command(&mut self, client: usize, command: &[u8], level_time: i32) -> bool {
        if !command.eq_ignore_ascii_case(b"spot") {
            return false;
        }
        if !self.passes_gates(client, command, NO_INTERMISSION | ALIVE) {
            return true;
        }
        let allow = self.cvars.integer(ALLOW_SPOTTING);
        if allow == 0 || self.gametype < GT_TEAM {
            return true;
        }
        let Some(me) = self.peer_mut(client) else {
            return true;
        };
        if me.last_spot_time > level_time || me.session.team == TEAM_SPECTATOR {
            return true;
        }
        me.last_spot_time = level_time + 250;
        let (team, view) = (me.session.team, me.state.view_angles());
        let mut eye = me.state.origin();
        eye[2] += me.state.view_height() as f32;
        let (sorted, _) = self.sorted();
        let target = sorted
            .iter()
            .map(|&other| usize::from(other))
            .find(|&other| {
                self.spottable(other, (client, team, allow == 2), (eye, view), level_time)
            });
        let Some(target) = target else {
            return true;
        };
        let Some(spotted) = self.peer_mut(target) else {
            return true;
        };
        spotted.last_spotted_time = level_time;
        let at = spotted.state.origin();
        if let Some(me) = self.peer_mut(client) {
            me.last_spot_time = level_time + 1_000;
        }
        for mate in sorted.into_iter().map(usize::from) {
            if self
                .peer(mate)
                .is_some_and(|peer| peer.session.team == team)
            {
                self.spot_icon(mate, at, level_time);
                self.told
                    .push(Told::One(mate, b"cp \"^1!\n\n\n\n\n\n\n\n\n\n\"".to_vec()));
            }
        }
        true
    }

    /// Whether `other` may be spotted by `spotter` of `team` (anyone, `any_team`), looking
    /// from `eye` along `view`. Spectators are never spotted (jaPRO did not check).
    fn spottable(
        &self,
        other: usize,
        (spotter, team, any_team): (usize, i32, bool),
        (eye, view): ([f32; 3], [f32; 3]),
        level_time: i32,
    ) -> bool {
        let Some(peer) = self.peer(other) else {
            return false;
        };
        if other == spotter
            || peer.session.team == TEAM_SPECTATOR
            || (!any_team && peer.session.team == team)
            || peer.last_spotted_time > level_time - 3_000
        {
            return false;
        }
        let origin = peer.state.origin();
        let towards: [f32; 3] = std::array::from_fn(|axis| origin[axis] - eye[axis]);
        if towards.iter().map(|axis| axis * axis).sum::<f32>() > RANGE * RANGE {
            return false;
        }
        let seen = self.map.as_ref().is_none_or(|map| {
            let world = WorldCollision {
                bsp: &map.bsp,
                scratch: &map.scratch,
            };
            world
                .trace(eye, [0.0; 3], [0.0; 3], origin, MASK_SOLID)
                .fraction
                == 1.0
        });
        let (pitch, yaw) = jkr_game_jka::damage::vector_to_angles(towards);
        seen && jkr_game_jka::force_throw::in_field_of_vision(view, 30.0, [pitch, yaw])
    }

    /// `SpotIcon`: a score plum of nothing for `mate` alone, wherever it is, over `at`.
    fn spot_icon(&mut self, mate: usize, at: [f32; 3], level_time: i32) {
        let mut plum = EventEntity {
            event: EV_SCOREPLUM,
            parameter: 1,
            origin: at,
            client: None,
            broadcast: true,
            extra: [(0, 0); 12],
        };
        plum.extra[0] = (ES_OTHER_ENTITY_NUM, mate as u32);
        plum.extra[1] = (ES_TIME, 0);
        if let Some(id) = self.pool.spawn_temporary(plum.state(), level_time, None) {
            self.pool.set_only_for(id, Some(mate as u16));
        }
    }
}
