//! Optional floating dust motes around the camera (`jkr_dust`), presentation only.
//!
//! Every mote is generated in `dust_motes.wgsl` from its
//! instance index and the frame time, so the effect owns no vertex or instance
//! buffer: one pipeline and a 16-byte uniform are created per world, and a
//! frame draws `6 × count` vertices. Motes are fixed in world space inside a
//! cube that wraps around the camera, fade out with distance and near the eye,
//! depth-test against the scene without writing depth, and take their colour
//! from the BSP light grid at the camera (sampled once per frame on the CPU).
//!
//! Competitive visibility is protected by construction: motes are one to two
//! world units across, their opacity is capped at [`PEAK_ALPHA`] and no mote is
//! drawn within 16 units of the eye.
//!
//! Per-map tuning is not implemented. It would be a multiplier applied to
//! [`Settings::intensity`] in [`Frame::new`]'s caller, resolved once at map load
//! (for example from a worldspawn key or a map-name table).

use crate::actor_instance::EntityLight;
use bytemuck::{Pod, Zeroable};
use jkr_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry, CvarValue};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

/// Console variable: 0 disables the effect, 1 is full density and opacity.
pub(crate) const CVAR: &str = "jkr_dust";
/// Motes drawn at intensity 1; lower intensities draw a prefix of the same set,
/// so changing the value adds or removes motes without moving the others.
pub(crate) const MAX_MOTES: u32 = 2048;
/// Opacity of a mote's centre at intensity 1, before distance fades.
pub(crate) const PEAK_ALPHA: f32 = 0.35;
/// Opacity kept at the lowest nonzero intensity, so sparse motes stay visible.
const ALPHA_FLOOR: f32 = 0.4;
/// Share of the directed light-grid term added to ambient: motes scatter light
/// from every side, so they are not shaded by a single direction.
const DIRECTED_SHARE: f32 = 0.5;

/// Live intensity shared by the console and every installed world.
#[derive(Clone, Default)]
pub(crate) struct Settings(Arc<AtomicU32>);

impl Settings {
    /// Register the archived cvar (default 0, off) and follow its changes.
    pub(crate) fn bind(cvars: &mut CvarRegistry) -> Result<Self, CvarError> {
        cvars.register(CvarDefinition::new(
            CVAR,
            0.0_f64,
            CvarFlags::ARCHIVE,
            "Floating dust motes around the camera, 0 off to 1; presentation only, applies immediately",
        ))?;
        Self::from_registered(cvars)
    }

    /// Seed from the registered value, including an archived one, then subscribe.
    fn from_registered(cvars: &mut CvarRegistry) -> Result<Self, CvarError> {
        let settings = Self::default();
        if let Some(cvar) = cvars.get(CVAR) {
            settings.set(&cvar.value);
        }
        let changed = settings.clone();
        cvars.on_change(CVAR, move |change| changed.set(&change.current))?;
        Ok(settings)
    }

    fn set(&self, value: &CvarValue) {
        let value = match value {
            CvarValue::Float(value) => *value,
            CvarValue::Integer(value) => *value as f64,
            _ => return,
        };
        if value.is_finite() {
            let value = value.clamp(0.0, 1.0) as f32;
            self.0.store(value.to_bits(), Ordering::Relaxed);
        }
    }

    /// Clamped intensity in 0..=1; one relaxed read per frame.
    pub(crate) fn intensity(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
}

/// Per-frame shader input: rgb = mote colour, w = peak opacity.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
struct DustUniform {
    color: [f32; 4],
}

/// What one frame draws, derived without allocation from the intensity and light.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Frame {
    count: u32,
    uniform: DustUniform,
}

impl Frame {
    /// Scale the mote count with intensity and light the motes from the grid.
    pub(crate) fn new(intensity: f32, light: &EntityLight) -> Self {
        let intensity = if intensity.is_finite() {
            intensity.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let count = (intensity * MAX_MOTES as f32).round() as u32;
        if count == 0 {
            return Self::default();
        }
        let color = std::array::from_fn::<f32, 3, _>(|axis| {
            (light.ambient[axis] + DIRECTED_SHARE * light.directed[axis]).clamp(0.0, 1.0)
        });
        let alpha = PEAK_ALPHA * (ALPHA_FLOOR + (1.0 - ALPHA_FLOOR) * intensity);
        Self {
            count,
            uniform: DustUniform {
                color: [color[0], color[1], color[2], alpha],
            },
        }
    }

    /// Number of mote instances to draw; zero skips the effect.
    pub(crate) fn count(&self) -> u32 {
        self.count
    }
}

/// World-lifetime pipeline and uniform; nothing is created per frame.
pub(crate) struct Runtime {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    frame: Frame,
}

impl Runtime {
    /// Build the pipeline for the scene attachment format and depth target.
    pub(crate) fn new(
        device: &wgpu::Device,
        camera: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        use wgpu::util::DeviceExt;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("JKR dust layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("JKR dust uniform"),
            contents: bytemuck::bytes_of(&DustUniform::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("JKR dust bind group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("JKR dust pipeline layout"),
            bind_group_layouts: &[Some(camera), Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("JKR dust shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("dust_motes.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("JKR dust pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::DepthTarget::FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            buffer,
            bind_group,
            frame: Frame::default(),
        }
    }

    /// Upload the uniform only when it changed; a disabled effect writes nothing.
    fn prepare(&mut self, queue: &wgpu::Queue, frame: Frame) {
        if frame.count() != 0 && frame.uniform != self.frame.uniform {
            queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(&frame.uniform));
        }
        if frame.count() != 0 {
            self.frame = frame;
        } else {
            self.frame.count = 0;
        }
    }

    /// Draw inside the world pass, after blended world and entity surfaces.
    fn draw<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
    ) {
        if self.frame.count() == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.draw(0..6, 0..self.frame.count());
    }
}

impl crate::GpuState {
    /// Light the motes at the final view origin; called with the main camera upload.
    pub(crate) fn prepare_dust_motes(&mut self, view_origin: [f32; 3]) {
        let intensity = self.context.dust_motes.intensity();
        let frame = if intensity > 0.0 {
            let light = self.entity_lighting.sample(&self.bsp, view_origin, &[]);
            Frame::new(intensity, &light)
        } else {
            Frame::default()
        };
        self.dust_motes.prepare(&self.queue, frame);
    }

    /// Submit the motes into the open main-view world pass.
    pub(crate) fn draw_dust_motes<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        self.dust_motes.draw(pass, &self.camera_bind_group);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry(value: Option<CvarValue>) -> (CvarRegistry, Settings) {
        let mut cvars = CvarRegistry::new();
        let settings = Settings::bind(&mut cvars).unwrap();
        if let Some(value) = value {
            cvars.set_value(CVAR, value).unwrap();
        }
        (cvars, settings)
    }

    #[test]
    fn defaults_off_and_clamps_live_changes() {
        let (mut cvars, settings) = registry(None);
        assert_eq!(settings.intensity(), 0.0);
        cvars.set_text(CVAR, "0.5").unwrap();
        assert_eq!(settings.intensity(), 0.5);
        cvars.set_text(CVAR, "7").unwrap();
        assert_eq!(settings.intensity(), 1.0);
        cvars.set_text(CVAR, "-2").unwrap();
        assert_eq!(settings.intensity(), 0.0);
    }

    #[test]
    fn archived_value_seeds_before_subscription() {
        let mut cvars = CvarRegistry::new();
        cvars
            .register(CvarDefinition::new(CVAR, 0.25_f64, CvarFlags::ARCHIVE, ""))
            .unwrap();
        let settings = Settings::from_registered(&mut cvars).unwrap();
        assert_eq!(settings.intensity(), 0.25);
    }

    #[test]
    fn zero_or_invalid_intensity_draws_nothing() {
        let light = EntityLight::FALLBACK;
        assert_eq!(Frame::new(0.0, &light).count(), 0);
        assert_eq!(Frame::new(f32::NAN, &light).count(), 0);
        assert_eq!(Frame::new(1e-5, &light), Frame::default());
    }

    #[test]
    fn intensity_scales_count_and_caps_opacity() {
        let light = EntityLight::FALLBACK;
        let half = Frame::new(0.5, &light);
        let full = Frame::new(1.0, &light);
        let over = Frame::new(3.0, &light);
        assert_eq!(half.count(), MAX_MOTES / 2);
        assert_eq!(full.count(), MAX_MOTES);
        assert_eq!(over, full);
        assert!(half.uniform.color[3] < full.uniform.color[3]);
        assert!(half.uniform.color[3] > 0.0);
        assert_eq!(full.uniform.color[3], PEAK_ALPHA);
    }

    #[test]
    fn colour_follows_the_light_grid_and_stays_in_range() {
        let dark = EntityLight {
            ambient: [0.1, 0.1, 0.12],
            directed: [0.0; 3],
            direction: [0.0, 0.0, 1.0],
        };
        let bright = EntityLight {
            ambient: [1.0; 3],
            directed: [3.0, 0.4, 0.0],
            direction: [0.0, 0.0, 1.0],
        };
        let dark = Frame::new(1.0, &dark).uniform.color;
        let bright = Frame::new(1.0, &bright).uniform.color;
        assert_eq!(&dark[..3], &[0.1, 0.1, 0.12]);
        assert_eq!(&bright[..3], &[1.0, 1.0, 1.0]);
    }

    #[test]
    fn uniform_matches_the_shader_block() {
        assert_eq!(std::mem::size_of::<DustUniform>(), 16);
    }
}
