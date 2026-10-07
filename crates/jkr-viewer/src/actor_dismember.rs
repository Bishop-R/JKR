//! Cut limbs on this client ([`jkr_client::dismember`]): each limb the server sends is
//! drawn from a copy of its owner's model playing on the owner's animation, the owner
//! loses the limb with all below it and shows the stump's cap, smoke rises from both cuts
//! and trails a flying limb every 400 ms, and an owner that lives again is whole
//! (`CG_ReattachLimb`). `cg_dismember` decides which cuts show (none by default).

use crate::{GpuState, effect_runtime};
use glam::Vec3;
use jkr_client::dismember::{self, Cut};
use jkr_game_jka::ghoul2_bolt::{model_bolt, world_bolt};
use jkr_model::g2_surfaces::SURFACE_NO_DESCENDANTS;
use jkr_protocol::{EntityState, Snapshot};
use jkr_runtime::{AnimationState, EntityId};
use std::error::Error;
use std::time::Instant;

/// `EF_DEAD`, `ET_GENERAL`.
const EF_DEAD: u32 = 1 << 1;
const ET_GENERAL: u8 = 0;
/// `cgs.effects.mBlasterSmoke`, and how often a flying limb trails it.
const SMOKE: &str = "blaster/smoke_bolton";
const TRAIL_MILLIS: i64 = 400;

/// What cuts did to one actor mesh: an owner's losses, or what a limb's copy is.
#[derive(Default)]
pub(crate) struct Cuts {
    /// `torsoBolt`: the parts it has lost.
    pub(crate) lost: u8,
    /// Its weapon went with the hand that held it.
    pub(crate) disarmed: bool,
    /// Its second saber and its jetpack went with the first cut.
    pub(crate) stripped: bool,
    /// On a limb's own copy, which limb it is.
    pub(crate) limb: Option<Limb>,
}

/// A limb's copy of its owner's model.
pub(crate) struct Limb {
    /// The owner's animation when it was cut, which the limb plays on.
    pub(crate) animation: AnimationState,
    cut: Cut,
    /// When a moving limb next trails smoke.
    trail_at: i64,
}

impl Limb {
    /// The bone the limb turns about.
    pub(crate) fn rotate_bone(&self) -> &str {
        self.cut.rotate_bone
    }
}

/// The most limb entities one frame looks at; more fly at once only in a massacre.
const MOST_LIMBS: usize = 32;

/// A limb entity in the presented snapshot, with what its cut needs of its owner.
#[derive(Clone, Copy, Default)]
struct LimbEntity {
    number: u16,
    part: u8,
    owner: u16,
    /// `lerpAngles`: the limb's angles.
    angles: [f32; 3],
    /// Whether it flies (`pos.trDelta` set).
    moving: bool,
    /// The owner's `eFlags` and torso animation.
    owner_state: Option<(u32, u16)>,
}

/// One frame of cuts, ahead of posing: copies whose limb is gone go back to the pool,
/// owners that live again are made whole, new limbs are cut, flying limbs trail smoke.
pub(crate) fn update(gpu: &mut GpuState, time: i64) -> Result<(), Box<dyn Error>> {
    let setting = gpu
        .console
        .as_ref()
        .and_then(|console| console.integer_cvar("cg_dismember"))
        .unwrap_or(0);
    let mut limbs = [LimbEntity::default(); MOST_LIMBS];
    let mut count = 0;
    let mut whole = [0; MOST_LIMBS];
    let mut wholes = 0;
    {
        let Some(snapshot) = crate::first_person_view::presented_snapshot(
            gpu.live_session.as_ref(),
            gpu.demo_session.as_ref(),
            time as i32,
        ) else {
            return Ok(());
        };
        for state in snapshot.entities.iter().filter(|state| is_limb(state)) {
            if count == MOST_LIMBS {
                break;
            }
            let owner = u16::try_from(state.model_index()).unwrap_or(state.other_entity_num2());
            limbs[count] = LimbEntity {
                number: state.number(),
                part: state.model_ghoul2(),
                owner,
                angles: state.angular_trajectory_base(),
                moving: state.trajectory_delta() != [0.0; 3],
                owner_state: owner_flags(snapshot, owner),
            };
            count += 1;
        }
        // `CG_ReattachLimb`'s test, for each owner that has lost something.
        for (index, mesh) in gpu.actor_meshes.iter().enumerate() {
            if mesh.cuts.lost == 0 || mesh.corpse_pool || mesh.limb_pool || wholes == MOST_LIMBS {
                continue;
            }
            let alive = mesh
                .entity_id
                .and_then(|id| u16::try_from(id.get() - 1).ok())
                .and_then(|number| owner_flags(snapshot, number))
                .is_some_and(|(flags, _)| flags & EF_DEAD == 0);
            if alive {
                whole[wholes] = index;
                wholes += 1;
            }
        }
    }
    let limbs = &limbs[..count];
    free_gone(gpu, limbs);
    for &index in &whole[..wholes] {
        let mesh = &mut gpu.actor_meshes[index];
        mesh.surfaces.reset(&mesh.preview.mesh, &mesh.preview.skin);
        mesh.cuts = Cuts::default();
    }
    if setting > 0 {
        for limb in limbs {
            if dismember::shown(limb.part, setting) {
                cut_limb(gpu, limb, time)?;
            }
        }
    }
    trail(gpu, limbs, time);
    Ok(())
}

/// A limb entity: `ET_GENERAL` with `s.weapon` `G2_MODEL_PART` and a part.
fn is_limb(state: &EntityState) -> bool {
    state.entity_type() == ET_GENERAL
        && state.weapon() == dismember::G2_MODEL_PART
        && (dismember::HEAD..=dismember::RIGHT_LEG).contains(&state.model_ghoul2())
}

/// Copies whose limb entity left the snapshot go back to the pool.
fn free_gone(gpu: &mut GpuState, limbs: &[LimbEntity]) {
    for mesh in gpu
        .actor_meshes
        .iter_mut()
        .filter(|mesh| mesh.cuts.limb.is_some())
    {
        let present = mesh.entity_id.is_some_and(|id| {
            limbs
                .iter()
                .any(|limb| u64::from(limb.number) + 1 == id.get())
        });
        if !present {
            mesh.entity_id = None;
            mesh.cuts.limb = None;
            mesh.surfaces.reset(&mesh.preview.mesh, &mesh.preview.skin);
        }
    }
}

/// The owner `number`'s `eFlags` and torso animation: the player's own state for the
/// client the snapshot is for (as `BG_PlayerStateToEntityState` makes its entity), its
/// entity's otherwise.
fn owner_flags(snapshot: &Snapshot, number: u16) -> Option<(u32, u16)> {
    if number == snapshot.player.client_num() {
        let player = &snapshot.player;
        // `BG_PlayerStateToEntityState`: a player without health is dead.
        let flags = if player.health() <= 0 {
            player.entity_flags() | EF_DEAD
        } else {
            player.entity_flags() & !EF_DEAD
        };
        return Some((flags, player.torso_animation()));
    }
    let state = snapshot
        .entities
        .binary_search_by_key(&number, |state| state.number())
        .ok()
        .map(|index| &snapshot.entities[index])?;
    Some((state.e_flags(), state.torso_animation()))
}

/// The cut of `limb`, once its owner is ready for it (`cg_ents.c:1052-1270`).
fn cut_limb(gpu: &mut GpuState, limb: &LimbEntity, time: i64) -> Result<(), Box<dyn Error>> {
    let limb_id = EntityId::new(u64::from(limb.number) + 1);
    if gpu
        .actor_meshes
        .iter()
        .any(|mesh| mesh.cuts.limb.is_some() && mesh.entity_id == Some(limb_id))
    {
        return Ok(());
    }
    let owner_id = EntityId::new(u64::from(limb.owner) + 1);
    let Some(owner_index) = gpu
        .actor_meshes
        .iter()
        .position(|mesh| !mesh.corpse_pool && !mesh.limb_pool && mesh.entity_id == Some(owner_id))
    else {
        return Ok(());
    };
    let owner_mesh = &gpu.actor_meshes[owner_index];
    if owner_mesh.cuts.lost & dismember::bit(limb.part) != 0 {
        // "already have this limb missing!"
        return Ok(());
    }
    let drawn_torso = owner_mesh
        .animator
        .requested
        .map_or(0, |requested| requested.upper.clip as u16);
    let ready = limb.owner_state.is_some_and(|(flags, torso)| {
        dismember::owner_ready(flags & EF_DEAD != 0, torso, drawn_torso)
    });
    if !ready {
        return Ok(());
    }
    let world = gpu
        .demo_session
        .as_ref()
        .map_or(&gpu.live_world, crate::demo_playback::Session::world);
    let (Some(owner), Some(limb_entity)) = (world.entity(owner_id), world.entity(limb_id)) else {
        return Ok(());
    };
    let Some(animation) = owner.animation() else {
        return Ok(());
    };
    let owner_origin = owner.sample(time).translation;
    let limb_origin = limb_entity.sample(time).translation;
    let model = &owner_mesh.preview.mesh;
    let cut = dismember::cut(limb.part, owner_mesh.animator.humanoid(), |name| {
        owner_mesh.surfaces.with_variant(model, name)
    });
    // Smoke from both cuts, read from the owner's pose as it was cut: the limb's tag as the
    // copy stands before it turns about its bone, at the limb's place.
    let owner_yaw = owner_mesh.render_yaw_degrees.unwrap_or(0.0);
    let smokes = [
        bolt_smoke(owner_mesh, cut.limb_tag, limb.angles, limb_origin, None),
        bolt_smoke(
            owner_mesh,
            cut.stub_tag,
            [0.0, owner_yaw, 0.0],
            owner_origin,
            None,
        ),
    ];
    let preview = owner_mesh.preview.clone();
    let appearance = owner_mesh.appearance.clone();
    let owner_switches = owner_mesh.surfaces.switches();
    // The owner: the limb off with all below it, the stump's cap on.
    let owner_mesh = &mut gpu.actor_meshes[owner_index];
    let (surfaces, model) = (&mut owner_mesh.surfaces, &owner_mesh.preview.mesh);
    surfaces.switch(model, &cut.limb, SURFACE_NO_DESCENDANTS);
    surfaces.switch(model, &cut.stub_cap, 0);
    surfaces.refresh(model);
    owner_mesh.cuts.lost |= dismember::bit(limb.part);
    owner_mesh.cuts.stripped = true;
    owner_mesh.cuts.disarmed |= dismember::disarms(limb.part);
    // The limb's copy: the owner as it was before the cut, walked from the limb's root
    // surface, the limb's cap on.
    let copy = take_copy(gpu, preview, &appearance, limb_id)?;
    let mesh = &mut gpu.actor_meshes[copy];
    let (surfaces, model) = (&mut mesh.surfaces, &mesh.preview.mesh);
    surfaces.set_switches(owner_switches);
    surfaces.set_root(model, &cut.limb);
    surfaces.switch(model, &cut.limb_cap, 0);
    surfaces.refresh(model);
    mesh.cuts = Cuts {
        limb: Some(Limb {
            animation,
            cut,
            trail_at: 0,
        }),
        ..Cuts::default()
    };
    for (origin, direction) in smokes.into_iter().flatten() {
        smoke(gpu, origin, direction, time);
    }
    Ok(())
}

/// A free copy of `appearance` from the limb pool, or a new one; its index.
fn take_copy(
    gpu: &mut GpuState,
    preview: crate::PlayerPreview,
    appearance: &jkr_runtime::Appearance,
    limb_id: EntityId,
) -> Result<usize, Box<dyn Error>> {
    if let Some(index) = gpu.actor_meshes.iter().position(|mesh| {
        mesh.limb_pool && mesh.cuts.limb.is_none() && &mesh.appearance == appearance
    }) {
        let mesh = &mut gpu.actor_meshes[index];
        mesh.entity_id = Some(limb_id);
        mesh.animator = crate::actor_pose::storage(&mesh.preview.animation)?;
        return Ok(index);
    }
    let mut mesh = gpu.upload_actor(preview, appearance, limb_id, [None, None])?;
    mesh.limb_pool = true;
    gpu.actor_meshes.push(mesh);
    gpu.actor_groups.push(Vec::with_capacity(1));
    Ok(gpu.actor_meshes.len() - 1)
}

/// A flying limb's trail: smoke from its cap every 400 ms while it moves
/// (`cg_ents.c:1288-1306`).
fn trail(gpu: &mut GpuState, limbs: &[LimbEntity], time: i64) {
    let world = gpu
        .demo_session
        .as_ref()
        .map_or(&gpu.live_world, crate::demo_playback::Session::world);
    let mut smokes = [None; MOST_LIMBS];
    for (slot, limb) in smokes.iter_mut().zip(limbs).filter(|(_, limb)| limb.moving) {
        let id = EntityId::new(u64::from(limb.number) + 1);
        let Some(mesh) = gpu
            .actor_meshes
            .iter_mut()
            .find(|mesh| mesh.cuts.limb.is_some() && mesh.entity_id == Some(id))
        else {
            continue;
        };
        let Some(origin) = world
            .entity(id)
            .map(|entity| entity.sample(time).translation)
        else {
            continue;
        };
        let pivot = pivot(mesh);
        let Some(copy) = mesh.cuts.limb.as_mut().filter(|copy| copy.trail_at < time) else {
            continue;
        };
        copy.trail_at = time + TRAIL_MILLIS;
        let tag = copy.cut.limb_tag;
        *slot = bolt_smoke(mesh, tag, limb.angles, origin, pivot);
    }
    for (origin, direction) in smokes.into_iter().flatten() {
        smoke(gpu, origin, direction, time);
    }
}

/// Where a limb's copy turns, in model space: the origin of its rotate bone as posed.
pub(crate) fn pivot(mesh: &crate::actor_mesh::ActorMesh) -> Option<Vec3> {
    let limb = mesh.cuts.limb.as_ref()?;
    let raw = model_bolt(
        &mesh.preview.mesh,
        &mesh.preview.animation,
        limb.rotate_bone(),
        mesh.animator.matrices(),
    )
    .ok()??;
    Some(Vec3::new(raw[0][3], raw[1][3], raw[2][3]))
}

/// Where bolt `tag` of `mesh` (posed as last drawn) stands for a model at `origin` turned
/// by `angles`, the model first moved so that `pivot` is its origin; and the bolt's
/// `NEGATIVE_Y` axis, which smoke rises along (straight up the second axis if it has none).
fn bolt_smoke(
    mesh: &crate::actor_mesh::ActorMesh,
    tag: &str,
    angles: [f32; 3],
    origin: [f32; 3],
    pivot: Option<Vec3>,
) -> Option<(Vec3, Vec3)> {
    let mut raw = model_bolt(
        &mesh.preview.mesh,
        &mesh.preview.animation,
        tag,
        mesh.animator.matrices(),
    )
    .ok()??;
    if let Some(pivot) = pivot {
        for (row, offset) in raw.iter_mut().zip(pivot.to_array()) {
            row[3] -= offset;
        }
    }
    let matrix = world_bolt(raw, angles, origin, [0.0; 3]);
    let point = Vec3::new(matrix[0][3], matrix[1][3], matrix[2][3]);
    let mut direction = -Vec3::new(matrix[0][1], matrix[1][1], matrix[2][1]);
    if direction == Vec3::ZERO {
        direction = Vec3::Y;
    }
    Some((point, direction))
}

/// `trap->FX_PlayEffectID(cgs.effects.mBlasterSmoke, origin, direction)`.
fn smoke(gpu: &mut GpuState, origin: Vec3, direction: Vec3, time: i64) {
    let Some(vfs) = gpu.vfs.clone() else {
        return;
    };
    effect_runtime::spawn_effect(
        &mut gpu.particles,
        &mut gpu.effect_aux,
        &mut gpu.effects,
        &vfs,
        SMOKE,
        origin,
        Instant::now(),
        (time as u32) ^ origin.x.to_bits(),
        0,
        &mut None,
        crate::combat_effects::rotation_from_direction(direction.to_array()),
    );
}
