//! Actor-to-hilt, blade, and motion-trail submission.

use crate::actor_instance::ActorInstance;
use crate::saber::{self, Instance};
use crate::saber_hilts::HiltCatalog;
use crate::saber_rgb::BladeColor;
use crate::saber_trail::{Edges, SegmentPool, StateSlab};
use glam::{Quat, Vec3};
use jkr_runtime::{HeldEquipment, HeldItemKind};

#[path = "saber_lights.rs"]
pub(crate) mod lights;

/// Submit both hilts and every numbered blade for one actor. `trails` is
/// `None` with `cg_saberTrail 0`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn submit(
    entity_id: u64,
    hands: [Option<(&str, saber::Attachment)>; 2],
    equipment: HeldEquipment,
    actor_origin: Vec3,
    actor_rotation: Quat,
    catalog: Option<&HiltCatalog>,
    states: &mut StateSlab,
    segments: &mut SegmentPool,
    object_groups: &mut [Vec<ActorInstance>],
    saber_instances: &mut Vec<Instance>,
    presentation_time: i64,
    mut trails: Option<&mut Edges<'_>>,
    lights: &mut crate::dynamic_lights::PointLightList,
) -> bool {
    if equipment.kind != HeldItemKind::EnergyBlade {
        return false;
    }
    let Some(catalog) = catalog else {
        return true;
    };
    if !equipment.primary_in_flight
        && let Some(tilt) = states.tilt_mut(entity_id)
    {
        // The primary is in the hand: the next throw tilts from its own pitch
        // (`cg_players.c:10429-10449`).
        tilt.reset();
    }
    for (saber_index, hand) in hands.into_iter().enumerate() {
        if saber_index == 0 && equipment.primary_in_flight {
            // The flying hilt carries this saber's blades and trail state
            // (`thrown_saber.rs`), as `CG_AddSaberBlade` shares
            // `client->saber[0].blade[n].trail` between hand and flight.
            continue;
        }
        let Some((name, attachment)) = hand else {
            continue;
        };
        let hilt = catalog.get(name);
        let (grip, weapon_rotation) =
            saber::world_attachment(actor_origin, actor_rotation, attachment);
        object_groups[hilt.mesh_index].push(ActorInstance::new(
            grip.to_array(),
            weapon_rotation.to_array(),
            [1.0; 3],
        ));
        let color = BladeColor::from_rgb(if saber_index == 0 {
            equipment.color
        } else {
            equipment.secondary_color
        });
        let mut light_blades = [None; 8];
        for blade_index in 0..usize::from(hilt.num_blades) {
            let Some(hilt_blade) = hilt.blade(blade_index) else {
                continue;
            };
            let active = blade_is_active(&equipment, saber_index, blade_index);
            let Some(state) = states.blade_mut(entity_id, saber_index, blade_index) else {
                continue;
            };
            let length = state
                .extension
                .update(active, hilt_blade.length, presentation_time);
            if length < 0.5 {
                continue;
            }
            let blade = saber::world_blade(
                grip,
                weapon_rotation,
                hilt_blade.socket,
                length,
                hilt_blade.radius,
            );
            let flicker = saber::Flicker::sample(
                entity_id.wrapping_mul(24) + (saber_index * 8 + blade_index) as u64,
                presentation_time,
            );
            saber_instances.extend(
                Instance::pair_flickering(blade, color, hilt_blade.length, flicker).map(|i| {
                    i.with_contact(
                        entity_id,
                        saber_index,
                        blade_index,
                        !hilt.no_wall_marks,
                        hilt.no_dlight,
                    )
                }),
            );
            light_blades[blade_index] = Some(blade);
            if let Some(edges) = trails.as_deref_mut()
                && let Some(quad) = state.trail.update(
                    edges.edge(blade),
                    presentation_time,
                    equipment.trail_duration_millis,
                    hilt_blade.trail_style,
                    color,
                    equipment.trail_duration_millis > 0,
                )
            {
                segments.insert(quad, presentation_time);
            }
        }
        self::lights::append(
            lights,
            &light_blades,
            color,
            hilt.num_blades,
            hilt.no_dlight,
            presentation_time,
            entity_id.wrapping_add(saber_index as u64),
        );
    }
    true
}

/// Partial holster keeps primary blade 1 but retracts staff extras and saber 2.
pub(crate) fn blade_is_active(
    equipment: &HeldEquipment,
    saber_index: usize,
    blade_index: usize,
) -> bool {
    if saber_index == 0 && blade_index == 0 {
        equipment.active
    } else {
        equipment.secondary_active
    }
}
