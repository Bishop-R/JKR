//! Retained actor geometry and animation state.
use super::*;

/// Map-lifetime actor mesh ranges and reusable pose storage.
pub(crate) struct ActorMesh {
    pub(crate) entity_id: Option<EntityId>,
    pub(crate) corpse_pool: bool,
    /// A copy kept for cut limbs ([`crate::actor_dismember`]).
    pub(crate) limb_pool: bool,
    pub(crate) body_identity: Option<jkr_client::BodyIdentity>,
    pub(crate) appearance: Appearance,
    pub(crate) preview: PlayerPreview,
    pub(crate) draws: Vec<ActorDraw>,
    pub(crate) vertex_ranges: Vec<PreviewVertexRange>,
    pub(crate) current_frames: (usize, usize),
    pub(crate) weapon_attachments: [Option<saber::Attachment>; 2],
    /// A vehicle's driver bolt in this frame's pose, model space.
    pub(crate) driver_seat: Option<[f32; 3]>,
    pub(crate) saber_names: [Option<String>; 2],
    pub(crate) angle_controller: jkr_client::LegacyPlayerAngleController,
    pub(crate) render_yaw_degrees: Option<f32>,
    pub(crate) audio_events: crate::actor_pose::sounds::State,
    pub(crate) animator: crate::actor_pose::evaluation::Slot,
    pub(crate) pose_vertices: Vec<GpuVertex>,
    /// Optional render-only joint upload; CPU trace posing never depends on this buffer.
    pub(crate) gpu_palette: Option<crate::actor_pose::gpu_skinning::Palette>,
    /// This frame's CPU skinning result, shared with visual point queries after upload.
    pub(crate) retained_pose: crate::actor_pose::RetainedPose,
    pub(crate) force_bones: crate::actor_pose::ForceBones,
    /// A disintegration as this client saw it begin (`EF_DISINTEGRATION`).
    pub(crate) dissolve: crate::actor_dissolve::Dissolve,
    /// A corpse's fade as this client saw it begin (`EV_BODYFADE`).
    pub(crate) fade: crate::corpse_fade::CorpseFade,
    /// The Ghoul2 damage marks it carries, and where it was drawn this frame.
    pub(crate) marks: crate::actor_marks::Marks,
    /// The draw of `head_eyes_mouth`, the inside of the face a corpse's fade switches off.
    pub(crate) mouth_draw: Option<usize>,
    /// Its surface switches (caps, cut limbs) and the draws they hide.
    pub(crate) surfaces: crate::actor_surfaces::Surfaces,
    /// What cuts did to it: limbs lost, or the limb it is.
    pub(crate) cuts: crate::actor_dismember::Cuts,
    /// How far a player who mind-tricks the viewer has faded from its sight.
    pub(crate) trick: jkr_client::trick_fade::TrickFade,
    /// How much of the model this frame draws, for its trick.
    pub(crate) body: jkr_client::trick_fade::BodyDraw,
}
