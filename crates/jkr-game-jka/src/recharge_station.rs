//! Multiplayer power converters (`codemp/game/g_misc.c`).
//!
//! The model and floor variants intentionally retain their different depletion,
//! timing and ammo rules. The host owns collision, player lookup and sound handles.

use crate::{npc_combat::AMMO_MAXIMA, userinfo::atoi};
use jkr_entity::Entity;
use jkr_protocol::PlayerState;

/// Which stock converter use callback runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `shield_power_converter_use`.
    Shield,
    /// `ammo_generic_power_converter_use` (floor unit).
    Ammo,
    /// `ammo_power_converter_use` (older model entity).
    ModelAmmo,
    /// `health_power_converter_use` (does not drain).
    Health,
}

/// The sounds registered for one converter. Zero preserves an unset reference handle.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sounds {
    /// Loop handle while dispensing.
    pub running: u16,
    /// Release/full sound; zero for model converters that never register one.
    pub done: u16,
    /// Depleted or class-refused sound.
    pub empty: u16,
}

/// A converter's game state, independent of its host entity number.
#[derive(Clone, Debug)]
pub struct Station {
    /// Stock use callback.
    pub kind: Kind,
    /// Whether spawn drops this converter to the ground.
    pub floor: bool,
    /// Authored or stock-default model path.
    pub model: String,
    /// Name reached by map target activation.
    pub targetname: String,
    /// Linked world position, updated by the host after floor placement.
    pub origin: [f32; 3],
    /// Authored model orientation.
    pub angles: [f32; 3],
    /// Current charge; older model ammo can leave this negative.
    pub count: i32,
    /// Initial charge, also the recharge ceiling.
    pub capacity: i32,
    /// Milliseconds between one-point recharge ticks.
    pub rate: i32,
    /// Whether this converter ignores depletion.
    pub no_drain: bool,
    /// Next permitted use; the reference compares strictly less than time.
    pub set_time: i32,
    /// Scheduled recharge callback.
    pub next_think: i32,
    /// Next one-point recharge deadline.
    pub recharge_at: i32,
    /// Release timeout refreshed by a successful use.
    pub release_at: i32,
    /// Player currently holding use on this station.
    pub activator: Option<usize>,
    /// Current loop handle, zero while silent.
    pub loop_sound: u16,
    /// Registered converter sounds.
    pub sounds: Sounds,
}

impl Station {
    /// `SP_misc_*_power_converter` and the two floor units, including their game filter.
    pub fn spawn(entity: &Entity, gametype: i32, time: i32) -> Option<Self> {
        let name = entity.classname()?;
        let (kind, floor, default_model) = if name.eq_ignore_ascii_case("misc_ammo_floor_unit") {
            (Kind::Ammo, true, "/models/items/a_pwr_converter.md3")
        } else if name.eq_ignore_ascii_case("misc_shield_floor_unit") {
            if !matches!(gametype, 7..=9) {
                return None;
            }
            (Kind::Shield, true, "/models/items/a_shield_converter.md3")
        } else if name.eq_ignore_ascii_case("misc_model_shield_power_converter") {
            (Kind::Shield, false, "")
        } else if name.eq_ignore_ascii_case("misc_model_ammo_power_converter") {
            (Kind::ModelAmmo, false, "")
        } else if name.eq_ignore_ascii_case("misc_model_health_power_converter") {
            (Kind::Health, false, "")
        } else {
            return None;
        };
        let integer = |key, default| entity.get(key).map_or(default, |v| atoi(v.as_bytes()));
        let count = integer("count", 200);
        let rate = if floor || kind == Kind::Shield {
            match integer("chargerate", 0) {
                0 => 100,
                n => n,
            }
        } else {
            0
        };
        Some(Self {
            kind,
            floor,
            model: entity
                .get("model")
                .filter(|s| !s.is_empty())
                .unwrap_or(default_model)
                .to_owned(),
            targetname: entity.get("targetname").unwrap_or_default().to_owned(),
            origin: crate::fx_runner::vector(entity, "origin"),
            angles: crate::fx_runner::spawn_angles(entity),
            count,
            capacity: count,
            rate,
            no_drain: (floor || kind == Kind::ModelAmmo) && integer("nodrain", 0) != 0,
            set_time: 0,
            next_think: time + 200,
            recharge_at: 0,
            release_at: 0,
            activator: None,
            loop_sound: 0,
            sounds: Sounds::default(),
        })
    }

    /// `r.mins` and `r.maxs` after floor placement.
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        if self.floor {
            ([-16.0, -16.0, 0.0], [16.0, 16.0, 40.0])
        } else {
            ([-16.0; 3], [16.0; 3])
        }
    }

    /// `check_recharge`; `holding` is the activator's current BUTTON_USE, false if gone.
    /// Returns the one-shot sound, including the reference's possible zero handle.
    pub fn think(&mut self, time: i32, holding: bool) -> Option<u16> {
        if self.next_think <= 0 || self.next_think > time {
            return None;
        }
        let mut sound = None;
        if self.release_at < time || self.activator.is_none() || !holding {
            if self.activator.is_some() {
                sound = Some(self.sounds.done);
            }
            self.loop_sound = 0;
            self.activator = None;
            self.release_at = 0;
        }
        if self.activator.is_none() && self.recharge_at < time {
            if self.count < self.capacity {
                self.count += 1;
            }
            self.recharge_at = time + self.rate;
        }
        self.next_think = time;
        sound
    }

    /// One use callback. `class` is the caller's Siege class index and armor limit,
    /// when valid; `health` is the entity's authoritative health, not the delayed stat.
    pub fn use_by(
        &mut self,
        client: usize,
        state: &mut PlayerState,
        health: &mut i32,
        class: Option<(usize, i32)>,
        siege: bool,
        time: i32,
    ) -> Option<u16> {
        if self.kind == Kind::Shield
            && siege
            && class.is_some_and(|(index, max)| index != 0 && max == 0)
        {
            return Some(self.sounds.empty);
        }
        let mut stop = true;
        if self.set_time < time {
            self.loop_sound = self.sounds.running;
            match self.kind {
                Kind::Shield | Kind::Health => {
                    self.set_time = time + 100;
                    let shield = self.kind == Kind::Shield;
                    let maximum = if shield && siege {
                        class.map_or(state.stats[8] as i32, |(_, max)| max)
                    } else {
                        state.stats[8] as i32
                    };
                    let current = if shield {
                        state.stats[5] as i32
                    } else {
                        *health
                    };
                    let difference = maximum - current;
                    if difference > 0 {
                        let add = difference.min(if shield { 2 } else { 5 }).min(self.count);
                        if shield {
                            if !self.no_drain {
                                self.count -= add;
                            }
                            if self.count <= 0 {
                                self.set_time = 0;
                            }
                            state.stats[5] = (current + add) as u32;
                        } else {
                            *health += add;
                        }
                        stop = false;
                        self.begin_use(client, time);
                    }
                }
                Kind::Ammo => {
                    self.begin_use(client, time);
                    stop = self.refill_ammo(state, siege);
                }
                Kind::ModelAmmo => {
                    self.set_time = time + 100;
                    if self.count != 0 {
                        let mut add = 0;
                        for (i, maximum) in AMMO_MAXIMA.iter().copied().enumerate().skip(2) {
                            add = ((f64::from(maximum) * 0.1) as i32).max(1);
                            if (state.ammo[i] as i32) < maximum {
                                state.ammo[i] = (state.ammo[i] as i32 + add).min(maximum) as u32;
                            }
                        }
                        if !self.no_drain {
                            self.count -= add;
                        }
                        stop = false;
                        self.begin_use(client, time);
                    }
                }
            }
        }
        if matches!(self.kind, Kind::Health | Kind::ModelAmmo) {
            if stop {
                self.loop_sound = 0;
            }
            return None;
        }
        if !stop && self.count > 0 {
            return None;
        }
        let sound = (self.loop_sound != 0 && self.set_time < time).then_some(if self.count <= 0 {
            self.sounds.empty
        } else {
            self.sounds.done
        });
        self.loop_sound = 0;
        if self.set_time < time {
            self.set_time = time + self.rate + 100;
        }
        sound
    }

    fn begin_use(&mut self, client: usize, time: i32) {
        self.release_at = time + 500;
        self.activator = Some(client);
    }

    fn refill_ammo(&mut self, state: &mut PlayerState, siege: bool) -> bool {
        let doubled = state.entity_flags() & crate::siege_class::EF_DOUBLE_AMMO != 0;
        let mut stop = true;
        // Stock retains gaveSome across ammo kinds, including kinds already full.
        let mut gave_some = false;
        for (i, normal) in AMMO_MAXIMA.iter().copied().enumerate().skip(2) {
            let add = ((f64::from(normal) * 0.05) as i32).max(1);
            let maximum = normal * if doubled { 2 } else { 1 };
            let held = state.ammo[i] as i32;
            if held < maximum {
                gave_some = !(siege && i == 5 && held >= 10);
                let amount = held + add;
                if siege && i == 5 && amount >= 10 {
                    state.ammo[i] = 10;
                } else {
                    state.ammo[i] = amount.min(maximum) as u32;
                    if amount < maximum {
                        stop = false;
                    }
                }
            }
            if !self.no_drain && gave_some {
                self.count -= ((f64::from(add) * 0.2) as i32).max(1);
                if self.count <= 0 {
                    self.count = 0;
                    return true;
                }
            }
        }
        stop
    }
}
