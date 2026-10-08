//! `func_usable` on this server ([`jkr_game_jka::usables`]): each an `ET_MOVER` of its
//! inline model — with its `model2` — standing where the map put it, met by every trace
//! while it is there and sent to nobody while it is away. Uses reach it from what
//! targets it, from a player's use key and, given health, from blows.

use super::*;
use jkr_game_jka::usables::{Usable, Used};

/// `CONTENTS_BODY`: what `G_TestEntityPosition` looks for in a returning brush's box.
const CONTENTS_BODY: u32 = 0x100;
/// The wire fields `modelindex2`, `constantLight`, `time` and `frame`.
const ES_MODEL2: usize = 41;
const ES_CONSTANT_LIGHT: usize = 64;
const ES_TIME: usize = 65;
const ES_FRAME: usize = 83;
/// `origin` on the wire, beside `pos.trBase`.
const ES_ORIGIN: [usize; 3] = [11, 12, 13];

/// The level's usables, each with its entity.
#[derive(Debug, Default)]
pub(super) struct Usables {
    pub(super) placed: Vec<(EntityId, Usable)>,
    /// The usable whose targets are being fired: `G_UseTargets2` never uses the entity
    /// that fires ("Entity used itself.").
    firing: Option<usize>,
}

impl Usables {
    /// The ones that are there, as obstacles to every trace, each through its own model.
    pub(super) fn obstacles(&self) -> impl Iterator<Item = BoxObstacle> + '_ {
        self.placed
            .iter()
            .filter(|(_, usable)| usable.contents != 0)
            .map(|(id, usable)| BoxObstacle {
                entity: id.legacy_number(),
                origin: usable.origin,
                bounds: usable.bounds,
                contents: usable.contents,
                model: Some(usable.model),
                linked_box: None,
            })
    }

    /// Which usable entity `number` is.
    pub(super) fn position(&self, number: u16) -> Option<usize> {
        self.placed
            .iter()
            .position(|(id, _)| id.legacy_number() == number)
    }

    /// The ones a blow uses, as a blast finds them: every one with health, there or not,
    /// as one that went away is still linked (`SV_LinkEntity`'s box, a unit wider).
    pub(super) fn splash_targets(&self) -> impl Iterator<Item = SplashTarget> + '_ {
        self.placed
            .iter()
            .filter(|(_, usable)| usable.takes_damage)
            .map(|(id, usable)| {
                let (mins, maxs) = usable.bounds;
                SplashTarget {
                    number: id.legacy_number(),
                    bounds: (
                        std::array::from_fn(|axis| usable.origin[axis] + mins[axis] - 1.0),
                        std::array::from_fn(|axis| usable.origin[axis] + maxs[axis] + 1.0),
                    ),
                    origin: usable.origin,
                    takes_damage: true,
                }
            })
    }

    /// Usable entity `number`, if it is one.
    pub(super) fn get(&self, number: u16) -> Option<&Usable> {
        self.position(number).map(|index| &self.placed[index].1)
    }
}

impl NativeGame {
    /// `SP_func_usable` for every usable the map places in this game type.
    pub(super) fn spawn_usables(&mut self) {
        self.stock.usables = Usables::default();
        let Some(map) = self.map.take() else { return };
        for entity in map.entities.iter().skip(1) {
            if !jkr_game_jka::spawn_table::kept_by_reference(entity, self.gametype) {
                continue;
            }
            let bounds_of = |model| crate::map::brush_bounds(&map.bsp, model);
            let Some(usable) = jkr_game_jka::usables::spawn(entity, bounds_of) else {
                continue;
            };
            let model2 = usable.model2.as_ref().map_or(0, |name| {
                self.models.index(name.as_bytes(), &mut |index, value| {
                    self.told.push(Told::ConfigString {
                        index,
                        previous: Vec::new(),
                        value: value.to_vec(),
                    })
                })
            });
            if let Some(id) = self.pool.spawn_entity(usable_state(&usable, model2), 0) {
                self.pool.set_bounds(id, usable.bounds);
                self.stock.usables.placed.push((id, usable));
                self.publish_usable(self.stock.usables.placed.len() - 1);
            }
        }
        self.map = Some(map);
    }

    /// `G_UseTargets` reaching the usables called `name`, used for `activator`.
    pub(super) fn use_usables(&mut self, name: &str, activator: usize, level_time: i32) {
        if name.is_empty() {
            return;
        }
        for index in 0..self.stock.usables.placed.len() {
            if !self.stock.usables.placed[index]
                .1
                .targetname
                .eq_ignore_ascii_case(name)
            {
                continue;
            }
            if self.stock.usables.firing == Some(index) {
                eprintln!("WARNING: Entity used itself.");
                continue;
            }
            self.use_usable(index, activator, level_time);
        }
    }

    /// `TryUse` reaching usable entity `number`: one the use key may use
    /// (`ValidUseTarget`, Siege's `teamuser`) holds `client` in the use pose and is used.
    /// Whether it was.
    pub(super) fn use_usable_by_key(&mut self, client: usize, number: u16, time: i32) -> bool {
        let Some(index) = self.stock.usables.position(number) else {
            return false;
        };
        let team = self.peer(client).map_or(0, |peer| peer.session.team);
        let friendly_fire = self.integer_cvar(b"g_ff_objectives", 0) != 0;
        if !self.stock.usables.placed[index]
            .1
            .answers_use_key(self.gametype, team, friendly_fire)
        {
            return false;
        }
        if let Some(peer) = self.peer_mut(client) {
            super::bridge_use::hold_use_pose(peer, true);
        }
        self.use_usable(index, client, time);
        true
    }

    /// `G_Damage` on usable entity `number` by `attacker`: one with health is used by the
    /// blow — nothing is, before a Siege round begins. Whether `number` is a usable.
    pub(super) fn strike_usable(&mut self, number: u16, attacker: usize, time: i32) -> bool {
        let Some(index) = self.stock.usables.position(number) else {
            return false;
        };
        let begun = self.siege.as_ref().is_some_and(|siege| siege.round.begun);
        if super::bridge_client_impact::round_waiting(self.gametype, begun)
            || !self.stock.usables.placed[index].1.takes_damage
        {
            return true;
        }
        let clear = self.usable_box_clear(index);
        let used = self.stock.usables.placed[index].1.struck(time, clear);
        self.carry_out_usable(index, used, attacker, time);
        true
    }

    /// `G_RunThink` for the usables whose think is due: back in use, or back solid.
    pub(super) fn run_usables(&mut self, level_time: i32) {
        for index in 0..self.stock.usables.placed.len() {
            if !self.stock.usables.placed[index].1.think_due(level_time) {
                continue;
            }
            let clear = self.usable_box_clear(index);
            let used = self.stock.usables.placed[index].1.think(level_time, clear);
            self.carry_out_usable(index, used, usize::MAX, level_time);
        }
    }

    /// `target_activate` and `target_deactivate` reaching the usables called `name`.
    pub(super) fn set_usables_active(&mut self, name: &str, active: bool) {
        for (_, usable) in self.stock.usables.placed.iter_mut() {
            if usable.targetname.eq_ignore_ascii_case(name) {
                usable.inactive = !active;
            }
        }
    }

    /// `GlobalUse` of usable `index` for `activator`.
    fn use_usable(&mut self, index: usize, activator: usize, level_time: i32) {
        let clear = self.usable_box_clear(index);
        let used = self.stock.usables.placed[index].1.used(level_time, clear);
        self.carry_out_usable(index, used, activator, level_time);
    }

    /// What a use came to: the entity as it now is, and the targets it fires —
    /// `target` for `activator`, or `target2` for nobody, as nothing sets a usable's own
    /// `activator`.
    fn carry_out_usable(&mut self, index: usize, used: Used, activator: usize, time: i32) {
        self.publish_usable(index);
        let usable = &self.stock.usables.placed[index].1;
        let (name, activator) = match used {
            Used::Target => (usable.target.clone(), activator),
            Used::Returned => (usable.target2.clone(), usize::MAX),
            Used::Nothing | Used::Waiting => return,
        };
        let firing = self.stock.usables.firing.replace(index);
        self.fire_targets(&name, activator, time);
        self.stock.usables.firing = firing;
    }

    /// Whether usable `index` may come back now: `G_TestEntityPosition` with
    /// `CONTENTS_BODY` finds no body in its box. Only one that waits for that, and is
    /// away, looks.
    fn usable_box_clear(&mut self, index: usize) -> bool {
        let usable = &self.stock.usables.placed[index].1;
        if !usable.waits_for_clear() || usable.contents != 0 {
            return true;
        }
        let (origin, (mins, maxs)) = (usable.origin, usable.bounds);
        self.gather_obstacles(usize::MAX);
        let Some(map) = self.map.as_ref() else {
            return true;
        };
        let trace = WithPlayers {
            world: WorldCollision {
                bsp: &map.bsp,
                scratch: &map.scratch,
            },
            players: &self.obstacles,
        }
        .trace(origin, mins, maxs, origin, CONTENTS_BODY);
        !trace.start_solid
    }

    /// Usable `index`'s entity as the usable now is: sent or not, drawn or not, the
    /// frame its shader shows.
    fn publish_usable(&mut self, index: usize) {
        let (id, usable) = &self.stock.usables.placed[index];
        let (id, sent, eflags, frame) = (*id, usable.sent(), usable.eflags, usable.frame);
        if let Some(state) = self.pool.state_mut(id) {
            state.set_raw_field(ES_EFLAGS, eflags);
            state.set_raw_field(ES_FRAME, frame as u32);
        }
        self.pool.show(id, sent);
    }
}

/// A usable's entity: `ET_MOVER` of its inline model and `model2`, standing at its
/// origin, lit and animated as its keys say.
fn usable_state(usable: &Usable, model2: u16) -> EntityState {
    let mut state = EntityState::zero(0, &jkr_protocol::LEGACY_ENTITY_FIELDS);
    state.set_raw_field(ES_ENTITY_TYPE, jkr_game_jka::movers::ET_MOVER);
    jkr_game_jka::triggers::set_brush_model(&mut state, usable.model);
    state.set_raw_field(ES_MODEL2, u32::from(model2));
    for axis in 0..3 {
        for field in [ES_POS_BASE[axis], ES_ORIGIN[axis]] {
            state.set_raw_field(field, usable.origin[axis].to_bits());
        }
    }
    state.set_raw_field(ES_CONSTANT_LIGHT, usable.constant_light);
    state.set_raw_field(ES_TIME, usable.shader_frames() as u32);
    state
}
