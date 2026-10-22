//! A dying player's det packs (`player_die`, `g_combat.c:2416-2420`): blown 100 to 300 ms
//! on with their warning (`BlowDetpacks`), or — a Siege team change — freed at once
//! (`RemoveDetpacks`), before the rest of the death draws from the generator. Also what
//! a flying det pack makes of what it runs into (`charge_stick`).

use super::*;
use jkr_game_jka::mines::{ChargeFrame, Struck, blow_det_packs, remove_det_packs};

/// `GT_SIEGE`.
const GT_SIEGE: i32 = 7;
/// The wire fields `eType` and `weapon`; `ET_MOVER`.
const ES_TYPE: usize = 8;
const ES_WEAPON: usize = 14;
const ET_MOVER: u32 = 6;

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
                struck: &|_| Struck::Bounce,
            };
            blow_det_packs(&mut peer.state, charges, level_time, &mut rng, &mut frame)
        };
        if changed {
            peer.movement = peer.movement.reseeded(&peer.state);
        }
        self.deaths.rng = rng;
    }
}

impl NativeGame {
    /// What entity `number` is to a det pack running into it (`charge_stick`): a
    /// breakable brush (never moving here), another mover, a client or a thing without a
    /// weapon, or a thing with one.
    pub(super) fn charge_struck(&self, number: u16) -> Struck {
        if self
            .breakables
            .iter()
            .any(|(id, _)| id.legacy_number() == number)
        {
            return Struck::Breakable;
        }
        let state = self
            .pool
            .legacy_id(number)
            .and_then(|id| self.pool.state(id));
        let field = |index| state.and_then(|state| state.raw_field(index)).unwrap_or(0);
        if field(ES_TYPE) == ET_MOVER {
            return Struck::Mover;
        }
        let client = usize::from(number) < self.players.places() || self.npcs.roster.is_npc(number);
        if client || field(ES_WEAPON) == 0 {
            Struck::Bounce
        } else {
            Struck::Explode
        }
    }
}
