//! The ring trail, two fading beams and wall burst of a concussion alternate shot.

use crate::{combat_effects, effect_runtime, impact_spawn, impacts, particle_types::MAX_PARTICLES};
use jkr_protocol::EntityState;
use std::time::Instant;

pub(super) fn spawn(entity: &EntityState, sinks: &mut impact_spawn::Sinks<'_>, now: Instant) {
    let Some(shot) = jkr_client::concussion_shot::Shot::decode(entity) else {
        return;
    };
    let seed = u32::from(entity.number()) | (84 << 16);
    for (index, origin) in shot.rings().take(MAX_PARTICLES).enumerate() {
        if sinks.particles.len() >= MAX_PARTICLES {
            break;
        }
        effect_runtime::spawn_effect(
            sinks.particles,
            sinks.auxiliary,
            sinks.effects,
            sinks.vfs,
            "concussion/alt_ring",
            origin.into(),
            now,
            seed ^ index as u32,
            0,
            sinks.audio,
            combat_effects::rotation_from_direction(shot.ring_direction),
        );
    }
    for (shader, width, color, lifetime) in [
        ("gfx/effects/blueLine", 10.0, [1.0; 3], 175),
        ("gfx/misc/whiteline2", 7.0, [0.75, 0.5, 1.0], 150),
    ] {
        impact_spawn::spawn_line(
            sinks.particles,
            sinks.effects,
            impacts::Line {
                start: shot.start,
                end: shot.beam_end,
                size: [0.1, width],
                alpha: [1.0, 0.0],
                color,
                lifetime_millis: lifetime,
                shader,
            },
            now,
            seed,
        );
    }
    for name in ["concussion/explosion", "disruptor/alt_miss"] {
        effect_runtime::spawn_effect(
            sinks.particles,
            sinks.auxiliary,
            sinks.effects,
            sinks.vfs,
            name,
            shot.impact.into(),
            now,
            seed,
            0,
            sinks.audio,
            combat_effects::rotation_from_direction(shot.normal),
        );
    }
}
