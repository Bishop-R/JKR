//! The map's logic entities (OpenJK `codemp/game/g_trigger.c`, `g_target.c`): the timers,
//! counters and relays that decide *when* and *which* of a map's other entities are used,
//! and the targets that act on whoever set them off.
//!
//! | Classname | Reference |
//! | --- | --- |
//! | `func_timer` | `SP_func_timer`, `func_timer_use`, `func_timer_think` (`g_trigger.c:1771-1810`) |
//! | `target_random` | `SP_target_random`, `target_random_use` (`g_target.c:699-769`) |
//! | `target_counter` | `SP_target_counter`, `target_counter_use` (`g_target.c:629-691`) |
//! | `trigger_always` | `SP_trigger_always`, `trigger_always_think` (`g_trigger.c:893-905`) |
//! | `target_kill` | `SP_target_kill`, `target_kill_use` (`g_target.c:557-564`) |
//! | `target_teleporter` | `SP_target_teleporter`, `target_teleporter_use` (`g_target.c:463-488`) |
//! | `target_play_music` | `SP_target_play_music`, `target_play_music_use` (`g_target.c:990-1020`) |
//!
//! A logic entity never touches the world itself: a use or a think returns the [`Deed`]s it
//! asks of the level, in the reference's order, and the server carries them out — fires a
//! name (`G_UseTargets`), uses one entity of a name (`GlobalUse`), hurts, teleports or
//! changes the music. The game's own generator (`Q_irand`, `Q_flrand`) is the caller's.
//!
//! Who sets an entity off (an activator) is an entity identity: a client's is its slot,
//! anything else's the caller's own numbering, which must never collide with a slot. Only a
//! client activator is hurt or teleported.

use crate::player_death::Rng;
use jkr_entity::Entity;

/// `FRAMETIME`: a started timer's first think is a frame after the map's spawn.
const FRAMETIME: i32 = 100;
/// `trigger_always`' delay, "to make sure our use targets are present".
const ALWAYS_DELAY: i32 = 300;
/// `target_counter`'s spawnflag 128: deactivated once it has fired.
const COUNTER_DEACTIVATES: u32 = 128;
/// `PUSH_CONSTANT`: a `target_push` that pushes straight at its target.
const PUSH_CONSTANT: u32 = 2;

/// `VectorNormalize`: the length, and the vector made a unit long (a zero vector is left
/// alone).
fn normalize(vector: &mut [f32; 3]) -> f32 {
    let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    if length != 0.0 {
        let scale = 1.0 / length;
        for axis in vector.iter_mut() {
            *axis *= scale;
        }
    }
    length
}

/// What a logic entity is, with what its spawn read.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// `func_timer`: fires its targets every `wait` seconds, give or take `random`.
    Timer { wait: f32, random: f32 },
    /// `target_random`: uses one of the entities its target names, at random; only once
    /// with spawnflag 1.
    Random,
    /// `target_counter`: fires after `count` uses (`target2` on each one before), then
    /// starts again `bounce` times (the reference reads no key for it: always 0).
    Counter {
        count: i32,
        initial: i32,
        bounce: i32,
    },
    /// `trigger_always`: fires its targets once, 300 ms into the level, and is gone.
    Always,
    /// `target_kill`: kills whoever set it off.
    Kill,
    /// `target_teleporter`: sends whoever set it off to one of the entities its target
    /// names (`G_PickTarget`).
    Teleporter,
    /// `target_play_music`: the level's music (`CS_MUSIC`) becomes `music`.
    PlayMusic { music: String },
    /// `target_level_change`: the server goes to the map `map` (`map <map>`).
    LevelChange { map: String },
    /// `target_give`: the items its target names, touched by whoever set it off.
    Give,
    /// `target_push`: throws whoever set it off with `velocity` — the direction its angles
    /// name times `speed`, or worked out toward its target a frame on (`AimAtTarget`):
    /// straight at it with `constant` (spawnflag 2), else in an arc whose top it is.
    /// `sound` (spawnflag 1, "bouncepad") plays the Force jump's sound.
    Push {
        velocity: [f32; 3],
        speed: f32,
        constant: bool,
        sound: bool,
    },
    /// An immediate score, powerup or screen-shake action.
    Action(crate::map_target_action::Action),
}

/// One logic entity of the level.
#[derive(Clone, Debug, PartialEq)]
pub struct Logic {
    /// Its identity as an activator (`ent->s.number`): what a started timer fires for.
    pub number: usize,
    /// The classname it was spawned from, as the lump spells it.
    pub classname: String,
    /// `targetname`, `target` and `target2`.
    pub targetname: String,
    pub target: String,
    pub target2: String,
    pub spawnflags: u32,
    pub origin: [f32; 3],
    pub kind: Kind,
    /// `nextthink`: zero for none.
    pub next_think: i32,
    /// `activator`: whoever last set it off, `None` for nobody.
    pub activator: Option<usize>,
    /// `FL_INACTIVE` (`target_deactivate`, a counter's spawnflag 128): `GlobalUse` refuses it.
    pub inactive: bool,
    /// `ent->use`: a `target_random` with spawnflag 1 clears it once used; a
    /// `trigger_always` never has one.
    pub usable: bool,
    /// Freed (`G_FreeEntity`): a `trigger_always` after its think.
    pub freed: bool,
}

/// Why the reference refuses a logic entity at spawn.
#[derive(Clone, Debug, PartialEq)]
pub enum Refused {
    /// `target_play_music` without a `music` key: the reference stops the map
    /// (`ERR_DROP`); this server refuses the entity alone and says so.
    NoMusic { origin: [f32; 3] },
    /// `target_level_change` without a `mapname`: the same.
    NoMapname { origin: [f32; 3] },
}

/// What a use or a think asks of the level, in the reference's order.
#[derive(Clone, Debug, PartialEq)]
pub enum Deed {
    /// `G_UseTargets2(self, activator, name)`: every standing entity called `name` but
    /// the one firing is used.
    Fire {
        name: String,
        activator: Option<usize>,
    },
    /// `target_random`'s `GlobalUse`: the `pick`-th (from 1) standing entity called
    /// `name`, not counting the one firing, in entity order.
    FireOne {
        name: String,
        pick: usize,
        activator: Option<usize>,
    },
    /// `G_Damage(activator, NULL, NULL, NULL, NULL, 100000, DAMAGE_NO_PROTECTION,
    /// MOD_TELEFRAG)`: only an activator that takes damage (a client) is hurt.
    Kill { activator: usize },
    /// `TeleportPlayer(activator, dest->s.origin, dest->s.angles)` to a destination the
    /// caller picks among the entities called `destination` (`G_PickTarget`, C `rand()`);
    /// only a client activator (`if (!activator->client) return`).
    Teleport {
        activator: usize,
        destination: String,
    },
    /// `trap->SetConfigstring(CS_MUSIC, music)`.
    Music(String),
    /// `trap->SendConsoleCommand(EXEC_NOW, "map <map>")`.
    ChangeLevel(String),
    /// `Use_Target_Give`: the items called `target`, touched by `activator`, then gone.
    Give { activator: usize, target: String },
    /// `AimAtTarget` for a `target_push`: the caller picks its target (`G_PickTarget`)
    /// and calls [`Logic::aim_push`] — or frees it, with no target to aim at.
    AimPush,
    /// `Use_target_push`: `activator` thrown with `velocity`, if it is a client walking
    /// or floating; `sound` at most every 1.5 s (`fly_sound_debounce_time`).
    Push {
        activator: usize,
        velocity: [f32; 3],
        sound: bool,
    },
    /// An immediate target action on the activating entity, if any.
    Action(crate::map_target_action::Action, Option<usize>),
    /// The rest of `func_timer_think`, after the uses before it have run (they may draw
    /// from the generator first): the caller calls [`Logic::rearm`].
    Rearm,
}

impl Logic {
    /// The spawn function for `entity`, or `None` when it is no logic entity. `number` is
    /// its identity as an activator.
    pub fn spawn(entity: &Entity, number: usize, level_time: i32) -> Option<Result<Self, Refused>> {
        let classname = entity.classname()?;
        let text = |key: &str| entity.get(key).unwrap_or_default().to_owned();
        let float = |key: &str, default: f32| {
            entity
                .get(key)
                .map_or(default, |value| crate::text_parse::atof(value.as_bytes()))
        };
        let spawnflags = entity
            .get("spawnflags")
            .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()))
            as u32;
        let origin = crate::fx_runner::vector(entity, "origin");
        let mut logic = Self {
            number,
            classname: classname.to_owned(),
            targetname: text("targetname"),
            target: text("target"),
            target2: text("target2"),
            spawnflags,
            origin,
            kind: Kind::Kill,
            next_think: 0,
            activator: None,
            inactive: false,
            usable: true,
            freed: false,
        };
        match classname.to_ascii_lowercase().as_str() {
            "func_timer" => {
                let wait = float("wait", 1.0);
                let mut random = float("random", 1.0);
                if random >= wait {
                    // "was - FRAMETIME, but FRAMETIME is in msec (100) and these numbers are in *seconds*!"
                    random = wait - 1.0;
                }
                logic.kind = Kind::Timer { wait, random };
                if spawnflags & 1 != 0 {
                    logic.next_think = level_time + FRAMETIME;
                    logic.activator = Some(number);
                }
            }
            "target_random" => logic.kind = Kind::Random,
            "target_counter" => {
                let count = entity
                    .get("count")
                    .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()));
                let count = if count == 0 { 2 } else { count };
                logic.kind = Kind::Counter {
                    count,
                    initial: count,
                    bounce: 0,
                };
            }
            "trigger_always" => {
                logic.kind = Kind::Always;
                logic.usable = false;
                logic.next_think = level_time + ALWAYS_DELAY;
            }
            "target_kill" => logic.kind = Kind::Kill,
            "target_teleporter" => logic.kind = Kind::Teleporter,
            "target_play_music" => {
                let Some(music) = entity.get("music") else {
                    return Some(Err(Refused::NoMusic { origin }));
                };
                logic.kind = Kind::PlayMusic {
                    music: music.to_owned(),
                };
            }
            "target_give" => logic.kind = Kind::Give,
            "target_level_change" => {
                let map = entity.get("mapname").unwrap_or_default();
                if map.is_empty() {
                    return Some(Err(Refused::NoMapname { origin }));
                }
                logic.kind = Kind::LevelChange {
                    map: map.to_owned(),
                };
            }
            "target_push" => {
                let speed = match float("speed", 0.0) {
                    0.0 => 1_000.0,
                    speed => speed,
                };
                let direction = crate::movers::movedir(crate::fx_runner::spawn_angles(entity));
                logic.kind = Kind::Push {
                    velocity: direction.map(|axis| axis * speed),
                    speed,
                    constant: spawnflags & PUSH_CONSTANT != 0,
                    sound: spawnflags & 1 != 0,
                };
                if !logic.target.is_empty() {
                    logic.next_think = level_time + FRAMETIME;
                }
            }
            _ => logic.kind = Kind::Action(crate::map_target_action::Action::spawn(entity)?),
        }
        Some(Ok(logic))
    }

    /// Whether `G_RunThink` runs its think at `level_time`.
    pub fn due(&self, level_time: i32) -> bool {
        !self.freed && self.next_think > 0 && self.next_think <= level_time
    }

    /// `G_RunThink`: its think, `nextthink` cleared first.
    pub fn think(&mut self) -> Vec<Deed> {
        self.next_think = 0;
        match self.kind {
            Kind::Timer { .. } => self.timer_fires(),
            Kind::Always => {
                // `G_UseTargets(ent, ent)`, then `G_FreeEntity`.
                self.freed = true;
                vec![Deed::Fire {
                    name: self.target.clone(),
                    activator: Some(self.number),
                }]
            }
            Kind::Push { .. } => vec![Deed::AimPush],
            _ => Vec::new(),
        }
    }

    /// `AimAtTarget` (`g_trigger.c:1060-1118`) for a `target_push` standing at its origin,
    /// at `target` (the picked entity's `s.origin`) in `gravity`: straight at it at its
    /// speed when constant, else the throw whose arc tops out there. A throw that takes no
    /// time frees it, as the reference does.
    pub fn aim_push(&mut self, target: [f32; 3], gravity: f32) {
        let Kind::Push {
            velocity,
            speed,
            constant,
            ..
        } = &mut self.kind
        else {
            return;
        };
        let origin = self.origin;
        let mut along: [f32; 3] = std::array::from_fn(|axis| target[axis] - origin[axis]);
        if *constant {
            normalize(&mut along);
            *velocity = along.map(|axis| axis * *speed);
            return;
        }
        let height = target[2] - origin[2];
        // `sqrt( height / ( .5 * gravity ) )` in double, made a float.
        let time = (f64::from(height) / (0.5 * f64::from(gravity))).sqrt() as f32;
        if time == 0.0 {
            self.freed = true;
            return;
        }
        along[2] = 0.0;
        let distance = normalize(&mut along);
        let forward = distance / time;
        *velocity = [along[0] * forward, along[1] * forward, time * gravity];
    }

    /// `GlobalUse` of this entity for `activator`. `named` is how many standing entities
    /// other than this one are called its `target`, which a `target_random` draws among.
    pub fn use_logic(
        &mut self,
        activator: Option<usize>,
        named: usize,
        rng: &mut Rng,
    ) -> Vec<Deed> {
        if self.inactive || !self.usable || self.freed {
            return Vec::new();
        }
        match &mut self.kind {
            Kind::Timer { .. } => {
                self.activator = activator;
                // On: off. Off: on, firing now.
                if self.next_think != 0 {
                    self.next_think = 0;
                    return Vec::new();
                }
                self.timer_fires()
            }
            Kind::Random => {
                if self.spawnflags & 1 != 0 {
                    self.usable = false;
                }
                match named {
                    0 => Vec::new(),
                    1 => vec![Deed::Fire {
                        name: self.target.clone(),
                        activator,
                    }],
                    _ => vec![Deed::FireOne {
                        name: self.target.clone(),
                        pick: rng.irand(1, named as i32) as usize,
                        activator,
                    }],
                }
            }
            Kind::Counter {
                count,
                initial,
                bounce,
            } => {
                if *count == 0 {
                    return Vec::new();
                }
                *count -= 1;
                if *count != 0 {
                    return if self.target2.is_empty() {
                        Vec::new()
                    } else {
                        vec![Deed::Fire {
                            name: self.target2.clone(),
                            activator,
                        }]
                    };
                }
                if self.spawnflags & COUNTER_DEACTIVATES != 0 {
                    self.inactive = true;
                }
                self.activator = activator;
                let fired = vec![Deed::Fire {
                    name: self.target.clone(),
                    activator,
                }];
                if *bounce != 0 {
                    *count = *initial;
                    if *bounce > 0 {
                        *bounce -= 1;
                    }
                }
                fired
            }
            Kind::Always => Vec::new(),
            Kind::Kill => activator
                .map(|activator| vec![Deed::Kill { activator }])
                .unwrap_or_default(),
            Kind::Teleporter => activator
                .map(|activator| {
                    vec![Deed::Teleport {
                        activator,
                        destination: self.target.clone(),
                    }]
                })
                .unwrap_or_default(),
            Kind::PlayMusic { music } => vec![Deed::Music(music.clone())],
            Kind::LevelChange { map } => vec![Deed::ChangeLevel(map.clone())],
            Kind::Give => activator
                .filter(|_| !self.target.is_empty())
                .map(|activator| {
                    vec![Deed::Give {
                        activator,
                        target: self.target.clone(),
                    }]
                })
                .unwrap_or_default(),
            Kind::Push {
                velocity, sound, ..
            } => activator
                .map(|activator| {
                    vec![Deed::Push {
                        activator,
                        velocity: *velocity,
                        sound: *sound,
                    }]
                })
                .unwrap_or_default(),
            Kind::Action(action) => vec![Deed::Action(action.clone(), activator)],
        }
    }

    /// `func_timer_think`: its targets used for its activator, then [`Deed::Rearm`].
    fn timer_fires(&mut self) -> Vec<Deed> {
        vec![
            Deed::Fire {
                name: self.target.clone(),
                activator: self.activator,
            },
            Deed::Rearm,
        ]
    }

    /// The end of `func_timer_think`: the next firing `1000 * (wait + Q_flrand(-1, 1) *
    /// random)` on — reckoned in float, as the reference adds `level.time` to a float and
    /// truncates.
    pub fn rearm(&mut self, level_time: i32, rng: &mut Rng) {
        let Kind::Timer { wait, random } = self.kind else {
            return;
        };
        let spread = rng.flrand(-1.0, 1.0);
        self.next_think = (level_time as f32 + 1000.0 * (wait + spread * random)) as i32;
    }

    /// `count` as the transcripts print it (`ent->count`): a counter's remaining uses,
    /// zero for anything else.
    pub fn count(&self) -> i32 {
        match self.kind {
            Kind::Counter { count, .. } => count,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(pairs: &[(&str, &str)]) -> Entity {
        let mut text = String::from("{\n");
        for (key, value) in pairs {
            text.push_str(&format!("\"{key}\" \"{value}\"\n"));
        }
        text.push('}');
        jkr_entity::parse_entity_lump(text.as_bytes())
            .unwrap()
            .remove(0)
    }

    /// `SP_target_push`: its angles times its speed (a thousand unset); with a target, an
    /// aim a frame on (`AimAtTarget`) whose arc tops out at the target — or, with
    /// `PUSH_CONSTANT`, straight at it at its speed.
    #[test]
    fn a_target_push_throws_along_its_angles_or_at_its_target() {
        let plain = entity(&[
            ("classname", "target_push"),
            ("angles", "0 90 0"),
            ("speed", "300"),
        ]);
        let push = Logic::spawn(&plain, 7, 1_000).unwrap().unwrap();
        let Kind::Push { velocity, .. } = push.kind else {
            panic!("not a push: {:?}", push.kind);
        };
        assert!(velocity[0].abs() < 0.001 && (velocity[1] - 300.0).abs() < 0.001);
        assert_eq!(push.next_think, 0);
        let mut deeds = push.clone();
        assert_eq!(
            deeds.use_logic(Some(3), 0, &mut Rng::default()),
            vec![Deed::Push {
                activator: 3,
                velocity,
                sound: false,
            }]
        );
        let aimed = entity(&[
            ("classname", "target_push"),
            ("target", "top"),
            ("spawnflags", "1"),
        ]);
        let mut push = Logic::spawn(&aimed, 7, 1_000).unwrap().unwrap();
        assert_eq!(push.next_think, 1_100);
        assert_eq!(push.think(), vec![Deed::AimPush]);
        push.aim_push([0.0, 320.0, 160.0], 800.0);
        let Kind::Push {
            velocity, sound, ..
        } = push.kind
        else {
            panic!("not a push");
        };
        // A rise of 160 in a gravity of 800 takes sqrt(0.4) s, which the throw crosses
        // the 320 units in and climbs at 800 times.
        let time = 0.4_f64.sqrt() as f32;
        assert!((velocity[1] - 320.0 / time).abs() < 0.01);
        assert!((velocity[2] - time * 800.0).abs() < 0.01);
        assert!(sound);
        // Level with it: no time to throw in, so it is freed.
        push.aim_push([0.0, 320.0, 0.0], 800.0);
        assert!(push.freed);
    }

    /// `target_give`: used by a client, the items its target names are touched by it; with
    /// no target, or no client, nothing.
    #[test]
    fn a_give_touches_its_target_items_for_a_client() {
        let give = entity(&[("classname", "target_give"), ("target", "loot")]);
        let mut give = Logic::spawn(&give, 7, 0).unwrap().unwrap();
        assert_eq!(
            give.use_logic(Some(2), 0, &mut Rng::default()),
            vec![Deed::Give {
                activator: 2,
                target: "loot".to_owned(),
            }]
        );
        assert!(give.use_logic(None, 0, &mut Rng::default()).is_empty());
        let aimless = entity(&[("classname", "target_give")]);
        let mut aimless = Logic::spawn(&aimless, 7, 0).unwrap().unwrap();
        assert!(
            aimless
                .use_logic(Some(2), 0, &mut Rng::default())
                .is_empty()
        );
    }

    /// `target_level_change`: used, the server goes to its map; without one the reference
    /// stops the map, this server refuses the entity.
    #[test]
    fn a_level_change_goes_to_its_map() {
        let change = entity(&[
            ("classname", "target_level_change"),
            ("mapname", "mp/duel1"),
        ]);
        let mut change = Logic::spawn(&change, 7, 0).unwrap().unwrap();
        assert_eq!(
            change.use_logic(None, 0, &mut Rng::default()),
            vec![Deed::ChangeLevel("mp/duel1".to_owned())]
        );
        let none = entity(&[("classname", "target_level_change")]);
        assert!(matches!(
            Logic::spawn(&none, 7, 0),
            Some(Err(Refused::NoMapname { .. }))
        ));
    }
}
