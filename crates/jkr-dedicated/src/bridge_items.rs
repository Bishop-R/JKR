//! The items a level places on this server: spawned as the level starts
//! (`FinishSpawningItem`) and run each frame (`G_RunItem`, `RespawnItem`).

use super::*;
use jkr_game_jka::items::{ITEMS, Kind};

/// `CONTENTS_SOLID`, what a denied player's view of the powerup is traced through.
const CONTENTS_SOLID: u32 = 1;
/// `PERS_PLAYEREVENTS` and its `PLAYEREVENT_DENIEDREWARD` bit.
const PERS_PLAYEREVENTS: usize = 5;
const PLAYEREVENT_DENIEDREWARD: u32 = 1;
/// How near a player must stand to be denied a powerup (`Pickup_Powerup`).
const DENIED_RANGE: f32 = 192.0;

impl NativeGame {
    /// `FinishSpawningItem` for every item the map placed: dropped to the floor (or
    /// left hanging), a trigger in the world from the game's first moment.
    pub(super) fn spawn_items(&mut self) {
        let spawns = self.item_spawns();
        let Some(map) = self.map.take() else { return };
        let mut kept = Vec::new();
        for placed in &map.items {
            // `G_SpawnItem`: a powerup's return sound is registered as it is placed.
            if ITEMS[placed.item].kind == Kind::Powerup {
                self.sound_index_for(jkr_game_jka::items::POWERUP_RESPAWN_SOUND);
            }
            if spawns(placed.item) {
                kept.push(placed.clone());
            }
        }
        // `G_FindTeams` before the items finish spawning: a team's followers start out of
        // the world.
        let chains = jkr_game_jka::items::find_teams(&mut kept);
        let mut spawned = vec![None; kept.len()];
        for (index, placed) in kept.iter().enumerate() {
            let mut trace =
                |start: [f32; 3], mins: [f32; 3], maxs: [f32; 3], end: [f32; 3], mask: u32| {
                    WorldCollision {
                        bsp: &map.bsp,
                        scratch: &map.scratch,
                    }
                    .trace(start, mins, maxs, end, mask)
                };
            let Some(pickup) = jkr_game_jka::items::finish_spawning(placed, &mut trace) else {
                continue;
            };
            if let Some(number) = self.pool.spawn_entity(pickup.state.clone(), 0) {
                self.pool.set_bounds(number, pickup.bounds);
                self.items.push((number, pickup));
                spawned[index] = Some(number);
            }
        }
        for chain in chains {
            let members: Vec<EntityId> = chain.iter().filter_map(|&index| spawned[index]).collect();
            for (number, pickup) in self.items.iter_mut() {
                if members.contains(number) {
                    pickup.team.clone_from(&members);
                }
            }
        }
        self.map = Some(map);
    }

    /// Whether an item spawned now would stay: `G_SpawnItem`'s disabled weapons and
    /// `FinishSpawningItem`'s game-type filter (the flags outside the flag games, the
    /// duels' health and armour, and the rest), as this server is set.
    pub(super) fn item_spawns(&self) -> impl Fn(usize) -> bool + use<> {
        use jkr_game_jka::items::{removed_by_gametype, weapon_disabled};
        let (gametype, weapons) = (self.gametype, self.settings.weapons_disabled);
        let (saber_only, force_disabled) =
            (self.settings.saber_only(), self.settings.disabled != 0);
        move |item| {
            !weapon_disabled(item, gametype, weapons)
                && !removed_by_gametype(item, gametype, saber_only, force_disabled)
        }
    }

    /// The items' respawns at `server_time` (`RespawnItem`): a trigger and drawn again,
    /// with `EV_ITEM_RESPAWN` on itself.
    pub(super) fn run_items(&mut self, server_time: i32) {
        // `G_RunItem` for the dropped ones: their flight through the map, their end.
        let previous_time = self.previous_frame_time;
        let flag_game = bridge_ctf::flag_game(self.gametype);
        let flag_throw = self.flag_rules().throw;
        let mut gone_flags = Vec::new();
        let mut landed_flags = Vec::new();
        let Self {
            items, pool, map, ..
        } = self;
        items.retain_mut(|(number, pickup)| {
            if pickup.dropped.is_none() {
                return true;
            }
            let run = match map {
                Some(map) => {
                    let world = WorldCollision {
                        bsp: &map.bsp,
                        scratch: &map.scratch,
                    };
                    jkr_game_jka::dropped_items::run(
                        pickup,
                        server_time,
                        previous_time,
                        &mut |start, mins, maxs, end, mask| {
                            world.trace(start, mins, maxs, end, mask)
                        },
                        &|point| world.point_contents(point),
                        flag_throw,
                    )
                }
                None => jkr_game_jka::dropped_items::run(
                    pickup,
                    server_time,
                    previous_time,
                    &mut |_, _, _, end, _| jkr_game_jka::pmove::MovementTrace::miss(end),
                    &|_| 0,
                    flag_throw,
                ),
            };
            let is_flag =
                jkr_game_jka::items::ITEMS[pickup.item].kind == jkr_game_jka::items::Kind::Team;
            // A dropped flag whose time is up, or that is lost, goes home instead
            // (`Team_DroppedFlagThink`, `Team_FreeEntity`), which frees it.
            let gone = matches!(
                run,
                jkr_game_jka::dropped_items::DroppedRun::Expired
                    | jkr_game_jka::dropped_items::DroppedRun::Lost
            );
            if gone && flag_game && is_flag {
                gone_flags.push((
                    pickup.item,
                    run == jkr_game_jka::dropped_items::DroppedRun::Lost,
                ));
                return true;
            }
            match run {
                jkr_game_jka::dropped_items::DroppedRun::Stays => {}
                jkr_game_jka::dropped_items::DroppedRun::Landed => {
                    if is_flag {
                        landed_flags.push(*number);
                    }
                }
                _ => {
                    pool.free(*number, server_time);
                    return false;
                }
            }
            pool.set_state(*number, &pickup.state);
            true
        });
        for (item, lost) in gone_flags {
            self.flag_expired(item, lost, server_time);
        }
        self.flags_in_death_pits(&landed_flags, server_time);
        for index in 0..self.items.len() {
            let (_, pickup) = &mut self.items[index];
            if pickup.respawn_at == 0 || pickup.respawn_at > server_time {
                continue;
            }
            // `G_RunThink` clears the think before `RespawnItem`, which may bring back
            // another of the item's team.
            pickup.respawn_at = 0;
            self.respawn_placed_item(index, server_time);
        }
    }

    /// `RespawnItem` for item `index`: back in the world — or, for an item of a team,
    /// the one of its team `rand` draws (`rand() % count` down the chain from its
    /// leader) — a powerup's return heard.
    fn respawn_placed_item(&mut self, index: usize, level_time: i32) {
        let team = &self.items[index].1.team;
        let index = if team.is_empty() {
            index
        } else {
            let drawn = team[self.rand.next() as usize % team.len()];
            self.items
                .iter()
                .position(|(number, _)| *number == drawn)
                .unwrap_or(index)
        };
        if ITEMS[self.items[index].1.item].kind == Kind::Powerup {
            let sound = self.sound_index_for(jkr_game_jka::items::POWERUP_RESPAWN_SOUND);
            if let Some(event) = jkr_game_jka::items::respawn_sound(&self.items[index].1, sound)
                && let Some(id) = self.pool.spawn_temporary(event.state(), level_time, None)
            {
                self.pool.set_broadcast(id, event.broadcast);
            }
        }
        bridge_ctf::respawn_item(&mut self.items, &mut self.pool, index, level_time);
    }

    /// Items `G_Find` reaches by `targetname`, in the map's order.
    fn items_named(&self, name: &str) -> Vec<usize> {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, (_, pickup))| {
                pickup
                    .targetname
                    .as_deref()
                    .is_some_and(|own| own.eq_ignore_ascii_case(name))
            })
            .map(|(index, _)| index)
            .collect()
    }

    /// `Use_Item` (`g_items.c:2800-2802`) for the items called `name`: each back in the
    /// world (`RespawnItem`), a used item that starts out of it included.
    pub(super) fn use_items(&mut self, name: &str, level_time: i32) {
        for index in self.items_named(name) {
            self.respawn_placed_item(index, level_time);
        }
    }

    /// `Use_Target_Give` (`g_target.c:31-55`) for `client`: every item called `target`
    /// touched by it (`Touch_Item`, which gives nothing of one out of the world — so of
    /// a used item only once something brought it back), then taken out of the world for
    /// good.
    pub(super) fn give_items(&mut self, client: usize, target: &str, level_time: i32) {
        if target.is_empty() {
            return;
        }
        for index in self.items_named(target) {
            let alive = self
                .peer(client)
                .is_some_and(|peer| peer.health > 0 && peer.playing());
            if alive && jkr_game_jka::items::present(&self.items[index].1) {
                let _ = self.touch_item(client, index, level_time);
            }
            jkr_game_jka::items::withdraw(&mut self.items[index].1);
            self.publish_item(index);
        }
    }

    /// `G_SoundIndex`: sound `name`'s index, its config string published when it is new.
    pub(super) fn sound_index_for(&mut self, name: &[u8]) -> u16 {
        let mut registered = None;
        let index = self.sounds.index(name, &mut |slot, value| {
            registered = Some((slot, value.to_vec()))
        });
        if let Some((slot, value)) = registered {
            self.publish_config_string(slot, &value);
        }
        index
    }

    /// `Pickup_Powerup`'s anti-reward (`g_items.c:2082-2124`): every other living player
    /// within 192 units of the powerup at `at`, facing it with no solid brush between (and
    /// on another team in the team games), is told `taker` denied it them. The reference
    /// traces against solid entities too; doors in the way are not weighed here.
    pub(super) fn deny_nearby(&mut self, taker: usize, at: [f32; 3]) {
        let Some(team) = self.peer(taker).map(|peer| peer.session.team) else {
            return;
        };
        let team_game = self.gametype >= jkr_game_jka::match_end::GT_TEAM;
        let Some(map) = self.map.as_ref() else {
            return;
        };
        let world = WorldCollision {
            bsp: &map.bsp,
            scratch: &map.scratch,
        };
        let denied: Vec<usize> = (0..self.players.places())
            .filter(|&client| client != taker)
            .filter(|&client| {
                let Some(peer) = self.peer(client) else {
                    return false;
                };
                if peer.health <= 0 || (team_game && peer.session.team == team) {
                    return false;
                }
                let origin = peer.state.origin();
                let delta: [f32; 3] = std::array::from_fn(|axis| at[axis] - origin[axis]);
                let length = delta.iter().map(|axis| axis * axis).sum::<f32>().sqrt();
                if length > DENIED_RANGE {
                    return false;
                }
                let [pitch, yaw, _] = peer.state.view_angles().map(f32::to_radians);
                let forward = [
                    pitch.cos() * yaw.cos(),
                    pitch.cos() * yaw.sin(),
                    -pitch.sin(),
                ];
                // `VectorNormalize` leaves a zero vector as it is.
                let scale = if length > 0.0 { 1.0 / length } else { 0.0 };
                let facing: f32 = (0..3).map(|axis| delta[axis] * scale * forward[axis]).sum();
                facing >= 0.4
                    && world
                        .trace(origin, [0.0; 3], [0.0; 3], at, CONTENTS_SOLID)
                        .fraction
                        == 1.0
            })
            .collect();
        for client in denied {
            if let Some(peer) = self.peer_mut(client) {
                peer.state.persistent[PERS_PLAYEREVENTS] ^= PLAYEREVENT_DENIEDREWARD;
            }
        }
    }
}
