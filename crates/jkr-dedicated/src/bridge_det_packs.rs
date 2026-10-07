//! A dying player's det packs (`player_die`, `g_combat.c:2416-2420`): blown 100 to 300 ms
//! on with their warning (`BlowDetpacks`), or — a Siege team change — freed at once
//! (`RemoveDetpacks`), before the rest of the death draws from the generator.

use super::*;
use jkr_game_jka::mines::{ChargeFrame, blow_det_packs, remove_det_packs};

/// `GT_SIEGE`.
const GT_SIEGE: i32 = 7;

impl NativeGame {
    /// `player_die`'s det packs for `client`, dying by `means` at `level_time`.
    pub(super) fn det_packs_on_death(&mut self, client: usize, means: u32, level_time: i32) {
        let team_change =
            self.gametype == GT_SIEGE && means == jkr_game_jka::means_of_death::MOD_TEAM_CHANGE;
        let mut rng = self.deaths.rng;
        let Self {
            server,
            world,
            players,
            charges,
            sounds,
            told,
            pool,
            ..
        } = self;
        let Some(peer) = players
            .at(client)
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle))
        else {
            return;
        };
        let changed = if team_change {
            remove_det_packs(&mut peer.state, charges, level_time)
        } else {
            let mut frame = ChargeFrame {
                sounds: &mut |name| {
                    sounds.index(name, &mut |index, value| {
                        told.push(Told::ConfigString {
                            index,
                            previous: Vec::new(),
                            value: value.to_vec(),
                        })
                    })
                },
                raise: &mut |event| {
                    let _ = pool.spawn_temporary(event.state(), level_time, None);
                },
            };
            blow_det_packs(&mut peer.state, charges, level_time, &mut rng, &mut frame)
        };
        if changed {
            peer.movement = peer.movement.reseeded(&peer.state);
        }
        self.deaths.rng = rng;
    }
}
