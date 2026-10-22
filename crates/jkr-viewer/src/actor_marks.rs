//! Ghoul2 damage marks on the actors drawn ([`jkr_client::ghoul2_marks`]). A mark this
//! frame's impacts ask for is laid on the struck model in this frame's pose
//! (`G2API_AddSkinGore`, [`jkr_model::g2_gore`]) and kept on its actor surface by surface
//! until its time is up (`RenderSurfaces` drops a record at `mDeleteTime`). Each frame
//! every kept mark's triangles are skinned with the frame's pose, placed as the actor is
//! drawn and cut to the mark's square — stock's mark shaders `clampmap` a picture whose
//! edges are clear — and handed to the decal layer, which blends them over the view as
//! stock's renderer does, polygon offset included.
//!
//! As in stock, the mark is laid where Ghoul2 puts the model for traces — at the entity's
//! origin, turned by its view's yaw — while the model is drawn turned by its legs; the
//! mark then rides the vertices it was laid on.

use super::*;
use crate::decal_marks::DecalVertex;
use jkr_client::ghoul2_marks::{self, MarkRequest};
use jkr_model::g2_collision::{self, CollisionRecord, CollisionScratch, GHOUL2_ROOT, PosedModel};
use jkr_model::g2_gore::{self, GoreLod, GoreProjection, GoreScratch, GoreSurface};
use jkr_protocol::EntityState;

/// Mark requests one frame can hold until the poses are ready; more are dropped.
const MAX_PENDING: usize = 64;
/// `SOLID_BMODEL`: an entity clipped by its brush model, not a box.
const SOLID_BMODEL: u32 = 0x00ff_ffff;
/// The box `CG_BuildSolidList` gives the viewer's own player, which the snapshot never
/// carries (`cg_predict.c:105-139`): 15 across, `DEFAULT_MINS_2` down, `DEFAULT_MAXS_2` up.
const LOCAL_PLAYER_SOLID: u32 = ((40 + 32) << 16) | (24 << 8) | 15;

/// Where an actor was drawn this frame, as its marks need it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Placement {
    pub(crate) entity: EntityId,
    /// `cent->lerpOrigin`: where Ghoul2 puts the model for traces and gore (turned by the
    /// view's yaw, looked up only for a struck actor).
    pub(crate) origin: [f32; 3],
    /// `cent->modelScale`, a zero percentage counting as whole.
    pub(crate) scale: f32,
    /// The entity's `solid`: the box a trace must touch before its model is tested.
    pub(crate) solid: u32,
    /// The drawn instance: position, rotation (Ghoul2's quarter turn included) and scale.
    pub(crate) position: Vec3,
    pub(crate) rotation: Quat,
    pub(crate) drawn_scale: Vec3,
    /// Drawn in the main view, not only in mirrors.
    pub(crate) shown: bool,
    /// The presentation time it was drawn at.
    pub(crate) time: i64,
}

impl Placement {
    /// Where `entity` was drawn this frame as `instance`, standing at `lerp_origin`;
    /// `local` for the viewer's own player.
    pub(crate) fn of(
        entity: &jkr_runtime::SceneEntity,
        (state, local): (Option<&EntityState>, bool),
        lerp_origin: [f32; 3],
        instance: &ActorInstance,
        shown: bool,
        time: i64,
    ) -> Self {
        let percent = state.map_or(0, |state| state.model_scale_percent());
        Self {
            entity: entity.id,
            origin: lerp_origin,
            scale: if percent == 0 {
                1.0
            } else {
                percent as f32 / 100.0
            },
            solid: state.map_or(
                if local { LOCAL_PLAYER_SOLID } else { 0 },
                EntityState::solid,
            ),
            position: Vec3::from_array(instance.position),
            rotation: Quat::from_array(instance.rotation),
            drawn_scale: Vec3::from_array(instance.scale),
            shown,
            time,
        }
    }
}

/// One mark's record on one surface (`SGoreSurface`).
struct Record {
    surface: usize,
    /// The share at LOD 0, the one drawn; `None` where the mark touched only lower LODs.
    share: Option<GoreLod>,
    /// Index into [`ghoul2_marks::SHADERS`].
    shader: usize,
    delete_at: i64,
}

/// An actor's marks, kept while it stays the same entity.
#[derive(Default)]
pub(crate) struct Marks {
    entity: Option<EntityId>,
    /// Where it was drawn last; set by the submission, which sees actors only shared.
    placement: std::cell::Cell<Option<Placement>>,
    records: Vec<Record>,
}

impl Marks {
    /// Where the actor was drawn this frame.
    pub(crate) fn place(&self, placement: Placement) {
        self.placement.set(Some(placement));
    }

    /// Its placement if it was drawn at `now`; a new entity starts unmarked.
    fn placed(&mut self, now: i64) -> Option<Placement> {
        let placement = self
            .placement
            .get()
            .filter(|placement| placement.time == now)?;
        if self.entity != Some(placement.entity) {
            self.records.clear();
            self.entity = Some(placement.entity);
        }
        Some(placement)
    }

    /// `G2API_ClearSkinGore`: a respawned player (`rcg`, `ircg`) comes back unmarked.
    pub(crate) fn clear(&mut self) {
        self.records.clear();
    }
}

/// Requests, limits and scratch storage shared by every actor.
pub(crate) struct Runtime {
    pending: Vec<MarkRequest>,
    taken: Vec<MarkRequest>,
    /// `cg_ghoul2Marks`: a model carrying this many records takes no more; 0 lays none.
    pub(crate) limit: usize,
    shaders: [Arc<str>; 3],
    collision: CollisionScratch,
    records: Vec<CollisionRecord>,
    gore: GoreScratch,
    surfaces: Vec<GoreSurface>,
    positions: Vec<[f32; 3]>,
    corners: Vec<DecalVertex>,
    clipped: Vec<DecalVertex>,
    seed: u32,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            pending: Vec::with_capacity(MAX_PENDING),
            taken: Vec::with_capacity(MAX_PENDING),
            limit: ghoul2_marks::DEFAULT_LIMIT,
            shaders: ghoul2_marks::SHADERS.map(Arc::from),
            collision: CollisionScratch::default(),
            records: Vec::with_capacity(g2_collision::MAX_COLLISIONS),
            gore: GoreScratch::default(),
            surfaces: Vec::new(),
            positions: Vec::new(),
            corners: Vec::with_capacity(3),
            clipped: Vec::with_capacity(crate::decal_marks::MAX_VERTICES_ON_DECAL),
            seed: 0x9e37_79b9,
        }
    }
}

impl Runtime {
    /// A mark, laid once this frame's poses are ready; past [`MAX_PENDING`] a frame's
    /// further requests are dropped.
    pub(crate) fn request(&mut self, mark: MarkRequest) {
        if self.pending.len() < MAX_PENDING {
            self.pending.push(mark);
        }
    }

    /// `flrand(low, high)`: presentation only, as stock's client random is.
    fn random(&mut self, low: f32, high: f32) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        low + (high - low) * ((self.seed >> 8) as f32 / (1 << 24) as f32)
    }
}

/// This frame's marks: lay the requested ones on the actors drawn, drop expired ones, and
/// hand every kept mark's polygons to the decal layer. Runs after the actors are submitted.
pub(crate) fn frame(gpu: &mut GpuState, now: i64) {
    let runtime = &mut gpu.effect_aux.marks;
    gpu.effect_aux.decals.begin_model_marks();
    for mesh in &mut gpu.actor_meshes {
        mesh.marks.placed(now);
    }
    std::mem::swap(&mut runtime.pending, &mut runtime.taken);
    let world = gpu
        .demo_session
        .as_ref()
        .map_or(&gpu.live_world, demo_playback::Session::world);
    let mut taken = std::mem::take(&mut runtime.taken);
    for mark in taken.drain(..) {
        if runtime.limit == 0 {
            continue;
        }
        let target = EntityId::new(u64::from(mark.target) + 1);
        let yaw = view_yaw(world, target, now);
        let Some(mesh) = gpu.actor_meshes.iter_mut().find(|mesh| {
            mesh.marks
                .placement
                .get()
                .is_some_and(|placement| placement.entity == target && placement.time == now)
        }) else {
            continue;
        };
        lay(runtime, mesh, &mark, yaw, now);
    }
    runtime.taken = taken;
    for mesh in &mut gpu.actor_meshes {
        mesh.marks.records.retain(|record| now < record.delete_at);
        let Some(placement) = mesh
            .marks
            .placed(now)
            .filter(|placement| placement.shown && !mesh.marks.records.is_empty())
        else {
            continue;
        };
        emit(
            &mut gpu.effect_aux.decals,
            &gpu.effect_aux.marks.shaders,
            mesh,
            &placement,
            (
                &mut gpu.effect_aux.marks.positions,
                &mut gpu.effect_aux.marks.corners,
                &mut gpu.effect_aux.marks.clipped,
            ),
        );
    }
}

/// `CG_AddGhoul2Mark` (after `CG_G2MarkEvent`'s trace, for a missile) on `mesh`.
fn lay(runtime: &mut Runtime, mesh: &mut ActorMesh, mark: &MarkRequest, yaw: f32, now: i64) {
    let style = mark.style;
    let Some(placement) = mesh.marks.placed(now) else {
        return;
    };
    if mesh.marks.records.len() >= runtime.limit {
        return;
    }
    let posed = posed(mesh, &placement);
    let axes = jkr_game_jka::pmove::flight::angles_to_axis([0.0, yaw, 0.0]);
    let start = surface_point(runtime, &posed, &placement, axes, mark);
    let mut direction = Vec3::from_array(mark.end) - Vec3::from_array(start);
    if direction.length() < 0.1 {
        return;
    }
    direction = direction.normalize();
    let size = runtime.random(style.size.0, style.size.1);
    let projection = GoreProjection {
        hit: start,
        direction: direction.to_array(),
        size: [size, size],
        angle: runtime.random(0.0, 6.28),
        front_faces: true,
        back_faces: true,
    };
    let lifetime = runtime.random(style.lifetime_ms.0 as f32, style.lifetime_ms.1 as f32 + 1.0);
    let shader = ghoul2_marks::SHADERS
        .iter()
        .position(|name| *name == style.shader)
        .unwrap_or(0);
    runtime.surfaces.clear();
    // `CG_AddGhoul2Mark` copies the scale with `VectorCopy`'s operands reversed, so the
    // gore is always laid on the model at its own size; only the trace above is scaled.
    let unscaled = PosedModel {
        scale: [1.0; 3],
        ..posed
    };
    if g2_gore::add_skin_gore(
        &unscaled,
        placement.origin,
        axes,
        &projection,
        ghoul2_marks::MARK_LODS,
        &mut runtime.gore,
        &mut runtime.surfaces,
    )
    .is_err()
    {
        return;
    }
    for surface in runtime.surfaces.drain(..) {
        mesh.marks.records.push(Record {
            surface: surface.surface,
            share: surface.lods.into_iter().next().flatten(),
            shader,
            delete_at: now + lifetime as i64,
        });
    }
}

/// Where the mark starts: with `eventParm` set, the point the shot's Ghoul2 trace finds
/// on the model (`CG_G2Trace` at `cg_g2TraceLod`), tested only once it touches the box;
/// a trace that finds nothing ends at `origin2`. Without it, the missile's own origin.
fn surface_point(
    runtime: &mut Runtime,
    posed: &PosedModel<'_>,
    placement: &Placement,
    axes: [[f32; 3]; 3],
    mark: &MarkRequest,
) -> [f32; 3] {
    if !mark.trace {
        return mark.origin;
    }
    model_hit(runtime, posed, placement, axes, mark.origin, mark.end)
        .map_or(mark.end, |record| record.position)
}

/// `CG_G2TraceCollide` on the model of `target` as drawn at `now`: where the segment first
/// meets it, and the surface's normal there, in the world. `None` when the target was not
/// drawn this frame or the segment misses it.
pub(crate) fn trace_model(
    meshes: &[ActorMesh],
    world: &World,
    runtime: &mut Runtime,
    target: u16,
    (start, end): ([f32; 3], [f32; 3]),
    now: i64,
) -> Option<([f32; 3], [f32; 3])> {
    let id = EntityId::new(u64::from(target) + 1);
    let (mesh, placement) = meshes.iter().find_map(|mesh| {
        let placement = mesh.marks.placement.get()?;
        (placement.entity == id && placement.time == now).then_some((mesh, placement))
    })?;
    let axes = jkr_game_jka::pmove::flight::angles_to_axis([0.0, view_yaw(world, id, now), 0.0]);
    model_hit(
        runtime,
        &posed(mesh, &placement),
        &placement,
        axes,
        start,
        end,
    )
    .map(|record| (record.position, record.normal))
}

/// The segment against the placed model: its box first (`CG_ClipMoveToEntities`), then
/// the model at `cg_g2TraceLod`; the nearest hit.
fn model_hit(
    runtime: &mut Runtime,
    posed: &PosedModel<'_>,
    placement: &Placement,
    axes: [[f32; 3]; 3],
    start: [f32; 3],
    end: [f32; 3],
) -> Option<CollisionRecord> {
    box_trace(start, end, placement.origin, placement.solid)?;
    g2_collision::collision_detect(
        std::slice::from_ref(posed),
        placement.origin,
        start,
        end,
        ghoul2_marks::TRACE_LOD,
        0.0,
        axes,
        &mut runtime.collision,
        &mut runtime.records,
    )
    .ok()?;
    runtime.records.first().copied()
}

/// The actor's body as Ghoul2 traces it: this frame's pose, the quarter-turn root and
/// `modelScale`.
fn posed<'a>(mesh: &'a ActorMesh, placement: &Placement) -> PosedModel<'a> {
    PosedModel {
        glm: &mesh.preview.mesh,
        bones: mesh.animator.matrices(),
        placement: GHOUL2_ROOT,
        overrides: &[],
        scale: [placement.scale; 3],
    }
}

/// `lerpAngles[YAW]`: the entity's view yaw as presented at `now`.
fn view_yaw(world: &World, id: EntityId, now: i64) -> f32 {
    world
        .entity(id)
        .and_then(|entity| entity.sample_pose(now))
        .map_or(0.0, |pose| pose.view_angles_degrees[1])
}

/// A point trace through an entity's encoded box (`CM_TempModel` in
/// `CG_ClipMoveToEntities`, `cg_predict.c:260-300`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BoxTrace {
    /// Where the segment enters the box; 1 for one that starts inside and leaves.
    pub(crate) fraction: f32,
    pub(crate) start_solid: bool,
    /// The segment never leaves the box.
    pub(crate) all_solid: bool,
}

/// The segment against the entity's encoded box at `origin`; `None` when it misses.
pub(crate) fn box_trace(
    start: [f32; 3],
    end: [f32; 3],
    origin: [f32; 3],
    solid: u32,
) -> Option<BoxTrace> {
    if solid == 0 || solid == SOLID_BMODEL {
        return None;
    }
    let x = (solid & 255) as f32;
    let down = ((solid >> 8) & 255) as f32;
    let up = ((solid >> 16) & 255) as f32 - 32.0;
    let low = [origin[0] - x, origin[1] - x, origin[2] - down];
    let high = [origin[0] + x, origin[1] + x, origin[2] + up];
    let inside =
        |point: [f32; 3]| (0..3).all(|axis| (low[axis]..=high[axis]).contains(&point[axis]));
    if inside(start) {
        // Leaving a solid is no hit; never leaving it is.
        let all_solid = inside(end);
        return Some(BoxTrace {
            fraction: if all_solid { 0.0 } else { 1.0 },
            start_solid: true,
            all_solid,
        });
    }
    let (mut enter, mut leave) = (0.0_f32, 1.0_f32);
    for axis in 0..3 {
        let delta = end[axis] - start[axis];
        if delta.abs() < f32::EPSILON {
            if start[axis] < low[axis] || start[axis] > high[axis] {
                return None;
            }
            continue;
        }
        let (a, b) = (
            (low[axis] - start[axis]) / delta,
            (high[axis] - start[axis]) / delta,
        );
        enter = enter.max(a.min(b));
        leave = leave.min(a.max(b));
        if enter > leave {
            return None;
        }
    }
    Some(BoxTrace {
        fraction: enter,
        start_solid: false,
        all_solid: false,
    })
}

/// A body drawn this frame as `CG_ClipMoveToEntities` clips against it: its entity number,
/// `lerpOrigin` and encoded box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Body {
    pub(crate) number: u16,
    pub(crate) origin: [f32; 3],
    pub(crate) solid: u32,
}

/// Every body drawn at `now`, by entity number.
pub(crate) fn bodies(meshes: &[ActorMesh], now: i64, out: &mut Vec<Body>) {
    out.clear();
    out.extend(meshes.iter().filter_map(|mesh| {
        let placement = mesh
            .marks
            .placement
            .get()
            .filter(|placement| placement.time == now)?;
        Some(Body {
            number: u16::try_from(placement.entity.get().checked_sub(1)?).ok()?,
            origin: placement.origin,
            solid: placement.solid,
        })
    }));
    out.sort_unstable_by_key(|body| body.number);
}

/// This frame's polygons of every mark on one actor: each marked vertex skinned once with
/// the frame's pose and placed as the actor is drawn, each marked triangle cut to the
/// mark's square.
fn emit(
    decals: &mut crate::decal_store::DecalStore,
    shaders: &[Arc<str>; 3],
    mesh: &ActorMesh,
    placement: &Placement,
    (positions, corners, clipped): (
        &mut Vec<[f32; 3]>,
        &mut Vec<DecalVertex>,
        &mut Vec<DecalVertex>,
    ),
) {
    let matrices = mesh.animator.matrices();
    let glm = &mesh.preview.mesh;
    for record in &mesh.marks.records {
        let Some(share) = &record.share else {
            continue;
        };
        positions.clear();
        positions.extend(share.vertices.iter().map_while(|&vertex| {
            let skinned = glm.skinned_position(0, record.surface, vertex as usize, matrices)?;
            let placed = placement.position
                + placement.rotation * (Vec3::from_array(skinned) * placement.drawn_scale);
            Some(placed.to_array())
        }));
        if positions.len() != share.vertices.len() {
            continue;
        }
        for triangle in &share.triangles {
            corners.clear();
            corners.extend(triangle.iter().filter_map(|&corner| {
                Some(DecalVertex {
                    position: *positions.get(corner as usize)?,
                    st: *share.coordinates.get(corner as usize)?,
                })
            }));
            if corners.len() == 3 && clip_to_square(corners, clipped) {
                decals.add_model_mark(&shaders[record.shader], clipped);
            }
        }
    }
}

/// The polygon cut to the mark's square, 0 to 1 on both axes (Sutherland–Hodgman), in
/// `clipped`; whether anything is left.
fn clip_to_square(polygon: &mut Vec<DecalVertex>, clipped: &mut Vec<DecalVertex>) -> bool {
    for (axis, keep_above) in [(0, true), (0, false), (1, true), (1, false)] {
        clipped.clear();
        let inside = |vertex: &DecalVertex| {
            if keep_above {
                vertex.st[axis] >= 0.0
            } else {
                vertex.st[axis] <= 1.0
            }
        };
        let edge = if keep_above { 0.0 } else { 1.0 };
        for index in 0..polygon.len() {
            let current = polygon[index];
            let next = polygon[(index + 1) % polygon.len()];
            if inside(&current) {
                clipped.push(current);
            }
            if inside(&current) != inside(&next) {
                let along = (edge - current.st[axis]) / (next.st[axis] - current.st[axis]);
                let lerp = |a: f32, b: f32| a + (b - a) * along;
                clipped.push(DecalVertex {
                    position: std::array::from_fn(|i| lerp(current.position[i], next.position[i])),
                    st: [
                        lerp(current.st[0], next.st[0]),
                        lerp(current.st[1], next.st[1]),
                    ],
                });
            }
        }
        std::mem::swap(polygon, clipped);
        if polygon.len() < 3 {
            return false;
        }
    }
    std::mem::swap(polygon, clipped);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corner(position: [f32; 3], st: [f32; 2]) -> DecalVertex {
        DecalVertex { position, st }
    }

    #[test]
    fn a_triangle_is_cut_to_the_marks_square_and_one_outside_it_is_dropped() {
        let mut polygon = vec![
            corner([0.0, 0.0, 0.0], [-1.0, 0.5]),
            corner([4.0, 0.0, 0.0], [3.0, 0.5]),
            corner([0.0, 4.0, 0.0], [-1.0, 4.5]),
        ];
        let mut clipped = Vec::new();
        assert!(clip_to_square(&mut polygon, &mut clipped));
        assert!(clipped.iter().all(|vertex| {
            vertex
                .st
                .iter()
                .all(|value| (-1e-6..=1.0 + 1e-6).contains(value))
        }));
        // Positions move with the coordinates: s 0 lies a quarter of the way along x.
        assert!(
            clipped
                .iter()
                .any(|vertex| (vertex.position[0] - 1.0).abs() < 1e-5)
        );
        let mut outside = vec![
            corner([0.0; 3], [2.0, 2.0]),
            corner([1.0, 0.0, 0.0], [3.0, 2.0]),
            corner([0.0, 1.0, 0.0], [2.0, 3.0]),
        ];
        assert!(!clip_to_square(&mut outside, &mut clipped));
    }

    #[test]
    fn a_shot_enters_a_players_box_starts_inside_it_or_misses_it() {
        // A player's box: 15 across, 24 down, 40 up.
        let solid = 15 | (24 << 8) | ((40 + 32) << 16);
        let origin = [100.0, 0.0, 0.0];
        let through = box_trace([0.0, 0.0, 0.0], [200.0, 0.0, 0.0], origin, solid).unwrap();
        assert!((through.fraction - 85.0 / 200.0).abs() < 1e-6);
        assert!(!through.start_solid && !through.all_solid);
        assert_eq!(
            box_trace([0.0, 50.0, 0.0], [200.0, 50.0, 0.0], origin, solid),
            None
        );
        // Starting inside: leaving is no hit, staying is.
        let leaving = box_trace([100.0, 0.0, 10.0], [200.0, 0.0, 10.0], origin, solid).unwrap();
        assert_eq!(
            (leaving.fraction, leaving.start_solid, leaving.all_solid),
            (1.0, true, false)
        );
        let staying = box_trace([100.0, 0.0, 10.0], [101.0, 0.0, 10.0], origin, solid).unwrap();
        assert_eq!((staying.fraction, staying.all_solid), (0.0, true));
        assert_eq!(box_trace([0.0; 3], [200.0, 0.0, 0.0], origin, 0), None);
    }
}
