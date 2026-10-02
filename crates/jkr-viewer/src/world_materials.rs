//! Map-lifetime GPU resources and draw lists for Q3 world shader stages.
//!
//! Static BSP, inline movers, skinned Ghoul2 actors, and rigid MD3/GLM models
//! share this runtime. Stage compilation lives exclusively in `world_stage`;
//! `model_materials` is only a cgame override-name adapter.

#[path = "world_material_draw.rs"]
mod draw;
#[path = "gi_probe_domain.rs"]
mod gi_probe_domain;
#[path = "indirect_draws.rs"]
mod indirect_draws;
#[path = "lamp_cache.rs"]
pub(crate) mod lamp_cache;
#[path = "lamp_geometry.rs"]
pub(crate) mod lamp_geometry;
#[path = "lighting_environment.rs"]
mod lighting_environment;
#[path = "world_lighting_mode.rs"]
pub(crate) mod lighting_mode;

#[path = "sun_shadows.rs"]
pub(crate) mod shadows;
#[path = "stage_table.rs"]
pub(crate) mod stage_table;

#[path = "world_flares.rs"]
pub(crate) mod flares;
#[path = "fog_draws.rs"]
mod fog_draws;
#[path = "fog_gpu.rs"]
mod fog_gpu;
#[path = "world_forced_alpha.rs"]
mod forced_alpha;
#[path = "world_material_forge.rs"]
mod forge;
#[path = "world_material_gpu.rs"]
mod gpu;
#[path = "model_light_grid.rs"]
mod model_grid;
#[path = "world_ssao.rs"]
pub(crate) mod ssao;

use fog_draws::FogDraw;
pub(crate) use fog_draws::FrameDraw;
use fog_gpu::FogGpu;
use jkr_shader::{FogPass, ShaderCull};

use super::{DrawBatch, ViewerMaterial, create_rgba8_texture};
#[path = "world_area_visibility.rs"]
pub(crate) mod areas;
use crate::dynamic_lights::{GpuPointLightBlock, PointLightList};
use crate::world_stage::{
    CompiledStage, GpuStage, MAX_SHADER_STAGES, PipelineKey, SORT_OPAQUE, collapse_multitexture,
    compile_hardware_stage, hardware_pipeline_key, material_stages,
};
pub(crate) use forge::{DetachedDraw, DetachedMaterials};
use forge::{Forge, build_passes, compile_material};
use gpu::{
    create_entity_pipeline, create_pipeline, create_sampler, create_stage_layout, load_stage_images,
};
pub(crate) use gpu::{upload_array, upload_lightmaps};
use image::RgbaImage;
use jkr_bsp::{Bsp, Visibility};
use jkr_shader::ShaderCatalog;
use jkr_vfs::VirtualFileSystem;
use std::error::Error;
use std::num::NonZeroU64;
use std::ops::Range;
use std::sync::Arc;
use std::time::Instant;
use wgpu::util::DeviceExt;

#[path = "gi_runtime.rs"]
pub(crate) mod gi;
#[path = "gi_probes.rs"]
pub(crate) mod gi_probes;
#[path = "model_sun.rs"]
mod model_sun;

macro_rules! stage_shader {
    ($sun:expr, $flares:expr) => {
        concat!(
            include_str!("vertex_transform.wgsl"),
            include_str!("gpu_skinning.wgsl"),
            include_str!("geometry_stage.wgsl"),
            include_str!("stage_runtime.wgsl"),
            include_str!("point_lights.wgsl"),
            include_str!("world_lighting_mode.wgsl"),
            include_str!("light_grid_sample.wgsl"),
            include_str!("model_light_grid.wgsl"),
            include_str!("surface_deform.wgsl"),
            include_str!("surface_tables.wgsl"),
            include_str!("surface_noise.wgsl"),
            include_str!("surface_sprites.wgsl"),
            $sun,
            $flares,
        )
    };
}
const STAGE_SHADER: &str = stage_shader!(
    include_str!("model_sun_off.wgsl"),
    include_str!("surface_flares.wgsl")
);
/// The shared real-time sources (sun visibility, probes, lamps and the light sum) with
/// the receiver group at `group`.
pub(crate) fn realtime_sources(group: u32) -> (String, String, String) {
    let visibility = include_str!("sun_visibility.wgsl").to_string();
    let probes = format!(
        "{}",
        gi_probes::sample_source(group, model_sun::PROBE_BASE)
            .replace(
                &format!("@group({group}) @binding(12) var probe_far_map: texture_depth_2d;"),
                ""
            )
            .replace("probe_far_map", "far_map")
    );
    let realtime = format!(
        "{}{}",
        crate::lamp_lights::source(group, model_sun::LAMP_BASE, true),
        concat!(
            include_str!("ambient_fill.wgsl"),
            include_str!("lamp_response.wgsl"),
            include_str!("sun_realtime.wgsl")
        )
        .replace("@group(3)", &format!("@group({group})"))
    );

    (visibility, probes, realtime)
}

/// The light pass's entity program: the stage program's vertex path (skinning, deforms)
/// with the light pass fragments, the receiver group at group 3 behind the stage and
/// geometry groups. Entities are lit into the buffer like the world and the stage
/// pipelines only upsample it, so no per-key program carries the cascades and probes.
pub(crate) fn entity_light_shader() -> String {
    let (visibility, probes, realtime) = realtime_sources(3);
    stage_shader!(include_str!("sun_visibility.wgsl"), "")
        .to_string()
        .replace(
            include_str!("sun_visibility.wgsl"),
            &format!(
                "{}{}{}{}{}{}{}{}",
                include_str!("sun_shadow_uniform.wgsl"),
                visibility,
                probes,
                realtime,
                include_str!("light_pass.wgsl").replace("@group(1)", "@group(3)"),
                include_str!("light_pass_entity.wgsl"),
                include_str!("model_sun_off.wgsl"),
                include_str!("sun_contact.wgsl")
            ),
        )
}

/// The world program of the real-time mode: the stage shader reading the half-resolution
/// light buffer instead of evaluating the cascades and probes per pixel.
pub(crate) fn world_sun_shader() -> &'static str {
    stage_shader!(
        concat!(
            include_str!("sun_shadow_uniform.wgsl"),
            include_str!("sun_highlights.wgsl"),
            include_str!("sun_realtime_buffer.wgsl")
        ),
        ""
    )
}

#[path = "world_draw_ranges.rs"]
pub(crate) mod draw_ranges;
#[path = "view_culling.rs"]
mod view_culling;

struct StagePass {
    /// What this stage gives the map's stage table; entity-only stages carry none.
    table: Option<stage_table::Source>,
    shadow_caster: bool,
    /// Opaque with an alpha test on its first texture (leaves, grates): the light pass draws
    /// it through the alpha-testing entity pipeline, so its holes stay holes.
    light_cutout: bool,
    ao_receiver: bool,
    bind_group: wgpu::BindGroup,
    geometry_group: wgpu::BindGroup,
    pipeline: usize,
    live_pipeline: usize,
    /// `pipeline` and `live_pipeline` under `RF_FORCE_ENT_ALPHA` ([`forced_alpha`]).
    forced_alpha_pipelines: [usize; 2],
}

#[derive(Clone)]
struct StaticDraw {
    view_cache: view_culling::Cache,
    indices: Range<u32>,
    clusters: Vec<usize>,
    /// World AABB from the flattened scene.
    bounds: [glam::Vec3; 2],
}

#[derive(Clone)]
struct MoverDraw {
    mesh: usize,
    indices: Range<u32>,
}

struct Material {
    view_bounded: bool,
    flare: bool,
    fog_pass: FogPass,
    fog_pipeline: Option<usize>,
    fog_draws: Vec<FogDraw>,
    blended: bool,
    /// Cull index (front, back, none) of an opaque material the half-resolution light pass
    /// draws; `None` surfaces upsample the nearest buffered depth instead.
    light_buffered: Option<u8>,
    /// The light pass draws the static surfaces with the first stage's alpha test. Without
    /// it a cut-out surface borrowed the light of whatever stood behind it and read as
    /// tinted glass: bright before a sunlit wall, dark before a shadow.
    light_cutout: bool,
    sort: f32,
    stages: Vec<StagePass>,
    static_draws: Vec<StaticDraw>,
    camera_ranges: draw_ranges::Cache,
    static_draws_by_cluster: Vec<Vec<StaticDraw>>,
    mover_draws: Vec<MoverDraw>,
}

/// Immutable map-lifetime stage tables, textures, pipelines, and draw lists.
/// The draw methods only traverse prebuilt lists and perform no allocation.
pub(crate) struct Runtime {
    pub(crate) view_culling: view_culling::State,
    shadow_bounds: [glam::Vec3; 2],
    shadow_hulls: Option<shadows::hulls::Hulls>,
    /// Every static sun caster as joined index runs, built on first use per map.
    caster_runs: std::cell::OnceCell<Vec<Range<u32>>>,
    /// BSP-sealed void/solid cells on the fixed probe lattice; load-time only.
    probe_domain: gi_probe_domain::Domain,
    environment_policy: lighting_environment::Policy,
    /// Optional probe-GI foundation, installed only for the real-time lighting mode.
    pub(crate) gi: Option<gi::Runtime>,
    /// Bounce colour and emission per flattened (source) material index.
    surfaces_by_source: Vec<crate::gi_voxels::Surface>,
    /// Local lights from the map's emissive faces (real-time mode).
    lamps: crate::lamp_lights::LampSet,
    /// One bind group for every opaque static world stage, where the device allows.
    stage_table: Option<stage_table::Table>,

    /// Per-view indirect draw lists of the material-free geometry passes.
    indirect: Option<indirect_draws::Lists>,
    /// Materials the current PVS cluster can show; see `draw_ranges::Active`.
    active: std::cell::RefCell<draw_ranges::Active>,
    /// Which world surfaces the static lamp cache serves; `None`: evaluate lamps directly.
    lamp_cache_pages: Option<lamp_cache::Pages>,
    /// The baked cache, created with the first real-time lighting setup of this map.
    lamp_cache: Option<lamp_cache::Cache>,
    shadows: Option<shadows::Runtime>,
    ssao: ssao::AmbientOcclusion,
    /// Accepted snapshot visibility, independent of entity and collision culling.
    pub(crate) areas: areas::Areas,
    flares: flares::Runtime,
    pub(crate) fog_mode: crate::fog_volumes::Mode,
    pub(crate) fogged_entities: usize,
    fog: FogGpu,
    opaque_fog: Vec<FogDraw>,
    opaque_fog_by_cluster: Vec<Vec<FogDraw>>,
    sky: crate::sky_stage::Runtime,
    materials: Vec<Material>,
    source_to_runtime: Vec<usize>,
    /// One depth-tested pipeline per key for world statics (bound to the forge's identity
    /// instance), movers and entities alike, plus the entities' no-depth variant. Slots
    /// fill at load for the keys the scene uses and on first draw otherwise (every
    /// distinct program costs the driver about half a second on a cold cache).
    entity_pipelines: Vec<std::cell::OnceCell<wgpu::RenderPipeline>>,
    entity_no_depth_pipelines: Vec<std::cell::OnceCell<wgpu::RenderPipeline>>,
    opaque_order: Vec<PassRef>,
    blended_order: Vec<PassRef>,
    dynamic_light_buffer: wgpu::Buffer,
    lighting_mode: std::cell::Cell<u32>,
    forge: Forge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PassRef {
    material: usize,
    stage: usize,
}

type ImageCache = std::collections::HashMap<String, Arc<RgbaImage>>;

struct PendingStage {
    allow_ssao: bool,
    gpu: GpuStage,
    primary_pixels: Vec<Arc<RgbaImage>>,
    primary_key: String,
    primary_clamp: bool,
    secondary_pixels: Option<Vec<Arc<RgbaImage>>>,
    secondary_key: Option<String>,
    secondary_clamp: bool,
    lightmap: wgpu::TextureView,
    key: PipelineKey,
}

struct PendingMaterial {
    view_bounded: bool,
    flare: bool,
    fog_pass: FogPass,
    fog_pipeline: Option<usize>,
    fog_draws: Vec<FogDraw>,
    source_index: usize,
    /// Bounce colour and emission for global illumination.
    surface: crate::gi_voxels::Surface,
    emission_texture: emission::Texture,
    sort: f32,
    stages: Vec<PendingStage>,
    static_draws: Vec<StaticDraw>,
    mover_draws: Vec<MoverDraw>,
}

#[path = "world_material_build.rs"]
mod build;
#[path = "world_emission.rs"]
pub(crate) mod emission;
#[path = "world_visible_emission.rs"]
mod visible_emission;
pub(crate) use build::create_filtered_runtime;

#[path = "texture_filtering.rs"]
pub(crate) mod filtering;

fn finish_runtime(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut forge: Forge,
    sky: crate::sky_stage::Runtime,
    fog: FogGpu,
    pending: Vec<PendingMaterial>,
    resolved: usize,
) -> Result<(Runtime, usize), Box<dyn Error>> {
    let dynamic_light_buffer = forge.point_lights.clone();
    let source_count = pending
        .iter()
        .map(|material| material.source_index)
        .max()
        .map_or(0, |maximum| maximum + 1);
    let mut source_to_runtime = vec![usize::MAX; source_count];
    let mut surfaces_by_source = vec![crate::gi_voxels::Surface::default(); source_count];
    let mut runtime_materials = Vec::with_capacity(pending.len());
    let mut opaque_fog = Vec::new();
    for mut material in pending {
        surfaces_by_source[material.source_index] = material.surface;
        // Sort-5 depth-writing blends must draw colour before their EQUAL fog.
        // Keep those fog draws in the material order; batch ordinary opaque fog.
        if material.sort <= SORT_OPAQUE && material.fog_pass == FogPass::Equal {
            opaque_fog.append(&mut material.fog_draws);
        }
        source_to_runtime[material.source_index] = runtime_materials.len();
        let passes = build_passes(
            device,
            queue,
            &mut forge,
            &dynamic_light_buffer,
            material.stages,
        )?;
        runtime_materials.push(Material {
            view_bounded: material.view_bounded,
            flare: material.flare,
            fog_pass: material.fog_pass,
            fog_pipeline: material.fog_pipeline,
            fog_draws: material.fog_draws,
            blended: material.sort > SORT_OPAQUE,
            light_buffered: (material.sort <= SORT_OPAQUE
                && !material.flare
                && passes.first().is_some_and(|stage| stage.shadow_caster))
            .then(|| match forge.pipeline_keys[passes[0].pipeline].cull {
                Some(wgpu::Face::Front) => 0,
                Some(wgpu::Face::Back) => 1,
                None => 2,
            }),
            light_cutout: material.sort <= SORT_OPAQUE
                && !material.flare
                && passes.first().is_some_and(|stage| stage.light_cutout),
            sort: material.sort,
            stages: passes,
            static_draws: material.static_draws,
            camera_ranges: Default::default(),
            static_draws_by_cluster: Vec::new(),
            mover_draws: material.mover_draws,
        });
    }
    let opaque_order = build_draw_order(&runtime_materials, false);
    let blended_order = build_draw_order(&runtime_materials, true);
    let keys = forge.pipeline_keys.clone();
    let fog_surfaces = opaque_fog.len();
    fog_draws::compact(&mut opaque_fog);
    crate::log::progress(format_args!(
        "fog plan: {fog_surfaces} opaque ranges -> {} draws before PVS",
        opaque_fog.len(),
    ));
    let mut runtime = Runtime {
        view_culling: Default::default(),
        gi: None,
        surfaces_by_source,
        lamps: crate::lamp_lights::LampSet::default(),
        active: Default::default(),
        indirect: None,
        stage_table: None,

        lamp_cache_pages: None,
        lamp_cache: None,
        shadow_bounds: [glam::Vec3::ZERO; 2],
        shadow_hulls: None,
        caster_runs: std::cell::OnceCell::new(),
        probe_domain: gi_probe_domain::Domain::default(),
        environment_policy: lighting_environment::Policy::default(),
        ssao: ssao::AmbientOcclusion::default(),
        shadows: None,
        areas: areas::Areas::default(),
        flares: flares::Runtime::default(),
        fog_mode: crate::fog_volumes::Mode::default(),
        fogged_entities: 0,
        fog,
        opaque_fog,
        opaque_fog_by_cluster: Vec::new(),
        sky,
        materials: runtime_materials,
        source_to_runtime,
        entity_pipelines: Vec::with_capacity(keys.len()),
        entity_no_depth_pipelines: Vec::with_capacity(keys.len()),
        opaque_order,
        blended_order,
        dynamic_light_buffer,
        lighting_mode: std::cell::Cell::new(0),
        forge,
    };
    // One thread per key was measured twice as no faster on RADV (cold cache, 88
    // pipelines: 98 s sequential, 106 s parallel): the driver serialises. Sequential,
    // and only the variants each side of the scene uses; the rest compile on first draw.
    let pipelines_started = Instant::now();
    let mut world_keys = vec![false; keys.len()];
    let mut model_keys = vec![false; keys.len()];
    // Forced-alpha (Force Speed trail) variants of model stages: depth-tested only.
    let mut forced_keys = vec![false; keys.len()];
    for material in &runtime.materials {
        let world = !material.static_draws.is_empty() || !material.mover_draws.is_empty();
        for stage in &material.stages {
            for index in [stage.pipeline, stage.live_pipeline] {
                if world {
                    world_keys[index] = true;
                } else {
                    model_keys[index] = true;
                }
            }
            if !world {
                for index in stage.forced_alpha_pipelines {
                    forced_keys[index] = true;
                }
            }
        }
    }
    for index in 0..keys.len() {
        runtime.push_pipeline_slots();
        if world_keys[index] || model_keys[index] || forced_keys[index] {
            runtime.entity_pipeline(index, true);
        }
        if model_keys[index] {
            runtime.entity_pipeline(index, false);
        }
    }
    let both = world_keys
        .iter()
        .zip(&model_keys)
        .filter(|(w, m)| **w && **m)
        .count();
    crate::log::progress(format_args!(
        "compiled {} pipeline keys in {:.0} ms \
        (cumulative per variant, depth-tested, no-depth: {:?} ms; keys used by \
        world materials {}, by model materials {}, by both {}; the rest on first draw)",
        keys.len(),
        pipelines_started.elapsed().as_secs_f64() * 1e3,
        forge::compile_profile(),
        world_keys.iter().filter(|w| **w).count(),
        model_keys.iter().filter(|m| **m).count(),
        both
    ));
    runtime.flares = flares::Runtime::new(device, &runtime.forge, &runtime.materials);
    runtime.ssao = ssao::AmbientOcclusion::new(
        device,
        &runtime.forge.camera_layout,
        runtime.forge.format,
        &runtime.materials,
        &runtime.forge.pipeline_keys,
    );
    Ok((runtime, resolved))
}

fn build_draw_order(materials: &[Material], blended: bool) -> Vec<PassRef> {
    let mut order = materials
        .iter()
        .enumerate()
        .filter(|(_, material)| !material.flare && material.blended == blended)
        .flat_map(|(material, entry)| {
            (0..entry.stages.len() + usize::from(!entry.fog_draws.is_empty()))
                .map(move |stage| PassRef { material, stage })
        })
        .collect::<Vec<_>>();
    // A material's stages blend over one another: stage k must be drawn before stage k+1.
    // Sorting by pipeline first drew them in pipeline-index order, which only happens to be
    // stage order for the common replace-then-filter stack. Stage-major order keeps every
    // material's stages in sequence and still batches each level by pipeline; the order
    // between different opaque materials does not matter. Blended materials overlap what
    // is behind them, so each finishes all its stages before the next one starts.
    order.sort_by(|left, right| {
        let pipeline = |pass: &PassRef| {
            materials[pass.material]
                .stages
                .get(pass.stage)
                .map_or(usize::MAX, |stage| stage.pipeline)
        };
        let by_sort = materials[left.material]
            .sort
            .total_cmp(&materials[right.material].sort);
        if blended {
            by_sort
                .then(left.material.cmp(&right.material))
                .then(left.stage.cmp(&right.stage))
        } else {
            by_sort
                .then(left.stage.cmp(&right.stage))
                .then(pipeline(left).cmp(&pipeline(right)))
                .then(left.material.cmp(&right.material))
        }
    });
    order
}

/// Identical-PVS neighbours share one submission, including all their colour stages.
/// This offsets fog submission cost without changing which triangles are visible.
fn coalesce_static_draws(draws: &mut Vec<StaticDraw>) {
    for draw in draws.iter_mut() {
        draw.clusters.sort_unstable();
    }
    draws.dedup_by(|next, previous| {
        if previous.indices.end == next.indices.start && previous.clusters == next.clusters {
            previous.indices.end = next.indices.end;
            previous.bounds = [
                previous.bounds[0].min(next.bounds[0]),
                previous.bounds[1].max(next.bounds[1]),
            ];
            true
        } else {
            false
        }
    });
}

pub(crate) fn visible(
    clusters: &[usize],
    source: Option<usize>,
    visibility: Option<&Visibility>,
) -> bool {
    let (Some(source), Some(visibility)) = (source, visibility) else {
        return true;
    };
    clusters.is_empty()
        || clusters
            .iter()
            .any(|&cluster| visibility.is_cluster_visible(source, cluster))
}
