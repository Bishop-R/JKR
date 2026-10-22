//! Fixed-capacity per-frame drawing of stock shields and beam entities.
use super::*;
use crate::particle_types::PrimitiveShape;

pub(super) fn submit(sinks: &mut Sinks<'_>, snapshot: &Snapshot, now: Instant) {
    for state in &snapshot.entities {
        if state.e_flags() & 256 != 0 {
            continue;
        }
        if state.entity_type() == 4 {
            let origin = sinks
                .world
                .entity(jkr_runtime::EntityId::new(u64::from(state.number()) + 1))
                .map(|entity| entity.sample(sinks.presentation_time).translation)
                .unwrap_or_else(|| {
                    jkr_client::legacy_evaluate_trajectory(
                        state.trajectory_base(),
                        state.trajectory_delta(),
                        state.trajectory_type(),
                        state.trajectory_time(),
                        state.trajectory_duration(),
                        sinks.presentation_time as i32,
                    )
                });
            if let Some(face) = jkr_client::special_entities::shield_face(state, origin) {
                quad(sinks, face.points, face.shader, [1.0; 3], now);
            }
        } else if let Some(faces) = jkr_client::special_entities::beam_faces(state) {
            for face in faces {
                quad(sinks, face, "@jkr/solid-add", [1.0, 0.0, 0.0], now);
            }
        }
    }
}

fn quad(sinks: &mut Sinks<'_>, points: [[f32; 3]; 4], shader: &str, rgb: [f32; 3], now: Instant) {
    if sinks.particles.len() == sinks.particles.capacity() {
        return;
    }
    let constant = |v| {
        crate::effect_envelope::Envelope::from_values(v, v, 0.0, jkr_effect::CurveFlags::default())
    };
    sinks.particles.push(Particle {
        motion: crate::particle_motion::Motion::new(
            Vec3::from_array(points[0]),
            Vec3::ZERO,
            Vec3::ZERO,
            0.0,
            0.0,
            now,
        ),
        spawned_at: now,
        delay: Duration::ZERO,
        lifetime: Duration::from_secs(60),
        size: constant(1.0),
        start_length: 1.0,
        end_length: 1.0,
        streak: None,
        normal: None,
        alpha: constant(1.0),
        use_alpha: true,
        set_shader_time: false,
        rgb: rgb.map(constant),
        seed: 0,
        shader: sinks.effects.shader(shader),
        physics: crate::particle_physics::State::new(
            sinks.effects.code_primitive_definition(),
            0,
            0,
            0.0,
            jkr_effect::PrimitiveFlags::default(),
        ),
        shape: PrimitiveShape::FrameQuad {
            points: points.map(Vec3::from_array),
        },
    });
}
