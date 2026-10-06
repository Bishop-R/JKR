//! The players' portable E-Webs (`HI_EWEB`) on this server. The rules are
//! `jkr_game_jka::eweb`; this is where they meet the pool, their owners and the map.
//!
//! An E-Web is one of the map turrets' kind ([`MapTurrets`]): set down by its owner's
//! holdable, run every frame (`G_RunItem`, its think every frame too), met and struck as
//! the turrets are, its Ghoul2 instance the turrets' host's. Its owner is copied into the
//! rules' [`User`] for each call and back after it — moved, their legs played, their
//! weapons held — and its blast is dealt as a turret's before the rest of its death reads
//! the owner again.

use super::*;
use jkr_game_jka::eweb::{self, EWeb, Outcome, Use, User};
use jkr_game_jka::map_turret_world::object_take;

/// `stats[STAT_HOLDABLE_ITEMS]`, `stats[STAT_HOLDABLE_ITEM]`, `stats[STAT_WEAPONS]`.
const STAT_HOLDABLE_ITEMS: usize = 2;
const STAT_HOLDABLE_ITEM: usize = 3;
const STAT_WEAPONS: usize = 4;
/// Player-state fields: `weapon`, `legsAnim`, `legsTimer`, `emplacedIndex`.
const PS_WEAPON: usize = 47;
const PS_LEGS_ANIM: usize = 13;
const PS_LEGS_TIMER: usize = 21;
const PS_EMPLACED_INDEX: usize = 112;
/// `SETANIM_LEGS`.
const SETANIM_LEGS: u8 = jkr_game_jka::pmove_anim::SETANIM_LEGS;

impl NativeGame {
    /// `ItemUse_UseEWeb`: `client`'s E-Web put away, or one set down before them.
    pub(in crate::bridge) fn use_eweb(&mut self, client: usize, level_time: i32) {
        let Some(mut user) = self.eweb_user(client) else {
            return;
        };
        match eweb::use_eweb(&mut user, level_time) {
            Use::Refused => {}
            Use::PutAway => {
                let index = self.eweb_index(user.kit.index);
                let mut held = index.map(|index| self.map_turrets.ewebs.remove(index));
                eweb::disattach(
                    &mut user,
                    held.as_mut().map(|(_, placed)| placed),
                    level_time,
                );
                if let (Some(index), Some(held)) = (index, held) {
                    self.map_turrets.ewebs.insert(index, held);
                }
            }
            Use::Deploy => self.deploy_eweb(&mut user, level_time),
        }
        self.give_back_user(client, &user);
    }

    /// `EWeb_Create` in an entity held ready for it, let go again where it has no room.
    fn deploy_eweb(&mut self, user: &mut User, level_time: i32) {
        let Some(id) = self.pool.spawn_entity(
            EntityState::zero(0, &jkr_protocol::LEGACY_ENTITY_FIELDS),
            level_time,
        ) else {
            eprintln!("E-Web refused: entity budget exhausted");
            return;
        };
        let number = id.legacy_number();
        let map = self.map.take();
        // Gathered for the placer: its traces pass them.
        let created = self.with_turret_host(map.as_ref(), usize::from(user.number), |host| {
            eweb::create(user, number, level_time, host)
        });
        self.map = map;
        match created {
            Some(placed) => {
                self.map_turrets.ewebs.push((id, placed));
                self.publish_eweb(self.map_turrets.ewebs.len() - 1);
            }
            None => {
                self.pool.free(id, level_time);
                self.map_turrets
                    .work
                    .instances
                    .retain(|known| known.number != number);
            }
        }
    }

    /// `G_RunItem` for every E-Web in the order they were set down; then what their deaths
    /// asked for.
    pub(in crate::bridge) fn run_ewebs(&mut self, level_time: i32) {
        let previous_time = self.previous_frame_time;
        let mut index = 0;
        while index < self.map_turrets.ewebs.len() {
            // Out of its list while it runs: its own box is not among what it meets.
            let (id, mut placed) = self.map_turrets.ewebs.remove(index);
            let owner = usize::from(placed.owner);
            let mut user = self.eweb_user(owner);
            self.gather_obstacles(usize::from(id.legacy_number()));
            let map = self.map.take();
            let outcome = self.with_gathered_turret_host(map.as_ref(), |host| {
                eweb::run(&mut placed, user.as_mut(), level_time, previous_time, host)
            });
            self.map = map;
            if let Some(user) = &user {
                self.give_back_user(owner, user);
            }
            if self.settle_eweb(index, id, placed, outcome, level_time) {
                index += 1;
            }
        }
        self.carry_out_turret_work(level_time);
    }

    /// `G_Damage` on E-Web `target` (`object_take`'s blow): its health, and at none its
    /// death.
    pub(in crate::bridge) fn damage_eweb(
        &mut self,
        target: u16,
        blow: &jkr_game_jka::map_turret_world::ObjectBlow,
        level_time: i32,
    ) {
        let Some(index) = self.eweb_index(target) else {
            return;
        };
        let placed = &self.map_turrets.ewebs[index].1;
        let Some(take) = object_take(target, placed.takes_damage, 0, blow) else {
            return;
        };
        let owner = usize::from(placed.owner);
        let mut user = self.eweb_user(owner);
        let (id, mut placed) = self.map_turrets.ewebs.remove(index);
        let outcome = eweb::damage(&mut placed, user.as_mut(), take);
        if let Some(user) = &user {
            self.give_back_user(owner, user);
        }
        self.settle_eweb(index, id, placed, outcome, level_time);
        self.carry_out_turret_work(level_time);
    }

    /// `ClientDisconnect`'s part: the leaver's E-Web freed with them, without a blast.
    pub(in crate::bridge) fn eweb_owner_left(&mut self, client: usize, level_time: i32) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let number = std::mem::take(&mut peer.gear.eweb).index;
        peer.state.set_raw_field(PS_EMPLACED_INDEX, 0);
        if number == 0 {
            return;
        }
        if let Some(index) = self.eweb_index(number) {
            let (id, _) = self.map_turrets.ewebs.remove(index);
            self.free_eweb(id, level_time);
        }
    }

    /// An E-Web's call over: its blast dealt and the rest of its death done, then the
    /// E-Web put back at `index` and published — or freed. Whether it is still there.
    fn settle_eweb(
        &mut self,
        index: usize,
        id: EntityId,
        mut placed: EWeb,
        outcome: Outcome,
        level_time: i32,
    ) -> bool {
        if let Some(blast) = outcome.blast {
            let number = id.legacy_number();
            self.map_turrets.work.blasts.push(Blast {
                origin: blast.origin,
                attacker: Some(number),
                damage: blast.damage,
                radius: blast.radius,
                ignore: Some(number),
                means: blast.means,
            });
            self.carry_out_turret_work(level_time);
            let owner = usize::from(placed.owner);
            let mut user = self.eweb_user(owner);
            let map = self.map.take();
            self.with_turret_host(map.as_ref(), usize::MAX, |host| {
                eweb::after_blast(&mut placed, user.as_mut(), blast, level_time, host)
            });
            self.map = map;
            if let Some(user) = &user {
                self.give_back_user(owner, user);
            }
        }
        if placed.freed {
            self.free_eweb(id, level_time);
            return false;
        }
        self.map_turrets.ewebs.insert(index, (id, placed));
        self.publish_eweb(index);
        true
    }

    /// The entity and the server's Ghoul2 instance let go.
    fn free_eweb(&mut self, id: EntityId, level_time: i32) {
        let number = id.legacy_number();
        self.map_turrets
            .work
            .instances
            .retain(|known| known.number != number);
        self.pool.free(id, level_time);
    }

    /// Where E-Web `number` is in the list.
    fn eweb_index(&self, number: u16) -> Option<usize> {
        self.map_turrets
            .ewebs
            .iter()
            .position(|(id, _)| id.legacy_number() == number)
    }

    /// `client` as an E-Web reads them; `None` for no player.
    fn eweb_user(&self, client: usize) -> Option<User> {
        let peer = self.peer(client)?;
        let state = &peer.state;
        Some(User {
            number: client as u16,
            kit: peer.gear.eweb,
            in_use: true,
            connected: peer.begun,
            health: peer.health,
            team: peer.session.team,
            buttons: peer.last_command.buttons,
            bounds: peer.movement.box_bounds(),
            origin: state.origin(),
            view: state.view_angles(),
            weapon: u32::from(state.weapon()),
            weapons: state.stats[STAT_WEAPONS] as i32,
            items: state.stats[STAT_HOLDABLE_ITEMS] as i32,
            selected: state.stats[STAT_HOLDABLE_ITEM] as i32,
            emplaced: state.emplaced_index(),
            legs_anim: state.raw_field(PS_LEGS_ANIM).unwrap_or(0) as u16,
            legs_timer: state.legs_timer(),
            weapon_time: state.weapon_time(),
            hand_extend: u32::from(state.force_hand_extend()),
            legs_request: None,
        })
    }

    /// What the E-Web did to `client` written back: placed, held on it, their legs
    /// played, their movement told.
    fn give_back_user(&mut self, client: usize, user: &User) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        peer.gear.eweb = user.kit;
        let state = &mut peer.state;
        if state.origin() != user.origin {
            state.set_origin(user.origin);
        }
        if state.view_angles() != user.view {
            state.set_view_angles(user.view);
        }
        state.set_raw_field(PS_WEAPON, user.weapon);
        state.stats[STAT_WEAPONS] = user.weapons as u32;
        state.stats[STAT_HOLDABLE_ITEMS] = user.items as u32;
        state.stats[STAT_HOLDABLE_ITEM] = user.selected as u32;
        state.set_raw_field(PS_EMPLACED_INDEX, u32::from(user.emplaced));
        state.set_raw_field(PS_LEGS_TIMER, user.legs_timer as u32);
        peer.movement = peer.movement.reseeded(&peer.state);
        if let Some((animation, flags)) = user.legs_request {
            peer.movement
                .set_animation_parts(SETANIM_LEGS, animation, flags);
            peer.movement.write_player_state(&mut peer.state);
        }
    }

    /// E-Web `index`'s wire state into the pool, its box for the clients' PVS where it was
    /// last linked.
    fn publish_eweb(&mut self, index: usize) {
        let (id, placed) = &self.map_turrets.ewebs[index];
        self.pool.set_state(*id, &placed.state);
        let base: [f32; 3] = std::array::from_fn(|axis| {
            f32::from_bits(placed.state.raw_field([2, 1, 4][axis]).unwrap_or(0))
        });
        let (absmin, absmax) = placed.linked;
        self.pool.set_bounds(
            *id,
            (
                std::array::from_fn(|axis| absmin[axis] + 1.0 - base[axis]),
                std::array::from_fn(|axis| absmax[axis] - 1.0 - base[axis]),
            ),
        );
    }
}
