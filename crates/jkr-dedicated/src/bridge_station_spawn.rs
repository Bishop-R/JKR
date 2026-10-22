//! Map-lifetime asset registration and placement for stock converters.

use super::*;
use jkr_game_jka::recharge_station::{Kind, Sounds};

impl NativeGame {
    pub(in crate::bridge) fn spawn_stations(&mut self, entities: &[jkr_entity::Entity], time: i32) {
        self.stock.stations.0.clear();
        for (lump_index, entity) in entities.iter().enumerate().skip(1) {
            if !jkr_game_jka::spawn_table::kept_by_reference(entity, self.gametype) {
                continue;
            }
            let Some(mut station) = Station::spawn(entity, self.gametype, time) else {
                continue;
            };
            let Some(ground) = self.place_station(&mut station) else {
                continue;
            };
            let radar = self.gametype == 7 && (station.floor || station.kind == Kind::Health);
            let mut state = station_state(&station, ground);
            self.register_station(&mut station, &mut state, radar);
            if let Some(id) = self.pool.spawn_entity(state, time) {
                self.pool.set_bounds(id, station.bounds());
                self.pool.set_broadcast(id, radar);
                self.stock.stations.0.push(Placed {
                    id,
                    station,
                    lump_index,
                });
            }
        }
    }

    fn place_station(&mut self, station: &mut Station) -> Option<u32> {
        if !station.floor {
            return Some(0);
        }
        self.gather_obstacles(usize::MAX);
        let map = self.map.as_ref()?;
        let mut from = station.origin;
        from[2] += 0.1;
        let to = [from[0], from[1], from[2] - 4096.0];
        let (mins, mut maxs) = station.bounds();
        maxs[2] -= 0.1;
        let trace = WithPlayers {
            world: WorldCollision {
                bsp: &map.bsp,
                scratch: &map.scratch,
            },
            players: &self.obstacles,
        }
        .trace(from, mins, maxs, to, 1 | (1 << 12)); // MASK_SOLID
        if trace.start_solid {
            return None;
        }
        station.origin = trace.end_position;
        Some(u32::from(trace.entity_number))
    }

    fn register_station(&mut self, station: &mut Station, state: &mut EntityState, radar: bool) {
        let (run, done, empty) = sound_paths(station);
        station.sounds = Sounds {
            running: self.sound_index(run.as_bytes()),
            done: self.sound_index(done.as_bytes()),
            empty: self.sound_index(empty.as_bytes()),
        };
        state.set_raw_field(46, u32::from(self.station_model(&station.model)));
        if station.kind == Kind::Shield && !station.floor {
            state.set_raw_field(
                41,
                u32::from(self.station_model("/models/items/psd_big.md3")),
            );
        }
        if radar {
            let name = match station.kind {
                Kind::Shield => "gfx/mp/siegeicons/desert/shield_recharge",
                Kind::Ammo => "gfx/mp/siegeicons/desert/weapon_recharge",
                _ => "gfx/mp/siegeicons/desert/bacta",
            };
            let icon = self
                .map_turrets
                .icon_index(name.as_bytes(), &mut |index, value| {
                    self.told.push(Told::ConfigString {
                        index,
                        previous: Vec::new(),
                        value: value.to_vec(),
                    })
                });
            state.set_raw_field(ES_EFLAGS, 1 << 2); // EF_RADAROBJECT
            state.set_raw_field(18, u32::from(icon)); // genericenemyindex
        }
    }

    fn station_model(&mut self, path: &str) -> u16 {
        self.models.index(path.as_bytes(), &mut |index, value| {
            self.told.push(Told::ConfigString {
                index,
                previous: Vec::new(),
                value: value.to_vec(),
            })
        })
    }
}

fn sound_paths(station: &Station) -> (&'static str, &'static str, &'static str) {
    match station.kind {
        Kind::Shield => (
            "sound/interface/shieldcon_run",
            if station.floor {
                "sound/interface/shieldcon_done"
            } else {
                ""
            },
            "sound/interface/shieldcon_empty",
        ),
        Kind::Ammo => (
            "sound/interface/ammocon_run",
            "sound/interface/ammocon_done",
            "sound/interface/ammocon_empty",
        ),
        Kind::ModelAmmo => ("sound/player/pickupshield.wav", "", ""),
        Kind::Health => (
            "sound/player/pickuphealth.wav",
            "sound/interface/shieldcon_done",
            "",
        ),
    }
}

fn station_state(station: &Station, ground: u32) -> EntityState {
    let mut state = EntityState::zero(0, &jkr_protocol::LEGACY_ENTITY_FIELDS);
    for axis in 0..3 {
        for field in [ES_POS_BASE[axis], [11, 12, 13][axis]] {
            state.set_raw_field(field, station.origin[axis].to_bits());
        }
        for field in [[5, 3, 33][axis], [25, 9, 24][axis]] {
            state.set_raw_field(field, station.angles[axis].to_bits());
        }
    }
    state.set_raw_field(22, ground);
    state.set_raw_field(40, 1023); // owner = ENTITYNUM_NONE
    state.set_raw_field(57, 1); // shouldtarget
    // SV_LinkEntity's encoded box for client prediction: x/y radius, lower z,
    // upper z + 32. Each byte is clamped to at least one, even with mins.z == 0.
    let z = if station.floor {
        (1 << 8) | (72 << 16)
    } else {
        (16 << 8) | (48 << 16)
    };
    state.set_raw_field(26, 16 | z);
    if !station.no_drain && station.kind != Kind::Health {
        state.set_raw_field(ES_HEALTH, station.count as u32);
        state.set_raw_field(ES_MAX_HEALTH, station.count as u32);
    }
    state
}
