//! The part of the material runtime that outlives map load: everything
//! needed to compile one more Q3 material later — the stage bind-group
//! layout, the pipeline layout and shader, the samplers, the fallback
//! lightmap, the texture cache and the pipeline key table. World materials
//! and detached materials (the menu's player stage) go through the same
//! two steps: [`compile_material`] resolves stages and decodes images,
//! [`build_passes`] uploads textures and builds one bind group per stage.

use super::*;

#[path = "world_material_compile.rs"]
mod compile;
pub(super) use compile::compile_material;

/// Map-lifetime GPU state shared by every compiled material.
pub(super) struct Forge {
    /// Optional main-view sun program; authored and secondary views retain old lighting.
    pub(super) model_sun: Option<super::model_sun::Runtime>,
    /// Camera layout reused by optional main-view ambient correction pipelines.
    pub(super) camera_layout: wgpu::BindGroupLayout,
    /// Shared immutable spatial light grid for every model material in this map.
    pub(super) model_grid: wgpu::Buffer,
    pub(super) filtering: super::filtering::Policy,
    pub(super) geometry: wgpu::BindGroup,
    pub(super) flare_layout: wgpu::PipelineLayout,
    pub(super) empty_geometry_stage: wgpu::BindGroup,
    pub(super) stage_layout: wgpu::BindGroupLayout,
    pub(super) pipeline_layout: wgpu::PipelineLayout,
    pub(super) shader: wgpu::ShaderModule,
    /// The device, for pipelines compiled on first draw.
    pub(super) device: wgpu::Device,
    /// One identity actor instance: static world draws bind it so the instanced vertex
    /// path serves them unchanged.
    pub(super) identity_instance: wgpu::Buffer,
    pub(super) format: wgpu::TextureFormat,
    pub(super) repeat: wgpu::Sampler,
    pub(super) clamp: wgpu::Sampler,
    pub(super) fallback_lightmap: wgpu::TextureView,
    /// One pipeline per key in each of the runtime's four pipeline lists.
    /// The frame's point-light block, shared by every stage group and the receiver group.
    pub(super) point_lights: wgpu::Buffer,
    /// The queue, for per-view uploads made while a pass is being recorded.
    pub(super) queue: crate::frame_queue::FrameQueue,
    pub(super) pipeline_keys: Vec<PipelineKey>,
    pub(super) texture_cache: std::collections::HashMap<String, wgpu::TextureView>,
    /// Material-map layout, textures, frames and program; only for maps that found any.
    pub(super) material_maps: Option<super::material_maps::gpu::Gpu>,
}

impl Forge {
    pub(super) fn new(
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        camera_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        let stage_layout = create_stage_layout(device);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("JKR Q3 world stage runtime"),
            source: wgpu::ShaderSource::Wgsl(STAGE_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("JKR Q3 world stage pipeline layout"),
            bind_group_layouts: &[
                Some(camera_layout),
                Some(&stage_layout),
                Some(&crate::shared_geometry::quads::layout(device)),
            ],
            immediate_size: 0,
        });
        let fallback_lightmap = create_rgba8_texture(
            device,
            queue,
            "JKR unlit world lightmap",
            1,
            1,
            &[128, 128, 128, 255],
            true,
        );
        let flare_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("JKR depth-tested flare stages"),
            bind_group_layouts: &[
                Some(camera_layout),
                Some(&stage_layout),
                Some(&crate::shared_geometry::quads::layout(device)),
                Some(&super::flares::depth_layout(device)),
            ],
            immediate_size: 0,
        });
        let identity_instance = wgpu::util::DeviceExt::create_buffer_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("JKR identity world instance"),
                contents: bytemuck::bytes_of(&crate::ActorInstance::world_identity()),
                usage: wgpu::BufferUsages::VERTEX,
            },
        );
        Self {
            model_sun: None,
            camera_layout: camera_layout.clone(),
            model_grid: super::model_grid::empty(device),
            filtering: super::filtering::Policy::default(),
            geometry: crate::shared_geometry::quads::empty_binding(device),
            flare_layout,
            empty_geometry_stage: crate::world_stage::geometry_uniform::empty(device),
            stage_layout,
            pipeline_layout,
            identity_instance,
            device: device.clone(),
            queue: queue.clone(),
            point_lights: wgpu::util::DeviceExt::create_buffer_init(
                device,
                &wgpu::util::BufferInitDescriptor {
                    label: Some("JKR fixed world point lights"),
                    contents: bytemuck::bytes_of(&crate::dynamic_lights::empty_gpu_block()),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                },
            ),
            shader,
            format,
            repeat: create_sampler(device, false),
            clamp: create_sampler(device, true),
            fallback_lightmap,
            pipeline_keys: Vec::new(),
            texture_cache: std::collections::HashMap::with_capacity(512),
            material_maps: None,
        }
    }

    /// The program a pipeline of `key` compiles against: the material program for a
    /// material-mapped stage, the current stage program otherwise.
    pub(super) fn program_for(
        &self,
        key: PipelineKey,
    ) -> (&wgpu::PipelineLayout, &wgpu::ShaderModule) {
        match &self.material_maps {
            Some(maps) if key.geometry & super::material_maps::PIPELINE_BIT != 0 => {
                maps.program(self)
            }
            _ => self.program(),
        }
    }

    /// Index of `key` in the pipeline lists, registering it when new.
    pub(super) fn pipeline_index(&mut self, key: PipelineKey) -> (usize, bool) {
        match self.pipeline_keys.iter().position(|known| *known == key) {
            Some(index) => (index, false),
            None => {
                self.pipeline_keys.push(key);
                (self.pipeline_keys.len() - 1, true)
            }
        }
    }

    pub(super) fn texture(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        key: &str,
        pixels: &[Arc<RgbaImage>],
    ) -> Result<wgpu::TextureView, Box<dyn Error>> {
        if let Some(texture) = self.texture_cache.get(key) {
            return Ok(texture.clone());
        }
        let texture = if self.filtering.mipmapped() {
            super::filtering::mips::upload(device, queue, pixels)?
        } else {
            upload_array(device, queue, pixels)?
        };
        self.texture_cache.insert(key.to_owned(), texture.clone());
        Ok(texture)
    }
}

/// Stages of one material resolved and decoded but not yet uploaded, plus
/// the material's sort value and the number of resolved image lookups.
pub(super) struct CompiledMaterial {
    pub(super) sort: f32,
    /// Light supplied by textured emissive stages, independent of the diffuse paint.
    pub(super) emission: [f32; 3],
    pub(super) emission_texture: super::emission::Texture,
    pub(super) stages: Vec<PendingStage>,
    pub(super) resolved: usize,
}

/// Upload a compiled material's textures and stage table and build one
/// bind group per stage. Pipeline indices refer to the forge's key table;
/// the caller creates pipelines for any key this registered as new.
pub(super) fn build_passes(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    forge: &mut Forge,
    dynamic_lights: &wgpu::Buffer,
    stages: Vec<PendingStage>,
) -> Result<Vec<StagePass>, Box<dyn Error>> {
    let entry_size = std::mem::size_of::<GpuStage>();
    let alignment = usize::try_from(device.limits().min_uniform_buffer_offset_alignment)?;
    let stride = entry_size.div_ceil(alignment) * alignment;
    let mut table_bytes = vec![0_u8; stride * MAX_SHADER_STAGES];
    for (index, stage) in stages.iter().enumerate() {
        let start = index * stride;
        table_bytes[start..start + entry_size].copy_from_slice(bytemuck::bytes_of(&stage.gpu));
    }
    let stage_table = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("JKR immutable Q3 material stage table"),
        contents: &table_bytes,
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let mut passes = Vec::with_capacity(stages.len());
    for (stage_index, stage) in stages.into_iter().enumerate() {
        let primary = forge.texture(device, queue, &stage.primary_key, &stage.primary_pixels)?;
        let secondary = match (&stage.secondary_pixels, &stage.secondary_key) {
            (Some(pixels), Some(key)) => forge.texture(device, queue, key, pixels)?,
            _ => primary.clone(),
        };
        let sampler = |clamp: bool| if clamp { &forge.clamp } else { &forge.repeat };
        let entries = [
            wgpu::BindGroupEntry {
                binding: 7,
                resource: forge.model_grid.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&primary),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler(stage.primary_clamp)),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&secondary),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(sampler(stage.secondary_clamp)),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&stage.lightmap),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &stage_table,
                    offset: u64::try_from(stage_index * stride)?,
                    size: NonZeroU64::new(u64::try_from(entry_size)?),
                }),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: dynamic_lights.as_entire_binding(),
            },
        ];
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("JKR Q3 world stage bind group"),
            layout: &forge.stage_layout,
            entries: &entries,
        });
        let material_group = match (&stage.maps, &mut forge.material_maps) {
            (Some(maps), Some(gpu)) => {
                let sampler = if maps.clamp {
                    &forge.clamp
                } else {
                    &forge.repeat
                };
                Some(gpu.bind(device, queue, &entries, maps, sampler)?)
            }
            _ => None,
        };
        let (pipeline, _) = forge.pipeline_index(stage.key);
        let (live_pipeline, _) =
            forge.pipeline_index(super::visible_emission::live_key(stage.key, &stage.gpu));
        passes.push(StagePass {
            table: Some(super::stage_table::Source {
                gpu: stage.gpu,
                primary: primary.clone(),
                secondary: secondary.clone(),
                lightmap: stage.lightmap.clone(),
                clamp: [stage.primary_clamp, stage.secondary_clamp],
            }),
            ao_receiver: stage.allow_ssao && super::ssao::eligible(stage.key, &stage.gpu),
            shadow_caster: super::material_maps::without_maps(stage.key.geometry) == 0
                && stage.gpu.generators[2] == 0.0
                && stage.key.depth_write
                && stage.key.source == wgpu::BlendFactor::One
                && stage.key.destination == wgpu::BlendFactor::Zero,
            // Cut-out paint, or surface sprites (grass): alpha-tested, depth-writing.
            light_cutout: stage.gpu.generators[2] != 0.0
                && stage.key.depth_write
                && (stage.key.geometry & 2 != 0
                    || (stage.key.geometry & 3 == 0
                        && stage.key.source == wgpu::BlendFactor::One
                        && stage.key.destination == wgpu::BlendFactor::Zero)),
            geometry_group: crate::world_stage::geometry_uniform::bind(
                device,
                &stage_table,
                (stage_index * stride) as u64,
            ),
            bind_group,
            material_group,
            pipeline,
            live_pipeline,
        });
    }
    Ok(passes)
}

/// Materials compiled after map load for geometry that is not part of the
/// world's draw lists (the menu's player stage). Indexed by the position in
/// the slice handed to [`Runtime::compile_detached`].
pub(crate) struct DetachedMaterials {
    materials: Vec<DetachedMaterial>,
}

struct DetachedMaterial {
    blended: bool,
    stages: Vec<StagePass>,
}

/// One index range of a detached mesh and the detached material it uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DetachedDraw {
    pub(crate) indices: Range<u32>,
    pub(crate) material: usize,
}

impl Runtime {
    /// Compile `materials` against this runtime's shared stage state. New
    /// pipeline variants are appended to the runtime's pipeline lists, so
    /// existing world materials keep their indices.
    pub(crate) fn compile_detached(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        materials: &[ViewerMaterial],
    ) -> Result<DetachedMaterials, Box<dyn Error>> {
        let mut image_cache = ImageCache::with_capacity(64);
        let mut compiled = Vec::with_capacity(materials.len());
        let known = self.forge.pipeline_keys.len();
        for key in materials {
            let lightmap = self.forge.fallback_lightmap.clone();
            let material = compile_material(
                vfs,
                shaders,
                key,
                &lightmap,
                true,
                Default::default(),
                &mut image_cache,
                false,
            )?;
            let stages = build_passes(
                device,
                queue,
                &mut self.forge,
                &self.dynamic_light_buffer,
                material.stages,
            )?;
            compiled.push(DetachedMaterial {
                blended: material.sort > SORT_OPAQUE,
                stages,
            });
        }
        for key in self.forge.pipeline_keys[known..].to_vec() {
            self.push_pipelines(device, key);
        }
        Ok(DetachedMaterials {
            materials: compiled,
        })
    }

    /// Compile `materials` as entity materials of this runtime (no world
    /// surfaces of their own) and return the source index of the first one:
    /// draws that use them refer to `base + position in materials`. Existing
    /// materials keep their indices.
    pub(crate) fn append_entity_materials(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        materials: &[ViewerMaterial],
    ) -> Result<usize, Box<dyn Error>> {
        let base = self.source_to_runtime.len();
        let mut image_cache = ImageCache::with_capacity(64);
        let known = self.forge.pipeline_keys.len();
        for key in materials {
            let lightmap = self.forge.fallback_lightmap.clone();
            let material = compile_material(
                vfs,
                shaders,
                key,
                &lightmap,
                true,
                Default::default(),
                &mut image_cache,
                false,
            )?;
            let stages = build_passes(
                device,
                queue,
                &mut self.forge,
                &self.dynamic_light_buffer,
                material.stages,
            )?;
            self.source_to_runtime.push(self.materials.len());
            self.source_order.push((
                material.sort,
                stages.first().map_or(0, |stage| stage.pipeline),
            ));
            self.materials.push(Material {
                view_bounded: false,
                flare: false,
                // Late (cgame custom) materials have no world draws to buffer.
                light_buffered: None,
                light_cutout: false,
                fog_pass: shaders
                    .get(&key.shader)
                    .map_or(FogPass::Equal, |d| d.fog_pass()),
                fog_pipeline: self.fog.pipeline_with_geometry(
                    device,
                    self.forge.format,
                    shaders
                        .get(&key.shader)
                        .map_or(FogPass::Equal, |d| d.fog_pass()),
                    shaders
                        .get(&key.shader)
                        .map_or(ShaderCull::Front, |d| d.cull),
                    u8::from(
                        shaders
                            .get(&key.shader)
                            .is_some_and(|d| !d.deforms.is_empty()),
                    ),
                ),
                fog_draws: Vec::new(),
                blended: material.sort > SORT_OPAQUE,
                sort: material.sort,
                stages,
                static_draws: Vec::new(),
                camera_ranges: Default::default(),
                static_draws_by_cluster: Vec::new(),
                mover_draws: Vec::new(),
            });
        }
        for key in self.forge.pipeline_keys[known..].to_vec() {
            self.push_pipelines(device, key);
        }
        // A loaded model's stages join the map's stage table.
        if let Some(table) = &mut self.stage_table {
            table.append(device, &self.forge, &self.materials);
        }
        Ok(base)
    }

    /// Append one empty slot per list for the next key.
    pub(super) fn push_pipeline_slots(&mut self) {
        self.entity_pipelines.push(std::cell::OnceCell::new());
        self.entity_no_depth_pipelines
            .push(std::cell::OnceCell::new());
    }

    /// Create every variant of `key` now (late cgame materials are few).
    pub(super) fn push_pipelines(&mut self, _device: &wgpu::Device, _key: PipelineKey) {
        let index = self.entity_pipelines.len();
        self.push_pipeline_slots();
        self.entity_pipeline(index, true);
        self.entity_pipeline(index, false);
    }

    /// The world pipeline of key `index`: the depth-tested entity pipeline (statics bind
    /// the identity instance), so a key costs one driver compile for both.
    pub(super) fn world_pipeline(&self, index: usize) -> &wgpu::RenderPipeline {
        self.entity_pipeline(index, true)
    }

    /// The entity pipeline of key `index`, depth-tested or not, compiled on first use.
    pub(super) fn entity_pipeline(&self, index: usize, depth: bool) -> &wgpu::RenderPipeline {
        let slot = if depth {
            &self.entity_pipelines[index]
        } else {
            &self.entity_no_depth_pipelines[index]
        };
        slot.get_or_init(|| {
            let (layout, shader) = self.forge.program_for(self.forge.pipeline_keys[index]);
            timed(usize::from(!depth), || {
                create_entity_pipeline(
                    &self.forge.device,
                    layout,
                    shader,
                    self.forge.format,
                    self.forge.pipeline_keys[index],
                    depth,
                )
            })
        })
    }

    /// Draw detached geometry with the entity vertex path (buffer 0 vertices,
    /// buffer 1 one actor instance), opaque or blended materials only.
    pub(crate) fn draw_detached<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        geometry: &'pass wgpu::BindGroup,
        vertices: &'pass wgpu::Buffer,
        indices: &'pass wgpu::Buffer,
        instances: &'pass wgpu::Buffer,
        detached: &'pass DetachedMaterials,
        draws: &'pass [DetachedDraw],
        blended: bool,
    ) {
        if draws.is_empty() {
            return;
        }
        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(2, geometry, &[]);
        if let Some(sun) = &self.forge.model_sun {
            pass.set_bind_group(3, sun.binding(false), &[]);
        }
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_vertex_buffer(1, instances.slice(..));
        pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
        let mut last_pipeline = None;
        for draw in draws {
            let Some(material) = detached.materials.get(draw.material) else {
                continue;
            };
            if material.blended != blended {
                continue;
            }
            for stage in &material.stages {
                if last_pipeline != Some(stage.pipeline) {
                    pass.set_pipeline(self.entity_pipeline(stage.pipeline, true));
                    last_pipeline = Some(stage.pipeline);
                }
                pass.set_bind_group(1, stage.color_group(), &[]);
                pass.draw_indexed(draw.indices.clone(), 0, 0..1);
            }
        }
    }
}

/// Time one pipeline creation into the per-variant profile. The driver reuses a compiled
/// program across pipelines that share both stages, so the no-depth variant is nearly
/// free after the depth-tested one, and world and entity draws share one vertex entry
/// (statics bind the identity instance) so a key never compiles twice.
fn timed(index: usize, make: impl FnOnce() -> wgpu::RenderPipeline) -> wgpu::RenderPipeline {
    let started = Instant::now();
    let pipeline = make();
    COMPILE_MS[index].fetch_add(
        started.elapsed().as_millis() as u64,
        std::sync::atomic::Ordering::Relaxed,
    );
    pipeline
}

/// Cumulative driver time per variant (depth-tested, no-depth), for the load-profile
/// line: which program's compile a map load waits on.
static COMPILE_MS: [std::sync::atomic::AtomicU64; 2] =
    [const { std::sync::atomic::AtomicU64::new(0) }; 2];

/// Cumulative compile milliseconds per variant since start, in list order.
pub(super) fn compile_profile() -> [u64; 2] {
    COMPILE_MS
        .each_ref()
        .map(|v| v.load(std::sync::atomic::Ordering::Relaxed))
}
