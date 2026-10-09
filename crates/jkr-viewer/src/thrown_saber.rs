//! Flying primary hilt and blade submission, `codemp/cgame/cg_players.c:10050-10305`.
//!
//! The hilt keeps the entity's evaluated position, spin (yaw) and roll; its pitch
//! is the owner's client-side tilt towards 90, which lays the saber flat
//! (`saber_trail::tilt`), and a saber being pulled back faces away from its owner.
//! Blade ROLL is zeroed. The sockets, world-blade transform and glow/core pair are
//! shared with held and menu-stage sabers (`saber_submission.rs`,
//! `menu_stage/sabers.rs`); the menu's own throw animation is separate
//! (`menu_stage/throw.rs`).

use crate::actor_instance::ActorInstance;
use crate::saber::{self, Instance};
use crate::saber_hilts::HiltCatalog;
use crate::saber_rgb::BladeColor;
use crate::saber_trail::tilt;
use glam::{EulerRot, Quat, Vec3};
use jkr_client::{LegacyThrownSaber, LegacyThrownSabers};
use jkr_runtime::{EntityId, SceneEntity, Transform};
use std::time::Instant;

/// `cgs.media.yellowDroppedSaberShader` and `solidWhite` (`cg_main.c:646,825`).
const GLOW: &str = "gfx/effects/yellow_glow";
pub(crate) const SOLID_WHITE: &str = "gfx/effects/solidWhite_cull";

/// The yellow glow's level at `time`: `sin(cg.time * 0.003) * 0.08 + 0.1`, from 0.02 to 0.18.
fn glow_level(time: i64) -> f32 {
    (time as f32 * 0.003).sin() * 0.08 + 0.1
}

/// One glow sprite `size` across at `origin` for this frame (`FX_AddSprite` with
/// `FX_USE_ALPHA`, living a millisecond).
fn glow(sinks: &mut super::Sinks<'_>, origin: Vec3, size: f32, level: f32, now: Instant) {
    if sinks.particles.len() < sinks.particles.capacity() {
        sinks.particles.push(crate::Particle::frame_sprite(
            sinks.effects,
            origin,
            size,
            GLOW,
            ([1.0; 3], level),
            now,
        ));
    }
}

/// `CG_Player`'s glow at the hand that guides a thrown saber (`cg_players.c:10302-10335`).
pub(super) fn hand_glow(sinks: &mut super::Sinks<'_>, hand: Vec3, now: Instant) {
    let level = glow_level(sinks.presentation_time);
    glow(sinks, hand, 8.0, level, now);
}

/// Claim a flying saber before the ordinary rigid-object path can draw it twice.
pub(super) fn submit(
    sinks: &mut super::Sinks<'_>,
    thrown: Option<&LegacyThrownSabers<'_>>,
    entity: &SceneEntity,
    transform: Transform,
    now: Instant,
) -> bool {
    let Some(number) = entity
        .id
        .get()
        .checked_sub(1)
        .and_then(|id| u16::try_from(id).ok())
    else {
        return false;
    };
    let Some(owner) = thrown.and_then(|thrown| thrown.get(number)) else {
        return false;
    };
    let Some(catalog) = sinks.saber_hilts else {
        return false;
    };
    let owner_entity = u64::from(owner.owner) + 1;
    let Some(owner_tilt) = sinks.saber_states.tilt_mut(owner_entity) else {
        return false;
    };
    if !owner.in_flight {
        // Caught: the lingering entity draws nothing and the tilt starts over
        // (`cg_ents.c:871`, `cg_players.c:10448-10449`).
        owner_tilt.reset();
        return true;
    }
    let presented = Quat::from_array(transform.rotation);
    let (_, server_pitch, _) = presented.to_euler(EulerRot::ZYX);
    let pitch = owner_tilt.advance(server_pitch.to_degrees(), sinks.presentation_time);
    let returning_from = owner
        .returning
        .then(|| {
            sinks
                .world
                .entity(EntityId::new(owner_entity))
                .map(|owner| {
                    (
                        Vec3::from_array(transform.translation),
                        Vec3::from_array(owner.sample(sinks.presentation_time).translation),
                    )
                })
        })
        .flatten();
    let (hilt_rotation, blade_rotation) = tilt::flight_rotations(presented, pitch, returning_from);
    let pose = Transform {
        rotation: hilt_rotation.to_array(),
        ..transform
    };
    if let Some((hilt, blades)) = submit_hilt(
        owner,
        pose,
        blade_rotation,
        catalog,
        sinks.object_groups,
        sinks.saber_instances,
    ) {
        if owner.returning {
            saber_glow(sinks, pose, hilt.mesh_index, now);
        }
        crate::saber_submission::lights::append(
            sinks.lights,
            &blades,
            BladeColor::from_rgb(owner.color),
            hilt.num_blades,
            hilt.no_dlight,
            sinks.presentation_time,
            entity.id.get(),
        );
    }
    true
}

/// A saber being pulled back glows yellow (`CG_General` with `bolt3` 999, `cg_ents.c:1640-1677`),
/// as the Jedi Master's does on the ground: its model `mesh` again at `pose` under
/// `solidWhite_cull` tinted by the level (`RF_RGB_TINT`), and fourteen glow sprites along its
/// up axis, from four units above its origin to nine below.
pub(super) fn saber_glow(sinks: &mut super::Sinks<'_>, pose: Transform, mesh: usize, now: Instant) {
    let level = glow_level(sinks.presentation_time);
    if let Some(material) = sinks.material_overrides.force(SOLID_WHITE)
        && sinks.overrides.len() < sinks.overrides.capacity()
    {
        sinks.overrides.push(crate::entity_materials::OverrideInstance {
            mesh: crate::entity_materials::OverrideMesh::Object(mesh),
            material: Some(material),
            instance: ActorInstance::new(pose.translation, pose.rotation, pose.scale)
                .with_rgb_tint([level, level, 0.0, 1.0]),
            no_depth: false,
            forced_alpha: false,
            switched_off: None,
        });
    }
    let origin = Vec3::from_array(pose.translation);
    let up = Quat::from_array(pose.rotation) * Vec3::Z;
    for step in -4..10 {
        glow(sinks, origin - up * step as f32, 5.5, level, now);
    }
}

/// Emit one hilt at `transform` and each lit primary blade at `rotation`.
pub(super) fn submit_hilt(
    owner: LegacyThrownSaber<'_>,
    transform: Transform,
    rotation: Quat,
    catalog: &HiltCatalog,
    objects: &mut [Vec<ActorInstance>],
    blades: &mut Vec<Instance>,
) -> Option<(crate::saber_hilts::Hilt, [Option<saber::Blade>; 8])> {
    // CG_Player owns manual rendering only while saberInFlight (10050-10055);
    // CG_General ignores the lingering modelGhoul2=127 entity (cg_ents.c:871).
    if !owner.in_flight {
        return None;
    }
    let hilt = catalog.get(owner.name);
    objects[hilt.mesh_index].push(ActorInstance::new(
        transform.translation,
        transform.rotation,
        transform.scale,
    ));
    let origin = Vec3::from_array(transform.translation);
    let color = BladeColor::from_rgb(owner.color);
    let mut light_blades = [None; 8];
    for index in 0..usize::from(hilt.num_blades) {
        if index > 0 && !owner.extra_blades {
            break;
        }
        let Some(blade) = hilt.blade(index) else {
            continue;
        };
        let blade = saber::world_blade(origin, rotation, blade.socket, blade.length, blade.radius);
        blades.extend(Instance::pair(blade, color).map(|i| {
            i.with_contact(
                u64::from(owner.owner) + 1,
                2,
                index,
                !hilt.no_wall_marks,
                hilt.no_dlight,
            )
        }));
        light_blades[index] = Some(blade);
    }
    Some((hilt, light_blades))
}
