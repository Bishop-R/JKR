//! Who `G_MoverPush` moves (`g_mover.c:350-351`): players and NPCs — `ET_PLAYER` and
//! `ET_NPC` — for every mover this server runs: a door, a path mover, a scripted brush.
//! Each is offered as a [`Pushee`]; what the push decides is carried out here: moved (its
//! view turned with a turning mover), crushed (`MOD_CRUSH`), or — a dead NPC that will
//! not move — its body removed (`NPC_RemoveBody`).

use super::*;
use jkr_game_jka::movers;
use jkr_game_jka::path_movers::{Pushee, Unpushable};

/// `ps.delta_angles[YAW]` on the wire.
const PS_DELTA_YAW: usize = 11;
/// `FL_NOTARGET`: a dead NPC with it blocks as a living one does.
const FL_NOTARGET: u32 = 0x20;

impl NativeGame {
    /// Everyone a mover may push: the players in the game — a dead one by its corpse's
    /// box — and the NPCs that have begun, a dead one's body among them.
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
                dead: peer.health < 1,
                npc_body: false,
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
                dead: false,
                npc_body: npc.health <= 0 && npc.flags & FL_NOTARGET == 0,
            });
        players.chain(npcs).collect()
    }

    /// `G_TryPushingEntity` having moved `number` to `to` (`g_mover.c:212-250`): a
    /// player's or an NPC's player state there, its view turned by the mover's `yaw`
    /// (`delta_angles[YAW]`); an NPC's entity moved as far and linked where it now is.
    pub(super) fn shove(&mut self, number: u16, to: [f32; 3], yaw: i32) {
        if let Some(peer) = self.peer_mut(usize::from(number)) {
            peer.state.set_origin(to);
            let turned = (peer.state.raw_field(PS_DELTA_YAW).unwrap_or(0) as i32).wrapping_add(yaw);
            peer.state.set_raw_field(PS_DELTA_YAW, turned as u32);
            peer.movement = peer.movement.reseeded(&peer.state);
            return;
        }
        let actors = &mut self.npcs.roster.actors;
        let Some(npc) = actors.iter_mut().find(|npc| npc.number == number) else {
            return;
        };
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
        if self.npcs.roster.is_npc(victim) {
            let _ = self.strike(usize::MAX, usize::from(victim), request, false);
        } else {
            let _ = self.hurt(usize::from(victim), request);
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
                    high[2] = high[2].max(1.0);
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
                    for movers::Shoved { client, to, .. } in shoved {
                        self.shove(client, to, 0);
                    }
                    return None;
                }
                movers::Push::Blocked(blocker) => blocker,
            };
            let check = *candidates.iter().find(|check| check.number == blocker)?;
            match jkr_game_jka::path_movers::unpushable(damage, spawnflags, false, &check) {
                Unpushable::Blocks => return Some(blocker),
                Unpushable::Crushed(damage, flags) => {
                    self.crush(mover, blocker, damage, flags, level_time)
                }
                Unpushable::BodyRemoved => self.remove_body_in_the_way(blocker, level_time),
            }
            candidates.retain(|check| check.number != blocker);
        }
        None
    }
}
