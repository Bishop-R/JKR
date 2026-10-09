//! Reused collision and splash scratch for the level's entities.
use super::*;

impl NativeGame {
    /// Everyone `client` can run into: the other playing peers, each a body in the box of
    /// its last move, where it last thought (`r.currentOrigin`). In the players' order; the
    /// reference's order is its sector tree's, which only decides between stops at the
    /// very same fraction.
    pub(super) fn gather_obstacles(&mut self, client: usize) {
        let Self {
            server,
            world,
            players,
            obstacles,
            body_legs,
            missiles,
            husks,
            pool,
            charges,
            breakables,
            doors,
            last_frame_time,
            npcs,
            map_turrets,
            stock,
            mounted,
            scripts,
            corpses,
            lock_kinds,
            ..
        } = self;
        obstacles.clear();
        lock_kinds.clear();
        body_legs.clear();
        let Some(world) = server.world(*world) else {
            return;
        };
        body_legs.extend(players.holders().map(|handle| {
            handle
                .and_then(|handle| world.entity(handle))
                .filter(|peer| peer.begun && peer.body_active())
                .map(|peer| peer.state.leg_animation())
        }));
        for (other, handle) in players.holders().enumerate() {
            // A client that has not begun has no entity in the world yet; what `client`
            // owns (the gun it works) its traces pass.
            let peer = handle
                .and_then(|handle| world.entity(handle))
                .filter(|peer| {
                    other != client
                        && peer.begun
                        && peer.body_active()
                        && usize::from(peer.riding.owner) != client
                });
            if let Some(peer) = peer {
                lock_kinds.push((other as u16, cloaked(peer.entity.state())));
                let (mut bounds, mut contents) = (peer.movement.box_bounds(), peer.riding.contents);
                if let Some((corpse_contents, top)) = peer.corpse {
                    (bounds.1[2], contents) = (top, corpse_contents);
                }
                obstacles.push(BoxObstacle {
                    entity: other as u16,
                    origin: peer.state.origin(),
                    bounds,
                    contents,
                    model: None,
                    linked_box: None,
                });
            }
        }
        // The solid missiles in flight (a rocket) and their husks, linked until their
        // event's time is up — but the ones `client` owns, which its traces pass
        // (`SV_ClipMoveToEntities`).
        for (number, missile) in missiles.iter() {
            if missile.contents != 0 && missile.linked && usize::from(missile.owner) != client {
                obstacles.push(BoxObstacle {
                    entity: number.legacy_number(),
                    origin: missile.current,
                    bounds: missile.bounds,
                    contents: missile.contents,
                    model: None,
                    linked_box: None,
                });
            }
        }
        // The brushes, each its own model: breakables and doors and lifts.
        obstacles.extend(
            crate::collision::brush_obstacles(breakables, doors, *last_frame_time)
                .chain(stock.obstacles())
                .chain(scripts.brushes()),
        );
        map_turrets.add_obstacles(obstacles, client);
        husks.retain(|(_, _, id)| pool.is_linked(*id));
        obstacles.extend(
            husks
                .iter()
                .filter(|(_, owner, _)| usize::from(*owner) != client)
                .map(|(husk, ..)| *husk),
        );
        // A charge is passed by its owner while it flies; stuck (`SVF_OWNERNOTSHARED`), it
        // is solid to its owner too.
        obstacles.extend(
            charges
                .iter()
                .filter(|(_, charge)| {
                    usize::from(charge.missile.owner) != client || charge.owner_not_shared
                })
                .map(|(number, charge)| BoxObstacle {
                    entity: number.legacy_number(),
                    origin: charge.missile.current,
                    bounds: charge.missile.bounds,
                    contents: charge.missile.contents,
                    model: None,
                    linked_box: None,
                }),
        );
        Self::add_npc_bodies(npcs, obstacles, body_legs, client);
        // Every NPC is `ET_NPC`, a vehicle too: a rocket lock can lock on it.
        lock_kinds.extend(
            npcs.roster
                .actors
                .iter()
                .map(|npc| (npc.number, cloaked(&npc.state))),
        );
        obstacles.extend(Self::corpse_obstacles(pool, corpses, client));
        obstacles.extend(
            mounted.obstacles(
                client as u16,
                players
                    .at(client)
                    .and_then(|handle| world.entity(handle))
                    .map_or(jkr_protocol::ENTITY_NUMBER_NONE, |peer| peer.riding.owner),
            ),
        );
        // The solid siege items (`bridge_siege_items`).
        let items = self.siege_item_obstacles();
        self.obstacles.extend(items);
        // jaPRO's thrown flags (`CONTENTS_TRIGGER|CONTENTS_CORPSE`): shots meet them.
        self.obstacles.extend(
            self.items
                .iter()
                .filter(|(_, pickup)| pickup.contents & jkr_game_jka::corpses::CONTENTS_CORPSE != 0)
                .map(|(number, pickup)| BoxObstacle {
                    entity: number.legacy_number(),
                    origin: pickup.origin,
                    bounds: pickup.bounds,
                    contents: pickup.contents,
                    model: None,
                    linked_box: None,
                }),
        );
        // What a duel cuts the tracer off from (`DuelCull` in `SV_ClipMoveToEntities`).
        self.isolate_tracer_obstacles(client);
    }

    /// The players and NPCs a blast may reach, with their linked boxes.
    pub(super) fn gather_splash_targets(&mut self) {
        let Self {
            server,
            world,
            players,
            splash_targets,
            npcs,
            map_turrets,
            mounted,
            ..
        } = self;
        splash_targets.clear();
        if let Some(world) = server.world(*world) {
            for (number, handle) in players.holders().enumerate() {
                let peer = handle
                    .and_then(|handle| world.entity(handle))
                    .filter(|peer| peer.begun && peer.body_active());
                if let Some(peer) = peer {
                    let (bottom, mut top) = peer.movement.box_bounds();
                    if let Some((_, corpse_top)) = peer.corpse {
                        top[2] = corpse_top;
                    }
                    let origin = peer.state.origin();
                    let bounds = (
                        std::array::from_fn(|axis| origin[axis] + bottom[axis] - 1.0),
                        std::array::from_fn(|axis| origin[axis] + top[axis] + 1.0),
                    );
                    splash_targets.push(SplashTarget {
                        number: number as u16,
                        bounds,
                        origin,
                        takes_damage: true,
                    });
                }
            }
        }
        Self::add_npc_splash_targets(npcs, splash_targets);
        map_turrets.add_splash_targets(splash_targets);
        splash_targets.extend(mounted.splash_targets());
        splash_targets.extend(self.stock.shields.splash_targets());
        splash_targets.extend(self.stock.usables.splash_targets());
        splash_targets.extend(super::bridge_breakables::breakable_splash_targets(
            &self.breakables,
        ));
        splash_targets.extend(self.stock.scenery.splash_targets());
        splash_targets.extend(super::bridge_doors::mover_splash_targets(
            &self.doors,
            self.last_frame_time,
        ));
        splash_targets.extend(Self::corpse_splash_targets(
            &self.pool,
            &self.corpses,
            self.last_frame_time,
        ));
    }
}

/// `s.powerups & (1 << PW_CLOAKED)`.
fn cloaked(state: &jkr_protocol::EntityState) -> bool {
    const ES_POWERUPS: usize = 77;
    const PW_CLOAKED: u32 = 11;
    state.raw_field(ES_POWERUPS).unwrap_or(0) & (1 << PW_CLOAKED) != 0
}
