//! Sabers in the stage model's hands. Each hilt is its own detached mesh
//! (the shared object buffers only exist inside a session) parked on the
//! hand bolt of the pose the stage was skinned with, and its blades go
//! through the same glow/core renderer as in-game sabers, so the menu shows
//! exactly the hilt, sockets and colours the player will carry.

use super::throw::{Pose, Throw};
use super::*;
use crate::menu_backdrop::Focus;
use crate::player_menu::StageSabers;
use crate::saber::{self, Attachment, HAND_BOLT, LEFT_HAND_BOLT};
use crate::saber_hilts::{HiltBlade, hilt_blades, hilt_sockets};
use crate::saber_rgb::BladeColor;

/// The last request the hands were synced to, so a request that has not
/// changed costs a comparison and nothing else — including one whose hilt
/// failed to load (it is retried only when the request changes).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Request {
    hilts: [Option<String>; 2],
    colors: [[u8; 3]; 2],
}

impl Request {
    pub(super) fn matches(&self, wanted: &StageSabers<'_>) -> bool {
        self.colors == wanted.colors
            && self
                .hilts
                .iter()
                .zip(wanted.hilts)
                .all(|(current, wanted)| current.as_deref() == wanted)
    }

    pub(super) fn from(wanted: &StageSabers<'_>) -> Self {
        Self {
            hilts: wanted.hilts.map(|name| name.map(str::to_owned)),
            colors: wanted.colors,
        }
    }
}

/// One loaded hilt in a hand.
pub(super) struct StageSaber {
    color: BladeColor,
    blades: [Option<HiltBlade>; 8],
    num_blades: u8,
    draws: Vec<DetachedDraw>,
    materials: DetachedMaterials,
    vertex_buffer: wgpu::Buffer,
    geometry_binding: wgpu::BindGroup,
    index_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
}

impl StageSaber {
    /// Draw the hilt's opaque or blended surfaces.
    pub(super) fn draw<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        world_materials: &'pass world_materials::Runtime,
        camera: &'pass wgpu::BindGroup,
        blended: bool,
    ) {
        world_materials.draw_detached(
            pass,
            camera,
            &self.geometry_binding,
            &self.vertex_buffer,
            &self.index_buffer,
            &self.instance_buffer,
            &self.materials,
            &self.draws,
            blended,
        );
    }
}

impl GpuState {
    /// Reload the hands for a changed request. Sockets and blade parameters
    /// come from the `.sab` definitions; a hilt that fails to load leaves
    /// its hand empty.
    pub(super) fn sync_stage_sabers(&mut self, request: Request) {
        for hand in 0..2 {
            let color = BladeColor::from_rgb(request.colors[hand]);
            self.menu_stage.sabers[hand] = request.hilts[hand].as_deref().and_then(|name| {
                self.build_stage_saber(name, color)
                    .inspect_err(|error| {
                        eprintln!("player stage could not load saber {name}: {error}");
                    })
                    .ok()
            });
        }
        self.menu_stage.request = request;
        self.menu_stage.sabers_dirty = true;
    }

    fn build_stage_saber(
        &mut self,
        name: &str,
        color: BladeColor,
    ) -> Result<StageSaber, Box<dyn Error>> {
        let vfs = self.vfs.clone().ok_or("player stage has no VFS")?;
        if self.menu_stage.saber_definitions.is_none() {
            self.menu_stage.saber_definitions = Some(crate::saber_defs::load(&vfs)?);
        }
        let definition = self
            .menu_stage
            .saber_definitions
            .as_ref()
            .and_then(|definitions| definitions.get(&name.to_ascii_lowercase()))
            .ok_or_else(|| format!("no saber definition named {name}"))?
            .clone();
        let asset = vfs
            .read(&definition.model)?
            .ok_or_else(|| format!("hilt model {} is missing", definition.model))?;
        let model = Glm::parse(&asset.bytes)?;
        let sockets = hilt_sockets(&model)?;
        let mut flattened = FlattenedScene::default();
        let draws = append_static_glm_mesh(&mut flattened, &model)?;
        let materials = self.world_materials.compile_detached(
            &self.device,
            &self.queue,
            &vfs,
            &self.shaders,
            &flattened.materials,
        )?;
        let draws = draws
            .into_iter()
            .map(|draw| DetachedDraw {
                indices: draw.indices,
                material: draw.material,
            })
            .collect();
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("JKR player stage hilt vertices"),
                contents: bytemuck::cast_slice(&flattened.vertices),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE,
            });
        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("JKR player stage hilt indices"),
                contents: bytemuck::cast_slice(&flattened.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        let instance = ActorInstance::new([0.0; 3], Quat::IDENTITY.to_array(), [1.0; 3]);
        let quads = crate::shared_geometry::quads::upload(
            &self.device,
            flattened.vertices.len(),
            &flattened.indices,
            0,
        );
        let geometry_binding =
            crate::shared_geometry::quads::bind(&self.device, &vertex_buffer, &quads);
        let instance_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("JKR player stage hilt instance"),
                contents: bytemuck::bytes_of(&instance),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
        Ok(StageSaber {
            color,
            blades: hilt_blades(&definition, &sockets),
            num_blades: definition.num_blades,
            draws,
            materials,
            vertex_buffer,
            geometry_binding,
            index_buffer,
            instance_buffer,
        })
    }

    /// Step every hand's throw for this frame and remember where its hilt
    /// is while out of the hand. Two hilts float side by side across the
    /// focus, one alone on it. Without a hand, a hilt or a focus point a
    /// saber simply stays held.
    pub(super) fn advance_stage_throw(&mut self, thrown: bool, focus: Option<Focus>, now: Instant) {
        let stage = &mut self.menu_stage;
        let held = stage.sabers.iter().flatten().count();
        for hand in 0..2 {
            let attachment = stage.actor.as_ref().and_then(|actor| {
                let (grip, rotation) =
                    saber::world_attachment(actor.origin, actor.rotation, actor.hands[hand]?);
                Some(Pose { grip, rotation })
            });
            let blade = stage.sabers[hand]
                .as_ref()
                .and_then(|saber| saber.blades[0].as_ref())
                .map(|blade| {
                    let direction = Vec3::from_array(blade.socket.direction).normalize_or(Vec3::Y);
                    (
                        direction,
                        Vec3::from_array(blade.socket.origin).dot(direction),
                    )
                });
            let flying = match blade {
                Some((blade, root)) => {
                    let spot = focus.map(|focus| {
                        focus.point
                            + focus.right * float_offset(hand, held)
                            + Vec3::Z * (FLOAT_ROOT - root)
                    });
                    stage.throw[hand].advance(thrown, attachment, spot, blade, now)
                }
                None => {
                    stage.throw[hand] = Throw::Held;
                    None
                }
            };
            if flying.is_some() || stage.flying[hand].is_some() {
                stage.sabers_dirty = true;
            }
            stage.flying[hand] = flying;
        }
    }

    /// Start this frame's saber instance list: cleared, then seeded with the
    /// stage model's blades (session sabers are appended by entity
    /// submission afterwards). Hilt instances are rewritten only when the
    /// pose or the hilts changed.
    pub(crate) fn begin_saber_instances(&mut self) {
        self.saber_instances.clear();
        let stage = &mut self.menu_stage;
        let Some(actor) = &stage.actor else {
            return;
        };
        for (hand, saber) in stage.sabers.iter().enumerate() {
            let (Some(saber), Some(attachment)) = (saber, actor.hands[hand]) else {
                continue;
            };
            let (grip, rotation) = match stage.flying[hand] {
                Some(pose) => (pose.grip, pose.rotation),
                None => saber::world_attachment(actor.origin, actor.rotation, attachment),
            };
            if stage.sabers_dirty {
                // In the hand the hilt shares the model's light sample; out of
                // it, the grid is read where the hilt actually is.
                let light = match stage.flying[hand] {
                    Some(pose) => self
                        .entity_lighting
                        .sample(&self.bsp, pose.grip.to_array(), &[]),
                    None => actor.light,
                };
                let mut instance =
                    ActorInstance::new(grip.to_array(), rotation.to_array(), [1.0; 3]);
                instance.set_light(light);
                self.queue
                    .write_buffer(&saber.instance_buffer, 0, bytemuck::bytes_of(&instance));
            }
            for blade in saber
                .blades
                .iter()
                .take(usize::from(saber.num_blades))
                .flatten()
            {
                let blade =
                    saber::world_blade(grip, rotation, blade.socket, blade.length, blade.radius);
                self.saber_instances
                    .extend(saber::Instance::pair(blade, saber.color));
            }
        }
        stage.sabers_dirty = false;
    }
}

/// Sideways distance between two floating hilts.
const FLOAT_SPACING: f32 = 17.0;
/// Height of a floating hilt's blade root above the focus point. Hilts
/// keep their model origin at different distances below the blade
/// (`single_1` 4.5 units, `single_9` 6.0, the staffs 11.6), so the blade,
/// not the origin, is what is placed on the focus: cycling hilts swaps the
/// hilt under a blade that stays put.
const FLOAT_ROOT: f32 = 4.5;

/// Where along the focus's right axis `hand`'s hilt floats when `held`
/// hands carry one: centred alone, the right hand's hilt to the viewer's
/// left of a pair.
fn float_offset(hand: usize, held: usize) -> f32 {
    if held < 2 {
        0.0
    } else {
        (hand as f32 - 0.5) * FLOAT_SPACING
    }
}

/// Both hand bolts of the whole-body frame the stage was skinned with.
pub(super) fn hand_attachments(preview: &PlayerPreview, frame: usize) -> [Option<Attachment>; 2] {
    let Ok(matrices) = preview.animation.frame_matrices(frame) else {
        return [None; 2];
    };
    [HAND_BOLT, LEFT_HAND_BOLT]
        .map(|bolt| saber::attachment_from_matrices(preview, &matrices, bolt))
}
