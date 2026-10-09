//! The map's shooters and lasers on this server ([`jkr_game_jka::map_shooters`]): a
//! `shooter_blaster` is an entity every client is sent (its fire is heard there), a
//! `target_laser` an `ET_BEAM` sent while it is on, from its origin to where its line
//! stops (`s.origin2`). Both are used by name; a laser's burn is `G_Damage` by whoever
//! turned it on, a shooter's bolt a missile of its own.

use super::*;
use jkr_entity::Entity;
use jkr_game_jka::map_shooters::{self, LASER_MASK, Laser, LaserThink, Shooter, WP_BLASTER};

/// The wire fields `eType`, `weapon`, `origin` and `origin2`; `ET_BEAM`.
const ES_TYPE: usize = 8;
const ES_WEAPON: usize = 14;
const ES_ORIGIN: [usize; 3] = [11, 12, 13];
const ES_ORIGIN2: [usize; 3] = [56, 60, 53];
const ET_BEAM: u32 = 7;
/// `EV_FIRE_WEAPON`, which `Use_Shooter` adds to the shooter.
const EV_FIRE_WEAPON: u32 = 27;
/// `MOD_TARGET_LASER`; `ENTITYNUM_WORLD`.
const MOD_TARGET_LASER: u32 = 40;
const ENTITYNUM_WORLD: u16 = 1_022;

/// The level's shooters and lasers, each with its entity.
#[derive(Debug, Default)]
pub(super) struct MapShooters {
    shooters: Vec<(EntityId, Shooter)>,
    lasers: Vec<(EntityId, Laser)>,
}

impl MapShooters {
    /// The attacker a shooter's bolt or a laser's own burn is credited to: the entity
    /// itself, which is no client.
    pub(super) fn attacker(&self, number: u16) -> Option<Attacker> {
        let ours = self
            .shooters
            .iter()
            .map(|(id, _)| id)
            .chain(self.lasers.iter().map(|(id, _)| id))
            .any(|id| id.legacy_number() == number);
        ours.then_some(Attacker {
            npc: false,
            client: number,
            max_health: 100,
            team: 0,
            saber_knockback: [0.0; 4],
        })
    }
}

impl NativeGame {
    /// `SP_shooter_blaster` and `SP_target_laser` for everything of theirs the map places
    /// in this game type; a shooter registers the blaster (`RegisterItem`).
    pub(super) fn spawn_map_shooters(&mut self, entities: &[Entity], level_time: i32) {
        self.stock.shooters = MapShooters::default();
        for entity in entities.iter().skip(1) {
            if !jkr_game_jka::spawn_table::kept_by_reference(entity, self.gametype) {
                continue;
            }
            if let Some(shooter) = map_shooters::spawn_shooter(entity, level_time) {
                let mut state = placed_state(shooter.origin);
                state.set_raw_field(ES_WEAPON, WP_BLASTER);
                if let Some(id) = self.pool.spawn_entity(state, level_time) {
                    self.stock.shooters.shooters.push((id, shooter));
                }
                if let Some(blaster) = jkr_game_jka::items::find("weapon_blaster") {
                    self.register_items([blaster]);
                }
            } else if let Some(laser) = map_shooters::spawn_laser(entity, level_time) {
                let mut state = placed_state(laser.origin);
                state.set_raw_field(ES_TYPE, ET_BEAM);
                if let Some(id) = self.pool.spawn_entity(state, level_time) {
                    // Linked first by its think.
                    self.pool.show(id, false);
                    self.stock.shooters.lasers.push((id, laser));
                }
            }
        }
    }

    /// `G_RunThink` for the shooters (`InitShooter_Finish`: the target picked) and the
    /// lasers (their start, then a burn a frame while on).
    pub(super) fn run_map_shooters(&mut self, level_time: i32) {
        for index in 0..self.stock.shooters.shooters.len() {
            let shooter = &self.stock.shooters.shooters[index].1;
            if shooter.finish_at == 0 || shooter.finish_at > level_time {
                continue;
            }
            let target = shooter.target.clone();
            let aim = self.pick_target_origin(&target);
            let shooter = &mut self.stock.shooters.shooters[index].1;
            (shooter.finish_at, shooter.aim) = (0, aim);
        }
        for index in 0..self.stock.shooters.lasers.len() {
            let laser = &self.stock.shooters.lasers[index].1;
            if laser.next_think == 0 || laser.next_think > level_time {
                continue;
            }
            if !laser.started && !laser.target.is_empty() {
                let target = laser.target.clone();
                let aim = self.find_target_middle(index, &target);
                self.stock.shooters.lasers[index].1.aim = aim;
            }
            self.gather_obstacles(usize::MAX);
            let thought = {
                let (map, obstacles) = (self.map.as_ref(), &self.obstacles);
                let laser = &mut self.stock.shooters.lasers[index].1;
                laser.think(level_time, |from, to| laser_line(map, obstacles, from, to))
            };
            self.carry_out_laser(index, thought, level_time);
        }
    }

    /// `G_UseTargets` reaching the shooters and lasers called `name`, used for
    /// `activator`: a shooter fires, a laser goes on or off.
    pub(super) fn use_map_shooters(&mut self, name: &str, activator: usize, level_time: i32) {
        if name.is_empty() {
            return;
        }
        for index in 0..self.stock.shooters.shooters.len() {
            let (id, shooter) = &self.stock.shooters.shooters[index];
            if shooter.inactive || !shooter.targetname.eq_ignore_ascii_case(name) {
                continue;
            }
            let id = *id;
            let missile = shooter.fire(id.legacy_number(), level_time, &mut self.deaths.rng);
            if let Some(bolt) = self.pool.spawn_entity(missile.state.clone(), level_time) {
                self.pool.set_bounds(bolt, missile.bounds);
                self.missiles.push((bolt, missile));
            }
            self.raise_on(id, EV_FIRE_WEAPON, 0, level_time);
        }
        let activator = u16::try_from(activator).ok();
        for index in 0..self.stock.shooters.lasers.len() {
            let laser = &self.stock.shooters.lasers[index].1;
            if laser.inactive || !laser.targetname.eq_ignore_ascii_case(name) {
                continue;
            }
            self.gather_obstacles(usize::MAX);
            let thought = {
                let (map, obstacles) = (self.map.as_ref(), &self.obstacles);
                let laser = &mut self.stock.shooters.lasers[index].1;
                laser.used(activator, level_time, |from, to| {
                    laser_line(map, obstacles, from, to)
                })
            };
            self.carry_out_laser(index, thought, level_time);
        }
    }

    /// `target_activate` and `target_deactivate` reaching the shooters and lasers called
    /// `name` (`GlobalUse`'s `FL_INACTIVE`).
    pub(super) fn set_map_shooters_active(&mut self, name: &str, active: bool) {
        let shooters = self
            .stock
            .shooters
            .shooters
            .iter_mut()
            .map(|(_, s)| (&s.targetname, &mut s.inactive));
        let lasers = self
            .stock
            .shooters
            .lasers
            .iter_mut()
            .map(|(_, l)| (&l.targetname, &mut l.inactive));
        for (targetname, inactive) in shooters.chain(lasers) {
            if targetname.eq_ignore_ascii_case(name) {
                *inactive = !active;
            }
        }
    }

    /// What a laser's think or use came to: its beam sent (or not) and, burning, `G_Damage`
    /// on what its line met (never entity 0, nor the world) by whoever turned it on.
    fn carry_out_laser(&mut self, index: usize, thought: LaserThink, level_time: i32) {
        let (id, laser) = &self.stock.shooters.lasers[index];
        let (id, on, end, movedir) = (*id, laser.on(), laser.end, laser.movedir);
        let attacker = laser.activator.unwrap_or(id.legacy_number());
        let damage = laser.damage;
        if let Some(state) = self.pool.state_mut(id) {
            for axis in 0..3 {
                state.set_raw_field(ES_ORIGIN2[axis], end[axis].to_bits());
            }
        }
        self.pool.show(id, on);
        let LaserThink::Burned { struck, end } = thought else {
            return;
        };
        if struck == 0 || struck >= ENTITYNUM_WORLD {
            return;
        }
        let request = DamageRequest {
            level_time,
            attacker: self.attacker_for(attacker),
            direction: Some(movedir),
            point: Some(end),
            damage,
            flags: jkr_game_jka::damage::DAMAGE_NO_KNOCKBACK,
            means: MOD_TARGET_LASER,
        };
        let _ = self.strike(usize::from(attacker), usize::from(struck), request, false);
    }

    /// `G_PickTarget(name)`: where one of the entities the level keeps under `name` stands,
    /// drawn with the C `rand` as the reference draws; said on the console when there is
    /// none.
    fn pick_target_origin(&mut self, name: &str) -> Option<[f32; 3]> {
        let places: Vec<[f32; 3]> = self
            .named_entities(name)
            .map(|(origin, _)| origin)
            .collect();
        if places.is_empty() {
            super::bridge_map_logic::pick_target_missing(name);
            return None;
        }
        Some(places[self.rand.next() as usize % places.len()])
    }

    /// `target_laser_start`'s `G_Find`: the middle of the first entity the level keeps
    /// under `name` (`s.origin` plus half its `r.mins` and `r.maxs`), or the reference's
    /// complaint when there is none.
    fn find_target_middle(&mut self, index: usize, name: &str) -> Option<[f32; 3]> {
        let found = self.named_entities(name).next();
        if found.is_none() {
            let origin = self.stock.shooters.lasers[index].1.origin;
            eprintln!(
                "target_laser at ({} {} {}): {name} is a bad target",
                origin[0] as i32, origin[1] as i32, origin[2] as i32
            );
        }
        found.map(|(origin, (mins, maxs))| {
            std::array::from_fn(|axis| origin[axis] + 0.5 * mins[axis] + 0.5 * maxs[axis])
        })
    }

    /// The entities the level keeps under `name`, in the map's order: where each stands and
    /// its box (an inline model's own, or none).
    fn named_entities<'a>(
        &'a self,
        name: &'a str,
    ) -> impl Iterator<Item = ([f32; 3], ([f32; 3], [f32; 3]))> + 'a {
        let gametype = self.gametype;
        self.map.iter().flat_map(move |map| {
            map.entities.iter().skip(1).filter_map(move |entity| {
                let named = entity
                    .get("targetname")
                    .is_some_and(|own| own.eq_ignore_ascii_case(name));
                if !named || !jkr_game_jka::spawn_table::kept_by_reference(entity, gametype) {
                    return None;
                }
                let origin = entity.vector("origin").ok().flatten().unwrap_or([0.0; 3]);
                let bounds = entity
                    .get("model")
                    .and_then(|model| model.strip_prefix('*'))
                    .and_then(|index| index.parse().ok())
                    .and_then(|index| crate::map::brush_bounds(&map.bsp, index))
                    .unwrap_or(([0.0; 3], [0.0; 3]));
                Some((origin, bounds))
            })
        })
    }
}

/// A placed entity's state: where it stands, as both `s.origin` and `s.pos.trBase`.
fn placed_state(origin: [f32; 3]) -> EntityState {
    let mut state = EntityState::zero(0, &jkr_protocol::LEGACY_ENTITY_FIELDS);
    for axis in 0..3 {
        for field in [ES_POS_BASE[axis], ES_ORIGIN[axis]] {
            state.set_raw_field(field, origin[axis].to_bits());
        }
    }
    state
}

/// A laser's line (`CONTENTS_SOLID | CONTENTS_BODY | CONTENTS_CORPSE`) through the world
/// and everything in it: the entity it met and where it stopped.
fn laser_line(
    map: Option<&LoadedMap>,
    obstacles: &[BoxObstacle],
    from: [f32; 3],
    to: [f32; 3],
) -> (u16, [f32; 3]) {
    let Some(map) = map else {
        return (jkr_protocol::ENTITY_NUMBER_NONE, to);
    };
    let trace = WithPlayers {
        world: WorldCollision {
            bsp: &map.bsp,
            scratch: &map.scratch,
        },
        players: obstacles,
    }
    .trace(from, [0.0; 3], [0.0; 3], to, LASER_MASK);
    (trace.entity_number, trace.end_position)
}
