mod actor_instance;
mod actor_load;
mod actor_mesh;
mod actor_pose;
mod actor_tint;
mod actor_world_submission;
mod animation_timing;
mod app_launch;
mod assets;
mod audio;
mod audio_output;
mod auto_switch;
mod bolt;
mod camera;
mod camera_uniform;
mod capture;
mod cgame_options;
mod chat;
mod client_guid;
mod client_state;
mod clientinfo_refresh;

mod shader_image;

mod clock_trace;
mod combat_effects;
mod config_string_refresh;
mod connection;
mod connection_commands;
mod console;
mod console_runtime;
mod crosshair_scan;
mod cut_trace;
mod damage_feedback;
mod decal_marks;
mod decal_store;
mod decoded_image_cache;
mod demo_playback;
mod dynamic_lights;
mod effect_assets;
mod effect_aux;
mod effect_blend;
mod effect_electricity;
mod effect_emitter;
mod effect_envelope;
mod effect_geometry;
mod effect_geometry_gpu;
mod vehicle_assets;
mod vehicle_pose;

mod effect_library;
mod effect_runtime;
mod effect_shapes;
mod effect_submission;
mod effect_texcoords;
mod effect_wave;
mod entity_instance;
mod entity_lighting;
mod entity_materials;
mod first_person_view;
mod first_person_weapon;
mod fog_frame;
mod fog_volumes;
mod force_overlay_submission;
mod frame_pacing;
mod frame_split;

mod frame_target;
mod game_menu_actions;
mod gpu_context;
mod gpu_phases;
mod gpu_texture;
mod ground_hud;
mod hud;
mod hud_runtime;
mod impact_spawn;
mod impacts;
mod ingame_menu;
mod input;
mod keybind_editor;
mod lamp_lights;
mod launch;
mod load_profile;
mod local_actor_state;
mod local_prediction;
mod localization;
mod log;
mod main_scene_pass;
mod menu;
mod menu_backdrop;
mod menu_stage;
mod menu_widgets;
mod menu_world;
mod missile_trails;
mod model_materials;
mod movement_collision;
mod movers;
mod muzzle_effects;
mod muzzle_flash;
mod npc_refresh;
mod object_meshes;
mod particle_atlas;
mod particle_draw;
mod portal;
mod prediction_preview;
mod scene_views;
mod scope;
mod surface_tables;
mod viewer_app;
use object_meshes::StaticModelMesh;
mod gi_voxels;
mod live_session;
mod net_timing;
mod particle_motion;
mod particle_physics;
mod particle_spawn;
mod particle_types;
mod pickups;
mod platform;
mod player_animation;
mod player_assets;
mod player_menu;
mod player_shadows;
mod pointer_input;
mod presentation_clock;
mod projectiles;
mod render_helpers;
mod runtime_settings;
mod saber;
mod saber_clash_flare;
mod saber_defs;
mod saber_gpu;
mod saber_hilts;
mod saber_rgb;
mod saber_submission;
mod saber_trail;
mod saber_trail_gpu;
mod scene_flatten;
mod scoreboard;
mod screenshot;
mod server_browser;
mod server_commands;
mod session_transition;
mod settings;
mod shared_geometry;
mod sky_stage;
mod snapshot_presentation;
mod static_models;
mod text;
mod ui_renderer;
mod weapon_view;
mod world_materials;
mod world_props;
mod world_stage;
use actor_instance::ActorInstance;
use actor_mesh::ActorMesh;
use animation_timing::animation_frame;
use camera_uniform::CameraUniform;
use effect_runtime::{EffectLibrary, Particle};
use entity_instance::EntityInstance;
use glam::camera::rh::{proj::directx::perspective, view::look_at_mat4};
use glam::{Mat4, Quat, Vec3};
use gpu_context::FrameStatus;
use gpu_texture::{create_rgba8_texture, decode_image, load_shader_texture};
use ingame_menu::Page as GameMenuPage;
use jkr_bsp::{Aabb, Bsp, TraceScratch};
use jkr_client::{
    ClientSession, LegacyMapEffects, LegacyMissileEffects, LegacySaberClashFlare,
    LegacyWorldAdapter, ServerClock, legacy_model_appearance,
};
use jkr_effect::ComponentKind;
use jkr_model::{AnimationConfig, AnimationSequence, Gla, Glm, Md3, Md3Tag, Skin, SkinnedVertex};
use jkr_protocol::{GameState, InfoString, Snapshot};
use jkr_runtime::{Appearance, EntityId, EntityKind, World, WorldId};
use jkr_scene::{MeshBuildOptions, StaticWorld};
use jkr_shader::{ShaderCatalog, StageBlend, WaveForm};
use jkr_vfs::VirtualFileSystem;
use local_prediction::LocalPrediction;
use localization::Localization;
use particle_types::ParticleBlend;
use player_animation::{GpuPlayerAnimation, PreviewVertexRange};
use player_assets::{PlayerPreview, load_player_appearance, load_player_preview};
use render_helpers::{angle_to_short, append_instance_group, mesh_center, texture_layout_entry};

use scene_flatten::{
    ActorDraw, DrawBatch, FlattenedScene, ViewerMaterial, append_actor_mesh, append_md3_mesh,
    append_static_glm_mesh, preview_gpu_vertex,
};
use shared_geometry::SharedGeometry;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::error::Error;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use text::{MAX_TEXT_VERTICES, TextVertex, UiFont, append_text, visible_text_width};
use ui_renderer::ShapeRenderer;
use wgpu::util::DeviceExt;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{DeviceEvent, ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};
fn main() {
    log::init();
    if let Err(error) = app_launch::run() {
        log::progress(format_args!("jkr-viewer: {error}"));
        std::process::exit(1);
    }
}
mod gpu_vertex;
use gpu_vertex::GpuVertex;

use audio::GameAudio;
use hud_runtime::HudUniform;

struct GpuState {
    context: Arc<gpu_context::Context>,
    window: Option<Arc<Window>>,
    post_aa: Option<frame_target::aa::Runtime>,
    render_scale: Option<frame_target::scale::Runtime>,
    screenshots: screenshot::Manager,
    device: wgpu::Device,
    queue: wgpu::Queue,
    configuration: wgpu::SurfaceConfiguration,
    present_modes: Vec<wgpu::PresentMode>,
    adapter_name: String,
    adapter_backend: String,
    size: PhysicalSize<u32>,
    world_materials: world_materials::Runtime,
    scene_views: scene_views::Runtime,
    entity_pipeline: wgpu::RenderPipeline,
    particle_pipelines: particle_draw::Pipelines,
    particle_atlas: ParticleAtlas,
    effect_geometry: effect_geometry_gpu::Runtime,
    hud_pipeline: wgpu::RenderPipeline,
    ui_shapes: ShapeRenderer,
    text_pipeline: wgpu::RenderPipeline,
    saber_gpu: saber_gpu::Runtime,
    geometry: SharedGeometry,
    entity_instance_buffer: wgpu::Buffer,
    actor_instance_buffer: wgpu::Buffer,
    actor_meshes: Vec<ActorMesh>,
    actor_workers: actor_pose::evaluation::Pool,
    object_meshes: Vec<StaticModelMesh>,
    emitter_model_catalog: effect_emitter::ModelCatalog,
    mover_catalog: movers::Catalog,
    pickup_catalog: pickups::Catalog,
    first_person_weapon: first_person_weapon::Catalog,
    first_person_view: first_person_view::Tracker,
    model_material_overrides: model_materials::Overrides,
    saber_hilts: Option<saber::HiltCatalog>,
    saber_states: saber_trail::StateSlab,
    saber_trail_segments: saber_trail::SegmentPool,
    projectiles: Vec<projectiles::Presented>,
    missile_effects: LegacyMissileEffects,
    dynamic_lights: dynamic_lights::PointLightList,
    effect_aux: effect_aux::Runtime,
    muzzle_effects: jkr_client::LegacyMuzzleEffects,
    force_overlays: jkr_client::LegacyForceOverlayTracker,
    force_overlays_last: usize,
    actor_groups: Vec<Vec<ActorInstance>>,
    object_groups: Vec<Vec<ActorInstance>>,
    static_models: static_models::StaticModels,
    pickup_override_instances: Vec<entity_materials::OverrideInstance>,
    mover_groups: Vec<Vec<ActorInstance>>,
    movers: Vec<movers::Presented>,
    pickups: Vec<pickups::Presented>,
    entity_instances: Vec<EntityInstance>,
    saber_instances: Vec<saber::Instance>,
    particle_groups: [Vec<EntityInstance>; effect_blend::PIPELINE_COUNT],
    actor_instances: Vec<ActorInstance>,
    actor_instance_ranges: Vec<Range<u32>>,
    object_instance_ranges: Vec<Range<u32>>,
    pickup_override_ranges: Vec<entity_materials::OverrideRange>,
    entity_draw_queue: entity_materials::Queue,
    mover_instance_ranges: Vec<Range<u32>>,
    bsp: Bsp,
    trace_scratch: TraceScratch,
    entity_lighting: entity_lighting::EntityLighting,
    /// Live player model behind the Player screen.
    menu_stage: menu_stage::MenuStage,
    clientinfo_watch: clientinfo_refresh::ClientInfoWatch,
    config_string_refresh: config_string_refresh::ConfigStringRefresh,
    /// Parsed shader catalogue, kept for materials compiled after map load.
    shaders: ShaderCatalog,
    decal_surfaces: decal_marks::DecalSurfaces,
    player_shadows: player_shadows::State,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    hud_buffer: wgpu::Buffer,
    hud_bind_group: wgpu::BindGroup,
    text_vertex_buffer: wgpu::Buffer,
    classic_text_vertex_buffer: wgpu::Buffer,
    text_bind_group: wgpu::BindGroup,
    text_layout: wgpu::BindGroupLayout,
    text_sampler: wgpu::Sampler,
    classic_text_bind_group: Option<wgpu::BindGroup>,
    ui_font: UiFont,
    classic_hud_font: Option<UiFont>,
    text_vertices: Vec<TextVertex>,
    classic_text_vertices: Vec<TextVertex>,
    hud: hud::HudOverlay,
    scoreboard: scoreboard::Scoreboard,
    chat: chat::ChatOverlay,
    game_menu: bool,
    in_game_menu: ingame_menu::InGameMenu,
    game_menu_page: GameMenuPage,
    game_menu_row: usize,
    auto_opened_team_menu: bool,
    depth: DepthTarget,
    camera_position: Vec3,
    camera_yaw: f32,
    camera_pitch: f32,
    mouse_look: pointer_input::MouseLook,
    field_of_view: f32,
    scope: scope::Zoom,
    scope_mask: Option<scope::Mask>,
    ground_hud: ground_hud::GroundHud,
    applied_fullscreen: bool,
    applied_resolution: [u32; 2],
    frame_pacer: frame_pacing::FramePacer,
    /// Optional per-pass GPU timing printed with the frame-budget report.
    gpu_phases: Option<gpu_phases::Profiler>,
    third_person_camera: camera::ThirdPersonCamera,
    far_plane: f32,
    gameplay_input: input::GameplayInput,
    pointer_captured: bool,
    cursor_policy: pointer_input::CursorPolicy,
    cursor_position: [f32; 2],
    ui_epoch: Instant,
    last_frame: Instant,
    last_cluster: i32,
    player_animation: Option<GpuPlayerAnimation>,
    live_session: Option<ClientSession>,
    demo_session: Option<demo_playback::Session>,

    live_world: World,
    legacy_world_adapter: Option<LegacyWorldAdapter>,
    presentation_clock: presentation_clock::SnapshotPresentationClock,
    net_timing: net_timing::NetTiming,
    network_command_due: Instant,
    intermission_score_request_time: Option<i32>,
    /// Server-time estimate that stamps outgoing user commands.
    server_clock: ServerClock,
    clock_trace: clock_trace::ClockTrace,
    cut_trace: cut_trace::CutTrace,
    local_prediction: LocalPrediction,
    local_actor_state: local_actor_state::Tracker,
    third_person: bool,
    /// Evidence cameras (spectate/look-at/orbit) leave the local actor at
    /// its entity transform instead of pinning it under the camera.
    detached_camera: bool,

    selected_weapon: Option<u8>,
    weapon_selected_at: Option<Instant>,
    weapon_selection_label: String,
    pending_generic_command: u8,
    vfs: Option<Arc<VirtualFileSystem>>,
    pending_map_reload: bool,
    world_load_task: Option<session_transition::WorldLoadTask>,
    world_install_task: Option<session_transition::WorldInstallTask>,
    world_load_state: session_transition::LoadStateMachine,
    world_load_started: Option<Instant>,
    world_load_map: String,
    snapshot_observations: snapshot_presentation::Counters,
    obituaries: jkr_client::ObituaryTracker,
    lagometer: jkr_client::LagometerSamples,
    crosshair_scan: crosshair_scan::State,
    auto_switch: auto_switch::Tracker,
    previous_particle_events: HashMap<u16, u16>,
    particles: Vec<Particle>,
    pending_particle_effects: particle_physics::PendingEffects,
    impacts: impacts::Pool,
    /// `cg.lastFPFlashPoint`: the view gun's flash point of the last frame.
    last_first_person_flash: Option<[f32; 3]>,
    saber_clash_flare: LegacySaberClashFlare,
    damage_feedback: damage_feedback::Feedback,
    effects: EffectLibrary,
    /// Map music read on the install thread, handed to the audio at the cut.
    sound_prefetch: audio::SoundPrefetch,
    map_effects: LegacyMapEffects,
    localization: Localization,
    console: Option<console::ViewerConsole>,
    client_menu: Option<menu::ClientMenu>,
    join_task: Option<connection::JoinTask>,
    last_connect_address: Option<String>,
    game_data: PathBuf,
    connect_timeline: Option<log::ConnectTimeline>,
    load_event_gap: log::EventGap,
    transition_report_pending: bool,

    completed_map_changes: u32,
    live_map_installed: bool,

    quit_requested: bool,
    /// The boot world the standalone menu lives in (see `menu_world`).
    is_menu_world: bool,
    /// The map being joined, seen through the menu's gate.
    portal: portal::Destination,
}
use assets::GpuWorldInput;

impl GpuState {
    async fn new(window: Arc<Window>, input: GpuWorldInput) -> Result<Self, Box<dyn Error>> {
        let size = window.inner_size();
        Self::new_for_target(Some(window), [size.width, size.height], input).await
    }

    async fn new_for_target(
        window: Option<Arc<Window>>,
        target_size: [u32; 2],
        input: GpuWorldInput,
    ) -> Result<Self, Box<dyn Error>> {
        let context = gpu_context::Context::new(window, input.console.as_ref()).await?;
        Self::new_with_context(context, target_size, input).await
    }

    async fn new_with_context(
        context: Arc<gpu_context::Context>,
        target_size: [u32; 2],
        input: GpuWorldInput,
    ) -> Result<Self, Box<dyn Error>> {
        let input = assets::prepare_instances(input)?;
        let GpuWorldInput {
            scene,
            bsp,
            vfs,
            shaders,
            world_minimums,
            world_maximums,
            camera_origin,
            camera_yaw,
            player_preview,
            live_session,
            demo_session,
            build_game_state,
            build_snapshot,
            mut console,
            mut client_menu,
            game_data,
            mut connect_timeline,
            completed_map_changes,
        } = input;
        let mut load_profile = load_profile::LoadProfile::start();
        let session_active =
            live_session.is_some() || demo_session.is_some() || build_game_state.is_some();
        let active_game_state = live_session
            .as_ref()
            .map(ClientSession::game_state)
            .or_else(|| {
                demo_session
                    .as_ref()
                    .map(demo_playback::Session::game_state)
            })
            .or(build_game_state.as_ref());
        let active_snapshot = live_session
            .as_ref()
            .map(ClientSession::latest_snapshot)
            .or_else(|| {
                demo_session
                    .as_ref()
                    .map(demo_playback::Session::latest_snapshot)
            })
            .or(build_snapshot.as_ref());
        let world_id = WorldId::new(1);
        let mut live_world = World::new(world_id);
        let mut legacy_world_adapter = active_game_state.map(|_| LegacyWorldAdapter::new(world_id));
        let latest_scene_server_time = active_snapshot.map_or(0, |snapshot| snapshot.server_time);
        let presentation_clock = presentation_clock::SnapshotPresentationClock::new(
            latest_scene_server_time,
            Instant::now(),
        );
        if let (Some(adapter), Some(snapshot), Some(game_state)) = (
            &mut legacy_world_adapter,
            active_snapshot,
            active_game_state,
        ) {
            adapter.apply_snapshot(snapshot, game_state, &mut live_world);
        }
        let server_clock = ServerClock::unanchored(Instant::now());
        let animation_config = player_preview
            .as_ref()
            .map(|preview| preview.config.as_ref());
        let local_prediction =
            LocalPrediction::new(active_snapshot, animation_config, active_game_state, &vfs);
        let config_string_refresh =
            config_string_refresh::ConfigStringRefresh::new(active_game_state);
        let map_effects = active_game_state
            .map_or_else(LegacyMapEffects::empty, LegacyMapEffects::from_game_state);
        let missile_effects = active_game_state.map_or_else(
            LegacyMissileEffects::empty,
            LegacyMissileEffects::from_game_state,
        );
        let size = PhysicalSize::new(target_size[0].max(1), target_size[1].max(1));
        let localization = Localization::load(&vfs);
        menu::attach_world(&mut client_menu, Arc::clone(&vfs), &bsp, console.as_ref());
        if let Some(console) = &mut console {
            console.attach_script_vfs(Arc::clone(&vfs));
        }
        let gpu_phases = gpu_phases::Profiler::new(&context.device, &context.queue);
        let device = &context.device;
        let queue = &context.queue;
        let window = &context.window;
        let surface = &context.surface;
        let format = context.format;
        let vsync = console
            .as_ref()
            .and_then(|console| console.bool_cvar("r_vsync"))
            .unwrap_or(false);
        let present_mode = context.present_mode(vsync);
        let screenshot_setup = screenshot::setup(&context, console.as_ref(), &game_data);
        let configuration = wgpu::SurfaceConfiguration {
            usage: screenshot_setup.surface_usage,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode: context.alpha_mode,
            view_formats: Vec::new(),
        };
        if console.is_some()
            && let Some(surface) = &surface
        {
            surface.configure(&device, &configuration);
        }

        let mut flattened = scene_flatten::prepare(&scene, &bsp, &shaders)?;
        load_profile.mark("flatten-bsp");
        let player_animation = player_preview
            .map(|preview| GpuPlayerAnimation::append(&mut flattened, preview))
            .transpose()?;
        let mut actor_meshes = if session_active {
            let active_world = demo_session
                .as_ref()
                .map_or(&live_world, demo_playback::Session::world);
            actor_load::load_actor_meshes(&vfs, active_game_state, active_world, &mut flattened)?
        } else {
            Vec::new()
        };
        load_profile.mark("player-models");
        let mover_catalog = movers::build_catalog(&bsp, &mut flattened);
        let decal_surfaces = decal_marks::DecalSurfaces::from_flattened(&flattened, &bsp);
        let mut object_meshes = if session_active {
            object_meshes::load(
                &vfs,
                &bsp,
                demo_session
                    .as_ref()
                    .map_or(&live_world, demo_playback::Session::world),
                active_game_state,
                map_effects
                    .effect_names()
                    .chain(missile_effects.effect_names()),
                &mut flattened,
            )
        } else {
            Vec::new()
        };
        let saber_hilts = if session_active {
            saber::load_hilts(
                &vfs,
                actor_meshes
                    .iter()
                    .flat_map(|mesh| mesh.saber_names.iter().flatten().map(String::as_str)),
                &mut flattened,
                &mut object_meshes,
            )
            .map_err(|error| eprintln!("could not load saber hilts: {error}"))
            .ok()
        } else {
            None
        };
        let pickup_catalog = pickups::Catalog::build(&object_meshes);
        let first_person_weapon = first_person_weapon::Catalog::build(
            &vfs,
            &object_meshes,
            actor_meshes
                .first()
                .map(|mesh| mesh.preview.config.as_ref()),
        );
        let emitter_model_catalog = effect_emitter::ModelCatalog::build(&object_meshes);
        let static_models = static_models::StaticModels::build(&bsp, &object_meshes);
        let model_material_overrides = model_materials::append_overrides(&mut flattened.materials);
        if let Some(timeline) = &mut connect_timeline {
            timeline.mark(log::TimelinePhase::Models);
        }
        load_profile.mark("object-models");
        let geometry = actor_pose::gpu_skinning::upload(&device, &mut actor_meshes, &flattened)?;
        load_profile.mark("geometry-upload");
        let world_minimums = Vec3::from_array(world_minimums);
        let world_maximums = Vec3::from_array(world_maximums);
        let far_plane = (world_maximums - world_minimums).length().max(4096.0) * 2.0;
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("JKR camera"),
            contents: bytemuck::bytes_of(&CameraUniform {
                view_projection: Mat4::IDENTITY.to_cols_array_2d(),
                camera_position: camera_origin,
                view_forward: [camera_yaw.cos(), camera_yaw.sin(), 0.0],
                _padding: 0.0,
                shader_time: 0.0,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("JKR camera layout"),
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
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("JKR camera bind group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let hud_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("JKR HUD state"),
            contents: bytemuck::bytes_of(&HudUniform {
                crosshair_color: cgame_options::crosshair_color(None),
                health_ratio: 1.0,
                armor_ratio: 0.0,
                force_ratio: 1.0,
                menu_open: 0.0,
                inverse_width: 1.0 / configuration.width as f32,
                inverse_height: 1.0 / configuration.height as f32,
                menu_row: 0.0,
                menu_row_count: 1.0,
                crosshair: 1.0,
                hud_visible: 1.0,
                status_visible: 1.0,
                menu_kind: 0.0,
                menu_phase: 0.0,
                damage_x: 0.0,
                damage_y: 0.0,
                damage_alpha: 0.0,
                damage_strength: 0.0,
                health_bar: [0.0; 4],
                armor_bar: [0.0; 4],
                force_bar: [0.0; 4],
                _padding: [0.0; 3],
                crosshair_parameters: [0.0, 0.0, 640.0, 480.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let hud_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("JKR HUD layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let hud_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("JKR HUD bind group"),
            layout: &hud_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: hud_buffer.as_entire_binding(),
            }],
        });
        let modern_atlas =
            text::load_modern(window.as_ref().map_or(1.0, |window| window.scale_factor()))?;
        let ui_font = modern_atlas.font;
        let font_view = gpu_texture::create_rgba8_texture_mipmapped(
            &device,
            &queue,
            "JKR UI font atlas",
            &modern_atlas.image,
            true,
            text::ATLAS_MIP_LEVELS,
        );
        let text_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("JKR text layout"),
            entries: &[
                texture_layout_entry(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let text_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("JKR text sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let text_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("JKR text bind group"),
            layout: &text_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&font_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&text_sampler),
                },
            ],
        });
        let classic_atlas = text::load_classic(&vfs).ok();
        let (classic_hud_font, classic_text_bind_group) =
            classic_atlas.map_or((None, None), |atlas| {
                let view = create_rgba8_texture(
                    &device,
                    &queue,
                    "JKR classic HUD font atlas",
                    atlas.image.width(),
                    atlas.image.height(),
                    atlas.image.as_raw(),
                    true,
                );
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("JKR classic HUD text bind group"),
                    layout: &text_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&text_sampler),
                        },
                    ],
                });
                (Some(atlas.font), Some(bind_group))
            });
        let (mut world_materials, resolved_world_stages) =
            world_materials::create_filtered_runtime(
                &device,
                &queue,
                &camera_layout,
                context.scene_format(),
                &bsp,
                &vfs,
                &shaders,
                &flattened.materials,
                &flattened.draws,
                (&flattened.vertices, &flattened.indices),
                &mover_catalog.meshes,
                context.filtering,
                context.sun_shadows.day.enabled,
            )?;
        world_materials.bind_geometry(&geometry);
        world_materials.install_gi(&device, &flattened, context.sun_shadows.day.enabled);
        let scene_views = scene_views::Runtime::new(
            &device,
            &camera_layout,
            (context.scene_format(), [size.width, size.height]),
            &flattened,
            &bsp,
            &shaders,
        );
        load_profile.mark("world-materials");
        if let Some(timeline) = &mut connect_timeline {
            timeline.mark(log::TimelinePhase::Materials);
        }
        log::progress(format_args!(
            "resolved {resolved_world_stages} shared world/entity stages; uploaded {} lightmaps",
            bsp.render().lightmap_count()
        ));
        let particle_layout = particle_atlas::layout(&device);
        let required_effect_shaders = effect_assets::required_shaders(
            &vfs,
            map_effects
                .effect_names()
                .chain(missile_effects.effect_names()),
        );
        let particle_atlas = particle_atlas::create(
            &device,
            &queue,
            &particle_layout,
            &vfs,
            &shaders,
            &required_effect_shaders,
        )?;
        let effect_geometry = effect_geometry_gpu::Runtime::new(
            &device,
            &camera_layout,
            &particle_layout,
            frame_target::aa::effects::FORMAT,
        );
        load_profile.mark("effect-atlas");
        let (entity_pipeline, particle_pipelines) = particle_draw::create(
            device,
            &camera_layout,
            &particle_layout,
            context.scene_format(),
        );
        let saber_gpu = saber_gpu::Runtime::new(
            &device,
            &queue,
            &vfs,
            &shaders,
            &camera_layout,
            frame_target::aa::effects::FORMAT,
        )?;
        let hud_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("JKR HUD shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("hud.wgsl").into()),
        });
        let hud_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("JKR HUD pipeline layout"),
            bind_group_layouts: &[Some(&hud_layout)],
            immediate_size: 0,
        });
        let hud_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("JKR HUD pipeline"),
            layout: Some(&hud_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &hud_shader,
                entry_point: Some("vertex_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &hud_shader,
                entry_point: Some("fragment_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DepthTarget::FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("JKR text shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("text.wgsl").into()),
        });
        let text_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("JKR text pipeline layout"),
            bind_group_layouts: &[Some(&text_layout)],
            immediate_size: 0,
        });
        let text_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("JKR text pipeline"),
            layout: Some(&text_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &text_shader,
                entry_point: Some("vertex_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(TextVertex::layout())],
            },
            fragment: Some(wgpu::FragmentState {
                module: &text_shader,
                entry_point: Some("fragment_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DepthTarget::FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let text_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("JKR dynamic text vertices"),
            size: (MAX_TEXT_VERTICES * std::mem::size_of::<TextVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let classic_text_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("JKR dynamic classic HUD text vertices"),
            size: (MAX_TEXT_VERTICES * std::mem::size_of::<TextVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let entity_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("JKR dynamic entity instances"),
            size: (1_024 * std::mem::size_of::<EntityInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let actor_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("JKR actor instances"),
            size: (1_024 * std::mem::size_of::<ActorInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let depth = DepthTarget::new(&device, configuration.width, configuration.height);
        let (ui_shapes, hud) = hud::icons::install(&device, &queue, format, &vfs, &shaders);
        let (scope_mask, ground_hud) = (
            scope::Mask::new(&device, &queue, format, &vfs, &shaders),
            ground_hud::GroundHud::new(device, context.scene_format(), &text_layout),
        );
        load_profile.mark("pipelines-ui");

        let third_person = live_session.is_some()
            || build_game_state.is_some()
            || demo_session
                .as_ref()
                .is_some_and(|session| session.camera().third_person());
        let game_menu = active_snapshot.is_some_and(|snapshot| snapshot.player.is_spectator());
        let actor_groups = (0..actor_meshes.len())
            .map(|_| Vec::with_capacity(4))
            .collect();
        let object_groups = object_meshes::instance_groups(&object_meshes);
        let mover_groups = movers::instance_groups(&mover_catalog.meshes);
        let effects = config_string_refresh::preload_effects(&vfs, &map_effects, &missile_effects);
        load_profile.mark("effect-graphs");
        let sound_prefetch = audio::SoundPrefetch::read(&vfs, build_game_state.as_ref(), &bsp);
        load_profile.mark("sound-prefetch");
        if let Some(timeline) = &mut connect_timeline {
            timeline.mark(log::TimelinePhase::Sounds);
        }
        let live_map_installed = live_session.is_some();
        let trace_scratch = bsp.trace_scratch();
        let entity_lighting = entity_lighting::EntityLighting::from_world(&bsp);
        Ok(Self {
            render_scale: None,
            post_aa: frame_target::aa::Runtime::for_context(
                &context,
                [configuration.width, configuration.height],
            ),
            window: context.window.clone(),
            device: context.device.clone(),
            queue: context.queue.clone(),
            present_modes: context.present_modes.clone(),
            adapter_name: context.adapter_name.clone(),
            adapter_backend: context.adapter_backend.clone(),
            context,

            screenshots: screenshot_setup.manager,
            configuration,
            size,
            world_materials,
            scene_views,
            entity_pipeline,
            particle_pipelines,
            particle_atlas,
            effect_geometry,
            hud_pipeline,
            ui_shapes,
            text_pipeline,
            saber_gpu,
            geometry,
            entity_instance_buffer,
            actor_instance_buffer,
            actor_meshes,
            actor_workers: actor_pose::evaluation::Pool::new(),
            object_meshes,
            emitter_model_catalog,
            mover_catalog,
            pickup_catalog,
            first_person_weapon,
            first_person_view: first_person_view::Tracker::default(),

            model_material_overrides,
            saber_hilts,
            saber_states: saber_trail::StateSlab::default(),
            saber_trail_segments: saber_trail::SegmentPool::default(),
            projectiles: Vec::with_capacity(jkr_protocol::MAX_LEGACY_ENTITIES),
            missile_effects,
            dynamic_lights: dynamic_lights::PointLightList::default(),
            effect_aux: effect_aux::Runtime::default(),
            muzzle_effects: jkr_client::LegacyMuzzleEffects::new(),
            force_overlays: jkr_client::LegacyForceOverlayTracker::default(),
            force_overlays_last: 0,
            actor_groups,
            object_groups,
            static_models,
            pickup_override_instances: Vec::with_capacity(force_overlay_submission::CAPACITY),
            mover_groups,
            movers: Vec::with_capacity(jkr_protocol::MAX_LEGACY_ENTITIES),
            pickups: Vec::with_capacity(jkr_protocol::MAX_LEGACY_ENTITIES),
            entity_instances: Vec::with_capacity(1_024),
            saber_instances: Vec::with_capacity(saber::MAX_BLADE_INSTANCES),
            particle_groups: std::array::from_fn(|_| Vec::with_capacity(1_024)),
            actor_instances: Vec::with_capacity(1_024),
            actor_instance_ranges: Vec::with_capacity(64),
            object_instance_ranges: Vec::with_capacity(256),
            pickup_override_ranges: Vec::with_capacity(jkr_protocol::MAX_LEGACY_ENTITIES),
            entity_draw_queue: entity_materials::Queue::new(),
            mover_instance_ranges: Vec::with_capacity(bsp.render().models().len()),
            crosshair_scan: crosshair_scan::State::new(&bsp),
            bsp,
            trace_scratch,
            entity_lighting,
            menu_stage: menu_stage::MenuStage::default(),
            clientinfo_watch: clientinfo_refresh::ClientInfoWatch::new(),
            config_string_refresh,
            shaders,
            decal_surfaces,
            player_shadows: player_shadows::State::default(),
            camera_buffer,
            camera_bind_group,
            hud_buffer,
            hud_bind_group,
            text_vertex_buffer,
            classic_text_vertex_buffer,
            text_bind_group,
            text_layout,
            text_sampler,
            classic_text_bind_group,
            ui_font,
            classic_hud_font,
            text_vertices: Vec::with_capacity(MAX_TEXT_VERTICES),
            classic_text_vertices: Vec::with_capacity(4_096),
            hud,
            scoreboard: scoreboard::Scoreboard::new(),
            chat: chat::ChatOverlay::new(),
            game_menu,
            in_game_menu: ingame_menu::InGameMenu::new(),
            game_menu_page: if game_menu {
                GameMenuPage::Team
            } else {
                GameMenuPage::Main
            },
            game_menu_row: 0,
            auto_opened_team_menu: game_menu,
            depth,
            camera_position: Vec3::from_array(camera_origin),
            camera_yaw,
            camera_pitch: 0.0,
            mouse_look: pointer_input::MouseLook::default(),
            field_of_view: 90.0,
            scope: scope::Zoom::default(),
            scope_mask,
            ground_hud,
            applied_fullscreen: false,
            applied_resolution: [size.width, size.height],
            frame_pacer: frame_pacing::FramePacer::new(),
            gpu_phases,
            third_person_camera: camera::ThirdPersonCamera::default(),
            far_plane,
            gameplay_input: input::GameplayInput::default(),
            pointer_captured: false,
            cursor_policy: pointer_input::CursorPolicy::new(),
            cursor_position: [0.0; 2],
            ui_epoch: Instant::now(),
            last_frame: Instant::now(),
            last_cluster: i32::MIN,
            player_animation,
            live_session,
            demo_session,

            live_world,
            legacy_world_adapter,
            presentation_clock,
            net_timing: net_timing::NetTiming::default(),
            network_command_due: Instant::now(),
            intermission_score_request_time: None,
            server_clock,
            clock_trace: clock_trace::ClockTrace::new(),
            cut_trace: cut_trace::CutTrace::new(),
            local_prediction,
            local_actor_state: local_actor_state::Tracker::default(),
            third_person,
            detached_camera: false,

            selected_weapon: None,
            weapon_selected_at: None,
            weapon_selection_label: String::with_capacity(40),
            pending_generic_command: 0,
            effects,
            sound_prefetch,
            vfs: Some(vfs),
            pending_map_reload: false,
            world_load_task: None,
            world_install_task: None,
            world_load_state: session_transition::LoadStateMachine::new(),
            world_load_started: None,
            world_load_map: String::with_capacity(64),
            snapshot_observations: snapshot_presentation::Counters::default(),
            obituaries: jkr_client::ObituaryTracker::new(),
            lagometer: jkr_client::LagometerSamples::new(),
            auto_switch: auto_switch::Tracker::default(),
            previous_particle_events: HashMap::new(),
            particles: Vec::with_capacity(effect_runtime::MAX_PARTICLES),
            pending_particle_effects: particle_physics::PendingEffects::new(),
            impacts: impacts::Pool::new(),
            last_first_person_flash: None,
            saber_clash_flare: LegacySaberClashFlare::default(),
            damage_feedback: damage_feedback::Feedback::default(),
            map_effects,
            localization,
            console,
            client_menu,
            join_task: None,
            last_connect_address: None,
            game_data,
            connect_timeline,
            load_event_gap: log::EventGap::default(),
            transition_report_pending: false,

            completed_map_changes,
            live_map_installed,

            quit_requested: false,
            is_menu_world: false,
            portal: portal::Destination::new(),
        }
        .with_render_scale()
        .with_sun_shadows())
    }

    fn rebuild_modern_text(&mut self, dpi_scale: f64) -> Result<(), Box<dyn Error>> {
        let atlas = text::load_modern(dpi_scale)?;
        let view = gpu_texture::create_rgba8_texture_mipmapped(
            &self.device,
            &self.queue,
            "JKR Inter UI font atlas",
            &atlas.image,
            true,
            text::ATLAS_MIP_LEVELS,
        );
        self.text_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("JKR text bind group"),
            layout: &self.text_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.text_sampler),
                },
            ],
        });
        self.ui_font = atlas.font;
        Ok(())
    }

    fn render_inner(
        &mut self,
        game_audio: &mut Option<GameAudio>,
        timing: &mut frame_pacing::budget::Timer,
    ) -> FrameStatus {
        use frame_pacing::budget::Phase;
        timing.mark(Phase::Shell);
        self.prepare_timed_frame(game_audio);
        let now = Instant::now();
        let delta_seconds = now.duration_since(self.last_frame).as_secs_f32().min(0.05);
        self.last_frame = now;
        let visual_now = now;
        self.local_prediction
            .begin_frame(visual_now.duration_since(self.ui_epoch).as_millis() as i32);
        self.update_live_session(game_audio, visual_now, timing);
        timing.mark(Phase::Preview);
        self.update_demo_playback(game_audio, visual_now);
        let presentation_time = self.demo_session.as_ref().map_or_else(
            || self.presentation_clock.sample(Instant::now()),
            demo_playback::Session::current_server_time,
        );
        prediction_preview::tick(self, game_audio, Instant::now(), presentation_time as i32);
        timing.mark(Phase::Config);
        self.refresh_config_strings(game_audio);
        timing.mark(Phase::View);
        menu_backdrop::drive(self, visual_now);
        // The backdrop keeps the view while a joined map loads behind it.
        let backdrop_view = menu_backdrop::standalone_menu_visible(self);
        self.update_menu_stage(visual_now);
        let local_view = self
            .demo_session
            .as_ref()
            .is_none_or(|session| !matches!(session.camera(), demo_playback::Camera::Spectate(_)));
        self.effect_aux
            .resolve_shakes(self.camera_position, visual_now, local_view);
        let intermission_view = self
            .live_session
            .as_ref()
            .map(ClientSession::latest_snapshot)
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .map(demo_playback::Session::latest_snapshot)
            })
            .and_then(|snapshot| jkr_client::IntermissionView::from_player_state(&snapshot.player))
            .filter(|_| !backdrop_view);
        let forward = Vec3::new(
            self.camera_yaw.cos() * self.camera_pitch.cos(),
            self.camera_yaw.sin() * self.camera_pitch.cos(),
            self.camera_pitch.sin(),
        );
        let horizontal_forward = Vec3::new(self.camera_yaw.cos(), self.camera_yaw.sin(), 0.0);
        let right = Vec3::new(-self.camera_yaw.sin(), self.camera_yaw.cos(), 0.0);
        self.finish_audio_frame(game_audio, forward, right);
        let mut movement = Vec3::ZERO;
        if self.gameplay_input.held(input::GameButton::Forward) {
            movement += horizontal_forward;
        }
        if self.gameplay_input.held(input::GameButton::Back) {
            movement -= horizontal_forward;
        }
        if self.gameplay_input.held(input::GameButton::MoveRight) {
            movement += right;
        }
        if self.gameplay_input.held(input::GameButton::MoveLeft) {
            movement -= right;
        }
        if self.gameplay_input.held(input::GameButton::Up) {
            movement += Vec3::Z;
        }
        if self.gameplay_input.held(input::GameButton::Down) {
            movement -= Vec3::Z;
        }
        if self.live_session.is_none() && self.demo_session.is_none() {
            let speed = if self.gameplay_input.held(input::GameButton::Speed) {
                1_800.0
            } else {
                600.0
            };
            self.camera_position += movement.normalize_or_zero() * speed * delta_seconds;
        }
        net_timing::frame(self, presentation_time);
        clock_trace::tick(self, presentation_time, visual_now);
        let first_person = (!backdrop_view)
            .then(|| first_person_view::camera(self, presentation_time as i32))
            .flatten();
        // cg_view.c:1597-1608: the decaying prediction error shifts only the
        // view origin; the model root keeps the predicted origin.
        let error_offset = self.local_prediction.view_offset();
        let mut view_up = Vec3::Z;
        let (branch, (view_position, view_target)) = if let Some(view) = intermission_view {
            self.third_person_camera.reset();
            let pitch = -view.angles[0].to_radians();
            let yaw = view.angles[1].to_radians();
            let direction = Vec3::new(
                yaw.cos() * pitch.cos(),
                yaw.sin() * pitch.cos(),
                pitch.sin(),
            );
            let origin = Vec3::from_array(view.origin);
            ("intermission", (origin, origin + direction * 256.0))
        } else if (self.live_session.is_some() || self.demo_session.is_some())
            && self.third_person
            && !self.detached_camera
            && !backdrop_view
        {
            (
                "third-person",
                camera::damped_third_person(self, error_offset, delta_seconds, presentation_time),
            )
        } else if let Some(camera) = first_person {
            self.third_person_camera.reset();
            let (position, target, up) = camera.look();
            view_up = up;
            ("first-person", (position, target))
        } else {
            self.third_person_camera.reset();
            let free = (self.camera_position, self.camera_position + forward * 256.0);
            (if backdrop_view { "backdrop" } else { "free" }, free)
        };
        cut_trace::tick(self, branch, (view_position, view_target), visual_now);
        // The third-person camera already traced from the shifted origin.
        let error_offset = if branch == "third-person" {
            Vec3::ZERO
        } else {
            error_offset
        };
        let (view_position, view_target) = effect_aux::apply_camera_offset(
            view_position + error_offset,
            view_target + error_offset,
            self.effect_aux.camera_offset(visual_now),
        );
        self.update_fog_setting();
        let view = look_at_mat4(view_position, view_target, view_up);
        let aspect = self.configuration.width as f32 / self.configuration.height as f32;
        let fov = self.scope_fov(presentation_time as i32, game_audio);
        let projection = perspective(fov.to_radians(), aspect, 2.0, self.far_plane);
        // rd-vanilla `tr_shade.cpp:367` derives tess.shaderTime from the
        // frame/refdef time. Legacy remap timeOffset is zero on this path.
        self.upload_scene_camera(CameraUniform {
            view_projection: (projection * view).to_cols_array_2d(),
            camera_position: view_position.to_array(),
            view_forward: (view_target - view_position).normalize().to_array(),
            _padding: if self.scene_views.sky_only_fog {
                2.0
            } else {
                0.0
            },
            shader_time: presentation_time as f32 * 0.001,
        });
        timing.mark(Phase::Hud);
        let damage = self.damage_feedback.sample(presentation_time as i32);
        self.update_auto_switch();
        let mut hud_visibility =
            ground_hud::frame(self, intermission_view.is_some(), presentation_time as i32);
        if self.scope.mode != 0 && cgame_options::scope_style(self.console.as_ref()) != 1 {
            hud_visibility.crosshair = false;
            hud_visibility.crosshair_names = false;
        }
        let viewport = [
            self.configuration.width as f32,
            self.configuration.height as f32,
        ];
        let text_scale = (self.configuration.height as f32 / 1_080.0).clamp(0.85, 1.35);
        hud_runtime::update(
            self,
            view_position,
            (view_target, view_up, fov),
            presentation_time as i32,
            intermission_view.is_some(),
        );
        let classic_hud = self
            .console
            .as_ref()
            .and_then(|console| console.bool_cvar("cg_classicHudFont"))
            .unwrap_or(false);
        let hud_font = if classic_hud {
            self.classic_hud_font.as_ref().unwrap_or(&self.ui_font)
        } else {
            &self.ui_font
        };
        let hud_layout = self.hud.layout(
            hud_font,
            viewport,
            runtime_settings::hud_scale(self.console.as_ref()),
            hud_visibility,
            presentation_time.max(0) as u64,
        );
        let [health_ratio, armor_ratio, force_ratio] = self.hud.displayed_ratios();
        let (menu_kind, visual_menu_row) = self
            .client_menu
            .as_ref()
            .filter(|menu| menu.is_visible())
            .map_or(
                (0.0, self.game_menu_row as f32),
                menu::ClientMenu::visual_selection,
            );
        self.queue.write_buffer(
            &self.hud_buffer,
            0,
            bytemuck::bytes_of(&HudUniform {
                crosshair_color: self.hud.targeting.color,
                health_ratio,
                armor_ratio,
                force_ratio,
                menu_open: menu_backdrop::shader_menu_state(self),
                inverse_width: 1.0 / self.configuration.width as f32,
                inverse_height: 1.0 / self.configuration.height as f32,
                menu_row: visual_menu_row,
                menu_row_count: self.game_menu_row_count() as f32,
                crosshair: hud::options::crosshair_size(
                    self.console.as_ref(),
                    hud_visibility.crosshair,
                ),
                hud_visible: f32::from(hud_visibility.hud),
                status_visible: 0.0,
                menu_kind,
                menu_phase: now.duration_since(self.ui_epoch).as_secs_f32(),
                damage_x: damage.map_or(0.0, |sample| sample.x),
                damage_y: damage.map_or(0.0, |sample| sample.y),
                damage_alpha: damage.map_or(0.0, |sample| sample.alpha),
                damage_strength: damage.map_or(0.0, |sample| sample.strength),
                health_bar: hud_layout.health_bar,
                armor_bar: hud_layout.armor_bar,
                force_bar: hud_layout.force_bar,
                _padding: [0.0; 3],
                crosshair_parameters: self.hud.targeting.parameters(viewport),
            }),
        );
        self.text_vertices.clear();
        self.classic_text_vertices.clear();
        let information_visible = (self.live_session.is_some() || self.demo_session.is_some())
            && self
                .console
                .as_ref()
                .and_then(|c| c.bool_cvar("cg_draw2D"))
                .unwrap_or(true)
            && !self.game_menu
            && !self
                .console
                .as_ref()
                .is_some_and(|console| console.is_open())
            && !self
                .client_menu
                .as_ref()
                .is_some_and(|menu| menu.is_visible());
        if information_visible && intermission_view.is_none() {
            hud_runtime::append(
                self,
                classic_hud,
                hud_visibility,
                hud_layout,
                text_scale,
                viewport,
            );
        }
        let scoreboard_visible = information_visible
            && (intermission_view.is_some() || self.gameplay_input.held(input::GameButton::Scores))
            && self
                .console
                .as_ref()
                .and_then(|c| c.bool_cvar("cg_drawScores"))
                .unwrap_or(true);
        let chat_visible = scoreboard::chat_visible(
            scoreboard_visible,
            information_visible,
            self.chat.wants_history(self.console.as_ref()),
        );
        if chat_visible {
            self.append_configured_chat(viewport, text_scale);
        }
        if self
            .weapon_selected_at
            .is_some_and(|selected| selected.elapsed() < Duration::from_secs(2))
            && !self.weapon_selection_label.is_empty()
        {
            let width = visible_text_width(
                &self.ui_font,
                &self.weapon_selection_label,
                text_scale * 1.1,
            );
            append_text(
                &mut self.text_vertices,
                &self.ui_font,
                &self.weapon_selection_label,
                [(viewport[0] - width) * 0.5, viewport[1] * 0.78],
                text_scale * 1.1,
                viewport,
            );
        }
        if self.game_menu {
            let team_sizes = self.live_session.as_ref().map_or([0, 0], |session| {
                ingame_menu::team_sizes(session.game_state())
            });
            self.in_game_menu.append(
                ingame_menu::View {
                    page: self.game_menu_page,
                    selected_row: self.game_menu_row,
                    team: self
                        .live_session
                        .as_ref()
                        .map_or(3, |session| session.latest_snapshot().player.team()),
                    team_game: self.is_team_game(),
                    red_players: team_sizes[0],
                    blue_players: team_sizes[1],
                    vote_active: self.vote_active(),
                    _frame: std::marker::PhantomData,
                },
                &mut self.text_vertices,
                &self.ui_font,
                viewport,
            );
        }
        if scoreboard_visible {
            scoreboard::append_overlay(self, viewport, text_scale * 1.05);
        }
        if let Some(menu) = &mut self.client_menu {
            menu.append_overlay(&mut self.text_vertices, &self.ui_font, viewport, text_scale);
        }
        if let Some(console) = &mut self.console {
            console.append_overlay(&mut self.text_vertices, &self.ui_font, viewport, text_scale);
        }
        if hud::family::fps(self.console.as_ref()) {
            append_text(
                &mut self.text_vertices,
                &self.ui_font,
                self.frame_pacer.label(),
                [(viewport[0] - 780.0).max(8.0), 18.0],
                text_scale * 0.8,
                viewport,
            );
        }
        let layers = [
            information_visible.then(|| &self.hud.identification.list),
            information_visible.then(|| self.hud.draw_list()),
            chat_visible.then(|| self.chat.draw_list()),
            scoreboard_visible.then(|| self.scoreboard.draw_list()),
            self.game_menu.then(|| self.in_game_menu.draw_list()),
            self.client_menu.as_ref().and_then(|menu| menu.draw_list()),
            self.console.as_ref().map(|console| console.draw_list()),
        ];
        self.ui_shapes
            .prepare_layers(&self.queue, layers.into_iter().flatten(), viewport);
        let text_vertex_count = u32::try_from(self.text_vertices.len()).unwrap_or(0);
        if !self.text_vertices.is_empty() {
            self.queue.write_buffer(
                &self.text_vertex_buffer,
                0,
                bytemuck::cast_slice(&self.text_vertices),
            );
        }
        let classic_text_vertex_count =
            u32::try_from(self.classic_text_vertices.len()).unwrap_or(0);
        if !self.classic_text_vertices.is_empty() {
            self.queue.write_buffer(
                &self.classic_text_vertex_buffer,
                0,
                bytemuck::cast_slice(&self.classic_text_vertices),
            );
        }

        timing.mark(Phase::Pose);
        {
            self.assign_corpse_meshes();
        }
        {
            if let Err(error) = self.update_actor_animations(presentation_time) {
                eprintln!("remote actor animation stopped: {error}");
            }
        }

        timing.mark(Phase::World);
        let leaf_index = self.bsp.leaf_at(self.camera_position.to_array());
        let camera_cluster = self.bsp.leaves()[leaf_index].cluster;
        let source_cluster = usize::try_from(camera_cluster).ok();
        if camera_cluster != self.last_cluster {
            self.last_cluster = camera_cluster;
        }

        let local_entity_id = self
            .live_session
            .as_ref()
            .map(ClientSession::latest_snapshot)
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .map(demo_playback::Session::latest_snapshot)
            })
            .map(|snapshot| u64::from(snapshot.player.client_num()) + 1);
        let fallback_mesh = self
            .actor_meshes
            .iter()
            .position(|mesh| !mesh.corpse_pool && mesh.entity_id.is_none());
        self.actor_groups.iter_mut().for_each(Vec::clear);
        self.object_groups.iter_mut().for_each(Vec::clear);
        self.pickup_override_instances.clear();
        self.mover_groups.iter_mut().for_each(Vec::clear);
        self.entity_instances.clear();
        self.begin_saber_instances();
        self.particle_groups.iter_mut().for_each(Vec::clear);
        self.dynamic_lights.clear();
        let active_snapshot = first_person_view::presented_snapshot(
            self.live_session.as_ref(),
            self.demo_session.as_ref(),
            presentation_time as i32,
        );
        if let Some(snapshot) = active_snapshot {
            let active_world = self
                .demo_session
                .as_ref()
                .map_or(&self.live_world, demo_playback::Session::world);
            if let Some(audio) = game_audio {
                audio.update_frame_loops(
                    snapshot,
                    presentation_time as i32,
                    self.camera_position.to_array(),
                    active_world,
                    &self.bsp,
                );
            }
            self.map_effects.update(snapshot, presentation_time as i32);
            effect_runtime::spawn_map_effect_requests(
                &mut self.particles,
                &mut self.effect_aux,
                &self.map_effects,
                self.vfs
                    .as_ref()
                    .expect("live sessions retain their mounted VFS"),
                &mut self.effects,
                game_audio,
                visual_now,
            );
            missile_trails::update_and_spawn(
                &mut self.missile_effects,
                snapshot,
                presentation_time as i32,
                &mut self.particles,
                &mut self.effect_aux,
                &mut self.effects,
                self.vfs
                    .as_ref()
                    .expect("live sessions retain their mounted VFS"),
                game_audio,
                &mut self.dynamic_lights,
                visual_now,
            );
            projectiles::collect(snapshot, presentation_time as i32, &mut self.projectiles);
            movers::collect(snapshot, presentation_time as i32, &mut self.movers);
            self.local_prediction
                .pickups(snapshot, presentation_time as i32, &mut self.pickups);
            pickups::simple::prepare(&mut self.pickups, self.console.as_ref());
            pickups::spawn_cones(
                &self.pickups,
                &mut self.particles,
                &mut self.effect_aux,
                &mut self.effects,
                self.vfs
                    .as_ref()
                    .expect("live sessions retain their mounted VFS"),
                game_audio,
                presentation_time as i32,
                visual_now,
            );
            self.muzzle_effects
                .update(snapshot, presentation_time as i32);
        } else {
            self.projectiles.clear();
            self.movers.clear();
            self.pickups.clear();
        }
        let view_weapon = first_person_weapon::frame_inputs(
            self,
            active_snapshot,
            first_person,
            local_entity_id,
            presentation_time,
        );
        if let Some(inputs) = view_weapon {
            first_person_weapon::submit(&self.first_person_weapon, inputs, &mut self.object_groups);
            // `CG_AddPlayerWeapon` records the view gun's `tag_flash` for
            // trace-weapon beams (`cg_weapons.c:579`) and plays the
            // first-person muzzle effect there (`cg_weapons.c:704-762`).
            let socket = first_person_weapon::flash_socket(&self.first_person_weapon, inputs);
            self.last_first_person_flash = socket.map(|socket| socket.origin);
            if let Some(request) = active_snapshot
                .and_then(|snapshot| self.muzzle_effects.request(snapshot.player.client_num()))
                && let Some(socket) = socket
            {
                muzzle_effects::spawn(
                    &mut self.particles,
                    &mut self.effect_aux,
                    &mut self.effects,
                    self.vfs
                        .as_ref()
                        .expect("sessions retain their mounted VFS"),
                    game_audio,
                    request,
                    socket,
                    visual_now,
                    presentation_time as i32,
                );
            }
        }
        self.prepare_scene_views(view, projection, presentation_time as i32);
        movers::append_frame(self, presentation_time, visual_now);
        self.static_models.append_instances(&mut self.object_groups);
        pickups::simple::append_frame(self, visual_now);
        self.force_overlays_last = actor_world_submission::submit(
            self,
            local_entity_id,
            fallback_mesh,
            presentation_time,
            visual_now,
            game_audio,
        );
        timing.mark(Phase::Acquire);
        let (frame, output_view) = match frame_target::acquire(self) {
            Ok(target) => target,
            Err(status) => return status,
        };
        let target_view = self.scene_target(&output_view).clone();
        timing.mark(Phase::Effects);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("JKR frame encoder"),
            });
        if let Some(phases) = &self.gpu_phases {
            phases.begin(&mut encoder);
        }
        // The map being joined shows through the open gate: it is drawn
        // first, and the menu world draws over it without its own sky.
        let portal =
            menu_backdrop::gate_doorway(self, visual_now).map_or(portal::View::Absent, |doorway| {
                let shader_time = presentation_time as f32 * 0.001;
                self.draw_portal(&mut encoder, &target_view, doorway, shader_time)
            });
        dynamic_lights::entities::finish(self, presentation_time, visual_now, game_audio);
        let object_groups = &mut self.object_groups;
        let mover_groups = &mut self.mover_groups;
        let actor_groups = &mut self.actor_groups;
        let entity_instances = &mut self.entity_instances;
        let saber_instances = &mut self.saber_instances;
        let particle_groups = &mut self.particle_groups;
        let active_world = self
            .demo_session
            .as_ref()
            .map_or(&self.live_world, demo_playback::Session::world);
        for missile in &self.projectiles {
            match missile.visual {
                projectiles::Visual::Model(model) => {
                    if let Some(mesh) = self
                        .object_meshes
                        .iter()
                        .position(|mesh| mesh.appearance.model.eq_ignore_ascii_case(model))
                    {
                        object_groups[mesh].push(ActorInstance::new(
                            missile.origin,
                            missile.rotation,
                            [1.0; 3],
                        ));
                    }
                }
                // Effect-only missiles are emitted by the complete repeating
                // EFX graph in missile_trails; do not double-draw the older
                // representative shot sprite from step 11.
                projectiles::Visual::Effect(_) => {}
                projectiles::Visual::None => {}
            }
        }
        saber_clash_flare::append(
            &self.saber_clash_flare,
            presentation_time as i32,
            view_position,
            (view_target - view_position).normalize_or(forward),
            projection * view,
            &self.bsp,
            &mut self.trace_scratch,
            &self.particle_atlas,
            &mut particle_groups[1],
        );
        self.context.dynamic_light_settings.upload(
            &self.world_materials,
            &self.queue,
            &mut self.dynamic_lights,
        );
        let particle_now = Instant::now();
        particle_physics::update_and_spawn(
            &mut self.particles,
            &mut self.pending_particle_effects,
            &mut self.effect_aux,
            &mut self.effects,
            self.vfs
                .as_ref()
                .expect("particle effects retain their VFS"),
            game_audio,
            particle_now,
            &self.bsp,
            &mut self.trace_scratch,
        );
        effect_emitter::append_model_instances(
            &mut self.effect_aux.emitters,
            &self.emitter_model_catalog,
            object_groups,
        );
        self.world_materials.fogged_entities = entity_lighting::apply_frame(
            &self.entity_lighting,
            &self.bsp,
            &self.dynamic_lights,
            actor_groups,
            object_groups,
            &mut self.pickup_override_instances,
            self.world_materials.fogs(),
        );
        let global_particle_seconds = presentation_time as f32 * 0.001;
        player_shadows::request_all(
            &self.player_shadows,
            player_shadows::Inputs {
                world: active_world,
                presentation_time,
                local_entity: local_entity_id,
                local_root: camera::local_actor_root(
                    self.camera_position,
                    self.local_prediction.view_height(),
                    self.camera_yaw,
                )
                .0,
                bsp: &self.bsp,
                scratch: &mut self.trace_scratch,
                decals: &mut self.effect_aux.decals,
            },
            player_shadows::mesh_legs_yaw(&self.actor_meshes),
        );
        let atlas = &self.particle_atlas;
        self.effect_aux.decals.project_pending(
            &self.decal_surfaces,
            &self.bsp,
            particle_now,
            |shader| {
                effect_blend::slot(
                    atlas
                        .layers_for(shader, 0.0)
                        .iter()
                        .next()
                        .map_or(ParticleBlend::Add, |layer| layer.blend),
                )
            },
        );
        timing.mark(Phase::Encode);
        let particle_ranges = effect_submission::prepare(
            timing,
            effect_submission::Inputs {
                geometry: &mut self.effect_geometry,
                queue: &self.queue,
                encoder: &mut encoder,
                particles: &mut self.particles,
                decals: &mut self.effect_aux.decals,
                atlas: &self.particle_atlas,
                now: particle_now,
                global_seconds: global_particle_seconds,
                camera: self.camera_position,
                field_of_view: self.field_of_view,
                bsp: &self.bsp,
                trace_scratch: &mut self.trace_scratch,

                entity_instances,
                blended: particle_groups,
            },
        );
        timing.mark(Phase::EncodeInstances);
        Self::pack_frame_instances(
            &mut self.actor_instances,
            &mut self.actor_instance_ranges,
            &mut self.object_instance_ranges,
            &mut self.mover_instance_ranges,
            actor_groups,
            object_groups,
            mover_groups,
        );
        let actor_instances = &mut self.actor_instances;
        let actor_instance_ranges = &mut self.actor_instance_ranges;
        let object_instance_ranges = &mut self.object_instance_ranges;
        entity_materials::append_override_ranges(
            actor_instances,
            &self.pickup_override_instances,
            &mut self.pickup_override_ranges,
        );
        self.entity_draw_queue.rebuild(
            &self.world_materials,
            &self.actor_meshes,
            actor_instance_ranges,
            &self.object_meshes,
            object_instance_ranges,
            &self.pickup_override_ranges,
            actor_instances,
            self.camera_position,
        );
        self.saber_gpu.prepare(
            &self.queue,
            saber_instances,
            &mut self.saber_trail_segments,
            presentation_time,
        );
        if !entity_instances.is_empty() {
            self.queue.write_buffer(
                &self.entity_instance_buffer,
                0,
                bytemuck::cast_slice(entity_instances),
            );
        }
        if !actor_instances.is_empty() {
            self.queue.write_buffer(
                &self.actor_instance_buffer,
                0,
                bytemuck::cast_slice(actor_instances),
            );
        }
        let has_entity_instances = !entity_instances.is_empty();
        timing.mark(Phase::EncodeViews);
        if let Some(phases) = &self.gpu_phases {
            phases.mark(&mut encoder, "uploads");
        }
        self.draw_scene_views(&mut encoder, &particle_ranges);
        if let Some(phases) = &self.gpu_phases {
            phases.mark(&mut encoder, "views");
        }
        let visibility = self.bsp.render().visibility();
        timing.mark(Phase::EncodeWorld);
        self.encode_world_scene(
            &mut encoder,
            &target_view,
            portal,
            source_cluster,
            &particle_ranges,
            has_entity_instances,
        );
        timing.mark(Phase::EncodeOverlays);
        self.draw_frame_overlays(
            &mut encoder,
            &target_view,
            &output_view,
            source_cluster,
            visibility,
            text_vertex_count,
            classic_text_vertex_count,
        );
        timing.mark(Phase::EncodeCopy);
        if let Some(phases) = &self.gpu_phases {
            phases.mark(&mut encoder, "overlays+post+hud");
        }
        let screenshot_encoded = self.encode_console_screenshot(&mut encoder, frame.as_ref());
        if let Some(phases) = &self.gpu_phases {
            phases.mark(&mut encoder, "capture");
            phases.finish(&mut encoder);
        }
        timing.mark(Phase::Submit);
        self.frame_pacer.split.submit(&self.queue, encoder, timing);
        if let Some(phases) = &self.gpu_phases {
            phases.after_submit();
            phases.report();
        }
        if screenshot_encoded {
            self.screenshots.after_submit();
        }
        timing.mark(Phase::Present);
        if let Some(frame) = frame {
            self.queue.present(frame);
        }
        timing.mark(Phase::Other);
        self.complete_render_transition(game_audio);
        FrameStatus::Rendered
    }
}

struct ParticleAtlas {
    bind_group: wgpu::BindGroup,
    animations: HashMap<String, Vec<ParticleAtlasAnimation>>,
    fallback: [f32; 4],
}

struct ParticleAtlasAnimation {
    frames: Vec<[f32; 4]>,
    frequency: f32,
    one_shot: bool,
    blend: ParticleBlend,
    rgb_wave: Option<WaveForm>,
    alpha_wave: Option<WaveForm>,
    tc_scale: [f32; 2],
    tc_scroll: [f32; 2],
}

#[derive(Clone, Copy)]
struct ParticleLayerSample {
    uv_rect: [f32; 4],
    blend: ParticleBlend,
    rgb: f32,
    alpha: f32,
    uv_transform: [f32; 4],
}

impl ParticleAtlas {
    fn first_layer(&self, shader: &str, age_seconds: f32) -> ParticleLayerSample {
        self.animations
            .get(shader)
            .and_then(|animations| animations.first())
            .map(|animation| self.sample_animation(animation, age_seconds))
            .unwrap_or(ParticleLayerSample {
                uv_rect: self.fallback,
                blend: ParticleBlend::Add,
                rgb: 1.0,
                alpha: 1.0,
                uv_transform: [1.0, 1.0, 0.0, 0.0],
            })
    }

    fn sample_animation(
        &self,
        animation: &ParticleAtlasAnimation,
        age_seconds: f32,
    ) -> ParticleLayerSample {
        if animation.frames.len() == 1 || animation.frequency <= 0.0 {
            return ParticleLayerSample {
                uv_rect: animation.frames.first().copied().unwrap_or(self.fallback),
                blend: animation.blend,
                rgb: effect_wave::evaluate(animation.rgb_wave.as_ref(), age_seconds),
                alpha: effect_wave::evaluate(animation.alpha_wave.as_ref(), age_seconds),
                uv_transform: effect_texcoords::sample(
                    animation.tc_scale,
                    animation.tc_scroll,
                    age_seconds,
                ),
            };
        }
        let raw = (age_seconds.max(0.0) * animation.frequency).floor() as usize;
        let frame = if animation.one_shot {
            raw.min(animation.frames.len() - 1)
        } else {
            raw % animation.frames.len()
        };
        ParticleLayerSample {
            uv_rect: animation.frames[frame],
            blend: animation.blend,
            rgb: effect_wave::evaluate(animation.rgb_wave.as_ref(), age_seconds),
            alpha: effect_wave::evaluate(animation.alpha_wave.as_ref(), age_seconds),
            uv_transform: effect_texcoords::sample(
                animation.tc_scale,
                animation.tc_scroll,
                age_seconds,
            ),
        }
    }

    fn layers_for(&self, shader: &str, age_seconds: f32) -> effect_runtime::ParticleLayerSamples {
        let fallback = ParticleLayerSample {
            uv_rect: self.fallback,
            blend: ParticleBlend::Add,
            rgb: 1.0,
            alpha: 1.0,
            uv_transform: [1.0, 1.0, 0.0, 0.0],
        };
        let mut samples = effect_runtime::ParticleLayerSamples::new(fallback);
        if let Some(animations) = self.animations.get(shader) {
            for animation in animations.iter().take(8) {
                samples.push(self.sample_animation(animation, age_seconds));
            }
        }
        samples.ensure_fallback();
        samples
    }
}

mod depth_target;
use depth_target::DepthTarget;
