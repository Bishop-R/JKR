//! The world beyond the menu's gate: the map the client is joining.
//!
//! As soon as the target map is known (from the browser row, or from the
//! server's gamestate for an address join) it is loaded on the workers as a
//! static preview — geometry and sky only, no session — while the connection
//! proceeds. Once the preview is ready the gate is allowed to open, and every
//! frame the preview is drawn first from a camera that mirrors the menu
//! camera through the doorway; the menu world then draws over it without
//! its own sky, so the destination shows wherever the menu world is open.

pub(crate) mod clip;
#[path = "portal_render.rs"]
mod render;

use crate::gpu_context::Context;
use crate::menu_backdrop::Vantage;
use crate::session_transition::{WorldInstallPoll, WorldInstallTask, WorldLoadPoll, WorldLoadTask};
use crate::{CameraUniform, GpuState, GpuWorldInput};
use glam::Vec3;
use glam::camera::rh::{proj::directx::perspective, view::look_at_mat4};
use std::path::Path;
use std::sync::Arc;

/// An upright doorway: where it stands and which way "through" points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Frame {
    pub(crate) origin: Vec3,
    /// Radians.
    pub(crate) yaw: f32,
}

impl Frame {
    /// The same doorway, "through" pointing the other way.
    pub(crate) fn reversed(self) -> Self {
        Self {
            origin: self.origin,
            yaw: self.yaw + std::f32::consts::PI,
        }
    }

    /// Unit vector pointing through the doorway.
    pub(crate) fn forward(self) -> Vec3 {
        let (sin, cos) = self.yaw.sin_cos();
        Vec3::new(cos, sin, 0.0)
    }

    /// The far doorway of a map. A map with a gate of its own (the menu map
    /// joined for real) is a mirror: the far doorway is that same gate
    /// facing back, so the server's copy of the very corridor the menu
    /// camera stands in shows through the menu's gate, and the glide flies
    /// into it. Any other map is entered at its first deathmatch spawn (else
    /// its intermission view), stood on the floor so the two floors meet at
    /// the doorway, facing the way the spawn faces.
    pub(crate) fn from_bsp(bsp: &jkr_bsp::Bsp) -> Option<Self> {
        if let Some(gate) = crate::menu_backdrop::gate_for(bsp) {
            return Some(gate.doorway().reversed());
        }
        let entities = jkr_entity::parse_entity_lump(bsp.entities()).ok()?;
        let spawn = entities
            .iter()
            .find(|entity| entity.classname() == Some("info_player_deathmatch"))
            .and_then(|entity| {
                let origin = Vec3::from_array(entity.vector("origin").ok()??);
                let yaw = entity
                    .get("angle")
                    .and_then(|angle| angle.trim().parse::<f32>().ok())
                    .unwrap_or(0.0);
                Some((origin, yaw.to_radians()))
            });
        let (origin, yaw) = match spawn {
            Some(spawn) => spawn,
            None => {
                let vantage = Vantage::from_bsp(bsp)?;
                (vantage.origin, vantage.yaw)
            }
        };
        Some(Self {
            origin: floor_under(bsp, origin),
            yaw,
        })
    }
}

/// How far past the far doorway's plane, toward the menu world, the
/// destination is still drawn so the two floors meet without a seam.
const CLIP_OVERLAP: f32 = 1.0;

/// Where the eye is taken to be above a doorway's floor for visibility.
const EYE_ABOVE_FLOOR: f32 = 56.0;

/// How far below a spawn or view point the floor is looked for.
const FLOOR_REACH: f32 = 512.0;

/// `point` dropped onto the world floor beneath it (unchanged when nothing
/// solid lies within reach).
fn floor_under(bsp: &jkr_bsp::Bsp, point: Vec3) -> Vec3 {
    const CONTENTS_SOLID: u32 = 1;
    let end = point - Vec3::Z * FLOOR_REACH;
    let trace = bsp.trace_box(
        point.to_array(),
        end.to_array(),
        jkr_bsp::Aabb::POINT,
        CONTENTS_SOLID,
    );
    if trace.start_solid || trace.fraction >= 1.0 {
        point
    } else {
        Vec3::from_array(trace.end_position)
    }
}

/// A free camera pose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Camera {
    pub(crate) position: Vec3,
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
}

impl Camera {
    /// The pose in the destination world that sees, through `beyond`, what
    /// this camera sees through `doorway`: the same pose relative to the
    /// far frame as this one has to the near frame.
    pub(crate) fn through(self, doorway: Frame, beyond: Frame) -> Self {
        let turn = beyond.yaw - doorway.yaw;
        let (sin, cos) = turn.sin_cos();
        let local = self.position - doorway.origin;
        let rotated = Vec3::new(
            local.x * cos - local.y * sin,
            local.x * sin + local.y * cos,
            local.z,
        );
        Self {
            position: beyond.origin + rotated,
            yaw: self.yaw + turn,
            pitch: self.pitch,
        }
    }

    /// Whether the camera stands on the far side of `doorway`'s plane.
    pub(crate) fn is_past(self, doorway: Frame) -> bool {
        let (sin, cos) = doorway.yaw.sin_cos();
        let local = self.position - doorway.origin;
        local.x * cos + local.y * sin >= 0.0
    }
}

/// What the portal pass left on the frame, and so how the menu world draws
/// over it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum View {
    /// Nothing: the menu world clears the frame and draws its sky.
    Absent,
    /// The destination, seen through the doorway: the menu world draws over
    /// it without clearing colour or drawing its sky.
    Behind,
    /// The camera is through the doorway: the destination alone is on
    /// screen and the menu world is not drawn.
    Inside,
}

/// The preview of the map the client is joining, in whatever state its
/// load is in.
pub(crate) struct Destination {
    session_feed: Option<i32>,
    /// `maps/<map>.bsp` of the preview in flight or ready.
    map: Option<String>,
    load: Option<WorldLoadTask>,
    install: Option<WorldInstallTask>,
    world: Option<Box<GpuState>>,
    /// The doorway in the destination: its intermission vantage.
    frame: Frame,
    failed: bool,
}

impl Destination {
    pub(crate) const fn new() -> Self {
        Self {
            session_feed: None,
            map: None,
            load: None,
            install: None,
            world: None,
            frame: Frame {
                origin: Vec3::ZERO,
                yaw: 0.0,
            },
            failed: false,
        }
    }

    /// Point the preview at `map` (`None` ends it). A different map than the
    /// one in flight restarts the load.
    pub(crate) fn aim(&mut self, map: Option<&str>, vfs: Option<&Arc<jkr_vfs::VirtualFileSystem>>) {
        if self.map.as_deref() == map {
            return;
        }
        crate::log::progress(format_args!(
            "portal: aimed at {map:?} (was {:?})",
            self.map.as_deref()
        ));
        *self = Self::new();
        if let (Some(map), Some(vfs)) = (map, vfs) {
            self.map = Some(map.to_owned());
            self.load = Some(WorldLoadTask::start(Arc::clone(vfs), map.to_owned()));
        }
    }

    /// Replace the pre-join preview when the actual session identifies its content.
    pub(crate) fn aim_session(&mut self, map: &str, root: &Path, game: &jkr_protocol::GameState) {
        if self.map.as_deref() == Some(map) && self.session_feed == Some(game.checksum_feed) {
            return;
        }
        *self = Self::new();
        self.map = Some(map.to_owned());
        self.session_feed = Some(game.checksum_feed);
        match crate::assets::session_content::Selection::from_game(game) {
            Ok(selection) => {
                self.load = Some(WorldLoadTask::start_session(
                    root.to_owned(),
                    map.to_owned(),
                    selection,
                ))
            }
            Err(error) => self.fail(&error.to_string()),
        }
    }

    /// Advance the load: parse on one worker, then build the world against
    /// the shared context on another.
    pub(crate) fn poll(&mut self, context: &Arc<Context>, size: [u32; 2], game_data: &Path) {
        if let Some(task) = &self.load {
            match task.poll() {
                WorldLoadPoll::Pending => {}
                WorldLoadPoll::Failed(error) => self.fail(&error),
                WorldLoadPoll::Ready(loaded) => {
                    self.load = None;
                    self.frame = Frame::from_bsp(&loaded.bsp).unwrap_or(self.frame);
                    let bounds = loaded.bsp.render().models()[0].clone();
                    let input = GpuWorldInput {
                        scene: loaded.scene,
                        bsp: loaded.bsp,

                        vfs: loaded.vfs,
                        shaders: loaded.shaders,
                        world_minimums: bounds.minimums,
                        world_maximums: bounds.maximums,
                        camera_origin: self.frame.origin.to_array(),
                        camera_yaw: self.frame.yaw,
                        player_preview: None,
                        live_session: None,
                        demo_session: None,
                        build_game_state: None,
                        build_snapshot: None,
                        console: None,
                        client_menu: None,
                        game_data: game_data.to_path_buf(),
                        connect_timeline: None,
                        game_fonts: false,

                        completed_map_changes: 0,
                    };
                    self.install = Some(WorldInstallTask::start(
                        Arc::clone(context),
                        size,
                        input,
                        loaded.map_path,
                    ));
                }
            }
        }
        if let Some(task) = &mut self.install {
            match task.poll() {
                WorldInstallPoll::Pending => {}
                WorldInstallPoll::Failed(error) => self.fail(&error),
                WorldInstallPoll::Ready(world) => {
                    crate::log::progress(format_args!(
                        "portal: {} ready in {:.1} ms",
                        task.map_path,
                        task.started.elapsed().as_secs_f64() * 1_000.0
                    ));
                    self.install = None;
                    self.world = Some(Box::new(world));
                }
            }
        }
    }

    fn fail(&mut self, error: &str) {
        crate::log::progress(format_args!("portal: preview failed: {error}"));
        self.load = None;
        self.install = None;
        self.failed = true;
    }

    /// Whether the gate may open: the preview is drawable, or it will never
    /// be and the connection should not wait for it.
    pub(crate) fn ready(&self) -> bool {
        self.world.is_some() || self.failed
    }
}
