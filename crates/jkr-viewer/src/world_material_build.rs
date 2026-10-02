//! Map-load shared world/model material assembly.
use super::*;
#[path = "world_effect_lamps.rs"]
mod effect_lamps;
#[path = "world_flare_lamps.rs"]
mod flare_lamps;

/// Map-load filtering policy; the old entry point remains the unmodified baseline.
#[allow(clippy::too_many_arguments)]
pub(crate) fn create_filtered_runtime(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    camera_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    bsp: &Bsp,
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    materials: &[ViewerMaterial],
    draws: &[DrawBatch],
    geometry: (&[crate::GpuVertex], &[u32]),
    mover_meshes: &[super::super::movers::Mesh],
    filtering: super::filtering::Policy,
    realtime: bool,
    material_maps: super::material_maps::Settings,
) -> Result<(Runtime, usize), Box<dyn Error>> {
    build(
        device,
        queue,
        camera_layout,
        format,
        bsp,
        vfs,
        shaders,
        materials,
        draws,
        geometry,
        mover_meshes,
        true,
        filtering,
        realtime,
        material_maps,
    )
}

#[allow(clippy::too_many_arguments)]
fn build(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    camera_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    bsp: &Bsp,
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    materials: &[ViewerMaterial],
    draws: &[DrawBatch],
    geometry: (&[crate::GpuVertex], &[u32]),
    mover_meshes: &[crate::movers::Mesh],
    collapse: bool,
    filtering: super::filtering::Policy,
    realtime: bool,
    material_maps: super::material_maps::Settings,
) -> Result<(Runtime, usize), Box<dyn Error>> {
    let started = Instant::now();
    let mut forge = Forge::new(device, queue, camera_layout, format);
    forge.model_grid = super::model_grid::upload(device, bsp);
    forge.filtering = filtering;
    // The real-time lighting program is chosen before any pipeline exists, so every world
    // and entity pipeline compiles exactly once; enabling the sun later only swaps a group.
    if realtime {
        forge.model_sun = Some(super::model_sun::Runtime::new(device, &forge, None));
    }
    if filtering != super::filtering::Policy::default() {
        forge.repeat = device.create_sampler(&filtering.descriptor(false));
        forge.clamp = device.create_sampler(&filtering.descriptor(true));
    }
    let table = crate::fog_volumes::Table::build(bsp, shaders)?;
    let mut fog = FogGpu::new(device, camera_layout, table);
    let sky = crate::sky_stage::Runtime::new(
        device,
        queue,
        camera_layout,
        format,
        vfs,
        shaders,
        materials,
        draws,
    )?;
    let sky_done = Instant::now();
    let lightmaps = upload_lightmaps(device, queue, bsp)?;
    let lightmaps_done = Instant::now();
    let mut pending = Vec::new();
    let mut resolved = 0;
    // Resolving and decoding every material's images is the bulk of a map load and pure
    // CPU work: do it across all cores first, then assemble the GPU side in order.
    let mut compiled_materials = compile_all(
        vfs,
        shaders,
        materials,
        &lightmaps,
        &forge.fallback_lightmap,
        collapse,
        material_maps,
    )?;
    let compile_done = Instant::now();
    // Material maps: the extended layout and the world's vertex frames, only when some
    // stage of this map found maps.
    let mapped = compiled_materials
        .iter()
        .flatten()
        .flat_map(|material| &material.stages)
        .filter(|stage| stage.maps.is_some())
        .count();
    if mapped > 0 {
        let frames_started = Instant::now();
        let mut gpu = super::material_maps::gpu::Gpu::new(device, queue);
        let grid = crate::entity_lighting::EntityLighting::from_world(bsp).layout();
        let tangents = super::material_maps::frames::tangents(geometry.0, geometry.1);
        let frames = super::material_maps::frames::pack(geometry.0, &tangents, |point| {
            grid.map(|grid| grid.sample(bsp.render(), point, |_| [255.0; 3]).direction)
        });
        drop(tangents);
        gpu.set_frames(device, &frames);
        forge.material_maps = Some(gpu);
        crate::log::progress(format_args!(
            "material maps: {mapped} stages; frames for {} vertices in {:.1} ms",
            geometry.0.len(),
            frames_started.elapsed().as_secs_f64() * 1e3
        ));
    }
    let (mut fog_ms, mut surface_ms, mut draws_ms) = (0f64, 0f64, 0f64);
    // Every entry is referenced either by a world batch, an actor/model mesh,
    // or a cgame custom-shader override. Compiling one shared table prevents
    // the old model path from silently interpreting stages differently.
    for material_index in 0..materials.len() {
        let key = &materials[material_index];
        let sprite = crate::scene_flatten::sprites::source(&key.shader);
        let definition = shaders.get(sprite.map_or(key.shader.as_str(), |(name, _)| name));
        let is_sky = definition.is_some_and(|definition| definition.sky.is_some());
        let no_color_pass = definition.is_some_and(|definition| !definition.has_color_pass());
        let undrawn = is_sky || no_color_pass;
        let fog_pass = if key.shader.starts_with(crate::scene_flatten::flares::PREFIX) {
            FogPass::None
        } else if sprite.is_some() {
            definition
                .filter(|d| d.fog_pass() != FogPass::None)
                .map_or(FogPass::None, |_| FogPass::Equal)
        } else {
            definition.map_or(FogPass::Equal, |definition| definition.fog_pass())
        };
        let cull = if sprite.is_some() {
            ShaderCull::TwoSided
        } else {
            definition.map_or(ShaderCull::Front, |definition| definition.cull)
        };
        let fog_started = Instant::now();
        let fog_pipeline = fog.pipeline_with_geometry(
            device,
            format,
            fog_pass,
            cull,
            u8::from(definition.is_some_and(|d| !d.deforms.is_empty()))
                | if sprite.is_some() { 2 } else { 0 },
        );
        fog_ms += fog_started.elapsed().as_secs_f64() * 1e3;
        let draws_started = Instant::now();
        let mut fog_draws =
            fog_draws::collect(bsp, draws, mover_meshes, material_index, fog_pipeline);
        if sprite.is_some() || definition.is_some_and(|d| !d.deforms.is_empty()) {
            for draw in &mut fog_draws {
                draw.geometry_source = Some(material_index);
            }
        }
        let mut static_draws: Vec<StaticDraw> = draws
            .iter()
            .filter(|draw| !undrawn && draw.world_surface && draw.material == material_index)
            .map(|draw| StaticDraw {
                view_cache: Default::default(),
                indices: draw.indices.clone(),
                clusters: draw.clusters.clone(),
                bounds: draw.bounds.map(glam::Vec3::from_array),
            })
            .collect();
        coalesce_static_draws(&mut static_draws);
        draws_ms += draws_started.elapsed().as_secs_f64() * 1e3;
        let mover_draws = mover_meshes
            .iter()
            .enumerate()
            .flat_map(|(mesh, model)| {
                model
                    .draws
                    .iter()
                    .filter(move |draw| !undrawn && draw.material == material_index)
                    .map(move |draw| MoverDraw {
                        mesh,
                        indices: draw.indices.clone(),
                    })
            })
            .collect();
        let mut compiled = compiled_materials[material_index]
            .take()
            .expect("every material compiled once");
        resolved += compiled.resolved;
        let surface_started = Instant::now();
        let surface = gi_surface(&compiled, shaders.get(&key.shader));
        surface_ms += surface_started.elapsed().as_secs_f64() * 1e3;
        // Gloss for the sun highlight: environment-mapped materials (glass, hulls, chrome)
        // are glossy, everything else keeps a faint sheen.
        let gloss = if shaders.get(&key.shader).is_some_and(|d| {
            d.stages
                .iter()
                .any(|s| s.texture_generator == jkr_shader::TextureGenerator::Environment)
        }) {
            0.7
        } else {
            0.12
        };
        for stage in &mut compiled.stages {
            // Preserve per-bundle visible emission; gloss belongs to the receiver.
            stage.gpu.emission[3] = gloss;
        }
        pending.push(PendingMaterial {
            view_bounded: sprite.is_none() && definition.is_none_or(|d| d.deforms.is_empty()),
            flare: key.shader.starts_with(crate::scene_flatten::flares::PREFIX),
            fog_pass,
            fog_pipeline,
            fog_draws,
            source_index: material_index,
            surface,
            emission_texture: compiled.emission_texture,
            sort: compiled.sort,
            stages: compiled.stages,
            static_draws,
            mover_draws,
        });
    }
    pending.sort_by(|left, right| {
        left.sort
            .total_cmp(&right.sort)
            .then(left.source_index.cmp(&right.source_index))
    });
    let images_done = Instant::now();
    // Lamps: every emissive material's world triangles (ranges of the flattened scene
    // geometry) become area lights.
    let positions: Vec<([f32; 3], [f32; 3], [f32; 2])> = geometry
        .0
        .iter()
        .map(|v| (v.position, v.normal, v.texture_coordinates))
        .collect();
    let emitters: Vec<crate::lamp_lights::Emitter> = pending
        .iter()
        .filter(|material| material.surface.emission.iter().any(|c| *c > 0.))
        .map(|material| crate::lamp_lights::Emitter {
            omnidirectional: material.sort != SORT_OPAQUE,
            radiance: material.surface.emission,
            texture: &material.emission_texture,
            ranges: material
                .static_draws
                .iter()
                .map(|draw| draw.indices.clone())
                .collect(),
        })
        .collect();
    let mut extra = effect_lamps::extract(bsp, vfs, shaders)?;
    extra.extend(flare_lamps::extract(&pending, &positions, geometry.1));
    let lamps =
        crate::lamp_lights::LampSet::extract(&positions, geometry.1, &emitters).append(extra);
    let brightest = lamps
        .lamps
        .iter()
        .max_by(|a, b| a.power.total_cmp(&b.power));
    crate::log::progress(format_args!(
        "Lamps: {} area lights from {} emissive materials; \
        brightest {:?}; grid {:?} cells of {} units at {:?}",
        lamps.lamps.len(),
        emitters.len(),
        brightest,
        lamps.counts,
        lamps.cell,
        lamps.origin
    ));
    let mut result = finish_runtime(device, queue, forge, sky, fog, pending, resolved)?;
    result.0.lamps = lamps;
    // The static lamp cache covers lightmapped, light-buffered surfaces of the static world.
    if !result.0.lamps.lamps.is_empty() {
        let surfaces: Vec<super::lamp_cache::Surface> = draws
            .iter()
            .filter(|draw| {
                draw.world_surface
                    && result
                        .0
                        .material(draw.material)
                        .is_some_and(|material| material.light_buffered.is_some())
            })
            .map(|draw| super::lamp_cache::Surface {
                indices: draw.indices.clone(),
                page: materials[draw.material].lightmap,
            })
            .collect();
        let vertex_positions: Vec<[f32; 3]> = geometry.0.iter().map(|v| v.position).collect();
        let coordinates: Vec<[f32; 2]> =
            geometry.0.iter().map(|v| v.lightmap_coordinates).collect();
        result.0.lamp_cache_pages = super::lamp_cache::Pages::plan(
            &vertex_positions,
            &coordinates,
            geometry.1,
            &surfaces,
            device.limits().max_texture_array_layers,
        );
    }
    result.0.environment_policy = super::lighting_environment::Policy::from_bsp(bsp);
    result.0.shadow_bounds = bsp.render().vertices().iter().fold(
        [
            glam::Vec3::splat(f32::INFINITY),
            glam::Vec3::splat(f32::NEG_INFINITY),
        ],
        |[lo, hi], vertex| {
            let p = glam::Vec3::from_array(vertex.position);
            [lo.min(p), hi.max(p)]
        },
    );
    result.0.build_shadow_hulls(device, bsp, draws);
    result.0.probe_domain = super::gi_probe_domain::Domain::build(bsp, result.0.shadow_bounds);
    result.0.areas = areas::Areas::new(bsp);
    result.0.prepare_fog_visibility(bsp.render().visibility());
    result.0.prepare_camera_ranges();
    result.0.indirect = Some(super::indirect_draws::Lists::new(
        device,
        result
            .0
            .materials
            .iter()
            .map(|material| material.static_draws.len())
            .sum(),
    ));
    result.0.stage_table =
        super::stage_table::Table::build(device, &result.0.forge, &result.0.materials);

    let finished = Instant::now();
    crate::log::progress(format_args!(
        concat!(
            "material profile: sky={:.1}ms lightmaps={:.1}ms ",
            "resolve+decode={:.1}ms (parallel compile {:.1}ms, fog pipelines {:.1}ms, ",
            "draw lists {:.1}ms, bounce colours {:.1}ms) upload+pipelines={:.1}ms"
        ),
        sky_done.duration_since(started).as_secs_f64() * 1_000.0,
        lightmaps_done.duration_since(sky_done).as_secs_f64() * 1_000.0,
        images_done.duration_since(lightmaps_done).as_secs_f64() * 1_000.0,
        compile_done.duration_since(lightmaps_done).as_secs_f64() * 1_000.0,
        fog_ms,
        draws_ms,
        surface_ms,
        finished.duration_since(images_done).as_secs_f64() * 1_000.0,
    ));
    Ok(result)
}

/// Compile every material on a pool of threads (each with its own image cache; the
/// process-wide decoded-image cache deduplicates across them), in input order.
fn compile_all(
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    materials: &[ViewerMaterial],
    lightmaps: &std::collections::HashMap<i32, wgpu::TextureView>,
    fallback: &wgpu::TextureView,
    collapse: bool,
    material_maps: super::material_maps::Settings,
) -> Result<Vec<Option<super::forge::CompiledMaterial>>, Box<dyn Error>> {
    let workers = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .clamp(1, 16);
    let chunk = materials.len().div_ceil(workers).max(1);
    let results: Vec<Result<super::forge::CompiledMaterial, String>> =
        std::thread::scope(|scope| {
            let handles: Vec<_> = materials
                .chunks(chunk)
                .map(|keys| {
                    scope.spawn(move || {
                        let mut cache = ImageCache::with_capacity(64);
                        keys.iter()
                            .map(|key| {
                                let lightmap = lightmaps.get(&key.lightmap).unwrap_or(fallback);
                                compile_material(
                                    vfs,
                                    shaders,
                                    key,
                                    lightmap,
                                    collapse,
                                    material_maps,
                                    &mut cache,
                                )
                                .map_err(|error| error.to_string())
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|handle| handle.join().expect("material thread"))
                .collect()
        });
    results
        .into_iter()
        .map(|result| result.map(Some).map_err(|e| e.into()))
        .collect()
}

/// Bounce colour and light emission. Explicit compiler intensity takes precedence;
/// otherwise the resolved emissive stages supply the light missing from legacy BSPs.
pub(super) fn gi_surface(
    compiled: &super::forge::CompiledMaterial,
    definition: Option<&jkr_shader::ShaderDefinition>,
) -> crate::gi_voxels::Surface {
    let albedo = compiled
        .stages
        .iter()
        .find_map(|stage| stage.primary_pixels.first())
        .map_or([0.5; 3], |image| crate::gi_voxels::mean_linear_color(image));
    let emission = compiled.emission;
    crate::gi_voxels::Surface {
        albedo,
        emission,
        sky: definition.is_some_and(|d| d.sky.is_some()),
    }
}
