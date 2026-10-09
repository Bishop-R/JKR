//! Who `G_MoverPush` moves (`g_mover.c:350-351`): players and NPCs — `ET_PLAYER` and
//! `ET_NPC` — and the physics objects, here dropped items and the body queue's bodies, for
//! every mover this server runs: a door, a path mover, a scripted brush. Each is offered as
//! a [`Pushee`]; what the push decides is carried out here: moved (a client's view turned
//! with a turning mover), crushed (`MOD_CRUSH`), freed (a dropped weapon) or — a dead NPC
//! that will not move — its body removed (`NPC_RemoveBody`).

use super::*;
use jkr_game_jka::entity_pool::BODIES;
use jkr_game_jka::movers;
use jkr_game_jka::path_movers::{Fate, Pushee, Unpushable};

/// `ps.delta_angles[YAW]` on the wire.
const PS_DELTA_YAW: usize = 11;
/// `FL_NOTARGET`: a dead NPC with it blocks as a living one does.
const FL_NOTARGET: u32 = 0x20;

impl NativeGame {
    /// Everyone a mover may push: the players in the game — a dead one by its corpse's
    /// box — the NPCs that have begun, a dead one's body among them, and the physics
    /// objects: the dropped items, the bodies in the body queue, the det packs, the
    /// holocrons and the Jedi Master's saber lying in the world.
    pub(super) fn pushees(&self) -> Vec<Pushee> {
        let players = (0..self.players.places()).filter_map(|client| {
            let peer = self.peer(client)?;
            if !peer.playing() {
                return None;
            }
            let (bottom, mut top) = peer.movement.box_bounds();
            if let Some((_, corpse_top)) = peer.corpse {
                top[2] = corpse_top;
            }
            Some(Pushee {
                number: client as u16,
                origin: peer.state.origin(),
                bounds: (bottom, top),
                ground: peer.state.ground_entity_num(),
                client: true,
                fate: if peer.health < 1 {
                    Fate::Crushed
                } else {
                    Fate::Blocks
                },
                fragile: false,
            })
        });
        let npcs = self
            .npcs
            .roster
            .actors
            .iter()
            .filter(|npc| npc.begun())
            .map(|npc| Pushee {
                number: npc.number,
                origin: npc.player.origin(),
                bounds: npc.bounds(),
                ground: npc.player.ground_entity_num(),
                client: true,
                fate: if npc.health <= 0 && npc.flags & FL_NOTARGET == 0 {
                    Fate::BodyRemoved
                } else {
                    Fate::Blocks
                },
                fragile: false,
            });
        let items = self.items.iter().filter_map(|(id, pickup)| {
            jkr_game_jka::dropped_items::pushee(id.legacy_number(), pickup)
        });
        let bodies = (0..BODIES).filter_map(|slot| {
            let id = self.pool.body(slot).filter(|id| self.pool.is_linked(*id))?;
            self.corpses
                .pushee(slot, id.legacy_number(), self.pool.state(id)?)
        });
        let charges = self
            .charges
            .iter()
            .filter_map(|(id, charge)| jkr_game_jka::mines::pushee(id.legacy_number(), charge));
        let holocrons = self
            .holocrons
            .iter()
            .filter_map(|holocron| holocron.pushee());
        let saber = self
            .jedi_master
            .saber
            .as_ref()
            .and_then(|saber| saber.pushee());
        players
            .chain(npcs)
            .chain(items)
            .chain(bodies)
            .chain(charges)
            .chain(holocrons)
            .chain(saber)
            .collect()
    }

    /// `G_TryPushingEntity` having moved `number` to `to` (`g_mover.c:212-250`): a
    /// player's or an NPC's player state there, its view turned by the mover's `yaw`
    /// (`delta_angles[YAW]`), an NPC's entity moved as far and linked where it now is; a
    /// dropped item or a body there, its ground gone unless it was `carried`.
    pub(super) fn shove(&mut self, number: u16, to: [f32; 3], yaw: i32, carried: bool) {
        if let Some(peer) = self.peer_mut(usize::from(number)) {
            peer.state.set_origin(to);
            let turned = (peer.state.raw_field(PS_DELTA_YAW).unwrap_or(0) as i32).wrapping_add(yaw);
            peer.state.set_raw_field(PS_DELTA_YAW, turned as u32);
            peer.movement = peer.movement.reseeded(&peer.state);
            return;
        }
        let Self {
            npcs,
            items,
            pool,
            corpses,
            charges,
            holocrons,
            jedi_master,
            ..
        } = self;
        if let Some(npc) = npcs
            .roster
            .actors
            .iter_mut()
            .find(|npc| npc.number == number)
        {
            let from = npc.player.origin();
            npc.player.set_origin(to);
            let turned = (npc.player.raw_field(PS_DELTA_YAW).unwrap_or(0) as i32).wrapping_add(yaw);
            npc.player.set_raw_field(PS_DELTA_YAW, turned as u32);
            for (axis, field) in ES_POS_BASE.into_iter().enumerate() {
                let base = f32::from_bits(npc.state.raw_field(field).unwrap_or(0));
                npc.state
                    .set_raw_field(field, (base + (to[axis] - from[axis])).to_bits());
            }
            npc.current_origin = to;
            npc.relink();
        } else if let Some((id, pickup)) = items
            .iter_mut()
            .find(|(id, _)| id.legacy_number() == number)
        {
            jkr_game_jka::dropped_items::shove(pickup, to, carried);
            pool.set_state(*id, &pickup.state);
        } else if let Some((slot, id)) = (0..BODIES)
            .filter_map(|slot| Some((slot, pool.body(slot)?)))
            .find(|(_, id)| id.legacy_number() == number)
            && let Some(state) = pool.state_mut(id)
        {
            corpses.shove(slot, state, to, carried);
        } else if let Some((id, charge)) = charges
            .iter_mut()
            .find(|(id, _)| id.legacy_number() == number)
        {
            jkr_game_jka::mines::shove(charge, to, carried);
            pool.set_state(*id, &charge.missile.state);
        } else if let Some(holocron) = holocrons
            .iter_mut()
            .find(|holocron| holocron.id.legacy_number() == number)
        {
            holocron.shove(to, carried);
            pool.set_state(holocron.id, &holocron.missile.state);
        } else if let Some(saber) = jedi_master
            .saber
            .as_mut()
            .filter(|saber| saber.id.legacy_number() == number)
        {
            saber.shove(to, carried);
            pool.set_state(saber.id, &saber.missile.state);
        }
    }

    /// `G_Damage(check, pusher, pusher, vec3_origin, check->r.currentOrigin, 999, 0,
    /// MOD_UNKNOWN)` on the fragile object `number` a mover cannot move on: a det pack is
    /// set off.
    pub(super) fn break_in_the_way(&mut self, number: u16, level_time: i32) {
        let Self {
            charges, deaths, ..
        } = self;
        if let Some((_, charge)) = charges
            .iter_mut()
            .find(|(id, _)| id.legacy_number() == number)
        {
            jkr_game_jka::mines::hurt_charge(charge, 999, false, level_time, &mut deaths.rng);
        }
    }

    /// `G_FreeEntity` on the dropped weapon `number` a mover cannot move ("keep dropped
    /// weapons from blocking elevators").
    pub(super) fn free_in_the_way(&mut self, number: u16, level_time: i32) {
        if let Some(at) = self
            .items
            .iter()
            .position(|(id, _)| id.legacy_number() == number)
        {
            let (id, _) = self.items.remove(at);
            self.pool.free(id, level_time);
        }
    }

    /// `G_Damage(check, pusher, pusher, NULL, NULL, damage, flags, MOD_CRUSH)`: mover
    /// `mover`'s blow on the player or NPC `victim`.
    pub(super) fn crush(&mut self, mover: u16, victim: u16, damage: i32, flags: u32, time: i32) {
        let attacker = Attacker {
            npc: false,
            client: mover,
            max_health: 100,
            team: 0,
            saber_knockback: [0.0; 4],
        };
        let request = DamageRequest {
            level_time: time,
            attacker: Some(attacker),
            direction: None,
            point: None,
            damage,
            flags,
            means: movers::MOD_CRUSH,
        };
        // A player by `G_Damage` alone; an NPC, a body or anything else as any blow finds it.
        if self.peer(usize::from(victim)).is_some() {
            let _ = self.hurt(usize::from(victim), request);
        } else {
            let _ = self.strike(usize::MAX, usize::from(victim), request, false);
        }
    }

    /// `NPC_RemoveBody` for the dead NPC `number` a mover cannot move.
    pub(super) fn remove_body_in_the_way(&mut self, number: u16, level_time: i32) {
        let fired = self.with_roster(|roster, _, host| {
            let mut fired = Vec::new();
            roster.remove_body_now(number, level_time, host, &mut fired);
            fired
        });
        if let Some(fired) = fired {
            self.fire_from_npcs(fired, level_time);
        }
    }

    /// `G_MoverPush` for a brush mover — entity `mover` of inline model `model`, box
    /// `bounds` — travelling from `from` to `to` ([`movers::push_box_through`]), dealing
    /// `damage` with `spawnflags`: everyone it takes along is moved, and one it cannot move
    /// is answered by [`jkr_game_jka::path_movers::unpushable`], crushed or its body
    /// removed while the push goes on. Returns whoever blocks it; then nobody is moved.
    pub(super) fn push_brush(
        &mut self,
        (mover, model): (u16, usize),
        bounds: ([f32; 3], [f32; 3]),
        (from, to): ([f32; 3], [f32; 3]),
        (damage, spawnflags): (i32, u32),
        level_time: i32,
    ) -> Option<u16> {
        let mut candidates = self.pushees();
        while !candidates.is_empty() {
            // `G_TestEntityPosition` tests a client's box with its top at least 1.
            let boxes: Vec<_> = candidates
                .iter()
                .map(|check| {
                    let (low, mut high) = check.bounds;
                    if check.client {
                        high[2] = high[2].max(1.0);
                    }
                    (check.number, check.origin, (low, high), check.ground)
                })
                .collect();
            let pushed = match self.map.as_ref() {
                Some(map) => {
                    let world = WorldCollision {
                        bsp: &map.bsp,
                        scratch: &map.scratch,
                    };
                    // `G_TestEntityPosition`: may the box stand there at all?
                    let mut free = |_: u16, origin: [f32; 3], bounds: ([f32; 3], [f32; 3])| {
                        !world
                            .trace(origin, bounds.0, bounds.1, origin, 0x1)
                            .start_solid
                    };
                    // `G_TestEntityPosition` against the mover itself: is the box, where it
                    // stands, inside the mover's brush at its new place?
                    let mut inside = |_: u16, origin: [f32; 3], bounds: ([f32; 3], [f32; 3])| {
                        jkr_bsp::Aabb::new(bounds.0, bounds.1).is_ok_and(|bounds| {
                            map.bsp
                                .trace_transformed_model(
                                    model,
                                    to,
                                    None,
                                    origin,
                                    origin,
                                    bounds,
                                    u32::MAX,
                                )
                                .start_solid
                        })
                    };
                    movers::push_box_through(
                        bounds,
                        from,
                        to,
                        &boxes,
                        &mut free,
                        &mut inside,
                        mover,
                    )
                }
                // A world with no map in it has nothing to be blocked by.
                None => movers::push_box_through(
                    bounds,
                    from,
                    to,
                    &boxes,
                    &mut |_, _, _| true,
                    &mut |_, _, _| true,
                    mover,
                ),
            };
            let blocker = match pushed {
                movers::Push::Moved(shoved) => {
                    for movers::Shoved {
                        client,
                        to,
                        carried,
                    } in shoved
                    {
                        self.shove(client, to, 0, carried);
                    }
                    return None;
                }
                movers::Push::Blocked(blocker) => blocker,
            };
            let check = *candidates.iter().find(|check| check.number == blocker)?;
            if check.fragile {
                self.break_in_the_way(blocker, level_time);
            }
            match jkr_game_jka::path_movers::unpushable(damage, spawnflags, false, &check) {
                Unpushable::Blocks => return Some(blocker),
                Unpushable::Crushed(damage, flags) => {
                    self.crush(mover, blocker, damage, flags, level_time)
                }
                Unpushable::BodyRemoved => self.remove_body_in_the_way(blocker, level_time),
                Unpushable::Freed => self.free_in_the_way(blocker, level_time),
            }
            candidates.retain(|check| check.number != blocker);
        }
        None
    }
}
