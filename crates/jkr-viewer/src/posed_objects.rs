//! General Ghoul2 entities drawn posed, as `CG_General` poses them (`cg_ents.c:862-1415`): a
//! turret's or an E-Web's cannon turned by the server's bone angles (`:989-992`), the root
//! playing the frames the server names under `EF_G2ANIMATING` (`:994-1011`), and otherwise
//! the skeleton's first frame, which is what the renderer draws of a Ghoul2 model nothing
//! has animated (`tr_ghoul2.cpp:1993-2000`).
//!
//! Such entities carry small rigid machines whose vertices each follow one bone, or one fixed
//! blend of bones. At load each model is split into rigid parts, one object mesh per weight
//! set, placed right after the model's own mesh; each frame a posed entity's skeleton is
//! evaluated once and every part is drawn as an instance under the matrix its weights make
//! (exact for one bone; for a blend, that blend's rotation). A model with a triangle across
//! two weight sets cannot be cut this way and keeps drawing unposed, as does one whose
//! animation file is missing.
//!
//! Every part is lit and fogged at its entity's origin, as the renderer lights a whole model
//! (`CG_General` sets no `RF_LIGHTING_ORIGIN`).

use super::*;
use crate::actor_instance::EntityLight;
use crate::actor_pose::server_bone_angles;
use jkr_model::{
    BoneAngleCommand, BoneAngleMode, BoneAnimationCommand, BoneAxis, BoneOverridePose,
    OverrideEndBehavior,
};
use jkr_protocol::EntityState;

#[path = "posed_sentry.rs"]
mod sentry;
pub(crate) use sentry::Flash;

/// The weights a Ghoul2 vertex carries at most (`iMAX_G2_BONEWEIGHTS_PER_VERT`).
const MAX_WEIGHTS: usize = 4;
/// `EF_G2ANIMATING` (`bg_public.h:626`).
const EF_G2ANIMATING: u32 = 1 << 0;
/// The bone `EF_G2ANIMATING` plays its frames on (`cg_ents.c:1003`).
const ROOT_BONE: &str = "model_root";
/// `G2API_SetBoneAnim(..., BONE_ANIM_OVERRIDE_FREEZE | BONE_ANIM_BLEND, 1.0f, cg.time, -1,
/// 100)`: a frame a 50 ms, cross-faded over 100 ms.
const ANIMATION_SPEED: f32 = 1.0;
const ANIMATION_BLEND_MILLIS: u16 = 100;
/// The posed entities followed at once; one beyond them is drawn unposed.
const MAX_POSED: usize = 128;
/// `MAX_CLIENTS`: entities below it are players.
const MAX_CLIENTS: u16 = 32;
/// `activeForcePass` of a manned emplaced gun: `NUM_FORCE_POWERS + 1`.
const MANNED: u8 = 19;
/// `CLASS_VEHICLE` (`teams.h`).
const CLASS_VEHICLE: u8 = 53;
/// The bone an emplaced gun aims with (`cg_ents.c:1344`).
const AIM_BONE: &str = "Bone02";
/// The bone a sentry turns (`cg_turret.c:254`).
const HINGE_BONE: &str = "bone_hinge";
/// The highest an emplaced gun pitches (down is positive).
const AIM_PITCH_LIMIT: f32 = 40.0;
/// The renderer's root matrix under every Ghoul2 skeleton, a quarter turn about Z
/// (`tr_ghoul2.cpp:130-137`'s `identityMatrix`). A posed skeleton includes it, so a
/// replacing bone turn is absolute in its space, as Ghoul2's are.
const GHOUL2_ROOT: [[f32; 4]; 3] = [
    [0.0, -1.0, 0.0, 0.0],
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
];

/// A Ghoul2 model split into rigid parts: its skeleton and, for each part, the bones its
/// vertices follow. Part `i` is drawn as the object mesh `i + 1` after the model's own.
pub(crate) struct PosedModel {
    /// The model itself, for its surface bolts.
    glm: Glm,
    animation: Gla,
    /// `model_root`, where `EF_G2ANIMATING` plays; `None` for a skeleton without one.
    root: Option<usize>,
    /// `Bone02`, which a manned emplaced gun aims with.
    aim: Option<usize>,
    /// `bone_hinge`, which a sentry turns.
    hinge: Option<usize>,
    parts: Vec<Weights>,
}

/// The bones one rigid part follows and their weights' bits, sorted by bone; an entry of
/// [`Weights::NONE`] is unused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Weights([(u16, u32); MAX_WEIGHTS]);

impl Weights {
    const NONE: (u16, u32) = (u16::MAX, 0);

    /// `vertex`'s weights in `surface`; `None` for more than a Ghoul2 vertex carries or a
    /// bone beyond the skeleton's `bone_count`.
    fn of(
        surface: &jkr_model::GlmSurface,
        vertex: &jkr_model::GlmVertex,
        bone_count: usize,
    ) -> Option<Self> {
        let mut weights = [Self::NONE; MAX_WEIGHTS];
        let mut count = 0;
        for weight in vertex.weights.iter().filter(|weight| weight.weight != 0.0) {
            let bone = *surface.bone_references.get(weight.bone_reference)?;
            if bone >= bone_count {
                return None;
            }
            *weights.get_mut(count)? = (u16::try_from(bone).ok()?, weight.weight.to_bits());
            count += 1;
        }
        weights.sort_unstable();
        Some(Self(weights))
    }

    /// The part's matrix in `matrices`: its bones' skinning matrices, weighed.
    fn matrix(self, matrices: &[[[f32; 4]; 3]]) -> [[f32; 4]; 3] {
        let mut blended = [[0.0; 4]; 3];
        for (bone, bits) in self.0.into_iter().filter(|entry| *entry != Self::NONE) {
            let weight = f32::from_bits(bits);
            let matrix = matrices[usize::from(bone)];
            for (row, source) in blended.iter_mut().zip(matrix) {
                for (value, source) in row.iter_mut().zip(source) {
                    *value += weight * source;
                }
            }
        }
        blended
    }
}

/// `model` (its bind mesh drawn as `whole`) split into rigid parts appended to `scene`, with
/// the skeleton that moves them: the posed model and one object mesh per part, in part order.
/// `None` for a model that has no skeleton of its own, whose skeleton does not fit it, or
/// one of whose triangles spans two weight sets.
pub(crate) fn split(
    vfs: &VirtualFileSystem,
    model: &Glm,
    appearance: &Appearance,
    whole: &[ActorDraw],
    scene: &mut FlattenedScene,
) -> Option<(PosedModel, Vec<StaticModelMesh>)> {
    // `*default`: the engine's one-bone skeleton, nothing a server or a client turns.
    if model.animation_name.starts_with('*') {
        return None;
    }
    let path = format!("{}.gla", model.animation_name);
    let animation = Gla::parse(&vfs.read(&path).ok()??.bytes).ok()?;
    if animation.bones.len() != model.bone_count {
        eprintln!(
            "{}: skeleton {path} does not fit it; drawn unposed",
            appearance.model
        );
        return None;
    }
    cut(model, animation, appearance, whole, scene)
}

/// [`split`] once the skeleton is read: the parts cut along `model`'s weights.
fn cut(
    model: &Glm,
    animation: Gla,
    appearance: &Appearance,
    whole: &[ActorDraw],
    scene: &mut FlattenedScene,
) -> Option<(PosedModel, Vec<StaticModelMesh>)> {
    // The surfaces `append_static_glm_mesh` drew, in its order: one draw each.
    let lod = model.lods.first()?;
    let drawn = lod
        .surfaces
        .iter()
        .zip(&model.hierarchy)
        .filter(|(_, hierarchy)| !hierarchy.name.starts_with('*') && hierarchy.flags & 0x2 == 0)
        .map(|(surface, _)| surface)
        .collect::<Vec<_>>();
    if drawn.len() != whole.len() {
        return None;
    }
    let mut parts = Vec::<Weights>::new();
    let mut vertex_parts = Vec::with_capacity(drawn.len());
    for surface in &drawn {
        let mut of_vertex = Vec::with_capacity(surface.vertices.len());
        for vertex in &surface.vertices {
            let weights = Weights::of(surface, vertex, model.bone_count)?;
            let part = match parts.iter().position(|known| *known == weights) {
                Some(part) => part,
                None => {
                    parts.push(weights);
                    parts.len() - 1
                }
            };
            of_vertex.push(part);
        }
        if surface.triangles.iter().any(|triangle| {
            let part = of_vertex[triangle[0] as usize];
            triangle[1..]
                .iter()
                .any(|&corner| of_vertex[corner as usize] != part)
        }) {
            eprintln!(
                "{}: a triangle spans two bone weightings; drawn unposed",
                appearance.model
            );
            return None;
        }
        vertex_parts.push(of_vertex);
    }
    let mut meshes = Vec::with_capacity(parts.len());
    for part in 0..parts.len() {
        let vertex_start = scene.vertices.len();
        let mut draws = Vec::new();
        for ((surface, of_vertex), draw) in drawn.iter().zip(&vertex_parts).zip(whole) {
            let index_start = u32::try_from(scene.indices.len()).ok()?;
            let mut placed = vec![u32::MAX; surface.vertices.len()];
            for triangle in surface
                .triangles
                .iter()
                .filter(|triangle| of_vertex[triangle[0] as usize] == part)
            {
                for &corner in triangle {
                    let corner = corner as usize;
                    if placed[corner] == u32::MAX {
                        placed[corner] = u32::try_from(scene.vertices.len()).ok()?;
                        let vertex = &surface.vertices[corner];
                        scene.vertices.push(GpuVertex {
                            position: vertex.position,
                            normal: vertex.normal,
                            color: [1.0; 4],
                            texture_coordinates: vertex.texture_coordinates,
                            lightmap_coordinates: [0.0; 2],
                        });
                    }
                    scene.indices.push(placed[corner]);
                }
            }
            let index_end = u32::try_from(scene.indices.len()).ok()?;
            if index_end > index_start {
                draws.push(ActorDraw {
                    indices: index_start..index_end,
                    material: draw.material,
                });
            }
        }
        meshes.push(StaticModelMesh {
            // Never an entity's appearance: no lookup by model finds a part instead of its
            // model.
            appearance: Appearance {
                model: appearance.model.clone(),
                variant: format!("#part{part}"),
            },
            draws,
            center: mesh_center(&scene.vertices[vertex_start..]),
            flash_bolt: None,
            ghoul2: true,
            posed: None,
        });
    }
    let find = |name: &str| {
        animation
            .bones
            .iter()
            .position(|bone| bone.name.eq_ignore_ascii_case(name))
    };
    let (root, aim, hinge) = (find(ROOT_BONE), find(AIM_BONE), find(HINGE_BONE));
    Some((
        PosedModel {
            glm: model.clone(),
            animation,
            root,
            aim,
            hinge,
            parts,
        },
        meshes,
    ))
}

/// Whether `mesh` is a rigid part of a posed model rather than a model of its own.
pub(crate) fn is_part(mesh: &StaticModelMesh) -> bool {
    mesh.appearance.variant.starts_with("#part")
}

/// The gunner of the manned emplaced gun `state` is, if it is one (`CG_General`,
/// `cg_ents.c:1319-1321`): an entity past the players marked by `activeForcePass`, not a
/// vehicle.
pub(crate) fn gunner(state: &EntityState) -> Option<u16> {
    (state.number() >= MAX_CLIENTS
        && state.active_force_pass() == MANNED
        && state.npc_class() != CLASS_VEHICLE)
        .then(|| u16::try_from(state.emplaced_owner()).ok())
        .flatten()
}

/// The state `snapshot` holds for world entity `entity` (its number plus one).
pub(crate) fn entity_state(snapshot: Option<&Snapshot>, entity: u64) -> Option<&EntityState> {
    let number = u16::try_from(entity.checked_sub(1)?).ok()?;
    let snapshot = snapshot?;
    let index = snapshot
        .entities
        .binary_search_by_key(&number, EntityState::number)
        .ok()?;
    snapshot.entities.get(index)
}

/// One posed entity's draw this frame.
pub(crate) struct Draw<'a> {
    /// The object mesh of the entity's model; its parts follow it.
    pub(crate) mesh: usize,
    pub(crate) model: &'a PosedModel,
    /// The world entity (its number plus one).
    pub(crate) entity: u64,
    pub(crate) state: Option<&'a EntityState>,
    pub(crate) game_state: Option<&'a GameState>,
    pub(crate) transform: jkr_runtime::Transform,
    pub(crate) view_flags: u32,
    /// `cg.time`.
    pub(crate) time: i64,
    /// For a manned emplaced gun, where its gunner looks: the local view in first person,
    /// otherwise the gunner's presented angles (`cg_ents.c:1323-1334`).
    pub(crate) gunner_view: Option<[f32; 3]>,
    /// For a sentry (`bolt1`) with an enemy, where that enemy stands.
    pub(crate) sentry_enemy: Option<[f32; 3]>,
}

/// The skeletons of the posed entities, one per entity, kept while it is drawn: what
/// `cent->ghoul2` keeps for each of them.
#[derive(Default)]
pub(crate) struct Poses {
    slots: Vec<Slot>,
    /// This frame's part instances, to be lit at their entity's origin.
    lit: Vec<Lit>,
    frame: u32,
}

struct Slot {
    entity: u64,
    mesh: usize,
    pose: BoneOverridePose,
    /// `(torsoAnim, legsAnim, torsoFlip)` last played (`cent->pe`'s copy).
    played: (u16, u16, bool),
    /// The frame it was last drawn.
    seen: u32,
    /// A sentry's own turning, once it has been seen.
    sentry: Option<sentry::Sentry>,
}

/// What a posed entity's draw asks of its caller besides its instances.
pub(crate) struct Drawn {
    /// A sentry's muzzle flash, played once for a shot.
    pub(crate) flash: Option<Flash>,
}

/// One part instance: the object mesh's group, its place there, its entity's origin.
#[derive(Clone, Copy)]
struct Lit {
    group: usize,
    instance: usize,
    origin: [f32; 3],
}

impl Poses {
    /// Start a frame's draws.
    pub(crate) fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        self.lit.clear();
    }

    /// Pose `draw`'s entity as its state asks and push an instance of every part into
    /// `groups`; what else the draw asks of the caller, or `None` when no skeleton is free for
    /// it (it is drawn unposed then).
    pub(crate) fn submit(
        &mut self,
        draw: Draw<'_>,
        groups: &mut [Vec<ActorInstance>],
    ) -> Option<Drawn> {
        let frame = self.frame;
        let slot = slot(&mut self.slots, frame, draw.entity, draw.mesh, draw.model)?;
        let mut flash = None;
        slot.seen = frame;
        let model = draw.model;
        let animation = &model.animation;
        if let (Some(state), Some(game_state)) = (draw.state, draw.game_state) {
            if state.bone_orient() != 0 {
                for (bone, command) in server_bone_angles::turns(state, game_state, animation) {
                    // A bone of the skeleton always takes a command.
                    let _ = slot.pose.set_bone_angles(animation, bone, command);
                }
            }
            if state.e_flags() & EF_G2ANIMATING != 0 {
                animate(slot, model, state, draw.time);
            }
            if let (Some(view), Some(bone)) = (draw.gunner_view, model.aim) {
                let command = aim(view, state.angles()[1]);
                // A bone of the skeleton always takes a command.
                let _ = slot.pose.set_bone_angles(animation, bone, command);
            }
            // `CG_General` hands a sentry to `TurretClientRun` (`cg_ents.c:1409-1412`).
            if state.bolt1() {
                let origin = draw.transform.translation;
                let (pose, enemy) = (&mut slot.pose, draw.sentry_enemy);
                flash = sentry::run(
                    &mut slot.sentry,
                    pose,
                    model,
                    state,
                    origin,
                    enemy,
                    draw.time,
                );
            }
        }
        let matrices = slot.pose.evaluate(animation, draw.time).ok()?;
        for (index, weights) in model.parts.iter().enumerate() {
            let (position, rotation) = place(&draw.transform, weights.matrix(matrices));
            let mut instance = ActorInstance::new(position, rotation, draw.transform.scale);
            instance.view_flags = draw.view_flags;
            let group = draw.mesh + 1 + index;
            let Some(instances) = groups.get_mut(group) else {
                continue;
            };
            self.lit.push(Lit {
                group,
                instance: instances.len(),
                origin: draw.transform.translation,
            });
            instances.push(instance);
        }
        Some(Drawn { flash })
    }

    /// Light and fog this frame's parts at their entity's origin, replacing what the entity
    /// lighting found at their own; the change in fogged instances.
    pub(crate) fn relight(
        &self,
        groups: &mut [Vec<ActorInstance>],
        light: impl Fn([f32; 3]) -> EntityLight,
        fog: impl Fn([f32; 3]) -> u32,
    ) -> isize {
        let mut fogged = 0;
        let mut last: Option<([f32; 3], EntityLight, u32)> = None;
        for lit in &self.lit {
            let (light, fog_index) = match last {
                Some((origin, light, fog_index)) if origin == lit.origin => (light, fog_index),
                _ => {
                    let sampled = (light(lit.origin), fog(lit.origin));
                    last = Some((lit.origin, sampled.0, sampled.1));
                    sampled
                }
            };
            let Some(instance) = groups
                .get_mut(lit.group)
                .and_then(|group| group.get_mut(lit.instance))
            else {
                continue;
            };
            fogged += isize::from(fog_index != 0) - isize::from(instance.fog_index != 0);
            instance.set_light(light);
            instance.fog_index = fog_index;
        }
        fogged
    }
}

/// Where a part posed as `matrix` (the skeleton's space, Ghoul2's root included) is drawn
/// for an entity at `transform`: the instance's position and rotation, for the renderer's
/// `origin + rotation * (scale * v)`. That is the entity's turn over the part's matrix, as
/// the renderer places a Ghoul2 vertex; exact for a uniform scale and a rigid matrix, and for
/// a blend of bones the blend's rotation.
fn place(transform: &jkr_runtime::Transform, matrix: [[f32; 4]; 3]) -> ([f32; 3], [f32; 4]) {
    let rotation = Quat::from_array(transform.rotation);
    let part_rotation = Quat::from_mat3(&glam::Mat3::from_cols(
        Vec3::new(matrix[0][0], matrix[1][0], matrix[2][0]),
        Vec3::new(matrix[0][1], matrix[1][1], matrix[2][1]),
        Vec3::new(matrix[0][2], matrix[1][2], matrix[2][2]),
    ))
    .normalize();
    let offset = Vec3::new(matrix[0][3], matrix[1][3], matrix[2][3]);
    let position = Vec3::from_array(transform.translation)
        + rotation * (Vec3::from_array(transform.scale) * offset);
    (
        position.to_array(),
        (rotation * part_rotation).normalize().to_array(),
    )
}

/// `entity`'s skeleton among `slots`, kept from the frames it was drawn in; a fresh one for
/// an entity not followed yet, or one whose model changed, in the slot longest unused.
fn slot<'a>(
    slots: &'a mut Vec<Slot>,
    frame: u32,
    entity: u64,
    mesh: usize,
    model: &PosedModel,
) -> Option<&'a mut Slot> {
    let index = match slots.iter().position(|slot| slot.entity == entity) {
        Some(index) if slots[index].mesh == mesh => return slots.get_mut(index),
        Some(index) => index,
        None if slots.len() < MAX_POSED => {
            slots.push(Slot::new(entity, mesh, model));
            return slots.last_mut();
        }
        // One not drawn this frame or the last.
        None => slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| frame.wrapping_sub(slot.seen) >= 2)
            .max_by_key(|(_, slot)| frame.wrapping_sub(slot.seen))
            .map(|(index, _)| index)?,
    };
    slots[index] = Slot::new(entity, mesh, model);
    slots.get_mut(index)
}

impl Slot {
    fn new(entity: u64, mesh: usize, model: &PosedModel) -> Self {
        Self {
            entity,
            mesh,
            pose: BoneOverridePose::new(model.animation.bones.len()).with_root(GHOUL2_ROOT),
            played: (0, 0, false),
            seen: 0,
            sentry: None,
        }
    }
}

/// An emplaced gun's aim (`cg_ents.c:1336-1344`): its gunner's `view`, the pitch held
/// to 40 down and the yaw taken from the gun's own `gun_yaw`, as the absolute turn of
/// `Bone02` (`BONE_ANGLES_REPLACE` about -Y up, -X left, +Z forward).
fn aim(view: [f32; 3], gun_yaw: f32) -> BoneAngleCommand {
    BoneAngleCommand {
        angles_degrees: [view[0].min(AIM_PITCH_LIMIT), view[1] - gun_yaw, view[2]],
        mode: BoneAngleMode::Replace,
        up: BoneAxis::NegativeY,
        left: BoneAxis::NegativeX,
        forward: BoneAxis::PositiveZ,
    }
}

/// `CG_General`'s mini-animation (`cg_ents.c:994-1011`): on a change of `torsoAnim`,
/// `legsAnim` or `torsoFlip`, the root plays from the first to the second and holds there.
/// `G2API_SetBoneAnim` takes an end frame of zero as one (`G2_API.cpp:1282-1285`); a range
/// beyond the skeleton's frames installs nothing.
fn animate(slot: &mut Slot, model: &PosedModel, state: &EntityState, time: i64) {
    let played = (
        state.torso_animation(),
        state.leg_animation(),
        state.torso_flip(),
    );
    if played == slot.played {
        return;
    }
    slot.played = played;
    let Some(root) = model.root else {
        return;
    };
    let _ = slot.pose.set_bone_animation(
        &model.animation,
        root,
        BoneAnimationCommand {
            clip: usize::MAX,
            start_frame: i32::from(played.0),
            end_frame: i32::from(played.1.max(1)),
            speed: ANIMATION_SPEED,
            time_millis: time,
            set_frame: None,
            end_behavior: OverrideEndBehavior::Freeze,
            blend: true,
            blend_millis: ANIMATION_BLEND_MILLIS,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use jkr_model::{GlaBone, GlmLod, GlmSurface, GlmSurfaceHierarchy, GlmVertex, GlmWeight};

    const IDENTITY: [[f32; 4]; 3] = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
    ];

    fn bone(name: &str, parent: Option<usize>) -> GlaBone {
        GlaBone {
            name: name.to_owned(),
            flags: 0,
            parent,
            base_pose: IDENTITY,
            inverse_base_pose: IDENTITY,
            children: Vec::new(),
        }
    }

    /// A root with a cannon and a leg on it.
    fn skeleton() -> Gla {
        Gla {
            name: "test".to_owned(),
            scale: 1.0,
            bones: vec![
                bone("model_root", None),
                bone("cannon", Some(0)),
                bone("leg", Some(0)),
            ],
            frames: Vec::new(),
            compressed_bones: Vec::new(),
        }
    }

    /// `weights` name the surface's bone references: 0 the cannon, 1 the leg.
    fn vertex(position: [f32; 3], weights: &[(usize, f32)]) -> GlmVertex {
        GlmVertex {
            normal: [0.0, 0.0, 1.0],
            position,
            texture_coordinates: [0.0; 2],
            weights: weights
                .iter()
                .map(|&(bone_reference, weight)| GlmWeight {
                    bone_reference,
                    weight,
                })
                .collect(),
        }
    }

    /// One surface: a triangle on the cannon, one shared half and half by the cannon and the
    /// leg; `across` gives the second one a corner on the cannon alone.
    fn model(across: bool) -> Glm {
        let shared = [(0, 0.5), (1, 0.5)];
        let first: &[(usize, f32)] = if across { &[(0, 1.0)] } else { &shared };
        Glm {
            name: "test".to_owned(),
            animation_name: "test".to_owned(),
            bone_count: 3,
            hierarchy: vec![GlmSurfaceHierarchy {
                name: "body".to_owned(),
                flags: 0,
                shader: "textures/test".to_owned(),
                parent: None,
                children: Vec::new(),
            }],
            lods: vec![GlmLod {
                surfaces: vec![GlmSurface {
                    hierarchy_index: 0,
                    vertices: vec![
                        vertex([0.0, 0.0, 0.0], &[(0, 1.0)]),
                        vertex([1.0, 0.0, 0.0], &[(0, 1.0)]),
                        vertex([0.0, 1.0, 0.0], &[(0, 1.0)]),
                        vertex([0.0, 0.0, 2.0], first),
                        vertex([1.0, 0.0, 2.0], &shared),
                        vertex([0.0, 1.0, 2.0], &shared),
                    ],
                    triangles: vec![[0, 1, 2], [3, 4, 5]],
                    bone_references: vec![1, 2],
                }],
            }],
        }
    }

    fn appearance() -> Appearance {
        Appearance {
            model: "models/test.glm".to_owned(),
            variant: String::new(),
        }
    }

    #[test]
    fn a_model_is_cut_along_its_weights() {
        let model = model(false);
        let mut scene = FlattenedScene::default();
        let whole = append_static_glm_mesh(&mut scene, &model).unwrap();
        let before = scene.vertices.len();
        let (posed, meshes) = cut(&model, skeleton(), &appearance(), &whole, &mut scene).unwrap();
        assert_eq!(posed.root, Some(0));
        assert_eq!(posed.parts.len(), 2);
        assert_eq!(meshes.len(), 2);
        // Every part has its own three corners, its triangle and the model's material.
        assert_eq!(scene.vertices.len(), before + 6);
        for mesh in &meshes {
            assert!(is_part(mesh));
            assert_eq!(mesh.draws.len(), 1);
            assert_eq!(mesh.draws[0].indices.end - mesh.draws[0].indices.start, 3);
            assert_eq!(mesh.draws[0].material, whole[0].material);
        }
        assert_eq!(posed.parts[0].0[0], (1, 1.0f32.to_bits()));
        assert_eq!(
            posed.parts[1].0[..2],
            [(1, 0.5f32.to_bits()), (2, 0.5f32.to_bits())]
        );
        // A part's corners stay where the model has them.
        let corner = scene.indices[meshes[1].draws[0].indices.start as usize] as usize;
        assert_eq!(scene.vertices[corner].position, [0.0, 0.0, 2.0]);
    }

    #[test]
    fn a_triangle_across_two_weightings_leaves_the_model_whole() {
        let model = model(true);
        let mut scene = FlattenedScene::default();
        let whole = append_static_glm_mesh(&mut scene, &model).unwrap();
        let before = (scene.vertices.len(), scene.indices.len());
        let parts = cut(&model, skeleton(), &appearance(), &whole, &mut scene);
        assert!(parts.is_none());
        assert_eq!((scene.vertices.len(), scene.indices.len()), before);
    }

    #[test]
    fn a_blended_part_follows_its_bones_by_weight() {
        let mut matrices = vec![IDENTITY; 3];
        matrices[1][0][3] = 10.0;
        matrices[2][0][3] = 20.0;
        let weights = Weights([
            (1, 0.25f32.to_bits()),
            (2, 0.75f32.to_bits()),
            Weights::NONE,
            Weights::NONE,
        ]);
        let blended = weights.matrix(&matrices);
        assert_eq!(blended[0][3], 17.5);
        assert_eq!(blended[1][1], 1.0);
    }

    #[test]
    fn a_part_lands_where_the_renderer_puts_its_vertices() {
        // A part turned 30 degrees about Y and moved, on an entity turned 70 degrees, scaled.
        let turn = glam::Mat3::from_quat(Quat::from_rotation_y(30f32.to_radians()));
        let offset = Vec3::new(4.0, -7.0, 12.0);
        let matrix: [[f32; 4]; 3] = std::array::from_fn(|row| {
            [
                turn.x_axis[row],
                turn.y_axis[row],
                turn.z_axis[row],
                offset[row],
            ]
        });
        let transform = jkr_runtime::Transform {
            translation: [100.0, 50.0, -20.0],
            rotation: Quat::from_rotation_z(70f32.to_radians()).to_array(),
            scale: [1.25; 3],
        };
        let (position, rotation) = place(&transform, matrix);
        // The renderer: the entity's axes over the part's matrix, scaled in model space.
        let origin = Vec3::from_array(transform.translation);
        let entity = Quat::from_array(transform.rotation);
        for corner in [
            Vec3::ZERO,
            Vec3::new(10.0, 0.0, 0.0),
            Vec3::new(-3.0, 8.0, 5.0),
        ] {
            let expected = origin + entity * (1.25 * (turn * corner + offset));
            let drawn = Vec3::from_array(position) + Quat::from_array(rotation) * (1.25 * corner);
            assert!((drawn - expected).length() < 1e-3, "{drawn} != {expected}");
        }
    }
}
