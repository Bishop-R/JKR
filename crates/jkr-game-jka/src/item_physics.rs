//! `G_RunItem` and `G_BounceItem` (OpenJK `codemp/game/g_items.c:3143-3272`) for a world
//! object that is not a missile but has `physicsObject` set — the Jedi Master's saber while
//! held, a holocron, a set-down sentry ([`crate::sentry`]) or E-Web ([`crate::eweb`]) — in
//! the two halves its caller runs its think between: [`fall`], then [`bounce`].

use crate::pmove::{MovementCollision, MovementTrace};
use crate::weapon_fire::Missile;
use jkr_protocol::EntityState;

/// The entity's wire fields `G_RunItem` reads and writes.
const ES_POS_TIME: usize = 0;
const ES_POS_BASE: [usize; 3] = [2, 1, 4];
const ES_POS_DELTA: [usize; 3] = [6, 7, 10];
const ES_POS_DURATION: usize = 20;
const ES_GROUND: usize = 22;
const ES_POS_TYPE: usize = 23;
/// `TR_STATIONARY`, `TR_GRAVITY`, `CONTENTS_NODROP`.
pub(crate) const TR_STATIONARY: u32 = 0;
pub(crate) const TR_GRAVITY: u32 = 6;
pub(crate) const CONTENTS_NODROP: u32 = 0x800;

/// What the first half left: the object at rest (only its think runs), or moved along
/// its trajectory with the trace that moved it.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ItemMove {
    Resting,
    Moved(MovementTrace),
}

/// What the second half did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ItemBounce {
    /// The move struck nothing.
    Clear,
    /// It came to rest in a no-drop volume, where the reference frees it.
    NoDrop,
    /// It came to rest on a floor, a unit above it (`G_SetOrigin`, snapped).
    Stopped,
    /// It struck a wall and drops from it; a holocron's touch follows with the trace.
    Bounced(MovementTrace),
}

/// `G_RunItem` up to its think for an object with wire state `state` at `current`
/// (`r.currentOrigin`): off the ground it falls again; at rest it does nothing more; else it
/// is swept (`sweep`: its box with its pass entity and clip mask) from where it is to where
/// its trajectory has it now, and placed at the sweep's end.
pub(crate) fn fall(
    state: &mut EntityState,
    current: &mut [f32; 3],
    level_time: i32,
    sweep: impl FnOnce([f32; 3], [f32; 3]) -> MovementTrace,
) -> ItemMove {
    let get = |state: &EntityState, index: usize| state.raw_field(index).unwrap_or(0);
    // "if groundentity has been set to ENTITYNUM_NONE, it may have been pushed off an edge".
    if get(state, ES_GROUND) == u32::from(jkr_protocol::ENTITY_NUMBER_NONE)
        && get(state, ES_POS_TYPE) != TR_GRAVITY
    {
        state.set_raw_field(ES_POS_TYPE, TR_GRAVITY);
        state.set_raw_field(ES_POS_TIME, level_time as u32);
    }
    if get(state, ES_POS_TYPE) == TR_STATIONARY {
        return ItemMove::Resting;
    }
    let origin = crate::trajectory::legacy_evaluate_trajectory(
        read(state, ES_POS_BASE),
        read(state, ES_POS_DELTA),
        get(state, ES_POS_TYPE) as u8,
        get(state, ES_POS_TIME) as i32,
        get(state, ES_POS_DURATION) as i32,
        level_time,
    );
    let mut trace = sweep(*current, origin);
    *current = trace.end_position;
    if trace.start_solid {
        trace.fraction = 0.0;
    }
    ItemMove::Moved(trace)
}

/// `G_RunItem` after its think, for a move that struck something (`contents`: the point
/// contents where the object is now): `G_BounceItem` with no bounce left in it
/// (`physicsBounce` is never set) — at rest a unit above a floor, or dropping from a wall
/// from a unit off it.
pub(crate) fn bounce(
    state: &mut EntityState,
    current: &mut [f32; 3],
    trace: &MovementTrace,
    level_time: i32,
    previous_time: i32,
    contents: impl FnOnce([f32; 3]) -> u32,
) -> ItemBounce {
    if trace.fraction == 1.0 {
        return ItemBounce::Clear;
    }
    if contents(*current) & CONTENTS_NODROP != 0 {
        return ItemBounce::NoDrop;
    }
    // The velocity at the moment of the hit (`level.previousTime + (level.time -
    // level.previousTime) * trace->fraction`, summed as a float), reflected, then scaled by
    // `physicsBounce` — zero, which keeps each component's sign.
    let get = |index: usize| state.raw_field(index).unwrap_or(0);
    let hit_time =
        (previous_time as f32 + (level_time - previous_time) as f32 * trace.fraction) as i32;
    let velocity = crate::trajectory::legacy_evaluate_trajectory_delta(
        read(state, ES_POS_DELTA),
        get(ES_POS_TYPE) as u8,
        get(ES_POS_TIME) as i32,
        get(ES_POS_DURATION) as i32,
        hit_time,
    );
    let normal = trace.plane_normal;
    let dot = velocity.iter().zip(normal).map(|(a, b)| a * b).sum::<f32>();
    let reflected: [f32; 3] =
        std::array::from_fn(|axis| (velocity[axis] + -2.0 * dot * normal[axis]) * 0.0);
    write(state, ES_POS_DELTA, reflected);
    if normal[2] > 0.0 && reflected[2] < 40.0 {
        let mut end = trace.end_position;
        end[2] += 1.0;
        set_origin(state, current, crate::weapon_fire::snap_vector(end));
        state.set_raw_field(ES_GROUND, u32::from(trace.entity_number));
        return ItemBounce::Stopped;
    }
    *current = std::array::from_fn(|axis| current[axis] + normal[axis]);
    write(state, ES_POS_BASE, *current);
    state.set_raw_field(ES_POS_TIME, level_time as u32);
    ItemBounce::Bounced(*trace)
}

/// `G_SetOrigin`: stationary at `origin`, there now.
pub(crate) fn set_origin(state: &mut EntityState, current: &mut [f32; 3], origin: [f32; 3]) {
    write(state, ES_POS_BASE, origin);
    write(state, ES_POS_DELTA, [0.0; 3]);
    for (index, value) in [(ES_POS_TYPE, TR_STATIONARY), (ES_POS_TIME, 0), (ES_POS_DURATION, 0)] {
        state.set_raw_field(index, value);
    }
    *current = origin;
}

/// [`fall`] for a missile-shaped object, swept with its own box and clip mask.
pub(crate) fn item_move(
    missile: &mut Missile,
    level_time: i32,
    world: &dyn MovementCollision,
) -> ItemMove {
    let (bounds, mask) = (missile.bounds, missile.clip_mask);
    fall(&mut missile.state, &mut missile.current, level_time, |from, to| {
        world.trace(from, bounds.0, bounds.1, to, mask)
    })
}

/// [`bounce`] for a missile-shaped object.
pub(crate) fn item_bounce(
    missile: &mut Missile,
    trace: &MovementTrace,
    level_time: i32,
    previous_time: i32,
    world: &dyn MovementCollision,
) -> ItemBounce {
    bounce(
        &mut missile.state,
        &mut missile.current,
        trace,
        level_time,
        previous_time,
        |point| world.point_contents(point),
    )
}

fn read(state: &EntityState, fields: [usize; 3]) -> [f32; 3] {
    fields.map(|field| f32::from_bits(state.raw_field(field).unwrap_or(0)))
}

fn write(state: &mut EntityState, fields: [usize; 3], value: [f32; 3]) {
    for (field, value) in fields.into_iter().zip(value) {
        state.set_raw_field(field, value.to_bits());
    }
}
