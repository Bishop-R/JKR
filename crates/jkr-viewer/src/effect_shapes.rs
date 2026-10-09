//! Spawn-time state for Raven cylinder and electricity effect primitives, and the
//! traced end of line and electricity primitives.
//!
//! The argument mapping mirrors `CFxScheduler::CreateEffect` in
//! `codemp/client/FxScheduler.cpp:1498-1522`. Electricity reuses Raven's
//! historical flag aliases: `useModel`, `useBBox`, and `usePhysics` become
//! taper, branch, and grow in `CElectricity::Draw`.

use super::*;

const TRACE_DISTANCE: f32 = 16_384.0;

pub(crate) fn from_component(
    component: &jkr_effect::Component,
    effect_origin: Vec3,
    primitive_origin: Vec3,
    rotation: Quat,
    seed: u32,
) -> crate::particle_types::PrimitiveShape {
    match component.kind {
        ComponentKind::Cylinder => crate::particle_types::PrimitiveShape::Cylinder {
            axis: rotation * Vec3::X,
            size2: crate::effect_envelope::Envelope::from_curve(
                component.size2,
                seed.wrapping_add(34),
            ),
            length: crate::effect_envelope::Envelope::from_curve(
                component.length,
                seed.wrapping_add(37),
            ),
            trace_end: component.spawn_flags.origin2_from_trace,
            depth_hack: component.flags.depth_hack,
        },
        ComponentKind::Electricity => {
            let offset = Vec3::from_array(component.origin2.sample([
                seeded_unit(seed.wrapping_add(20)),
                seeded_unit(seed.wrapping_add(21)),
                seeded_unit(seed.wrapping_add(22)),
            ]));
            let end = if component.spawn_flags.origin2_from_trace {
                primitive_origin + trace_reach(component, rotation, offset)
            } else if component.flags.relative {
                primitive_origin + offset
            } else if component.spawn_flags.cheap_origin2 {
                effect_origin + offset
            } else {
                effect_origin + rotation * offset
            };
            crate::particle_types::PrimitiveShape::Electricity {
                end,
                chaos: component.chaos.sample(seeded_unit(seed.wrapping_add(40))),
                tapered: component.flags.use_model,
                branched: component.flags.apply_physics,
                grow: component.flags.use_bounding_box,
                trace_end: component.spawn_flags.origin2_from_trace,
                depth_hack: component.flags.depth_hack,
            }
        }
        _ => crate::particle_types::PrimitiveShape::Billboard,
    }
}

/// Where `FX_ORG2_FROM_TRACE` traces to from a Line's or an Electricity's origin
/// (`FxScheduler.cpp:1408-1428`): far along the effect's forward axis, moved by `offset`
/// (its `origin2`) when `org2isOffset` asks.
pub(crate) fn trace_reach(component: &jkr_effect::Component, rotation: Quat, offset: Vec3) -> Vec3 {
    let reach = rotation * Vec3::X * TRACE_DISTANCE;
    if !component.spawn_flags.origin2_is_offset {
        reach
    } else if component.spawn_flags.cheap_origin2 || component.flags.relative {
        reach + offset
    } else {
        reach + rotation * offset
    }
}

fn seeded_unit(mut seed: u32) -> f32 {
    seed ^= seed >> 16;
    seed = seed.wrapping_mul(0x7feb_352d);
    seed ^= seed >> 15;
    seed = seed.wrapping_mul(0x846c_a68b);
    seed ^= seed >> 16;
    (seed as f64 / u32::MAX as f64) as f32
}
