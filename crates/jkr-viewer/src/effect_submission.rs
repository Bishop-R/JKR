//! Builds the fixed effect draw lists shared by billboard and geometry FX.

use super::*;
use crate::particle_types::PrimitiveShape;

pub(crate) struct Inputs<'a> {
    pub(crate) geometry: &'a mut crate::effect_geometry_gpu::Runtime,
    pub(crate) queue: &'a wgpu::Queue,
    pub(crate) encoder: &'a mut wgpu::CommandEncoder,
    pub(crate) particles: &'a mut [Particle],
    pub(crate) decals: &'a mut crate::decal_store::DecalStore,
    pub(crate) atlas: &'a ParticleAtlas,
    pub(crate) now: Instant,
    pub(crate) global_seconds: f32,
    pub(crate) camera: Vec3,
    pub(crate) field_of_view: f32,
    pub(crate) bsp: &'a Bsp,
    pub(crate) trace_scratch: &'a mut TraceScratch,

    pub(crate) entity_instances: &'a mut Vec<EntityInstance>,
    pub(crate) blended: &'a mut [Vec<EntityInstance>; crate::effect_blend::PIPELINE_COUNT],
}

pub(crate) struct Ranges {
    pub(crate) opaque: Range<u32>,
    blended: [Range<u32>; crate::effect_blend::PIPELINE_COUNT],
}

impl Ranges {
    pub(crate) fn blended(&self) -> impl Iterator<Item = Range<u32>> + '_ {
        self.blended.iter().cloned()
    }
}

/// Prepare unchanged geometry, uploads, billboards and stable sort with disjoint host timings.
pub(crate) fn prepare(timing: &mut frame_pacing::budget::Timer, inputs: Inputs<'_>) -> Ranges {
    inputs.geometry.prepare(
        timing,
        inputs.queue,
        inputs.encoder,
        inputs.particles,
        inputs.decals,
        inputs.atlas,
        inputs.now,
        inputs.global_seconds,
        inputs.camera,
        inputs.field_of_view,
        inputs.bsp,
        inputs.trace_scratch,
    );
    timing.mark(frame_pacing::budget::Phase::EffectBillboards);

    let capacity = 1_024_usize.saturating_sub(inputs.entity_instances.len());
    'particles: for particle in inputs.particles.iter() {
        if !matches!(
            particle.shape,
            PrimitiveShape::Billboard | PrimitiveShape::FrameBillboard
        ) {
            continue;
        }
        let age = inputs.now.saturating_duration_since(particle.spawned_at);
        if age < particle.delay {
            continue;
        }
        let seconds = age.saturating_sub(particle.delay).as_secs_f32();
        let progress = (seconds / particle.lifetime.as_secs_f32()).clamp(0.0, 1.0);
        let motion = particle.motion.sample();
        let (size, base_alpha, color) = particle.sample_envelopes(seconds);
        let direction = particle
            .streak
            .map(|streak| streak_direction(particle, streak, progress))
            .unwrap_or(Vec3::ZERO);
        let direction = particle.normal.unwrap_or(direction).to_array();
        let (color_fade, vertex_alpha) =
            effect_runtime::particle_fade(particle.use_alpha, base_alpha);
        let shader_seconds = particle.shader_seconds(seconds, inputs.global_seconds);
        for layer in inputs
            .atlas
            .layers_for(&particle.shader, shader_seconds)
            .iter()
        {
            let emitted = inputs.blended.iter().map(Vec::len).sum::<usize>();
            if emitted >= capacity {
                break 'particles;
            }
            let uv_transform = billboard_uv_transform(particle.shape, layer.uv_transform);
            let instance = EntityInstance {
                position: motion.origin.to_array(),
                kind: if particle.normal.is_some() {
                    5
                } else if particle.streak.is_some() {
                    4
                } else {
                    3
                },
                size,
                alpha: vertex_alpha * layer.alpha,
                uv_rect: layer.uv_rect,
                color: [
                    color[0] * color_fade * layer.rgb,
                    color[1] * color_fade * layer.rgb,
                    color[2] * color_fade * layer.rgb,
                    1.0,
                ],
                direction,
                rotation: motion.rotation_degrees,
                uv_transform,
            };
            inputs.blended[crate::effect_blend::slot(layer.blend)].push(instance);
        }
    }
    timing.mark(frame_pacing::budget::Phase::EffectSort);

    inputs.blended[0].sort_by(|left, right| {
        let left_distance = Vec3::from_array(left.position).distance_squared(inputs.camera);
        let right_distance = Vec3::from_array(right.position).distance_squared(inputs.camera);
        right_distance.total_cmp(&left_distance)
    });
    Ranges {
        opaque: 0..u32::try_from(inputs.entity_instances.len()).unwrap_or(1_024),
        blended: std::array::from_fn(|index| {
            append_instance_group(inputs.entity_instances, &mut inputs.blended[index])
        }),
    }
}

/// The `[scale_u, scale_v, offset_u, offset_v]` texture transform for one billboard layer.
///
/// Frame icons use `RT_SPRITE`'s top-down image coordinates: OpenJK's
/// `RB_AddQuadStamp` puts t=0 at +up. The particle quad in `entity.wgsl` instead
/// has local v=1 there. Reflect its local v before applying the authored tcMod so
/// chat/connection and simple-item icons are upright; FX keep their existing
/// texture convention.
pub(crate) fn billboard_uv_transform(shape: PrimitiveShape, uv_transform: [f32; 4]) -> [f32; 4] {
    let [scale_u, scale_v, offset_u, offset_v] = uv_transform;
    if matches!(shape, PrimitiveShape::FrameBillboard) {
        [scale_u, -scale_v, offset_u, offset_v + scale_v]
    } else {
        uv_transform
    }
}

/// The texture coordinate, before `fract`, that `entity.wgsl` samples at one billboard
/// corner: corner `(x, y)` sits at `right * x + up * y`, its local coordinate is
/// `corner * 0.5 + 0.5`, and the layer transform scales then offsets it.
#[cfg(test)]
pub(crate) fn billboard_corner_texcoord(corner: [f32; 2], uv_transform: [f32; 4]) -> [f32; 2] {
    let [scale_u, scale_v, offset_u, offset_v] = uv_transform;
    [
        (corner[0] * 0.5 + 0.5) * scale_u + offset_u,
        (corner[1] * 0.5 + 0.5) * scale_v + offset_v,
    ]
}

fn streak_direction(particle: &Particle, streak: Vec3, progress: f32) -> Vec3 {
    if streak.length_squared() <= f32::EPSILON {
        return Vec3::ZERO;
    }
    if particle.start_length == 1.0 && particle.end_length == 1.0 {
        return streak;
    }
    streak.normalize()
        * (particle.start_length + (particle.end_length - particle.start_length) * progress)
}

#[cfg(test)]
mod tests {
    use super::{billboard_corner_texcoord, billboard_uv_transform};
    use crate::particle_types::PrimitiveShape;

    /// `RB_AddQuadStampExt` corners: top-left, top-right, bottom-right, bottom-left
    /// with their `(s, t)` texture coordinates.
    const RT_SPRITE_CORNERS: [([f32; 2], [f32; 2]); 4] = [
        ([-1.0, 1.0], [0.0, 0.0]),
        ([1.0, 1.0], [1.0, 0.0]),
        ([1.0, -1.0], [1.0, 1.0]),
        ([-1.0, -1.0], [0.0, 1.0]),
    ];

    #[test]
    fn frame_billboard_applies_tc_mod_after_rt_sprite_coordinates() {
        // tcMod scale 2 3, then a scroll offset: stock scales and offsets the
        // RT_SPRITE coordinates, top of the quad at t=0.
        let layer = [2.0, 3.0, 0.25, 0.5];
        let transform = billboard_uv_transform(PrimitiveShape::FrameBillboard, layer);
        for (corner, [s, t]) in RT_SPRITE_CORNERS {
            assert_eq!(
                billboard_corner_texcoord(corner, transform),
                [s * 2.0 + 0.25, t * 3.0 + 0.5],
                "corner {corner:?}"
            );
        }
    }

    #[test]
    fn ordinary_billboard_keeps_its_transform() {
        let layer = [2.0, 3.0, 0.25, 0.5];
        assert_eq!(
            billboard_uv_transform(PrimitiveShape::Billboard, layer),
            layer
        );
    }
}
