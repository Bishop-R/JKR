//! `misc_model_breakable` on this server ([`jkr_game_jka::map_props`]): each prop an
//! entity every client draws (`ET_GENERAL` with its model) and, solid, one every trace
//! meets — and clients predict, by the box `SV_LinkEntity` encodes for them.

use super::*;
use jkr_game_jka::map_props::{MASK_NPCSOLID, Prop, Refused};

/// `CONTENTS_SOLID|CONTENTS_BODY`: what makes `SV_LinkEntity` encode a box.
const PREDICTED_CONTENTS: u32 = 0x1 | 0x100;

/// The level's props, each with its entity.
#[derive(Debug, Default)]
pub(super) struct Props(Vec<(EntityId, Prop)>);

impl Props {
    /// The props with contents, as obstacles to every trace.
    pub(super) fn obstacles(&self) -> impl Iterator<Item = BoxObstacle> + '_ {
        self.0
            .iter()
            .filter(|(_, prop)| prop.contents != 0)
            .map(|(id, prop)| BoxObstacle {
                entity: id.legacy_number(),
                origin: prop.origin,
                bounds: prop.bounds,
                contents: prop.contents,
                model: None,
                linked_box: None,
            })
    }
}

impl NativeGame {
    /// `SP_misc_model_breakable` for every prop the map places in this game type.
    pub(super) fn spawn_props(&mut self, entities: &[jkr_entity::Entity], time: i32) {
        self.stock.props.0.clear();
        for entity in entities.iter().skip(1) {
            if !jkr_game_jka::spawn_table::kept_by_reference(entity, self.gametype) {
                continue;
            }
            let mut prop = match Prop::spawn(entity) {
                None => continue,
                Some(Err(Refused::NoModel { origin })) => {
                    eprintln!(
                        "misc_model_breakable not spawned (the reference stops the map): no \
                         model set at ({:.1} {:.1} {:.1})",
                        origin[0], origin[1], origin[2]
                    );
                    continue;
                }
                Some(Ok(prop)) => prop,
            };
            self.drop_prop(&mut prop);
            let model = self
                .models
                .index(prop.model.as_bytes(), &mut |index, value| {
                    self.told.push(Told::ConfigString {
                        index,
                        previous: Vec::new(),
                        value: value.to_vec(),
                    })
                });
            if let Some(id) = self.pool.spawn_entity(prop_state(&prop, model), time) {
                self.pool.set_bounds(id, prop.bounds);
                self.stock.props.0.push((id, prop));
            }
        }
    }

    /// `misc_model_breakable_gravity_init`'s drop to the floor, for a prop with `gravity`.
    fn drop_prop(&mut self, prop: &mut Prop) {
        if !prop.falls {
            return;
        }
        self.gather_obstacles(usize::MAX);
        let Some(map) = self.map.as_ref() else {
            return;
        };
        let (mins, maxs) = prop.bounds;
        let obstacles = &self.obstacles;
        prop.fall(|top, bottom| {
            let trace = WithPlayers {
                world: WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                },
                players: obstacles,
            }
            .trace(top, mins, maxs, bottom, MASK_NPCSOLID);
            (
                trace.all_solid,
                trace.start_solid,
                trace.fraction,
                trace.end_position,
            )
        });
    }
}

/// The prop's entity: `ET_GENERAL`, its model, standing where it stands, turned as the map
/// turned it, and — solid — the box `SV_LinkEntity` encodes for clients to predict
/// (`s.solid`: x/y radius, lower z, upper z + 32, each a byte of at least one).
fn prop_state(prop: &Prop, model: u16) -> EntityState {
    let mut state = EntityState::zero(0, &jkr_protocol::LEGACY_ENTITY_FIELDS);
    for axis in 0..3 {
        for field in [ES_POS_BASE[axis], [11, 12, 13][axis]] {
            state.set_raw_field(field, prop.origin[axis].to_bits());
        }
        for field in [[5, 3, 33][axis], [25, 9, 24][axis]] {
            state.set_raw_field(field, prop.angles[axis].to_bits());
        }
    }
    state.set_raw_field(40, 1023); // owner = ENTITYNUM_NONE
    state.set_raw_field(46, u32::from(model));
    if prop.contents & PREDICTED_CONTENTS != 0 {
        let (mins, maxs) = prop.bounds;
        let byte = |value: f32| (value as i32).clamp(1, 255) as u32;
        let solid = byte(maxs[0]) | (byte(-mins[2]) << 8) | (byte(maxs[2] + 32.0) << 16);
        state.set_raw_field(26, solid);
    }
    state
}
