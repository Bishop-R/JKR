//! Preserve full-precision depth-equal receiver attributes, then shade each pixel once.
//! The attribute pass retains draw order, including coplanar receivers and entities.
use super::*;

/// Geometry needs only the cascades and sun parameters, not lamp/probe resources.
pub(in crate::world_materials) fn sun_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let depth = |binding| {
        texture_entry(
            binding,
            wgpu::ShaderStages::FRAGMENT,
            wgpu::TextureSampleType::Depth,
        )
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("JKR receiver sun visibility"),
        entries: &[
            depth(0),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            depth(3),
            depth(4),
            depth(5),
            depth(6),
            super::bounds::layout_entry(),
        ],
    })
}

fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("JKR receiver attributes"),
        entries: &[
            texture_entry(
                0,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::TextureSampleType::Float { filterable: false },
            ),
            texture_entry(
                1,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::TextureSampleType::Float { filterable: false },
            ),
        ],
    })
}

/// Two map-sized scratch targets; original f32 attributes and sun visibility share channels.
pub(super) struct Targets {
    world: wgpu::TextureView,
    normal: wgpu::TextureView,
    group: wgpu::BindGroup,
    /// Lamp cache coordinates of each receiver; absent when the device cannot attach
    /// a third RGBA32F target.
    pub(in crate::world_materials) cache: Option<wgpu::TextureView>,
}
impl Targets {
    pub(super) fn new(device: &wgpu::Device, size: [u32; 2], _color: &wgpu::TextureView) -> Self {
        let world = target(
            device,
            size,
            "JKR world attributes",
            wgpu::TextureFormat::Rgba32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let normal = target(
            device,
            size,
            "JKR normal attributes",
            wgpu::TextureFormat::Rgba32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout(device),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&world),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&normal),
                },
            ],
        });
        let cache = (device.limits().max_color_attachment_bytes_per_sample
            >= crate::world_materials::lamp_cache::ATTACHMENT_BYTES)
            .then(|| {
                target(
                    device,
                    size,
                    "JKR lamp cache attributes",
                    wgpu::TextureFormat::Rgba32Float,
                    wgpu::TextureUsages::RENDER_ATTACHMENT,
                )
            });
        Self {
            world,
            normal,
            group,
            cache,
        }
    }
}

/// Fixed receiver and fullscreen programs, independent of target size.
pub(super) struct Pipelines {
    attributes: [wgpu::RenderPipeline; 2],

    entity: wgpu::RenderPipeline,
    light: wgpu::RenderPipeline,
    /// Lamp cache variants, compiled on the first frame of a map that has a cache.
    cached: std::cell::OnceCell<Cached>,
    sources: Sources,
}
struct Cached {
    attributes: [wgpu::RenderPipeline; 2],

    entity: wgpu::RenderPipeline,
    light: wgpu::RenderPipeline,
}
/// What the cached variants are compiled from, kept for their first use.
struct Sources {
    world: wgpu::ShaderModule,
    entity: wgpu::ShaderModule,
    world_layout: wgpu::PipelineLayout,
    entity_layout: wgpu::PipelineLayout,
    light_layout: [wgpu::BindGroupLayout; 3],
}
impl Pipelines {
    pub(super) fn new(
        device: &wgpu::Device,
        forge: &Forge,
        receiver: &wgpu::BindGroupLayout,
        world: &wgpu::ShaderModule,
        entity: &wgpu::ShaderModule,
    ) -> Self {
        let sun = sun_layout(device);
        let world_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&forge.camera_layout), Some(&sun)],
            immediate_size: 0,
        });
        let entity_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[
                Some(&forge.camera_layout),
                Some(&forge.stage_layout),
                Some(&crate::shared_geometry::quads::layout(device)),
                Some(&sun),
            ],
            immediate_size: 0,
        });
        let attributes = [false, true].map(|mover| {
            attribute_pipeline(
                device,
                &world_layout,
                world,
                if mover {
                    "mover_vertex"
                } else {
                    "static_vertex"
                },
                mover,
                false,
                false,
            )
        });

        let entity_pipeline = attribute_pipeline(
            device,
            &entity_layout,
            entity,
            "entity_light_vertex",
            true,
            true,
            false,
        );
        let light_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[
                Some(&forge.camera_layout),
                Some(receiver),
                Some(&layout(device)),
            ],
            immediate_size: 0,
        });
        let light = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("JKR once-per-pixel lighting"),
            layout: Some(&light_layout),
            vertex: wgpu::VertexState {
                module: world,
                entry_point: Some("receiver_vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: world,
                entry_point: Some("receiver_light"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
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
        Self {
            attributes,

            entity: entity_pipeline,
            light,
            cached: std::cell::OnceCell::new(),
            sources: Sources {
                world: world.clone(),
                entity: entity.clone(),
                world_layout,
                entity_layout,
                light_layout: [
                    forge.camera_layout.clone(),
                    receiver.clone(),
                    layout(device),
                ],
            },
        }
    }

    fn cached(&self, device: &wgpu::Device) -> &Cached {
        self.cached.get_or_init(|| {
            let from = &self.sources;
            let attributes = [false, true].map(|mover| {
                attribute_pipeline(
                    device,
                    &from.world_layout,
                    &from.world,
                    if mover {
                        "mover_vertex"
                    } else {
                        "static_vertex_cached"
                    },
                    mover,
                    false,
                    true,
                )
            });

            let entity = attribute_pipeline(
                device,
                &from.entity_layout,
                &from.entity,
                "entity_light_vertex",
                true,
                true,
                true,
            );
            let cache = crate::world_materials::lamp_cache::Cache::layout(device);
            let light_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[
                    Some(&from.light_layout[0]),
                    Some(&from.light_layout[1]),
                    Some(&from.light_layout[2]),
                    Some(&cache),
                ],
                immediate_size: 0,
            });
            let light = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("JKR once-per-pixel lighting, cached lamps"),
                layout: Some(&light_layout),
                vertex: wgpu::VertexState {
                    module: &from.world,
                    entry_point: Some("receiver_vertex"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &from.world,
                    entry_point: Some("receiver_light_cached"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: FORMAT,
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
            Cached {
                attributes,

                entity,
                light,
            }
        })
    }
}

/// `cached` adds the lamp cache coordinate target; its static variant reads the page
/// stream at vertex slot 1, where movers and entities read their instances.
fn attribute_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    entry: &str,
    mover: bool,
    deforms: bool,
    cached: bool,
) -> wgpu::RenderPipeline {
    let slot = if mover {
        crate::ActorInstance::layout()
    } else {
        crate::world_materials::lamp_cache::Cache::page_layout()
    };
    let buffers = [Some(crate::GpuVertex::layout()), Some(slot)];
    let targets = [0, 1, 2].map(|_| {
        Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba32Float,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })
    });
    let constants = [("geometry_deforms", 1.0), ("geometry_sprites", 1.0)];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("JKR exact receiver attributes"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: if deforms { &constants } else { &[] },
                ..Default::default()
            },
            buffers: &buffers[..if mover || cached { 2 } else { 1 }],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(if cached {
                "attributes_cached"
            } else {
                "attributes"
            }),
            compilation_options: Default::default(),
            targets: &targets[..if cached { 3 } else { 2 }],
        }),
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: crate::DepthTarget::FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Equal),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

impl super::super::super::Runtime {
    /// Render the exact winning surface before evaluating its light, without repeated shading.
    pub(super) fn draw_receiver_lighting(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        input: &FrameDraw<'_>,
        region: Option<[f32; 4]>,
    ) -> bool {
        let shadow = self.shadows.as_ref().unwrap();
        let buffer = shadow.light.as_ref().unwrap();
        let target = &buffer.receivers;

        let pipelines = &shadow.light_pipelines.as_ref().unwrap().receivers;
        // Lamp light comes from the map's static cache when the map, the device and
        // this light buffer all provide for it.
        let cache = self
            .lamp_cache
            .as_ref()
            .zip(target.cache.as_ref())
            .zip(shadow.cache_group.as_ref());
        if let Some(((cache, _), _)) = cache {
            cache.bake_once(
                &self.forge.device,
                encoder,
                input.vertices,
                input.indices,
                &shadow.lamps,
                self.shadow_bounds,
            );
        }
        let attachment = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })
        };
        let scissor = |pass: &mut wgpu::RenderPass<'_>| {
            if let Some(r) = region {
                let [w, h] = buffer.size;
                let x = ((r[0] * w as f32).floor() as u32).saturating_sub(2);
                let y = ((r[1] * h as f32).floor() as u32).saturating_sub(2);
                let right = ((r[2] * w as f32).ceil() as u32 + 2).min(w);
                let bottom = ((r[3] * h as f32).ceil() as u32 + 2).min(h);
                pass.set_scissor_rect(x, y, right - x, bottom - y);
            }
        };
        if let Some(((cache, coordinates), _)) = cache {
            let cached = pipelines.cached(&self.forge.device);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("JKR receiver pass"),
                color_attachments: &[
                    attachment(&target.world),
                    attachment(&target.normal),
                    attachment(coordinates),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &buffer.depth,
                    depth_ops: None,
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            scissor(&mut pass);

            self.draw_light_geometry(
                &mut pass,
                input,
                Some(&shadow.sun_group),
                Some(&cache.pages),
                |_, mover| &cached.attributes[usize::from(mover)],
                &cached.entity,
            );
        } else {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("JKR receiver pass"),
                color_attachments: &[attachment(&target.world), attachment(&target.normal)],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &buffer.depth,
                    depth_ops: None,
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            scissor(&mut pass);

            self.draw_light_geometry(
                &mut pass,
                input,
                Some(&shadow.sun_group),
                None,
                |_, mover| &pipelines.attributes[usize::from(mover)],
                &pipelines.entity,
            );
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("JKR deferred lighting"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &buffer.color,
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
        scissor(&mut pass);
        pass.set_pipeline(cache.map_or(&pipelines.light, |_| {
            &pipelines.cached(&self.forge.device).light
        }));
        pass.set_bind_group(0, input.camera, &[]);
        pass.set_bind_group(1, &shadow.light_group, &[]);
        pass.set_bind_group(2, &target.group, &[]);
        if let Some((_, group)) = cache {
            pass.set_bind_group(3, group, &[]);
        }
        pass.draw(0..3, 0..1);
        true
    }
}
