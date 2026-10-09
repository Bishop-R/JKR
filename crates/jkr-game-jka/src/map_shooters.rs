//! The map's shooters and lasers: `shooter_blaster` (`InitShooter`, `InitShooter_Finish`,
//! `Use_Shooter`, OpenJK `codemp/game/g_misc.c:1166-1220`) fires a blaster bolt each time it
//! is used, at its target or along its angles, a little scattered; `target_laser`
//! (`target_laser_start`, `target_laser_think`, `target_laser_use`, `g_target.c:331-430`)
//! is a beam that, while on, hurts whatever it meets for its damage every frame. No stock
//! map places either; custom maps do.

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
