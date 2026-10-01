//! First-person view weapon: the `_hand.md3` tag rig and the gun on it.
//!
//! `CG_AddViewWeapon` (`cg_weapons.c:782-905`) places the hand rig at the
//! view origin with the bobbed view angles, then `CG_AddPlayerWeapon`
//! (`:420-620`) hangs the item view model on the rig's `tag_weapon` and the
//! barrels on `tag_barrel*`, all with `RF_DEPTHHACK`. Retail hand rigs carry
//! no surfaces — only 15 frames of tags — so the gun and barrels are the
//! static frame-0 meshes already in `object_meshes` and only the tags lerp
//! per frame (`R_LerpTag`, `tr_model.cpp:1792-1821`). The policy (models,
//! frame mapping, bob) lives in `jkr_client::view_weapon`; this module is
//! the renderer glue.

use super::*;
#[path = "view_weapon_offsets.rs"]
mod offsets;
use glam::Mat3;
use jkr_client::{
    LegacyViewBobSample, LegacyViewWeaponAnimations, LegacyViewWeaponFrames, LegacyViewWeaponPose,
    legacy_angles_to_quaternion, legacy_view_weapon_frames, legacy_view_weapon_pose,
    legacy_view_weapon_visible, legacy_view_weapon_visible_as, legacy_view_weapons,
};
use jkr_protocol::Snapshot;

const WEAPON_SLOTS: usize = 19; // WP_NUM_WEAPONS, codemp/game/bg_weapons.h:60

/// One weapon's rig and the object meshes it positions.
struct Entry {
    hand: Md3,
    tag_weapon: usize,
    gun_mesh: usize,
    /// `(hand tag index, object mesh index)` per barrel.
    barrels: Vec<(usize, usize)>,
    /// The gun's frame-0 `tag_flash`, where the muzzle effect plays.
    flash: Option<Md3Tag>,
}

/// Load-time rigs per weapon number; `submit` is allocation-free.
#[derive(Default)]
pub(crate) struct Catalog {
    entries: Vec<Option<Entry>>,
    animations: Option<LegacyViewWeaponAnimations>,
    /// `cg_drawGun 0`: `CG_AddViewWeapon` skips the gun model but keeps the
    /// muzzle flash (`codemp/cgame/cg_weapons.c:811-820`).
    visible: bool,
}

impl Catalog {
    /// Add the gun and barrel models to the map-load appearance set.
    pub(crate) fn extend_appearances(appearances: &mut BTreeSet<Appearance>) {
        for (_, model) in legacy_view_weapons() {
            for path in std::iter::once(model.gun).chain(model.barrels.iter().map(|b| b.0)) {
                appearances.insert(Appearance {
                    model: path.to_owned(),
                    variant: String::new(),
                });
            }
        }
    }

    /// Parse the hand rigs and resolve the meshes; weapons whose assets are
    /// missing stay empty and simply draw nothing.
    pub(crate) fn build(
        vfs: &VirtualFileSystem,
        meshes: &[StaticModelMesh],
        humanoid: Option<&AnimationConfig>,
    ) -> Self {
        let mesh_index = |path: &str| {
            meshes
                .iter()
                .position(|mesh| mesh.appearance.model.eq_ignore_ascii_case(path))
        };
        let mut entries = Vec::with_capacity(WEAPON_SLOTS);
        entries.resize_with(WEAPON_SLOTS, || None);
        for (weapon, model) in legacy_view_weapons() {
            let entry = (|| {
                let asset = vfs.read(model.hand).ok()??;
                let hand = Md3::parse(&asset.bytes).ok()?;
                let tag_weapon = hand.tag_index("tag_weapon")?;
                let gun_mesh = mesh_index(model.gun)?;
                let flash = vfs
                    .read(model.gun)
                    .ok()
                    .flatten()
                    .and_then(|asset| Md3::parse(&asset.bytes).ok())
                    .and_then(|gun| Some(gun.tags.first()?[gun.tag_index("tag_flash")?].clone()));
                let barrels = model
                    .barrels
                    .iter()
                    .filter_map(|(path, tag)| Some((hand.tag_index(tag)?, mesh_index(path)?)))
                    .collect();
                Some(Entry {
                    hand,
                    tag_weapon,
                    gun_mesh,
                    barrels,
                    flash,
                })
            })();
            entries[usize::from(weapon)] = entry;
        }
        Self {
            entries,
            animations: humanoid.map(LegacyViewWeaponAnimations::new),
            visible: true,
        }
    }

    /// Latch `cg_drawGun` (retail default 1).
    pub(crate) fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }
}

/// Everything one frame needs, resolved from the snapshot up front.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Inputs {
    pub(crate) weapon: u8,
    pub(crate) frames: LegacyViewWeaponFrames,
    pub(crate) pose: LegacyViewWeaponPose,
}

/// First-person view of the local player: eye origin and look angles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct View {
    pub(crate) origin: Vec3,
    /// Radians; pitch is positive looking up, as the viewer camera stores it.
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
}

/// The local player's weapon state at the presented camera.
pub(crate) fn frame_inputs(
    state: &GpuState,
    snapshot: Option<&Snapshot>,
    camera: Option<first_person_view::Camera>,
    local_entity: Option<u64>,
    presentation_time: i64,
) -> Option<Inputs> {
    (|| {
        let view = camera.map_or(
            View {
                origin: state.camera_position,
                yaw: state.camera_yaw,
                pitch: state.camera_pitch,
            },
            |camera| camera.view(),
        );
        let predicted_weapon = state
            .live_session
            .is_some()
            .then(|| state.local_prediction.predicted_state())
            .flatten();
        inputs_with_weapon(
            &state.first_person_weapon,
            snapshot?,
            state.third_person,
            view,
            camera.map_or(0.0, |camera| camera.landing),
            local_torso_frame(&state.actor_meshes, local_entity, presentation_time),
            presentation_time,
            predicted_weapon,
        )
    })()
    .map(|mut inputs| {
        let (yaw, pitch) = camera.map_or((state.camera_yaw, state.camera_pitch), |camera| {
            (camera.yaw, camera.pitch)
        });
        inputs.pose.origin = offsets::apply(inputs.pose.origin, yaw, pitch, state.console.as_ref());
        inputs
    })
}

#[allow(clippy::too_many_arguments)]
fn inputs_with_weapon(
    catalog: &Catalog,
    snapshot: &Snapshot,
    third_person: bool,
    view: View,
    landing: f32,
    torso_frame: Option<f32>,
    presentation_time: i64,
    predicted_weapon: Option<&jkr_client::pmove::MovementState>,
) -> Option<Inputs> {
    let weapon = match predicted_weapon {
        Some(state) => legacy_view_weapon_visible_as(snapshot, third_person, state.weapon)?,
        None => legacy_view_weapon_visible(snapshot, third_person)?,
    };
    let state = &snapshot.player;
    let frames = match (&catalog.animations, torso_frame) {
        (Some(animations), Some(frame)) => {
            animations.frames(usize::from(state.torso_animation()), frame)
        }
        _ => legacy_view_weapon_frames("", 0, 0.0),
    };
    // Quake angles: positive pitch looks down (`AngleVectors`, `q_math.c`).
    let view_angles = [-view.pitch.to_degrees(), view.yaw.to_degrees(), 0.0];
    Some(Inputs {
        weapon,
        frames,
        pose: legacy_view_weapon_pose(
            view.origin.to_array(),
            view_angles,
            predicted_weapon.map_or_else(
                || LegacyViewBobSample::new(state.bob_cycle(), state.velocity()),
                |state| LegacyViewBobSample::new(state.bob_cycle, state.velocity),
            ),
            landing,
            presentation_time,
        ),
    })
}

/// Fractional torso frame of the local actor's posed mesh, for `inputs`.
pub(crate) fn local_torso_frame(
    meshes: &[ActorMesh],
    local_entity: Option<u64>,
    presentation_time: i64,
) -> Option<f32> {
    let mesh = meshes
        .iter()
        .find(|mesh| mesh.entity_id.map(|entity| entity.get()) == local_entity)?;
    mesh.animator
        .torso_frame(&mesh.preview.animation, presentation_time)
}

/// Push the gun and barrel instances for `inputs`; returns how many.
pub(crate) fn submit(
    catalog: &Catalog,
    inputs: Inputs,
    object_groups: &mut [Vec<ActorInstance>],
) -> usize {
    let Some(Some(entry)) = catalog.entries.get(usize::from(inputs.weapon)) else {
        return 0;
    };
    if !catalog.visible {
        return 0;
    }
    let (hand_origin, hand_rotation) = hand_frame(inputs);
    let frames = inputs.frames;
    let front_lerp = 1.0 - frames.back_lerp;
    let mut submitted = 0;
    for (tag, mesh) in
        std::iter::once((entry.tag_weapon, entry.gun_mesh)).chain(entry.barrels.iter().copied())
    {
        let tag = entry.hand.lerp_tag(
            tag,
            frames.old_frame as usize,
            frames.frame as usize,
            front_lerp,
        );
        let (origin, rotation) = position_on_tag(hand_origin, hand_rotation, &tag);
        let mut instance = ActorInstance::new(origin.to_array(), rotation.to_array(), [1.0; 3]);
        instance.depth_hack = 1.0;
        object_groups[mesh].push(instance);
        submitted += 1;
    }
    submitted
}

/// The muzzle socket of the gun placed by `inputs`: the gun's `tag_flash`
/// composed onto the hand's `tag_weapon` (`cg_weapons.c:576-579`, also
/// `cg.lastFPFlashPoint`). Tags are game-space, so no basis conversion.
pub(crate) fn flash_socket(catalog: &Catalog, inputs: Inputs) -> Option<muzzle_flash::Socket> {
    let Some(Some(entry)) = catalog.entries.get(usize::from(inputs.weapon)) else {
        return None;
    };
    let flash = entry.flash.as_ref()?;
    let (hand_origin, hand_rotation) = hand_frame(inputs);
    let frames = inputs.frames;
    let tag_weapon = entry.hand.lerp_tag(
        entry.tag_weapon,
        frames.old_frame as usize,
        frames.frame as usize,
        1.0 - frames.back_lerp,
    );
    let (gun_origin, gun_rotation) = position_on_tag(hand_origin, hand_rotation, &tag_weapon);
    let (origin, rotation) = position_on_tag(gun_origin, gun_rotation, flash);
    Some(muzzle_flash::Socket {
        origin: origin.to_array(),
        direction: (rotation * Vec3::X).to_array(),
    })
}

fn hand_frame(inputs: Inputs) -> (Vec3, Quat) {
    (
        Vec3::from_array(inputs.pose.origin),
        Quat::from_array(legacy_angles_to_quaternion(inputs.pose.angles)),
    )
}

/// `CG_PositionEntityOnTag` (`cg_ents.c:48-71`): the child origin is the
/// parent origin plus the tag origin along the parent axes, and the child
/// axes are the tag axes composed with the parent axes.
fn position_on_tag(parent_origin: Vec3, parent_rotation: Quat, tag: &Md3Tag) -> (Vec3, Quat) {
    let origin = parent_origin + parent_rotation * Vec3::from_array(tag.origin);
    let tag_rotation = Quat::from_mat3(&Mat3::from_cols(
        Vec3::from_array(tag.axes[0]),
        Vec3::from_array(tag.axes[1]),
        Vec3::from_array(tag.axes[2]),
    ));
    (origin, (parent_rotation * tag_rotation).normalize())
}
