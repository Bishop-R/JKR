//! The body queue's bodies in the level ([`jkr_game_jka::corpses`]): copied with the dead
//! player's box, run as physics objects each frame, met by every shot but their owner's,
//! and hurt through `G_Damage` until `body_die` burns them away.

use super::*;
use jkr_game_jka::corpses::{self, Fall};
use jkr_game_jka::entity_pool::BODIES;
use jkr_game_jka::map_turret_world::{ObjectBlow, object_take};

impl NativeGame {
    /// `G_RunItem` for every linked body that falls: swept with its box through the world
    /// and every entity in it — it comes to rest on a mover, which then carries it — brought
    /// to rest on a floor, unlinked for good in a no-drop volume.
    pub(super) fn run_corpses(&mut self, server_time: i32) {
        let previous_time = self.previous_frame_time;
        let falling = (0..BODIES).any(|slot| {
            self.pool
                .body(slot)
                .filter(|id| self.pool.is_linked(*id))
                .and_then(|id| self.pool.state(id))
                .is_some_and(corpses::moving)
        });
        if !falling {
            return;
        }
        self.gather_obstacles(usize::MAX);
        let Self {
            pool,
            corpses,
            map,
            obstacles,
            ..
        } = self;
        let Some(map) = map.as_ref() else {
            return;
        };
        let world = WithPlayers {
            world: WorldCollision {
                bsp: &map.bsp,
                scratch: &map.scratch,
            },
            players: obstacles,
        };
        for slot in 0..BODIES {
            let Some(id) = pool
                .body(slot)
                .filter(|id| pool.is_linked(*id) && pool.state(*id).is_some_and(corpses::moving))
            else {
                continue;
            };
            let Some(bounds) = corpses.body(slot).map(|body| body.bounds) else {
                continue;
            };
            let Some(state) = pool.state_mut(id) else {
                continue;
            };
            let fall = corpses.fall(
                slot,
                state,
                server_time,
                previous_time,
                |from, to| world.trace(from, bounds.0, bounds.1, to, corpses::CLIP_MASK),
                |point| world.point_contents(point),
            );
            if fall == Fall::NoDrop {
                pool.show(id, false);
            }
        }
    }

    /// The linked bodies `client`'s traces meet (`CONTENTS_CORPSE`): all but its own, which
    /// they pass (`r.ownerNum`).
    pub(super) fn corpse_obstacles(
        pool: &EntityPool,
        corpses: &jkr_game_jka::corpses::Corpses,
        client: usize,
    ) -> impl Iterator<Item = BoxObstacle> {
        (0..BODIES).filter_map(move |slot| {
            let id = pool.body(slot).filter(|id| pool.is_linked(*id))?;
            let body = corpses
                .body(slot)
                .filter(|body| usize::from(body.owner) != client)?;
            Some(BoxObstacle {
                entity: id.legacy_number(),
                origin: body.current,
                bounds: body.bounds,
                contents: corpses::CONTENTS_CORPSE,
                model: None,
                linked_box: None,
            })
        })
    }

    /// The linked bodies a blast may reach at `level_time`, with their linked boxes.
    pub(super) fn corpse_splash_targets<'a>(
        pool: &'a EntityPool,
        corpses: &'a jkr_game_jka::corpses::Corpses,
        level_time: i32,
    ) -> impl Iterator<Item = SplashTarget> + 'a {
        (0..BODIES).filter_map(move |slot| {
            let id = pool.body(slot).filter(|id| pool.is_linked(*id))?;
            let body = corpses.body(slot)?;
            let (bottom, top) = body.bounds;
            Some(SplashTarget {
                number: id.legacy_number(),
                bounds: (
                    std::array::from_fn(|axis| body.current[axis] + bottom[axis] - 1.0),
                    std::array::from_fn(|axis| body.current[axis] + top[axis] + 1.0),
                ),
                origin: body.current,
                takes_damage: corpses.takes_damage(slot, level_time),
            })
        })
    }

    /// Body `number`'s slot, while it lies there.
    pub(super) fn corpse_slot(&self, number: u16) -> Option<usize> {
        self.pool
            .legacy_id(number)
            .and_then(|id| self.pool.body_slot(id))
    }

    /// `G_Damage` on body `number`: what [`object_take`] leaves of the blow comes off the
    /// health its slot holds, and `body_die` may burn it away, to be rid of a second on.
    /// Whether `number` was a body.
    pub(super) fn strike_body(&mut self, number: u16, request: DamageRequest) -> bool {
        let Some(id) = self.pool.legacy_id(number) else {
            return false;
        };
        let Some(slot) = self.pool.body_slot(id) else {
            return false;
        };
        let blow = ObjectBlow {
            attacker: request
                .attacker
                .and_then(|attacker| self.blow_attacker(attacker.client)),
            damage: request.damage,
            flags: request.flags,
            means: request.means,
            siege: self.gametype == GAMETYPE_SIEGE,
            siege_round_begun: self.siege.as_ref().is_some_and(|siege| siege.round.begun),
            friendly_fire_objectives: self.integer_cvar(b"g_ff_objectives", 0) != 0,
        };
        let takes_damage = self.corpses.takes_damage(slot, request.level_time);
        if let Some(take) = object_take(number, takes_damage, 0, &blow)
            && let Some(state) = self.pool.state_mut(id)
            && let Some(rid) = self.corpses.hurt(slot, take, state, request.level_time)
        {
            self.pool.rid_body(id, rid);
        }
        true
    }
}
