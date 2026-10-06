//! Immediate map targets operating on authoritative player state and event entities.

use super::*;
use jkr_game_jka::map_target_action::Action;

/// `persistant[PERS_TEAM]` (`bg_public.h`).
const PERS_TEAM: usize = 3;
/// `GT_TEAM`: team deathmatch, the one game type `AddScore` also credits to the team.
const GT_TEAM: i32 = 6;

impl NativeGame {
    pub(super) fn apply_target_action(
        &mut self,
        action: Action,
        origin: [f32; 3],
        activator: Option<usize>,
        time: i32,
    ) {
        if let Some(event) = action.shake_event(origin) {
            if let Some(id) = self.pool.spawn_temporary(event.state(), time, None) {
                self.pool.set_broadcast(id, event.broadcast);
            }
            return;
        }
        let Some(client) = activator else {
            return;
        };
        match action {
            Action::Score(points) => {
                if self.limits.warmup_time != 0 {
                    return;
                }
                let Some(peer) = self.peer_mut(client) else {
                    return;
                };
                peer.state.persistent[PERS_SCORE] =
                    peer.state.persistent[PERS_SCORE].wrapping_add(points as u32);
                let team = peer.state.persistent[PERS_TEAM];
                if self.gametype == GT_TEAM && matches!(team, 1 | 2) {
                    self.team_scores[team as usize - 1] =
                        self.team_scores[team as usize - 1].wrapping_add(points);
                }
                self.calculate_ranks();
            }
            Action::RemovePowerups => self.remove_target_powerups(client, time),
            Action::Shake { .. } => unreachable!(),
        }
    }
}
