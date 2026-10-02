//! Fixed particle-pool records and shader-stage sampling helpers.

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ParticleBlend {
    Alpha,
    Add,
    AlphaAdd,
    Filter,
    TwiceModulate,
    DstColorAdd,
    OneMinusSrcAlpha,
    Unsupported,
}

pub(crate) struct Particle {
    pub(crate) motion: crate::particle_motion::Motion,

    pub(crate) spawned_at: Instant,
    pub(crate) delay: Duration,
    pub(crate) lifetime: Duration,
    pub(crate) size: crate::effect_envelope::Envelope,
    pub(crate) start_length: f32,
    pub(crate) end_length: f32,
    pub(crate) streak: Option<Vec3>,
    pub(crate) normal: Option<Vec3>,
    pub(crate) alpha: crate::effect_envelope::Envelope,
    pub(crate) use_alpha: bool,
    pub(crate) set_shader_time: bool,
    pub(crate) rgb: [crate::effect_envelope::Envelope; 3],
    pub(crate) seed: u32,
    pub(crate) shader: Arc<str>,
    pub(crate) physics: crate::particle_physics::State,
    pub(crate) shape: PrimitiveShape,
}

#[derive(Clone, Copy)]
pub(crate) enum PrimitiveShape {
    Billboard,
    /// A retained-world sprite replaced each frame rather than simulated over its lifetime.
    FrameBillboard,
    Cylinder {
        axis: Vec3,
        size2: crate::effect_envelope::Envelope,
        length: crate::effect_envelope::Envelope,
        trace_end: bool,
        depth_hack: bool,
    },
    Electricity {
        end: Vec3,
        chaos: f32,
        tapered: bool,
        branched: bool,
        grow: bool,
        trace_end: bool,
        depth_hack: bool,
    },
}

impl Particle {
    pub(crate) fn sample_envelopes(&self, elapsed_seconds: f32) -> (f32, f32, [f32; 3]) {
        let elapsed_millis = elapsed_seconds * 1_000.0;
        let lifetime_millis = self.lifetime.as_secs_f32() * 1_000.0;
        (
            self.size
                .sample(elapsed_millis, lifetime_millis, self.seed.wrapping_add(31)),
            self.alpha
                .sample(elapsed_millis, lifetime_millis, self.seed.wrapping_add(32)),
            std::array::from_fn(|axis| {
                self.rgb[axis].sample(
                    elapsed_millis,
                    lifetime_millis,
                    self.seed.wrapping_add(33 + axis as u32),
                )
            }),
        )
    }

    pub(crate) fn shader_seconds(&self, age: f32, global: f32) -> f32 {
        if self.set_shader_time { age } else { global }
    }
}

pub(crate) const MAX_PARTICLES: usize = 2_048;
const MAX_PARTICLE_SHADER_STAGES: usize = 8;

/// Borrowed stage selection; iteration evaluates at most eight original stages.
/// Empty or unknown shaders yield the same single fallback layer.
pub(crate) struct ParticleLayerSamples<'a> {
    atlas: &'a ParticleAtlas,
    animations: &'a [ParticleAtlasAnimation],
    seconds: f32,
}

impl<'a> ParticleLayerSamples<'a> {
    /// Bound the selection to the renderer's existing eight-stage capacity.
    pub(crate) fn new(
        atlas: &'a ParticleAtlas,
        animations: &'a [ParticleAtlasAnimation],
        seconds: f32,
    ) -> Self {
        Self {
            atlas,
            animations: &animations[..animations.len().min(MAX_PARTICLE_SHADER_STAGES)],
            seconds,
        }
    }

    /// Evaluate stages on demand, retaining order and the single-layer fallback.
    pub(crate) fn iter(&self) -> impl Iterator<Item = ParticleLayerSample> + '_ {
        (0..self.animations.len().max(1)).map(|index| {
            self.animations
                .get(index)
                .map(|animation| self.atlas.sample_animation(animation, self.seconds))
                .unwrap_or(ParticleLayerSample {
                    uv_rect: self.atlas.fallback,
                    blend: ParticleBlend::Add,
                    rgb: 1.0,
                    alpha: 1.0,
                    uv_transform: [1.0, 1.0, 0.0, 0.0],
                })
        })
    }
}

pub(crate) fn blend_for_stage(blend: Option<&StageBlend>) -> ParticleBlend {
    match blend {
        Some(StageBlend::Add) => ParticleBlend::Add,
        Some(StageBlend::Filter) => ParticleBlend::Filter,
        Some(StageBlend::Custom {
            source,
            destination,
        }) if source == "gl_src_alpha" && destination == "gl_one" => ParticleBlend::AlphaAdd,
        Some(StageBlend::Custom {
            source,
            destination,
        }) if source == "gl_dst_color" && destination == "gl_src_color" => {
            ParticleBlend::TwiceModulate
        }
        Some(StageBlend::Custom {
            source,
            destination,
        }) if source == "gl_dst_color" && destination == "gl_one" => ParticleBlend::DstColorAdd,
        Some(StageBlend::Custom {
            source,
            destination,
        }) if source == "gl_one" && destination == "gl_one_minus_src_alpha" => {
            ParticleBlend::OneMinusSrcAlpha
        }
        Some(StageBlend::Custom { .. }) => ParticleBlend::Unsupported,
        Some(StageBlend::Replace | StageBlend::Alpha) | None => ParticleBlend::Alpha,
    }
}

pub(crate) fn fade(use_alpha: bool, life_envelope: f32) -> (f32, f32) {
    if use_alpha {
        (1.0, life_envelope)
    } else {
        (life_envelope, 1.0)
    }
}
