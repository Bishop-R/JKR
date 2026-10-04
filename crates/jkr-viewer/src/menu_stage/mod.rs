//! The live player model on the menu's stage: while the backdrop camera is
//! on a route with a [`Stage`], the current `model` cvar is skinned into
//! its own vertex/index buffers, lit from the map like any actor, and drawn
//! through the shared material runtime. Changing the model rebuilds the
//! buffers on the spot, so the swap is visible the same frame. The sabers
//! in its hands live in [`sabers`]; the Saber tab's throw in [`throw`].

mod sabers;
mod throw;

use super::*;
use crate::actor_instance::EntityLight;
use crate::menu_backdrop::Stage;
use crate::player_assets::{GlaCache, load_player_appearance_with, split_model_cvar};
use crate::saber::Attachment;
use crate::world_materials::{DetachedDraw, DetachedMaterials};
use sabers::StageSaber;

/// Q3 `CONTENTS_SOLID`: the only content the stage floor trace stops on.
const CONTENTS_SOLID: u32 = 1;
/// Distance from a standing player's origin down to the soles: the legacy
/// standing box bottom (`DEFAULT_MINS_2`, `codemp/game/bg_public.h`).
const PLAYER_FEET_BELOW_ORIGIN: f32 = 24.0;
/// Farthest a stage may be dropped onto the floor below it. A longer fall
/// means the authored floor is not brush collision (a curved patch, say)
/// and the authored height is kept instead.
const MAX_STAGE_DROP: f32 = 96.0;
/// Stance before the menu has said which sabers are held.
const DEFAULT_STANCE: &str = "BOTH_STAND2";
/// Torso held while the saber is out (`PM_WeaponLightsaber`,
/// `codemp/game/bg_saber.c`: `BOTH_SABERPULL` with `SETANIM_FLAG_HOLD`
/// while `saberInFlight`); the stage plays it whole-body.
const THROW_STANCE: &str = "BOTH_SABERPULL";

/// Stage state owned by [`GpuState`]; empty until a player screen asks for a
/// model.
#[derive(Default)]
pub(crate) struct MenuStage {
    gla_cache: GlaCache,
    actor: Option<StageActor>,
    /// Model cvar whose load failed; not retried until the value changes.
    failed: Option<String>,
    /// `.sab` definitions, read once the first hilt is needed.
    saber_definitions: Option<BTreeMap<String, crate::saber_defs::Definition>>,
    /// Right and left hand.
    sabers: [Option<StageSaber>; 2],
    request: sabers::Request,
    /// Hilt instances need rewriting (pose or hilts changed).
    sabers_dirty: bool,
    /// Each hand's throw, and its hilt's pose while it is out of the hand.
    throw: [throw::Throw; 2],
    flying: [Option<throw::Pose>; 2],
}

struct StageActor {
    model: String,
    /// Animation the actor idles in, by `animation.cfg` name.
    stance: &'static str,
    preview: PlayerPreview,
    origin: Vec3,
    rotation: Quat,
    light: EntityLight,
    /// Right and left hand bolts of `current_frame`.
    hands: [Option<Attachment>; 2],
    vertex_ranges: Vec<PreviewVertexRange>,
    draws: Vec<DetachedDraw>,
    materials: DetachedMaterials,
    vertex_buffer: wgpu::Buffer,
    geometry_binding: wgpu::BindGroup,
    index_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    current_frame: usize,
    started: Instant,
}

impl GpuState {
    /// Load, swap or animate the stage model for this frame, and feed the
    /// menu images (model icons, map preview) to the UI atlas.
    pub(crate) fn update_menu_stage(&mut self, now: Instant) {
        menu::upload_menu_images(
            &mut self.client_menu,
            &mut self.ui_shapes,
            &self.device,
            &self.queue,
        );
        let wanted = menu_backdrop::standalone_menu_visible(self)
            .then(|| {
                self.client_menu
                    .as_ref()
                    .and_then(menu::ClientMenu::stage_model)
            })
            .flatten();
        let Some((stage, model)) = wanted else {
            self.menu_stage.actor = None;
            return;
        };
        let wanted = self
            .client_menu
            .as_ref()
            .map(menu::ClientMenu::stage_sabers);
        let thrown = wanted.as_ref().is_some_and(|sabers| sabers.thrown);
        let focus = self
            .client_menu
            .as_ref()
            .and_then(menu::ClientMenu::saber_focus);
        let (stance, request) = match wanted {
            Some(sabers) => (
                Some(sabers.stance),
                (!self.menu_stage.request.matches(&sabers)).then(|| sabers::Request::from(&sabers)),
            ),
            None => (None, None),
        };
        let loaded = self
            .menu_stage
            .actor
            .as_ref()
            .map(|actor| actor.model.as_str());
        if loaded != Some(model) {
            if self.menu_stage.failed.as_deref() == Some(model) {
                return;
            }
            let model = model.to_owned();
            match self.build_stage_actor(stage, &model, now) {
                Ok(actor) => self.menu_stage.actor = Some(actor),
                Err(error) => {
                    eprintln!("player stage could not load {model}: {error}");
                    self.menu_stage.failed = Some(model);
                    return;
                }
            }
        }
        if let Some(request) = request {
            self.sync_stage_sabers(request);
        }
        self.advance_stage_throw(thrown, focus, now);
        if let Some(stance) = stance {
            let held = self.menu_stage.throw.iter().all(throw::Throw::is_held);
            self.set_stage_stance(if held { stance } else { THROW_STANCE }, now);
        }
        if let Err(error) = self.animate_stage_actor(now) {
            eprintln!("player stage animation stopped: {error}");
            self.menu_stage.actor = None;
        }
    }

    /// Restart the actor in `stance` when the menu's style changed it. A
    /// model without that sequence keeps the one it has.
    fn set_stage_stance(&mut self, stance: &'static str, now: Instant) {
        let Some(actor) = &mut self.menu_stage.actor else {
            return;
        };
        if actor.stance == stance {
            return;
        }
        if let Some(sequence) = actor.preview.config.get(stance).cloned() {
            actor.preview.sequence = sequence;
            actor.started = now;
            actor.current_frame = usize::MAX;
        }
        actor.stance = stance;
    }

    fn build_stage_actor(
        &mut self,
        stage: Stage,
        model: &str,
        now: Instant,
    ) -> Result<StageActor, Box<dyn Error>> {
        let vfs = self.vfs.clone().ok_or("player stage has no VFS")?;
        let (directory, variant) = split_model_cvar(model);
        let mut preview = load_player_appearance_with(
            &vfs,
            &directory,
            variant,
            [0.0; 3],
            0.0,
            &mut self.menu_stage.gla_cache,
        )?;
        preview.origin = [0.0; 3];
        preview.yaw = 0.0;
        let stance = self
            .menu_stage
            .actor
            .as_ref()
            .map_or(DEFAULT_STANCE, |actor| actor.stance);
        if let Some(sequence) = preview.config.get(stance).cloned() {
            preview.sequence = sequence;
        }
        let started = now;
        let frame = animation_frame(&preview.sequence, 0.0);
        let mut flattened = FlattenedScene::default();
        let (actor_draws, vertex_ranges) = append_actor_mesh(&mut flattened, &preview, frame)?;
        let materials = self.world_materials.compile_detached(
            &self.device,
            &self.queue,
            &vfs,
            &self.shaders,
            &flattened.materials,
        )?;
        let draws = actor_draws
            .into_iter()
            .map(|draw| DetachedDraw {
                indices: draw.indices,
                material: draw.material,
            })
            .collect();
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("JKR player stage vertices"),
                contents: bytemuck::cast_slice(&flattened.vertices),
                usage: wgpu::BufferUsages::VERTEX
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::STORAGE,
            });
        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("JKR player stage indices"),
                contents: bytemuck::cast_slice(&flattened.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        let quads = crate::shared_geometry::quads::upload(
            &self.device,
            flattened.vertices.len(),
            &flattened.indices,
            0,
        );
        let geometry_binding =
            crate::shared_geometry::quads::bind(&self.device, &vertex_buffer, &quads);
        let origin = self.stage_floor(stage);
        let rotation = weapon_view::actor_world_rotation(
            Quat::from_rotation_z(stage.yaw.to_radians()).to_array(),
        );
        let light = self.entity_lighting.sample(&self.bsp, origin, &[]);
        let mut instance = ActorInstance::new(origin, rotation.to_array(), [1.0; 3]);
        instance.set_light(light);
        let instance_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("JKR player stage instance"),
                contents: bytemuck::bytes_of(&instance),
                usage: wgpu::BufferUsages::VERTEX,
            });
        self.menu_stage.sabers_dirty = true;
        Ok(StageActor {
            model: model.to_owned(),
            stance,
            hands: sabers::hand_attachments(&preview, frame),
            preview,
            origin: Vec3::from_array(origin),
            rotation,
            light,
            vertex_ranges,
            draws,
            materials,
            vertex_buffer,
            geometry_binding,
            index_buffer,
            instance_buffer,
            current_frame: frame,
            started,
        })
    }

    /// Settle the stage origin onto the floor just below the authored point
    /// (which names the floor height, give or take) so the model stands on
    /// the map. Player models hang `PLAYER_FEET_BELOW_ORIGIN` under their
    /// origin, so the origin sits that far above the floor.
    fn stage_floor(&self, stage: Stage) -> [f32; 3] {
        let [x, y, z] = stage.origin;
        let start = [x, y, z + PLAYER_FEET_BELOW_ORIGIN];
        let end = [x, y, z - MAX_STAGE_DROP];
        let trace = self.bsp.trace_box(start, end, Aabb::POINT, CONTENTS_SOLID);
        let floor = if trace.start_solid || trace.fraction >= 1.0 {
            z
        } else {
            trace.end_position[2]
        };
        [x, y, floor + PLAYER_FEET_BELOW_ORIGIN]
    }

    fn animate_stage_actor(&mut self, now: Instant) -> Result<(), Box<dyn Error>> {
        let Some(actor) = &mut self.menu_stage.actor else {
            return Ok(());
        };
        let elapsed = now.saturating_duration_since(actor.started).as_secs_f32();
        let frame = animation_frame(&actor.preview.sequence, elapsed);
        if frame == actor.current_frame {
            return Ok(());
        }
        let preview = &actor.preview;
        let surfaces = preview
            .mesh
            .skin(&preview.animation, &preview.skin, frame, 0)?;
        for range in &actor.vertex_ranges {
            let surface = surfaces
                .get(range.surface_index)
                .ok_or("stage model surface disappeared")?;
            if surface.vertices.len() != range.vertices.len() {
                return Err("stage model vertex count changed".into());
            }
            let vertices = surface
                .vertices
                .iter()
                .map(|vertex| preview_gpu_vertex(preview, vertex))
                .collect::<Vec<_>>();
            let byte_offset = u64::try_from(range.vertices.start)?
                .checked_mul(u64::try_from(std::mem::size_of::<GpuVertex>())?)
                .ok_or("stage model buffer offset overflow")?;
            self.queue.write_buffer(
                &actor.vertex_buffer,
                byte_offset,
                bytemuck::cast_slice(&vertices),
            );
        }
        actor.hands = sabers::hand_attachments(preview, frame);
        actor.current_frame = frame;
        self.menu_stage.sabers_dirty = true;
        Ok(())
    }
}

impl MenuStage {
    /// Draw the stage model's opaque or blended surfaces into the world pass.
    pub(crate) fn draw<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        world_materials: &'pass world_materials::Runtime,
        camera: &'pass wgpu::BindGroup,
        blended: bool,
    ) {
        let Some(actor) = &self.actor else {
            return;
        };
        world_materials.draw_detached(
            pass,
            camera,
            &actor.geometry_binding,
            &actor.vertex_buffer,
            &actor.index_buffer,
            &actor.instance_buffer,
            &actor.materials,
            &actor.draws,
            blended,
        );
        for saber in self.sabers.iter().flatten() {
            saber.draw(pass, world_materials, camera, blended);
        }
    }
}
