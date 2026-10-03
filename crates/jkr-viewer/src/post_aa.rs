//! Shared scene resolve and optional final display ramp. Neutral controls bypass their work.
use jkr_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry};
use wgpu::util::DeviceExt;

#[path = "post_color.rs"]
/// Cached scene-grade and display-ramp policy.
pub(crate) mod color;

#[path = "post_bloom.rs"]
mod bloom;

#[path = "effect_layer.rs"]
/// rd-vanilla's blended-effect framebuffer, merged by this resolve.
pub(crate) mod effects;

#[path = "post_hdr.rs"]
/// Scene precision and a colour-ratio-preserving display shoulder.
pub(crate) mod hdr;

/// Register an honestly named post-process control, on by default.
///
/// Measured at 0.026 ms per frame at 2560x1080 on an RX 9060 XT (interleaved A/B on a fixed
/// scene: 0.529/0.531 off against 0.558/0.554 on), so the visual gain is worth the cost by
/// default. `jkr_fxaa 0` restores the untouched base image exactly.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), CvarError> {
    hdr::register(cvars)?;
    cvars.register(CvarDefinition::new(
        "jkr_fxaa",
        1_i64,
        CvarFlags::ARCHIVE,
        "Scene FXAA (not MSAA); restart viewer to apply",
    ))?;
    cvars.on_change("jkr_fxaa", |_| {
        crate::log::progress(format_args!(
            "jkr_fxaa changed: restart viewer to apply scene AA"
        ));
    })
}

/// Resolve startup policy; retained by the process context across console-free map installs.
pub(crate) fn enabled(console: Option<&crate::console::ViewerConsole>) -> bool {
    console
        .and_then(|c| c.integer_cvar("jkr_fxaa"))
        .unwrap_or(1)
        != 0
}

/// A single-sample scene intermediate and an edge-directed resolve into the final output.
pub(crate) struct Runtime {
    /// Full scene destination, before the final AA resolve and HUD.
    pub(crate) scene: wgpu::TextureView,
    bind: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    parameters: wgpu::Buffer,
    fxaa: bool,
    hdr: hdr::Settings,
    /// Applied controls, allowing steady frames to skip updates.
    pub(super) policy: color::Policy,
    display: Option<Box<Runtime>>,
    bloom: Option<bloom::Bloom>,
    /// rd-vanilla's blended-effect framebuffer, merged by this resolve; production only.
    effects: Option<effects::Layer>,
    /// Retained inputs to rebuild [`Self::bind`] when the effect layer is resized.
    inputs: Inputs,
}

/// Views and sampler bound by the resolve, kept to rebind a resized effect layer.
struct Inputs {
    sample: wgpu::TextureView,
    sampler: wgpu::Sampler,

    effect_encoding: effects::Encoding,

    /// The resolve pipeline was built to merge an effect layer.
    merge: bool,
}

impl Runtime {
    /// Construct from process policy, including a console-free world installation.
    pub(crate) fn for_context(
        context: &crate::gpu_context::Context,
        size: [u32; 2],
    ) -> Option<Self> {
        Self::configured(
            &context.device,
            context.format,
            size,
            context.fxaa,
            context.post_color.policy(),
            context.hdr,
            Some(size),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn configured(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        size: [u32; 2],
        fxaa: bool,
        policy: color::Policy,
        hdr: hdr::Settings,
        effects: Option<[u32; 2]>,
    ) -> Option<Self> {
        if effects.is_none()
            && hdr.mode == 0
            && !fxaa
            && !policy.tonemap
            && !policy.bloom
            && policy.gamma == 1.0
        {
            return None;
        }
        let mut runtime = Self::pass_hdr(device, format, size, fxaa, policy, hdr, effects);
        if runtime.scene_effects() && policy.gamma != 1.0 {
            runtime.display = Some(Box::new(Self::pass(
                device,
                format,
                size,
                false,
                color::Policy {
                    gamma: policy.gamma,
                    ..Default::default()
                },
            )));
        }
        Some(runtime)
    }

    fn pass(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        size: [u32; 2],
        fxaa: bool,
        policy: color::Policy,
    ) -> Self {
        Self::pass_hdr(
            device,
            format,
            size,
            fxaa,
            policy,
            hdr::Settings::default(),
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn pass_hdr(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        size: [u32; 2],
        fxaa: bool,
        policy: color::Policy,
        hdr: hdr::Settings,
        effects: Option<[u32; 2]>,
    ) -> Self {
        let scene_format = hdr.format(format);
        let sample_format = scene_format.remove_srgb_suffix();
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("JKR FXAA scene"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: scene_format,
            usage,
            view_formats: &[sample_format],
        });
        let scene = texture.create_view(&Default::default());
        let bloom = policy
            .bloom
            .then(|| bloom::Bloom::new(device, &scene, size));
        let sample = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(sample_format),
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("JKR FXAA"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(include_str!("post_aa.wgsl"), include_str!("post_hdr.wgsl")).into(),
            ),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("JKR scene FXAA before HUD"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &[
                        ("SRGB_OUTPUT", if format.is_srgb() { 1.0 } else { 0.0 }),
                        ("HDR_INPUT", if hdr.mode != 0 { 1.0 } else { 0.0 }),
                        ("HDR_EXPOSURE", f64::from(hdr.exposure)),
                        ("EFFECTS", f64::from(u8::from(effects.is_some()))),
                    ],
                    ..Default::default()
                },
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("JKR FXAA clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let mut values = parameters(fxaa, policy);
        if hdr.mode != 0 {
            values[2] = 1.0;
        }
        let parameters = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("JKR color controls"),
            contents: bytemuck::cast_slice(&values),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let inputs = Inputs {
            sample,
            sampler,

            effect_encoding: effects::Encoding {
                scene: scene_format,
                exposure: hdr.exposure,
            },

            merge: effects.is_some(),
        };
        let layer = effects.map(|scene| effects::Layer::new(device, scene, inputs.effect_encoding));
        let bind = Self::bind(
            device,
            &pipeline,
            &parameters,
            bloom.as_ref(),
            &inputs,
            layer.as_ref(),
        );
        Self {
            scene,
            bind,
            pipeline,
            parameters,
            fxaa,
            hdr,
            policy,
            display: None,
            bloom,
            effects: layer,
            inputs,
        }
    }

    fn bind(
        device: &wgpu::Device,
        pipeline: &wgpu::RenderPipeline,
        parameters: &wgpu::Buffer,
        bloom: Option<&bloom::Bloom>,
        inputs: &Inputs,
        effects: Option<&effects::Layer>,
    ) -> wgpu::BindGroup {
        let view = |binding, view| wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::TextureView(view),
        };
        // Without a layer the merge is compiled out; the scene stands in for its images.
        let (blended, original) = effects.map_or((&inputs.sample, &inputs.sample), |layer| {
            (layer.blended(), layer.original())
        });
        let entries = [
            view(0, &inputs.sample),
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&inputs.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: parameters.as_entire_binding(),
            },
            view(3, bloom.map_or(&inputs.sample, |b| &b.output)),
            view(5, blended),
            view(6, original),
            wgpu::BindGroupEntry {
                binding: 7,
                resource: effects
                    .map_or(parameters, |layer| layer.region())
                    .as_entire_binding(),
            },
        ];
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("JKR FXAA scene sample"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        })
    }

    /// (Re)size the legacy effect layer to the scene targets; startup and resize only.
    /// Only a resolve built with an effect layer (`effects` in [`Self::configured`])
    /// merges one.
    pub(crate) fn fit_effects(&mut self, device: &wgpu::Device, scene: [u32; 2]) {
        if !self.inputs.merge
            || self
                .effects
                .as_ref()
                .is_some_and(|layer| layer.size() == scene)
        {
            return;
        }
        let layer = effects::Layer::new(device, scene, self.inputs.effect_encoding);
        self.bind = Self::bind(
            device,
            &self.pipeline,
            &self.parameters,
            self.bloom.as_ref(),
            &self.inputs,
            Some(&layer),
        );
        self.effects = Some(layer);
    }

    /// The legacy effect layer shared by every view this frame, when the resolve merges one.
    pub(crate) const fn effect_layer(&self) -> Option<&effects::Layer> {
        self.effects.as_ref()
    }

    /// Intermediate receiving UI; gamma corrects it without filtering any glyphs.
    pub(crate) fn hud_target<'a>(&'a self, output: &'a wgpu::TextureView) -> &'a wgpu::TextureView {
        if let Some(display) = &self.display {
            &display.scene
        } else if !self.scene_effects() {
            &self.scene
        } else {
            output
        }
    }

    /// Resolve only scene effects here; display gamma is deliberately later.
    pub(crate) fn draw_scene(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
    ) {
        if self.scene_effects() {
            self.draw(encoder, self.hud_target(output));
        }
    }

    /// Apply stock display gamma after all overlays, with no resampling.
    pub(crate) fn draw_display(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
    ) {
        if let Some(display) = &self.display {
            display.draw(encoder, output);
        } else if !self.scene_effects() {
            self.draw(encoder, output);
        }
    }

    fn scene_effects(&self) -> bool {
        self.hdr.mode != 0
            || self.fxaa
            || self.policy.tonemap
            || self.policy.bloom
            || self.effects.is_some()
    }

    /// Slider changes upload 16 bytes, without rebuilding pipelines or textures.
    pub(super) fn set_gamma(&mut self, queue: &crate::frame_queue::FrameQueue, gamma: f32) {
        self.policy.gamma = gamma;
        if let Some(display) = &mut self.display {
            display.set_gamma(queue, gamma);
        } else {
            let mut values = parameters(self.fxaa, self.policy);
            if self.hdr.mode != 0 {
                values[2] = 1.0;
            }
            queue.write_buffer(&self.parameters, 0, bytemuck::cast_slice(&values));
        }
    }

    /// Resolve scene color after flares/portals, without sampling or changing depth.
    pub(crate) fn draw(&self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView) {
        if let Some(bloom) = &self.bloom {
            bloom.draw(encoder);
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("JKR FXAA resolve"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn parameters(fxaa: bool, policy: color::Policy) -> [f32; 4] {
    [
        f32::from(u8::from(fxaa)),
        f32::from(u8::from(policy.tonemap)),
        if fxaa || policy.tonemap || policy.bloom {
            1.0
        } else {
            policy.gamma
        },
        if policy.bloom { 0.35 } else { 0.0 },
    ]
}
