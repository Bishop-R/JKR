//! Spawn-time state for Raven cylinder and electricity effect primitives.
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
                let mut trace_end = primitive_origin + rotation * Vec3::X * TRACE_DISTANCE;
                if component.spawn_flags.origin2_is_offset {
                    trace_end += if component.spawn_flags.cheap_origin2 || component.flags.relative
                    {
                        offset
                    } else {
                        rotation * offset
                    };
                }
                trace_end
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

fn seeded_unit(mut seed: u32) -> f32 {
    seed ^= seed >> 16;
    seed = seed.wrapping_mul(0x7feb_352d);
    seed ^= seed >> 15;
    seed = seed.wrapping_mul(0x846c_a68b);
    seed ^= seed >> 16;
    (seed as f64 / u32::MAX as f64) as f32
}
