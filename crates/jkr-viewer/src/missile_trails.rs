//! Viewer dispatch for allocation-free legacy missile effect requests, and for the
//! lasers of armed trip mines ([`jkr_client::trip_mine_beam`]), which stock plays from
//! the same client frame.

use crate::dynamic_lights::{PointLight, PointLightList};
use crate::effect_runtime::EffectLibrary;
use crate::impact_spawn::Sinks;
use crate::{GameAudio, Particle, combat_effects, effect_runtime};
use glam::Vec3;
use jkr_client::{LegacyMissileEffectMetrics, LegacyMissileEffects};
use jkr_protocol::Snapshot;
use jkr_vfs::VirtualFileSystem;
use std::time::Instant;

/// Cache weapon definitions while preparing the world, never while firing a shot.
pub(crate) fn load(
    game: Option<&jkr_protocol::GameState>,
    vfs: &VirtualFileSystem,
) -> LegacyMissileEffects {
    let Some(game) = game else {
        return LegacyMissileEffects::empty();
    };
    let mut effects = LegacyMissileEffects::from_game_state(game);
    effects.load_vehicles(game, vfs);
    effects
}

/// Evaluate and play one logical projectile-think request per eligible
/// ET_MISSILE for the rendered frame. EFX graph storage, particles, and point
/// lights are all preallocated by their owning runtimes.
#[allow(clippy::too_many_arguments)]
pub(crate) fn update_and_spawn(
    runtime: &mut LegacyMissileEffects,
    snapshot: &Snapshot,
    presentation_time: i32,
    particles: &mut Vec<Particle>,
    auxiliary: &mut crate::effect_aux::Runtime,
    effects: &mut EffectLibrary,
    vfs: &VirtualFileSystem,
    audio: &mut Option<GameAudio>,
    lights: &mut PointLightList,
    now: Instant,
) -> LegacyMissileEffectMetrics {
    let metrics = runtime.update(snapshot, presentation_time);
    // Trail EFX on the reference 8 ms cadence (`effect_cadence.rs`); lights every frame.
    let play = auxiliary.continuous.due(now);
    for request in runtime.requests() {
        if let Some(light) = request.light {
            lights.push(PointLight {
                origin: light.origin,
                radius: light.radius,
                color: light.color,
            });
        }
        if !play {
            continue;
        }
        if let Some(name) = request.extra_effect_name {
            for repetition in 0..request.extra_repetitions {
                spawn(
                    particles,
                    auxiliary,
                    effects,
                    vfs,
                    audio,
                    name,
                    request.origin,
                    request.direction,
                    now,
                    seed(request.entity_number, presentation_time, repetition),
                );
            }
        }
        spawn(
            particles,
            auxiliary,
            effects,
            vfs,
            audio,
            request.effect_name,
            request.origin,
            request.direction,
            now,
            seed(request.entity_number, presentation_time, u8::MAX),
        );
    }
    if play {
        let sinks = Sinks {
            particles,
            auxiliary,
            effects,
            vfs,
            audio,
        };
        spawn_trip_mine_beams(snapshot, presentation_time, sinks, now);
    }
    metrics
}

/// `CG_General`'s trip mine beam (`cg_ents.c:1903-1935`) for every armed mine in
/// `snapshot`. Stock plays it on every client frame; here it follows the trails' 8 ms
/// cadence (`effect_cadence.rs`), so its additive copies overlap as at `com_maxfps 125`.
fn spawn_trip_mine_beams(snapshot: &Snapshot, time: i32, sinks: Sinks<'_>, now: Instant) {
    let beams = snapshot.entities.iter().filter_map(|entity| {
        jkr_client::trip_mine_beam(entity, &snapshot.player, time)
            .map(|beam| (entity.number(), beam))
    });
    for (number, beam) in beams {
        for play in 0..beam.plays {
            spawn(
                sinks.particles,
                sinks.auxiliary,
                sinks.effects,
                sinks.vfs,
                sinks.audio,
                beam.effect_name,
                beam.origin,
                beam.direction,
                now,
                seed(number, time, play),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn(
    particles: &mut Vec<Particle>,
    auxiliary: &mut crate::effect_aux::Runtime,
    effects: &mut EffectLibrary,
    vfs: &VirtualFileSystem,
    audio: &mut Option<GameAudio>,
    name: &str,
    origin: [f32; 3],
    direction: [f32; 3],
    now: Instant,
    seed: u32,
) {
    effect_runtime::spawn_effect(
        particles,
        auxiliary,
        effects,
        vfs,
        name,
        Vec3::from_array(origin),
        now,
        seed,
        0,
        audio,
        combat_effects::rotation_from_direction(direction),
    );
}

fn seed(entity: u16, presentation_time: i32, component: u8) -> u32 {
    u32::from(entity)
        ^ (presentation_time as u32).rotate_left(11)
        ^ u32::from(component).wrapping_mul(0x9e37_79b9)
}
