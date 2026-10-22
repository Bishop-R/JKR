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
    OneMinusSrcColor,
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
    /// Explicit world-space face replaced at the next frame, like frame billboards.
    FrameQuad {
        points: [Vec3; 4],
    },
    /// A falling glass shard (`CPoly` from `CG_DoGlassQuad`).
    Shard(ShardShape),
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

/// A glass shard packed into the size of [`PrimitiveShape::FrameQuad`], so adding it
/// leaves every particle's size unchanged.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ShardShape {
    /// Corner offsets from the motion origin, in 1/8 units.
    corners: [[i16; 3]; 4],
    /// Pane coordinates in 1/32767 steps over `0..2`.
    st: [[u16; 2]; 4],
    /// Pitch and yaw spin in hundredths of a degree per 100 ms of movement.
    spin: [i16; 2],
    /// Milliseconds after spawning before the shard starts to fall and spin.
    moves_after: u16,
}

impl ShardShape {
    /// `corners` are cyclic offsets from the shard's midpoint (`CPoly::PolyInit`);
    /// `spin` is `rotationDelta`'s pitch and yaw.
    pub(crate) fn new(
        corners: [Vec3; 4],
        st: [[f32; 2]; 4],
        spin: [f32; 2],
        moves_after_ms: i32,
    ) -> Self {
        let fixed = |v: f32, scale: f32| (v * scale).round().clamp(-32_768.0, 32_767.0) as i16;
        Self {
            corners: corners.map(|c| c.to_array().map(|v| fixed(v, 8.0))),
            st: st.map(|p| p.map(|v| (v.clamp(0.0, 2.0) * 32_767.0).round() as u16)),
            spin: spin.map(|v| fixed(v, 100.0)),
            moves_after: moves_after_ms.clamp(0, i32::from(u16::MAX)) as u16,
        }
    }

    /// How long the shard stays stuck in the pane.
    pub(crate) fn moves_after(&self) -> std::time::Duration {
        std::time::Duration::from_millis(u64::from(self.moves_after))
    }

    /// World corners in strip order with their pane coordinates. Stock rotates the
    /// offsets a little every frame while the shard moves (`CPoly::Rotate`); this is
    /// the same rotation integrated over `moving_seconds`, independent of frame rate.
    pub(crate) fn corners(&self, origin: Vec3, moving_seconds: f32) -> [(Vec3, [f32; 2]); 4] {
        // `rotDelta * frameTime * 0.01` degrees per frame: ten times `rotDelta` per second.
        let rate = |v: i16| f32::from(v) * 0.01 * 10.0_f32.to_radians();
        let spin = Vec3::new(-rate(self.spin[0]), 0.0, -rate(self.spin[1]));
        let rotation = Quat::from_scaled_axis(spin * moving_seconds);
        let corner = |i: usize| {
            let offset = Vec3::from_array(self.corners[i].map(|v| f32::from(v) / 8.0));
            let st = self.st[i].map(|v| f32::from(v) / 32_767.0);
            (origin + rotation * offset, st)
        };
        [corner(0), corner(1), corner(3), corner(2)]
    }
}

impl PrimitiveShape {
    pub(crate) fn is_frame(self) -> bool {
        matches!(self, Self::FrameBillboard | Self::FrameQuad { .. })
    }
}

impl Particle {
    /// A camera-facing sprite for this frame only (`RT_SPRITE`, or an `FX_AddSprite` living a
    /// millisecond): `size` in stock's units, coloured `rgb`, blended by `alpha`, its image
    /// upright (`effect_submission::billboard_uv_transform`). The pool's frame sprites go
    /// when `pickups::simple::append_frame` starts the next frame.
    pub(crate) fn frame_sprite(
        effects: &mut crate::effect_runtime::EffectLibrary,
        origin: Vec3,
        size: f32,
        shader: &str,
        (rgb, alpha): ([f32; 3], f32),
        now: Instant,
    ) -> Self {
        let constant = |value| {
            crate::effect_envelope::Envelope::from_values(
                value,
                value,
                0.0,
                jkr_effect::CurveFlags::default(),
            )
        };
        Self {
            motion: crate::particle_motion::Motion::new(
                origin,
                Vec3::ZERO,
                Vec3::ZERO,
                0.0,
                0.0,
                now,
            ),
            spawned_at: now,
            delay: Duration::ZERO,
            lifetime: Duration::from_secs(60),
            size: constant(size),
            start_length: 1.0,
            end_length: 1.0,
            streak: None,
            normal: None,
            alpha: constant(alpha),
            use_alpha: true,
            set_shader_time: false,
            rgb: rgb.map(constant),
            seed: 0,
            shader: effects.shader(shader),
            physics: crate::particle_physics::State::new(
                effects.code_primitive_definition(),
                0,
                0,
                0.0,
                jkr_effect::PrimitiveFlags::default(),
            ),
            shape: PrimitiveShape::FrameBillboard,
        }
    }

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
        Some(StageBlend::Custom {
            source,
            destination,
        }) if source == "gl_one" && destination == "gl_one_minus_src_color" => {
            ParticleBlend::OneMinusSrcColor
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
