//! The players' portable assault sentries (`HI_SENTRY_GUN`) on this server. The rules are
//! `jkr_game_jka::sentry`; this is where they meet the pool, the players and the map.
//!
//! A sentry is one of the map turrets' kind ([`MapTurrets`]): set down by its owner's
//! holdable, run every frame (`G_RunItem`) with its think every tenth of a second, seen,
//! met and struck as the turrets are. Its owner's `sentryDeployed` refuses them a second
//! one until it is gone.

use super::*;
use jkr_game_jka::map_turret_world::object_take;
use jkr_game_jka::sentry::{self, Outcome, Owner, Sentry};

/// `ps.fd.sentryDeployed`.
const PS_SENTRY_DEPLOYED: usize = 106;

/// Sentry `id` as the level's traces meet it: its box where it stands, solid or not.
pub(super) fn obstacle(id: EntityId, placed: &Sentry) -> BoxObstacle {
    BoxObstacle {
        entity: id.legacy_number(),
        origin: placed.origin,
        bounds: sentry::BOUNDS,
        contents: placed.contents,
        model: None,
    }
}

impl NativeGame {
    /// `ItemUse_Sentry`: `client`'s sentry set down before them, and they marked as having
    /// one out.
    pub(in crate::bridge) fn place_sentry(&mut self, client: usize, level_time: i32) {
        let Some(peer) = self.peer(client) else {
            return;
        };
        let (origin, yaw, team) = (
            peer.state.origin(),
            peer.state.view_angles()[1],
            peer.session.team,
        );
        let Some(id) = self.pool.spawn_entity(
            EntityState::zero(0, &jkr_protocol::LEGACY_ENTITY_FIELDS),
            level_time,
        ) else {
            eprintln!("sentry placement refused: entity budget exhausted");
            return;
        };
        let gametype = self.gametype;
        let map = self.map.take();
        // Gathered for the placer, whose move may still trace after its item's use.
        let mut placed = self.with_turret_host(map.as_ref(), client, |host| {
            sentry::place(client as u16, origin, yaw, team, gametype, level_time, host)
        });
        self.map = map;
        placed
            .state
            .set_number(id.legacy_number())
            .expect("pool assigned a valid entity number");
        self.map_turrets.sentries.push((id, placed));
        self.publish_sentry(self.map_turrets.sentries.len() - 1);
        self.set_sentry_deployed(client, true);
    }

    /// `G_RunItem` for every sentry in the order they were set down: its move, its think
    /// when due, its wire state published; then what their deaths asked for.
    pub(in crate::bridge) fn run_sentries(&mut self, level_time: i32) {
        let previous_time = self.previous_frame_time;
        let map = self.map.take();
        let mut index = 0;
        while index < self.map_turrets.sentries.len() {
            let thinks = {
                let placed = &self.map_turrets.sentries[index].1;
                placed.next_think > 0 && placed.next_think <= level_time
            };
            if thinks {
                self.gather_turret_sights();
            }
            // Out of its list while it runs: what it reads was gathered whole.
            let (id, mut placed) = self.map_turrets.sentries.remove(index);
            let number = id.legacy_number();
            let owner = self.sentry_owner(&placed);
            // Its own move passes nobody: its own box is among what it meets.
            self.gather_obstacles(usize::from(number));
            self.obstacles.push(obstacle(id, &placed));
            let targets = std::mem::take(&mut self.map_turrets.sighted);
            let seen: &[_] = if thinks { &targets } else { &[] };
            let outcome = self.with_gathered_turret_host(map.as_ref(), |host| {
                sentry::run(
                    &mut placed,
                    number,
                    owner,
                    seen,
                    level_time,
                    previous_time,
                    host,
                )
            });
            self.map_turrets.sighted = targets;
            if self.settle_sentry(index, id, placed, outcome, level_time) {
                index += 1;
            }
        }
        self.map = map;
        self.carry_out_turret_work(level_time);
    }

    /// `G_Damage` on sentry `target` (`object_take`'s blow): its health, and at none its
    /// death.
    pub(in crate::bridge) fn damage_sentry(
        &mut self,
        target: u16,
        blow: &jkr_game_jka::map_turret_world::ObjectBlow,
        level_time: i32,
    ) {
        let Some(index) = self
            .map_turrets
            .sentries
            .iter()
            .position(|(id, _)| id.legacy_number() == target)
        else {
            return;
        };
        let placed = &self.map_turrets.sentries[index].1;
        let Some(take) = object_take(target, placed.takes_damage, 0, blow) else {
            return;
        };
        let owner = self.sentry_owner(placed);
        let (id, mut placed) = self.map_turrets.sentries.remove(index);
        let map = self.map.take();
        let outcome = self.with_turret_host(map.as_ref(), usize::MAX, |host| {
            sentry::damage(&mut placed, target, take, owner, host)
        });
        self.map = map;
        self.settle_sentry(index, id, placed, outcome, level_time);
        self.carry_out_turret_work(level_time);
    }

    /// A sentry's call over: its owner told it is gone where it said so, and the sentry put
    /// back at `index` and published — or freed. Whether it is still there.
    fn settle_sentry(
        &mut self,
        index: usize,
        id: EntityId,
        placed: Sentry,
        outcome: Outcome,
        level_time: i32,
    ) -> bool {
        if outcome.owner_undeployed {
            self.set_sentry_deployed(usize::from(placed.owner), false);
        }
        if placed.freed {
            self.pool.free(id, level_time);
            return false;
        }
        self.map_turrets.sentries.insert(index, (id, placed));
        self.publish_sentry(index);
        true
    }

    /// The sentry's owner while they are in the game, with their team now.
    fn sentry_owner(&self, placed: &Sentry) -> Option<Owner> {
        self.peer(usize::from(placed.owner)).map(|peer| Owner {
            team: peer.session.team,
        })
    }

    /// `ps.fd.sentryDeployed` of `client`, which their movement reads too.
    fn set_sentry_deployed(&mut self, client: usize, deployed: bool) {
        if let Some(peer) = self.peer_mut(client) {
            peer.state
                .set_raw_field(PS_SENTRY_DEPLOYED, u32::from(deployed));
            peer.movement = peer.movement.reseeded(&peer.state);
        }
    }

    /// Sentry `index`'s wire state into the pool, its box for the clients' PVS around where
    /// it stands.
    fn publish_sentry(&mut self, index: usize) {
        let (id, placed) = &self.map_turrets.sentries[index];
        self.pool.set_state(*id, &placed.state);
        let base: [f32; 3] = std::array::from_fn(|axis| {
            f32::from_bits(placed.state.raw_field([2, 1, 4][axis]).unwrap_or(0))
        });
        let (mins, maxs) = sentry::BOUNDS;
        self.pool.set_bounds(
            *id,
            (
                std::array::from_fn(|axis| mins[axis] + placed.origin[axis] - base[axis]),
                std::array::from_fn(|axis| maxs[axis] + placed.origin[axis] - base[axis]),
            ),
        );
    }
}
