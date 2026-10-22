//! Stock's saber compensation on this client ([`jkr_client::saber_compensation`]): for a
//! little while after the server reports a blade owner's hit, every drawn blade of that
//! owner is traced from hilt to tip each frame (5 ms apart at most); where the first thing
//! it strikes is the body the server named and it passes into that model, the hit's sparks
//! play and the blade leaves its glowing cut.

use crate::actor_marks::{Body, box_trace};
use crate::effect_runtime;
use glam::Vec3;
use jkr_bsp::{Aabb, Bsp, TraceScratch};
use jkr_client::ghoul2_marks;
use jkr_client::pmove::ENTITY_NUMBER_WORLD;
use jkr_client::saber_compensation::{DEBOUNCE_MS, OwnerHits, ServerHit};
use jkr_protocol::{ENTITY_NUMBER_NONE, Snapshot};
use std::time::Instant;

/// `MASK_SOLID`, `MASK_PLAYERSOLID` (`bg_public.h:1225-1226`).
const MASK_SOLID: u32 = 0x1001;
const MASK_PLAYERSOLID: u32 = 0x1111;
/// Blade owners an entity number can name (`MAX_GENTITIES`).
const OWNERS: usize = 1_024;

/// Each owner's last server hit and each blade's next compensation time.
pub(crate) struct Compensation {
    hits: Box<[OwnerHits]>,
    /// By blade contact key (`storageTime`).
    next: Box<[i64]>,
    /// `cg_saberClientVisualCompensation`.
    pub(crate) enabled: bool,
    /// This frame's effects, spawned once the traces are done.
    spawns: Vec<(&'static str, Vec3, Vec3)>,
    /// This frame's bodies, gathered once a blade needs them.
    bodies: Vec<Body>,
}

impl Default for Compensation {
    fn default() -> Self {
        Self {
            hits: vec![OwnerHits::default(); OWNERS].into_boxed_slice(),
            next: vec![i64::MIN; OWNERS * 24].into_boxed_slice(),
            enabled: true,
            spawns: Vec::with_capacity(64),
            bodies: Vec::with_capacity(64),
        }
    }
}

impl Compensation {
    /// `serverSaberHitIndex`/`Time`/`FleshImpact` of `owner`, from its `EV_SABER_HIT`.
    pub(crate) fn observe(&mut self, owner: u16, hit: ServerHit) {
        if let Some(hits) = self.hits.get_mut(usize::from(owner)) {
            hits.record(hit);
        }
    }

    /// A new map or session: nothing is remembered.
    pub(crate) fn clear(&mut self) {
        self.hits.fill(OwnerHits::default());
        self.next.fill(i64::MIN);
    }
}

/// `CG_SaberCompWork` for every drawn blade at presentation time `time`.
pub(crate) fn frame(
    gpu: &mut crate::GpuState,
    time: i64,
    now: Instant,
    audio: &mut Option<crate::GameAudio>,
) {
    let contacts = &mut gpu.effect_aux.saber_contacts;
    if !contacts.enabled || !contacts.compensation.enabled {
        return;
    }
    let compensation = &mut contacts.compensation;
    let world = gpu
        .demo_session
        .as_ref()
        .map_or(&gpu.live_world, crate::demo_playback::Session::world);
    let snapshot = crate::first_person_view::presented_snapshot(
        gpu.live_session.as_ref(),
        gpu.demo_session.as_ref(),
        time as i32,
    );
    let mut gathered = false;
    for instance in &gpu.saber_instances {
        let Some((key, blade, _, _)) = instance.contact() else {
            continue;
        };
        // `storageTime`: each blade is looked at 5 ms apart at most, hit or not.
        if compensation.next.get(key).is_none_or(|next| *next >= time) {
            continue;
        }
        compensation.next[key] = time + DEBOUNCE_MS;
        let owner = key / 24;
        let Some(hit) = compensation.hits.get(owner).and_then(|hits| hits.at(time)) else {
            continue;
        };
        if !hit.compensating(time) {
            continue;
        }
        if !gathered {
            crate::actor_marks::bodies(&gpu.actor_meshes, time, &mut compensation.bodies);
            gathered = true;
        }
        // The blade and one unit more, cut short where it meets the world and nudged a
        // tenth on (stock nudged it for a backward trace it has since switched off:
        // "sometimes it just makes too many effects", `cg_players.c:6012-6022`).
        let base = Vec3::from_array(blade.base);
        let tip = base + Vec3::from_array(blade.direction) * (blade.length + 1.0);
        let trace = gpu.bsp.trace_box_with(
            &mut gpu.trace_scratch,
            base.to_array(),
            tip.to_array(),
            Aabb::POINT,
            MASK_SOLID,
        );
        let mut stop = Vec3::from_array(trace.end_position);
        if trace.fraction < 1.0 {
            stop += (stop - base).normalize_or_zero() * 0.1;
        }
        let first = struck(
            (&gpu.bsp, &mut gpu.trace_scratch),
            snapshot,
            &compensation.bodies,
            owner as u16,
            (base, stop),
            time as i32,
        );
        if first != hit.victim {
            continue;
        }
        let Some((point, normal)) = crate::actor_marks::trace_model(
            &gpu.actor_meshes,
            world,
            &mut gpu.effect_aux.marks,
            hit.victim,
            (base.to_array(), stop.to_array()),
            time,
        ) else {
            continue;
        };
        let point = Vec3::from_array(point);
        if gpu.effect_aux.marks.limit > 0 {
            gpu.effect_aux.marks.request(ghoul2_marks::saber_mark(
                hit.victim,
                point.to_array(),
                (stop - point).to_array(),
            ));
        }
        let normal = Vec3::from_array(normal);
        let normal = if normal == Vec3::ZERO {
            Vec3::Y
        } else {
            normal
        };
        if compensation.spawns.len() < compensation.spawns.capacity() {
            compensation.spawns.push((hit.effect(), point, normal));
        }
    }
    let mut spawns = std::mem::take(&mut gpu.effect_aux.saber_contacts.compensation.spawns);
    if let Some(vfs) = gpu.vfs.as_deref() {
        for (index, (name, point, normal)) in spawns.drain(..).enumerate() {
            effect_runtime::spawn_effect(
                &mut gpu.particles,
                &mut gpu.effect_aux,
                &mut gpu.effects,
                vfs,
                name,
                point,
                now,
                (time as u32) ^ (index as u32).wrapping_mul(0x9e37_79b9),
                0,
                audio,
                crate::combat_effects::rotation_from_direction(normal.to_array()),
            );
        }
    }
    spawns.clear();
    gpu.effect_aux.saber_contacts.compensation.spawns = spawns;
}

/// `CG_Trace(&trace, from, NULL, NULL, to, owner, MASK_PLAYERSOLID)`'s `entityNum`: the
/// world first, then every solid of the snapshot in its order and the viewer's own player
/// last (`CG_BuildSolidList`), bodies where they were drawn this frame; a nearer hit, or a
/// solid never left, takes the trace, and a solid the trace starts inside claims it
/// without one (`CG_ClipMoveToEntities`, `cg_predict.c:232-366`).
///
/// Not here: the exemptions for objects the viewer owns (`genericenemyindex`) and for a
/// rider's own vehicle, and client-side permanent entities.
fn struck(
    (bsp, scratch): (&Bsp, &mut TraceScratch),
    snapshot: Option<&Snapshot>,
    bodies: &[Body],
    owner: u16,
    (from, to): (Vec3, Vec3),
    time: i32,
) -> u16 {
    let (start, end) = (from.to_array(), to.to_array());
    let world = bsp.trace_box_with(scratch, start, end, Aabb::POINT, MASK_PLAYERSOLID);
    let mut struck = Struck {
        fraction: world.fraction,
        all_solid: world.all_solid,
        entity: if world.fraction < 1.0 {
            ENTITY_NUMBER_WORLD
        } else {
            ENTITY_NUMBER_NONE
        },
    };
    let Some(snapshot) = snapshot else {
        return struck.entity;
    };
    let body = |number: u16| {
        bodies
            .binary_search_by_key(&number, |body| body.number)
            .ok()
            .map(|index| bodies[index])
    };
    let solids = snapshot
        .entities
        .iter()
        .map(|state| (state.number(), Some(state)));
    let local = snapshot.player.client_num();
    for (number, state) in solids.chain(std::iter::once((local, None))) {
        if struck.all_solid {
            break;
        }
        if number == owner {
            continue;
        }
        let trace = match (body(number), state) {
            (Some(body), _) => box_trace(start, end, body.origin, body.solid)
                .map(|trace| (trace.fraction, trace.start_solid, trace.all_solid)),
            (None, Some(state)) => crate::local_prediction::movers::Collider::from_entity(
                state,
                snapshot.server_time,
                time,
                false,
            )
            .map(|collider| {
                let trace = crate::movement_collision::target_trace(bsp, &collider, start, end);
                (trace.fraction, trace.start_solid, trace.all_solid)
            }),
            (None, None) => None,
        };
        if let Some((fraction, start_solid, all_solid)) = trace {
            struck.clip(number, fraction, start_solid, all_solid);
        }
    }
    struck.entity
}

/// What a trace has struck so far: `trace_t`'s `fraction`, `allsolid` and `entityNum`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Struck {
    fraction: f32,
    all_solid: bool,
    entity: u16,
}

impl Struck {
    /// One more solid's trace, as `CG_ClipMoveToEntities` folds it in.
    fn clip(&mut self, entity: u16, fraction: f32, start_solid: bool, all_solid: bool) {
        if all_solid || fraction < self.fraction {
            *self = Self {
                fraction,
                all_solid,
                entity,
            };
        } else if start_solid {
            self.entity = entity;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearer_solid_takes_the_trace_and_one_started_in_claims_it_without_one() {
        let mut struck = Struck {
            fraction: 1.0,
            all_solid: false,
            entity: ENTITY_NUMBER_NONE,
        };
        // A blade buried in its victim starts inside the victim's box and leaves it.
        struck.clip(4, 1.0, true, false);
        assert_eq!(struck.entity, 4);
        // A body met further on takes nothing; a nearer one takes the trace.
        struck.clip(6, 0.5, false, false);
        assert_eq!((struck.entity, struck.fraction), (6, 0.5));
        struck.clip(7, 0.8, false, false);
        assert_eq!(struck.entity, 6);
        // A solid the trace never leaves takes it whatever came before.
        struck.clip(9, 0.0, true, true);
        assert_eq!((struck.entity, struck.all_solid), (9, true));
    }
}
