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
        for placed in &map.items {
            // `G_SpawnItem`: a powerup's return sound is registered as it is placed.
            if ITEMS[placed.item].kind == Kind::Powerup {
                self.item_sound_index(jkr_game_jka::items::POWERUP_RESPAWN_SOUND);
            }
            let mut trace =
                |start: [f32; 3], mins: [f32; 3], maxs: [f32; 3], end: [f32; 3], mask: u32| {
                    WorldCollision {
                        bsp: &map.bsp,
                        scratch: &map.scratch,
                    }
                    .trace(start, mins, maxs, end, mask)
                };
            if !spawns(placed.item) {
                continue;
            }
            let Some(pickup) = jkr_game_jka::items::finish_spawning(placed, &mut trace) else {
                continue;
            };
            if let Some(number) = self.pool.spawn_entity(pickup.state.clone(), 0) {
                self.pool.set_bounds(number, pickup.bounds);
                self.items.push((number, pickup));
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
        let mut gone_flags = Vec::new();
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
                    )
                }
                None => jkr_game_jka::dropped_items::run(
                    pickup,
                    server_time,
                    previous_time,
                    &mut |_, _, _, end, _| jkr_game_jka::pmove::MovementTrace::miss(end),
                    &|_| 0,
                ),
            };
            // A dropped flag whose time is up, or that is lost, goes home instead
            // (`Team_DroppedFlagThink`, `Team_FreeEntity`), which frees it.
            let gone = matches!(
                run,
                jkr_game_jka::dropped_items::DroppedRun::Expired
                    | jkr_game_jka::dropped_items::DroppedRun::Lost
            );
            if gone
                && flag_game
                && jkr_game_jka::items::ITEMS[pickup.item].kind == jkr_game_jka::items::Kind::Team
            {
                gone_flags.push((
                    pickup.item,
                    run == jkr_game_jka::dropped_items::DroppedRun::Lost,
                ));
                return true;
            }
            if run != jkr_game_jka::dropped_items::DroppedRun::Stays {
                pool.free(*number, server_time);
                return false;
            }
            pool.set_state(*number, &pickup.state);
            true
        });
        for (item, lost) in gone_flags {
            self.flag_expired(item, lost, server_time);
        }
        for index in 0..self.items.len() {
            let (_, pickup) = &self.items[index];
            if pickup.respawn_at == 0 || pickup.respawn_at > server_time {
                continue;
            }
            if ITEMS[pickup.item].kind == Kind::Powerup {
                let sound = self.item_sound_index(jkr_game_jka::items::POWERUP_RESPAWN_SOUND);
                if let Some(event) = jkr_game_jka::items::respawn_sound(&self.items[index].1, sound)
                    && let Some(id) = self.pool.spawn_temporary(event.state(), server_time, None)
                {
                    self.pool.set_broadcast(id, event.broadcast);
                }
            }
            bridge_ctf::respawn_item(&mut self.items, &mut self.pool, index, server_time);
        }
    }

    /// `G_SoundIndex` for an item's sound, its config string published when it is new.
    fn item_sound_index(&mut self, name: &[u8]) -> u16 {
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
