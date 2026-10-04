//! BSP-backed collision adapter used by client movement prediction.

use crate::local_prediction::movers::Collider;
use glam::Vec3;
use jkr_bsp::{Aabb, Bsp};
use jkr_client::pmove::{ENTITY_NUMBER_WORLD, MovementCollision, MovementTrace};
use std::cell::RefCell;

#[path = "mover_collision.rs"]
mod inline;

/// Reuse transformed inline and packed-box collision for a crosshair entity trace.
pub(crate) fn target_trace(
    bsp: &Bsp,
    collider: &Collider,
    start: [f32; 3],
    end: [f32; 3],
) -> MovementTrace {
    inline::trace(
        bsp,
        collider,
        start,
        end,
        Aabb::new([0.0; 3], [0.0; 3]).unwrap(),
        1 | 0x100,
    )
}

pub(super) struct BspMovementCollision<'a> {
    bsp: &'a Bsp,
    scratch: RefCell<&'a mut jkr_bsp::TraceScratch>,
    movers: &'a [Collider],
}

impl<'a> BspMovementCollision<'a> {
    pub(super) fn with_movers(
        bsp: &'a Bsp,
        scratch: &'a mut jkr_bsp::TraceScratch,
        movers: &'a [Collider],
    ) -> Self {
        Self {
            bsp,
            scratch: RefCell::new(scratch),
            movers,
        }
    }
}

impl MovementCollision for BspMovementCollision<'_> {
    fn point_contents(&self, point: [f32; 3]) -> u32 {
        // Water-jump ledge probes need SOLID/PLAYERCLIP/BODY as well as liquids.
        let mut contents = self.bsp.point_contents(point, u32::MAX);
        for mover in self.movers {
            contents |= inline::contents(self.bsp, mover, point);
        }
        contents
    }
    fn trace(
        &self,
        start: [f32; 3],
        minimums: [f32; 3],
        maximums: [f32; 3],
        end: [f32; 3],
        content_mask: u32,
    ) -> MovementTrace {
        let bounds = Aabb::new(minimums, maximums).expect("pmove produces valid bounds");
        let trace = self.bsp.trace_box_with(
            &mut self.scratch.borrow_mut(),
            start,
            end,
            bounds,
            content_mask,
        );
        let mut result = MovementTrace {
            fraction: trace.fraction,
            end_position: trace.end_position,
            plane_normal: trace.plane.map_or([0.0; 3], |plane| plane.normal),
            surface_flags: trace.surface_flags,
            start_solid: trace.start_solid,
            all_solid: trace.all_solid,
            // CG_Trace, codemp/cgame/cg_predict.c:377-383: an escaping
            // startsolid trace with fraction 1 does not identify a world hit.
            entity_number: if trace.fraction < 1.0 {
                ENTITY_NUMBER_WORLD
            } else {
                jkr_protocol::ENTITY_NUMBER_NONE
            },
        };
        for mover in self.movers {
            let trace = inline::trace(self.bsp, mover, start, end, bounds, content_mask);
            // CG_ClipMoveToEntities also identifies an overlapping entity
            // when another surface supplied the nearer trace fraction.
            if trace.all_solid || trace.fraction < result.fraction {
                result = trace;
            } else if trace.start_solid {
                result.start_solid = true;
                result.entity_number = mover.entity;
            }
            if result.all_solid {
                break;
            }
        }
        result
    }
}

/// CG_Trace camera hull, including presented inline doors/platforms. BODY is
/// deliberately absent: the local rider and vehicle cannot occlude their own view.
pub(crate) fn camera_trace(
    bsp: &Bsp,
    scratch: &mut jkr_bsp::TraceScratch,
    snapshot: Option<&jkr_protocol::Snapshot>,
    time: i32,
    start: Vec3,
    end: Vec3,
) -> Vec3 {
    // MASK_SOLID | CONTENTS_PLAYERCLIP (codemp/game/bg_public.h, surfaceflags.h).
    const MASK: u32 = 0x1001 | 0x10;
    let bounds = Aabb::new([-4.0; 3], [4.0; 3]).expect("valid camera hull");
    let mut best = bsp.trace_box_with(scratch, start.to_array(), end.to_array(), bounds, MASK);
    if let Some(snapshot) = snapshot {
        for entity in &snapshot.entities {
            if entity.solid() != 0x00ff_ffff || entity.number() == snapshot.player.client_num() {
                continue;
            }
            let Some(collider) = Collider::from_entity(entity, time, time, false) else {
                continue;
            };
            let hit = inline::trace(
                bsp,
                &collider,
                start.to_array(),
                end.to_array(),
                bounds,
                MASK,
            );
            if hit.all_solid || hit.fraction < best.fraction {
                best.fraction = hit.fraction;
                best.end_position = hit.end_position;
                best.all_solid = hit.all_solid;
            }
            if best.all_solid {
                break;
            }
        }
    }
    Vec3::from_array(best.end_position)
}

/// A server pusher can leave groundEntityNum stale until the next Pmove.
/// Only reject carry when even the rider's foot footprint misses that mover.
/// This is a presentation guard, not a replacement for PM_GroundTrace.
pub(super) fn mover_supports_feet(bsp: &Bsp, mover: &Collider, origin: [f32; 3]) -> bool {
    let mut end = origin;
    end[2] -= 0.25;
    let feet = Aabb::new([-15.0, -15.0, -24.0], [15.0, 15.0, -24.0])
        .expect("constant player footprint is valid");
    let hit = inline::trace(
        bsp,
        mover,
        origin,
        end,
        feet,
        jkr_client::pmove::PLAYER_CONTENT_MASK,
    );
    hit.fraction != 1.0 || hit.start_solid
}
