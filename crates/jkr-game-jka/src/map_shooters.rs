//! The map's shooters and lasers: `shooter_blaster` (`InitShooter`, `InitShooter_Finish`,
//! `Use_Shooter`, OpenJK `codemp/game/g_misc.c:1166-1220`) fires a blaster bolt each time it
//! is used, at its target or along its angles, a little scattered; `target_laser`
//! (`target_laser_start`, `target_laser_think`, `target_laser_use`, `g_target.c:331-430`)
//! is a beam that, while on, hurts whatever it meets for its damage every frame;
//! `trigger_lightningstrike` (`Do_Strike`, `Think_Strike`, `Use_Strike`,
//! `g_trigger.c:756-875`) strikes a random spot in its box every second or so. No stock
//! map places any of them; custom maps do.

use crate::player_death::Rng;
use crate::weapon_fire::Missile;
use jkr_entity::Entity;

/// `WP_BLASTER`: what a shooter is drawn and heard as (`s.weapon`).
pub const WP_BLASTER: u32 = 5;
/// `FRAMETIME`: how often a laser thinks, and when it starts.
pub const FRAMETIME: i32 = 100;
/// How long a shooter waits before it looks for its target (`InitShooter`).
const FINISH_DELAY: i32 = 500;
/// How far a laser reaches.
const LASER_RANGE: f32 = 2_048.0;
/// What a laser's line meets: `CONTENTS_SOLID | CONTENTS_BODY | CONTENTS_CORPSE`.
pub const LASER_MASK: u32 = 0x1 | 0x100 | 0x200;
/// `BLASTER_VELOCITY`, `BLASTER_DAMAGE`, `MOD_BLASTER`: `WP_FireBlasterMissile` for a thing
/// that is no NPC.
const BLASTER_VELOCITY: f32 = 2_300.0;
const BLASTER_DAMAGE: i32 = 20;
const MOD_BLASTER: u32 = 6;

/// One `shooter_blaster`.
#[derive(Clone, Debug, PartialEq)]
pub struct Shooter {
    /// `s.origin`, where its bolts leave from.
    pub origin: [f32; 3],
    /// `G_SetMovedir` of its angles: where it fires without a target.
    pub movedir: [f32; 3],
    /// `random` as `InitShooter` leaves it: `sin(PI * random / 180)`, a degree unset.
    pub spread: f32,
    pub target: String,
    pub targetname: String,
    /// `enemy`: where the target `InitShooter_Finish` picked stands.
    pub aim: Option<[f32; 3]>,
    /// When `InitShooter_Finish` runs (500 ms in), for one with a target; 0 once it has.
    pub finish_at: i32,
    /// `FL_INACTIVE` (`target_deactivate`).
    pub inactive: bool,
}

/// One `target_laser`.
#[derive(Clone, Debug, PartialEq)]
pub struct Laser {
    /// `s.origin`, where its beam starts.
    pub origin: [f32; 3],
    /// `movedir`: along its angles, or at its target.
    pub movedir: [f32; 3],
    /// `damage`, one unset: what it does every frame to what it meets.
    pub damage: i32,
    pub target: String,
    pub targetname: String,
    /// `enemy`: the middle of what its target names, aimed at every frame.
    pub aim: Option<[f32; 3]>,
    /// Spawnflag 1: on once it starts.
    pub start_on: bool,
    /// `target_laser_start` has run (a frame in): it answers uses from then on.
    pub started: bool,
    /// When it starts, then thinks next; 0 while off.
    pub next_think: i32,
    /// `activator`: whoever turned it on; `None` for the laser itself.
    pub activator: Option<u16>,
    /// `s.origin2`: where its beam ends.
    pub end: [f32; 3],
    /// `FL_INACTIVE` (`target_deactivate`).
    pub inactive: bool,
}

/// `SP_shooter_blaster` for `entity`, or `None` for anything else.
pub fn spawn_shooter(entity: &Entity, level_time: i32) -> Option<Shooter> {
    if !entity.classname()?.eq_ignore_ascii_case("shooter_blaster") {
        return None;
    }
    let random = entity
        .get("random")
        .map_or(0.0, |value| crate::text_parse::atof(value.as_bytes()));
    let random = if random == 0.0 { 1.0 } else { random };
    let target = entity.get("target").unwrap_or_default().to_owned();
    Some(Shooter {
        origin: crate::fx_runner::vector(entity, "origin"),
        movedir: crate::movers::movedir(crate::fx_runner::spawn_angles(entity)),
        spread: (std::f32::consts::PI * random / 180.0).sin(),
        finish_at: if target.is_empty() {
            0
        } else {
            level_time + FINISH_DELAY
        },
        target,
        targetname: entity.get("targetname").unwrap_or_default().to_owned(),
        aim: None,
        inactive: false,
    })
}

impl Shooter {
    /// `Use_Shooter`: the bolt it fires for entity `me` — at its target or along its
    /// angles, scattered by two of the generator's draws over a perpendicular pair of
    /// axes — and, for the caller, its `EV_FIRE_WEAPON`.
    pub fn fire(&self, me: u16, level_time: i32, rng: &mut Rng) -> Missile {
        let mut direction = match self.aim {
            Some(at) => {
                let mut to = std::array::from_fn(|axis| at[axis] - self.origin[axis]);
                crate::player_angle_math::normalize(&mut to);
                to
            }
            None => self.movedir,
        };
        let up = crate::npc_nav_old::perpendicular_vector(direction);
        let right = crate::npc_nav_old::cross(up, direction);
        let degree = rng.flrand(-1.0, 1.0) * self.spread;
        direction = std::array::from_fn(|axis| direction[axis] + degree * up[axis]);
        let degree = rng.flrand(-1.0, 1.0) * self.spread;
        direction = std::array::from_fn(|axis| direction[axis] + degree * right[axis]);
        crate::player_angle_math::normalize(&mut direction);
        let mut missile = crate::weapon_fire::create_missile_by(
            me,
            self.origin,
            direction,
            BLASTER_VELOCITY,
            level_time,
            false,
        );
        missile.state.set_raw_field(14, WP_BLASTER);
        missile.damage = BLASTER_DAMAGE;
        missile.method_of_death = MOD_BLASTER;
        missile.bounce_count = 8;
        missile
    }
}

/// `SP_target_laser` for `entity`, or `None` for anything else: it starts a frame in.
pub fn spawn_laser(entity: &Entity, level_time: i32) -> Option<Laser> {
    if !entity.classname()?.eq_ignore_ascii_case("target_laser") {
        return None;
    }
    let damage = entity
        .get("dmg")
        .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()));
    let origin = crate::fx_runner::vector(entity, "origin");
    let spawnflags = entity
        .get("spawnflags")
        .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()));
    Some(Laser {
        origin,
        movedir: crate::movers::movedir(crate::fx_runner::spawn_angles(entity)),
        damage: if damage == 0 { 1 } else { damage },
        target: entity.get("target").unwrap_or_default().to_owned(),
        targetname: entity.get("targetname").unwrap_or_default().to_owned(),
        aim: None,
        start_on: spawnflags & 1 != 0,
        started: false,
        next_think: level_time + FRAMETIME,
        activator: None,
        end: origin,
        inactive: false,
    })
}

/// What a laser's think asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LaserThink {
    /// Nothing due.
    Idle,
    /// It started off: unlinked.
    Off,
    /// It burned along its line: the entity its trace met (0 is never hurt, as the
    /// reference tests the number for truth), and its end.
    Burned { struck: u16, end: [f32; 3] },
}

impl Laser {
    /// Whether it is on (`nextthink` set, once started).
    pub fn on(&self) -> bool {
        self.started && self.next_think > 0
    }

    /// `G_RunThink`: `target_laser_start` a frame in (on or off as its spawnflag says),
    /// then `target_laser_think` every frame while on. `trace` runs a line from its origin
    /// to a point, giving the entity met and where it stopped.
    pub fn think(
        &mut self,
        level_time: i32,
        trace: impl FnOnce([f32; 3], [f32; 3]) -> (u16, [f32; 3]),
    ) -> LaserThink {
        if self.next_think == 0 || self.next_think > level_time {
            return LaserThink::Idle;
        }
        if !self.started {
            self.started = true;
            if !self.start_on {
                self.next_think = 0;
                return LaserThink::Off;
            }
        }
        if let Some(at) = self.aim {
            let mut to: [f32; 3] = std::array::from_fn(|axis| at[axis] - self.origin[axis]);
            crate::player_angle_math::normalize(&mut to);
            self.movedir = to;
        }
        let end = std::array::from_fn(|axis| self.origin[axis] + LASER_RANGE * self.movedir[axis]);
        let (struck, end) = trace(self.origin, end);
        self.end = end;
        self.next_think = level_time + FRAMETIME;
        LaserThink::Burned { struck, end }
    }

    /// `target_laser_use` for `activator`: off when on; else on, burning at once.
    /// Nothing before it has started.
    pub fn used(
        &mut self,
        activator: Option<u16>,
        level_time: i32,
        trace: impl FnOnce([f32; 3], [f32; 3]) -> (u16, [f32; 3]),
    ) -> LaserThink {
        if !self.started {
            return LaserThink::Idle;
        }
        self.activator = activator;
        if self.next_think > 0 {
            self.next_think = 0;
            return LaserThink::Off;
        }
        self.next_think = level_time;
        self.think(level_time, trace)
    }
}

/// `MASK_PLAYERSOLID`: what a lightning strike's line stops at.
const MASK_PLAYERSOLID: u32 = 0x1 | 0x10 | 0x100 | 0x1000;
/// `trigger_lightningstrike`'s first think, half a second in.
const STRIKE_START: i32 = 500;
/// The angles its effect plays along: straight down.
const STRIKE_ANGLES: [f32; 3] = [90.0, 0.0, 0.0];

/// One `trigger_lightningstrike`.
#[derive(Clone, Debug, PartialEq)]
pub struct Lightning {
    /// `r.absmin`, `r.absmax`: its brush's box, a unit wider.
    pub absmin: [f32; 3],
    pub absmax: [f32; 3],
    /// `lightningfx`: the effect each strike plays.
    pub effect: String,
    /// `wait` and `random` in milliseconds (1000 and 2000 unset): the pause between
    /// strikes, and how much longer it may be.
    pub wait: f32,
    pub random: f32,
    /// `dmg` (50 unset), and `radius`: a blast at the strike rather than a hit along its
    /// line when set.
    pub damage: i32,
    pub radius: f32,
    /// `genericValue1`: switched off (START_OFF, or used).
    pub off: bool,
    /// `nextthink`; 0 while it waits for a use.
    pub next_think: i32,
    pub targetname: String,
    /// `FL_INACTIVE`.
    pub inactive: bool,
}

/// Why the reference stops the map over a lightning trigger; this server refuses the
/// trigger alone.
#[derive(Clone, Debug, PartialEq)]
pub enum Refused {
    /// "trigger_lightningstrike with no lightningfx".
    NoEffect,
}

/// `SP_trigger_lightningstrike` for `entity`, its brush model's box from `bounds_of`, or
/// `None` for anything else.
pub fn spawn_lightning(
    entity: &Entity,
    bounds_of: impl FnOnce(usize) -> Option<([f32; 3], [f32; 3])>,
    level_time: i32,
) -> Option<Result<Lightning, Refused>> {
    if !entity
        .classname()?
        .eq_ignore_ascii_case("trigger_lightningstrike")
    {
        return None;
    }
    let Some(effect) = entity.get("lightningfx").filter(|fx| !fx.is_empty()) else {
        return Some(Err(Refused::NoEffect));
    };
    let model = entity
        .get("model")
        .and_then(|name| name.strip_prefix('*'))
        .and_then(|index| index.parse().ok())?;
    let (mins, maxs) = bounds_of(model)?;
    let float = |key: &str| {
        entity
            .get(key)
            .map_or(0.0, |value| crate::text_parse::atof(value.as_bytes()))
    };
    let or = |value: f32, unset: f32| if value == 0.0 { unset } else { value };
    let damage = entity
        .get("dmg")
        .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()));
    let spawnflags = entity
        .get("spawnflags")
        .map_or(0, |value| crate::userinfo::atoi(value.as_bytes()));
    let origin = crate::fx_runner::vector(entity, "origin");
    Some(Ok(Lightning {
        absmin: std::array::from_fn(|axis| origin[axis] + mins[axis] - 1.0),
        absmax: std::array::from_fn(|axis| origin[axis] + maxs[axis] + 1.0),
        effect: effect.to_owned(),
        wait: or(float("wait"), 1_000.0),
        random: or(float("random"), 2_000.0),
        damage: if damage == 0 { 50 } else { damage },
        radius: float("radius"),
        off: spawnflags & 1 != 0,
        next_think: level_time + STRIKE_START,
        targetname: entity.get("targetname").unwrap_or_default().to_owned(),
        inactive: false,
    }))
}

/// A lightning trigger's think.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Strike {
    /// Nothing due, or switched off (it stops thinking until used).
    Idle,
    /// It struck from `from` down to `at`, its line meeting entity `struck` there.
    Struck {
        from: [f32; 3],
        at: [f32; 3],
        struck: u16,
    },
}

impl Lightning {
    /// `Think_Strike` and `Do_Strike`: next strike `wait` plus up to `random` on (the
    /// generator's integer draw), then a spot drawn in its box, struck from four units
    /// under its top down to its bottom along `MASK_PLAYERSOLID` (`line` gives whether it
    /// began in something solid, what it met and where it stopped) — a line that began in
    /// something solid tries again next frame.
    pub fn think(
        &mut self,
        level_time: i32,
        rng: &mut Rng,
        line: impl FnOnce([f32; 3], [f32; 3], u32) -> (bool, u16, [f32; 3]),
    ) -> Strike {
        if self.next_think == 0 || self.next_think > level_time {
            return Strike::Idle;
        }
        self.next_think = 0;
        if self.off {
            return Strike::Idle;
        }
        let pause = rng.irand(0, self.random as i32);
        self.next_think = (level_time as f32 + self.wait + pause as f32) as i32;
        let x = rng.flrand(self.absmin[0], self.absmax[0]);
        let y = rng.flrand(self.absmin[1], self.absmax[1]);
        let from = [x, y, self.absmax[2] - 4.0];
        let (start_solid, struck, at) = line(from, [x, y, self.absmin[2]], MASK_PLAYERSOLID);
        if start_solid {
            self.next_think = level_time;
            return Strike::Idle;
        }
        Strike::Struck { from, at, struck }
    }

    /// `Use_Strike`: off when on; on again, striking at once.
    pub fn used(&mut self, level_time: i32) {
        self.off = !self.off;
        if !self.off {
            self.next_think = level_time;
        }
    }

    /// The effect a strike plays at `from`: `G_PlayEffectID`, straight down.
    pub fn effect_event(effect: u32, from: [f32; 3]) -> crate::event_entity::EventEntity {
        crate::map_turret_world::effect_event(
            crate::map_turret_world::EV_PLAY_EFFECT_ID,
            effect,
            from,
            STRIKE_ANGLES,
        )
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

    /// `InitShooter`: a degree of scatter unset, its angle as its direction; `Use_Shooter`
    /// fires a twenty-point blaster bolt from its origin within that scatter.
    #[test]
    fn a_shooter_fires_a_scattered_bolt() {
        let shooter = spawn_shooter(
            &entity(&[
                ("classname", "shooter_blaster"),
                ("origin", "0 0 64"),
                ("angle", "90"),
            ]),
            0,
        )
        .unwrap();
        assert!((shooter.spread - (std::f32::consts::PI / 180.0).sin()).abs() < 1e-6);
        assert_eq!(shooter.finish_at, 0);
        let bolt = shooter.fire(40, 1_000, &mut Rng(7));
        assert_eq!((bolt.owner, bolt.damage, bolt.method_of_death), (40, 20, 6));
        let speed = [6, 7, 10].map(|field| f32::from_bits(bolt.state.raw_field(field).unwrap()));
        assert!(speed[1] > 2_290.0 && speed[0].abs() < 50.0 && speed[2].abs() < 50.0);
        let aimed = spawn_shooter(
            &entity(&[("classname", "shooter_blaster"), ("target", "t")]),
            2_000,
        )
        .unwrap();
        assert_eq!(aimed.finish_at, 2_500);
    }

    /// A lightning trigger needs its effect; it strikes half a second in, then every
    /// `wait` plus up to `random`, at a spot in its box, from four under its top; off,
    /// it stops until used.
    #[test]
    fn lightning_strikes_in_its_box() {
        let spawn = |pairs: &[(&str, &str)]| {
            spawn_lightning(
                &entity(pairs),
                |_| Some(([0.0, 0.0, 0.0], [100.0, 100.0, 200.0])),
                1_000,
            )
        };
        assert_eq!(
            spawn(&[("classname", "trigger_lightningstrike"), ("model", "*2")]),
            Some(Err(Refused::NoEffect))
        );
        let mut storm = spawn(&[
            ("classname", "trigger_lightningstrike"),
            ("model", "*2"),
            ("lightningfx", "env/lightning"),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(
            (storm.wait, storm.random, storm.damage),
            (1_000.0, 2_000.0, 50)
        );
        let mut rng = Rng(3);
        assert_eq!(
            storm.think(1_400, &mut rng, |_, _, _| unreachable!()),
            Strike::Idle
        );
        let struck = storm.think(1_500, &mut rng, |from, to, mask| {
            assert_eq!((from[2], to[2], mask), (197.0, -1.0, MASK_PLAYERSOLID));
            assert!((-1.0..=101.0).contains(&from[0]) && from[0] == to[0]);
            (false, 1_022, to)
        });
        assert!(matches!(struck, Strike::Struck { struck: 1_022, .. }));
        assert!((2_500..=4_500).contains(&storm.next_think));
        storm.used(2_000);
        assert!(storm.off);
        let due = storm.next_think;
        assert_eq!(
            storm.think(due, &mut rng, |_, _, _| unreachable!()),
            Strike::Idle
        );
        assert_eq!(storm.next_think, 0);
        storm.used(9_000);
        assert_eq!(storm.next_think, 9_000);
    }

    /// A laser starts a frame in, off unless its spawnflag says on; a use turns it on and
    /// burns at once, the next turns it off; on, it burns every frame.
    #[test]
    fn a_laser_burns_while_on() {
        let mut laser = spawn_laser(
            &entity(&[("classname", "target_laser"), ("angle", "0"), ("dmg", "3")]),
            0,
        )
        .unwrap();
        let line = |_: [f32; 3], end: [f32; 3]| (5, end);
        assert_eq!(laser.used(Some(2), 50, line), LaserThink::Idle);
        assert_eq!(laser.think(100, line), LaserThink::Off);
        assert!(!laser.on());
        let burned = laser.used(Some(2), 200, line);
        assert_eq!(
            burned,
            LaserThink::Burned {
                struck: 5,
                end: [2_048.0, 0.0, 0.0]
            }
        );
        assert_eq!(
            (laser.activator, laser.damage, laser.on()),
            (Some(2), 3, true)
        );
        assert_eq!(laser.think(250, line), LaserThink::Idle);
        assert!(matches!(laser.think(300, line), LaserThink::Burned { .. }));
        assert_eq!(laser.used(None, 350, line), LaserThink::Off);
        assert!(!laser.on());
    }
}
