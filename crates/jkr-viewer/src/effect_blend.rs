//! GPU blend states shared by instanced and tessellated effect primitives.

use super::*;

pub(crate) const PIPELINE_COUNT: usize = 7;

pub(crate) fn specifications() -> [(&'static str, wgpu::BlendState); PIPELINE_COUNT] {
    [
        ("JKR alpha FX", wgpu::BlendState::ALPHA_BLENDING),
        ("JKR additive FX", wgpu::BlendState::ADDITIVE),
        (
            "JKR alpha-add FX",
            factors(wgpu::BlendFactor::SrcAlpha, wgpu::BlendFactor::One),
        ),
        (
            "JKR filter FX",
            factors(wgpu::BlendFactor::Dst, wgpu::BlendFactor::Zero),
        ),
        (
            "JKR two-times-modulate FX",
            factors(wgpu::BlendFactor::Dst, wgpu::BlendFactor::Src),
        ),
        (
            "JKR destination-color additive FX",
            factors(wgpu::BlendFactor::Dst, wgpu::BlendFactor::One),
        ),
        (
            "JKR inverse-source-alpha destination FX",
            factors(wgpu::BlendFactor::One, wgpu::BlendFactor::OneMinusSrcAlpha),
        ),
    ]
}

pub(crate) fn slot(blend: ParticleBlend) -> usize {
    match blend {
        ParticleBlend::Alpha | ParticleBlend::Unsupported => 0,
        ParticleBlend::Add => 1,
        ParticleBlend::AlphaAdd => 2,
        ParticleBlend::Filter => 3,
        ParticleBlend::TwiceModulate => 4,
        ParticleBlend::DstColorAdd => 5,
        ParticleBlend::OneMinusSrcAlpha => 6,
    }
}

fn factors(source: wgpu::BlendFactor, destination: wgpu::BlendFactor) -> wgpu::BlendState {
    let component = wgpu::BlendComponent {
        src_factor: source,
        dst_factor: destination,
        operation: wgpu::BlendOperation::Add,
    };
    wgpu::BlendState {
        color: component,
        alpha: component,
    }
}
