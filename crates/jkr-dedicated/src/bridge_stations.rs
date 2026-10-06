//! The map's stock power converters: spawning, use traces, recharge and sounds.

use super::*;
use jkr_game_jka::recharge_station::Station;

#[path = "bridge_station_spawn.rs"]
mod spawn;

#[derive(Debug)]
struct Placed {
    id: EntityId,
    station: Station,
    lump_index: usize,
}

#[derive(Debug, Default)]
pub(super) struct Stations(Vec<Placed>);

impl Stations {
    pub(super) fn obstacles(&self) -> impl Iterator<Item = BoxObstacle> + '_ {
        self.0.iter().map(|placed| BoxObstacle {
            entity: placed.id.legacy_number(),
            origin: placed.station.origin,
            bounds: placed.station.bounds(),
            contents: 1,
            model: None,
        })
    }

    pub(super) fn named(&self, lump_index: usize) -> Option<usize> {
        self.0
            .iter()
            .position(|placed| placed.lump_index == lump_index)
    }
}

impl NativeGame {
    pub(super) fn run_stations(&mut self, time: i32) {
        for index in 0..self.stock.stations.0.len() {
            let holding = self.stock.stations.0[index]
                .station
                .activator
                .and_then(|client| self.peer(client))
                .is_some_and(|peer| {
                    peer.last_command.buttons & jkr_game_jka::use_key::BUTTON_USE != 0
                });
            let placed = &mut self.stock.stations.0[index];
            if placed.station.next_think <= 0 || placed.station.next_think > time {
                continue;
            }
            let sound = placed.station.think(time, holding);
            let origin = placed.station.origin;
            if let Some(state) = self.pool.state_mut(placed.id) {
                state.set_raw_field(ES_HEALTH, placed.station.count as u32);
                state.set_raw_field(55, u32::from(placed.station.loop_sound));
            }
            self.station_sound(origin, sound, time);
        }
    }

    pub(super) fn use_station(&mut self, client: usize, number: u16, time: i32) -> bool {
        let Some(index) = self
            .stock
            .stations
            .0
            .iter()
            .position(|placed| placed.id.legacy_number() == number)
        else {
            return false;
        };
        self.use_station_index(index, client, time);
        true
    }

    pub(super) fn use_named_stations(&mut self, name: &str, client: usize, time: i32) {
        for index in 0..self.stock.stations.0.len() {
            if self.stock.stations.0[index]
                .station
                .targetname
                .eq_ignore_ascii_case(name)
            {
                self.use_station_index(index, client, time);
            }
        }
    }

    pub(super) fn use_station_index(&mut self, index: usize, client: usize, time: i32) {
        let Some(handle) = self.players.at(client) else {
            return;
        };
        let Some(peer) = self
            .server
            .world_mut(self.world)
            .and_then(|world| world.entity_mut(handle))
        else {
            return;
        };
        let class = peer.siege_class_index.and_then(|index| {
            self.siege
                .as_ref()?
                .registry
                .classes
                .get(index)
                .map(|class| (index, class.max_armor))
        });
        let Some(placed) = self.stock.stations.0.get_mut(index) else {
            return;
        };
        let sound = placed.station.use_by(
            client,
            &mut peer.state,
            &mut peer.health,
            class,
            self.gametype == 7,
            time,
        );
        peer.movement = peer.movement.reseeded(&peer.state);
        let origin = placed.station.origin;
        if let Some(state) = self.pool.state_mut(placed.id) {
            state.set_raw_field(55, u32::from(placed.station.loop_sound));
        }
        self.station_sound(origin, sound, time);
    }

    fn station_sound(&mut self, origin: [f32; 3], sound: Option<u16>, time: i32) {
        // The older model converters leave done unset. Do not emit an invalid handle.
        if let Some(sound) = sound.filter(|sound| *sound != 0) {
            let event = EventEntity {
                event: 76,
                parameter: u32::from(sound),
                origin,
                client: None,
                broadcast: false,
                extra: [(0, 0); 12],
            };
            let _ = self.pool.spawn_temporary(event.state(), time, None);
        }
    }
}
