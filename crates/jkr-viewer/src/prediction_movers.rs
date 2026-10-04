//! Snapshot-owned inline/packed-box solids and stock rider adjustment.
//! Legacy entity limits are contained in this viewer compatibility adapter.

use jkr_client::{legacy_evaluate_trajectory, legacy_evaluate_trajectory_angles};
use jkr_protocol::{ENTITY_NUMBER_NONE, EntityState, Snapshot};

const ENTITY_SLOTS: usize = ENTITY_NUMBER_NONE as usize + 1;
const SOLID_BMODEL: u32 = 0x00ff_ffff;
const ET_MOVER: u8 = 6;

#[derive(Clone, Copy, Debug)]
struct Trajectory {
    base: [f32; 3],
    delta: [f32; 3],
    kind: u8,
    start: i32,
    duration: i32,
}

impl Trajectory {
    fn position(&self, time: i32) -> [f32; 3] {
        legacy_evaluate_trajectory(
            self.base,
            self.delta,
            self.kind,
            self.start,
            self.duration,
            time,
        )
    }

    fn angles(&self, time: i32) -> [f32; 3] {
        legacy_evaluate_trajectory_angles(
            self.base,
            self.delta,
            self.kind,
            self.start,
            self.duration,
            time,
        )
    }
}

/// One solid: brush models translate at `cg.physicsTime` and rotate at the
/// presented frame; packed boxes stay axis-aligned at their rendered origin.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Collider {
    pub(crate) entity: u16,
    pub(crate) model: usize,
    pub(crate) bounds: Option<jkr_bsp::Aabb>,
    pub(crate) origin: [f32; 3],
    pub(crate) axes: [[f32; 3]; 3],
    pub(crate) rotated: bool,
    mover: bool,
    position: Trajectory,
    angular: Trajectory,
    teleport: bool,
    actor: bool,
    original_kind: u8,
}

impl Collider {
    /// Retain the last presented brush transform while its server changes maps.
    pub(crate) fn frozen(mover: &jkr_client::LegacyMoverPresentation) -> Self {
        let stationary = |base| Trajectory {
            base,
            delta: [0.0; 3],
            kind: 0,
            start: 0,
            duration: 0,
        };
        Self {
            entity: mover.entity_number,
            model: mover.model_index,
            bounds: None,
            origin: mover.origin,
            axes: rotation_axes(mover.angles),
            rotated: mover.angles != [0.0; 3],
            mover: true,
            position: stationary(mover.origin),
            angular: stationary(mover.angles),
            teleport: false,
            actor: false,
            original_kind: ET_MOVER,
        }
    }

    /// Decode a snapshot solid for prediction or the shared targeting trace.
    pub(crate) fn from_entity(
        state: &EntityState,
        physics_time: i32,
        angle_time: i32,
        smooth_clients: bool,
    ) -> Option<Self> {
        if state.solid() == 0 || matches!(state.entity_type(), 2 | 4 | 5) {
            return None;
        }
        let bounds = if state.solid() == SOLID_BMODEL {
            if state.model_index() <= 0 {
                return None;
            }
            None
        } else {
            // CG_ClipMoveToEntities: x / down / (up + 32), in whole units.
            let x = (state.solid() & 255) as f32;
            let down = ((state.solid() >> 8) & 255) as f32;
            let up = ((state.solid() >> 16) & 255) as f32 - 32.0;
            Some(jkr_bsp::Aabb::new([-x, -x, -down], [x, x, up]).ok()?)
        };
        let mut position = Trajectory {
            base: state.trajectory_base(),
            delta: state.trajectory_delta(),
            kind: state.trajectory_type(),
            start: state.trajectory_time(),
            duration: state.trajectory_duration(),
        };
        let actor = state.number() < 32 || state.entity_type() == 13;
        if bounds.is_some() && actor && !smooth_clients {
            position.kind = 1;
        }
        let angular = Trajectory {
            base: state.angular_trajectory_base(),
            delta: state.angular_trajectory_delta(),
            kind: state.angular_trajectory_type(),
            start: state.angular_trajectory_time(),
            duration: state.angular_trajectory_duration(),
        };
        let mut result = Self {
            entity: state.number(),
            model: state.model_index() as usize,
            bounds,
            origin: position.position(physics_time),
            axes: [[0.0; 3]; 3],
            rotated: false,
            mover: state.entity_type() == ET_MOVER,
            position,
            angular,
            teleport: state.e_flags() & (1 << 3) != 0,
            actor,
            original_kind: state.trajectory_type(),
        };
        result.set_angle_time(angle_time);
        Some(result)
    }

    fn set_angle_time(&mut self, time: i32) {
        if self.bounds.is_some() {
            // Encoded boxes are axis-aligned, regardless of entity angles.
            self.origin = self.position.position(time);
            return;
        }
        let angles = self.angular.angles(time);
        self.rotated = angles != [0.0; 3];
        self.axes = rotation_axes(angles);
    }
}

/// Reused solid list plus O(1) ground-entity lookup. A prediction snapshot
/// and a displayed snapshot each own one list: their trajectories may differ
/// at lift starts/stops and must not be silently mixed.
pub(crate) struct Movers {
    /// CG_BuildSolidList's separate trigger list (cg_predict.c:68-80).
    pub(crate) triggers: Vec<jkr_client::prediction_items::PredictionTrigger>,
    pub(crate) colliders: Vec<Collider>,
    by_entity: Box<[Option<usize>]>,
    sequence: Option<i32>,
    snapshot_time: i32,
    smooth_clients: bool,
    permanents: Box<[EntityState]>,
}

impl Movers {
    pub(crate) fn new() -> Self {
        Self {
            triggers: Vec::with_capacity(ENTITY_SLOTS),
            colliders: Vec::with_capacity(ENTITY_SLOTS),
            by_entity: vec![None; ENTITY_SLOTS].into_boxed_slice(),
            sequence: None,
            snapshot_time: 0,
            smooth_clients: false,
            permanents: Box::default(),
        }
    }

    pub(crate) fn update(&mut self, snapshot: &Snapshot, angle_time: i32) {
        if self.sequence == Some(snapshot.message_sequence) {
            return;
        }
        self.sequence = Some(snapshot.message_sequence);
        self.snapshot_time = snapshot.server_time;
        self.colliders.clear();
        self.triggers.clear();
        self.by_entity.fill(None);
        for state in snapshot
            .entities
            .iter()
            .chain(self.permanents.iter().filter(|state| {
                jkr_client::legacy_permanent_visible(state, snapshot.player.origin())
                    && snapshot
                        .entities
                        .binary_search_by_key(&state.number(), EntityState::number)
                        .is_err()
            }))
        {
            if matches!(state.entity_type(), 2 | 4 | 5) {
                self.triggers.push(state.into());
                continue;
            }
            if self.by_entity[usize::from(state.number())].is_some() {
                continue;
            }
            let local = snapshot.player.client_num();
            // CG_ClipMoveToEntities skipNumber and server ownership exclusion.
            // genericenemyindex is read-only protocol netfield 18.
            let owner = state.integer_field(18).unwrap_or(0).wrapping_sub(1024);
            if state.number() == local || (state.number() > 32 && owner == i32::from(local)) {
                continue;
            }
            if let Some(collider) =
                Collider::from_entity(state, snapshot.server_time, angle_time, self.smooth_clients)
            {
                self.by_entity[usize::from(collider.entity)] = Some(self.colliders.len());
                self.colliders.push(collider);
            }
        }
    }

    pub(crate) fn set_angle_time(&mut self, time: i32) {
        for collider in &mut self.colliders {
            collider.set_angle_time(time);
        }
    }

    pub(crate) fn set_permanents(&mut self, game: Option<&jkr_protocol::GameState>) {
        self.permanents = game
            .into_iter()
            .flat_map(|game| game.baselines())
            .filter(|state| state.e_flags() & (1 << 7) != 0)
            .cloned()
            .collect();
        self.sequence = None;
    }

    fn set_smooth_clients(&mut self, enabled: bool) {
        if self.smooth_clients == enabled {
            return;
        }
        self.smooth_clients = enabled;
        for collider in &mut self.colliders {
            if collider.bounds.is_some() && collider.actor {
                collider.position.kind = if enabled { collider.original_kind } else { 1 };
            }
        }
    }

    /// Packed solids use cent->lerpOrigin, not the brush physics-time origin.
    /// Match CG_InterpolateEntityPosition and its teleport discontinuity.
    pub(crate) fn present_boxes(&mut self, shown: &Self, time: i32) {
        for collider in &mut self.colliders {
            if collider.bounds.is_none() {
                continue;
            }
            let Some(index) = shown.by_entity[usize::from(collider.entity)] else {
                continue;
            };
            let before = &shown.colliders[index];
            if before.bounds.is_none() || before.teleport != collider.teleport {
                continue;
            }
            if (before.position.kind == 1 || (before.actor && before.position.kind == 3))
                && self.snapshot_time > shown.snapshot_time
            {
                let fraction = ((time - shown.snapshot_time) as f32
                    / (self.snapshot_time - shown.snapshot_time) as f32)
                    .clamp(0.0, 1.0);
                let a = before.position.position(shown.snapshot_time);
                let b = collider.position.position(self.snapshot_time);
                collider.origin = std::array::from_fn(|i| a[i] + fraction * (b[i] - a[i]));
            } else {
                collider.origin = before.position.position(time);
            }
        }
    }

    /// `CG_AdjustPositionForMover` (`cg_ents.c:3005-3038`): translate only.
    /// Stock deliberately does not rotate a rider's origin or view angles.
    /// JKR's presentation clock can select an older trajectory than prediction
    /// at a start/stop. Subtract the actual collision pose and add the actual
    /// rendered pose, rather than evaluating the older trajectory at both times.
    pub(crate) fn adjust(
        &self,
        origin: [f32; 3],
        ground: u16,
        physics: &Self,
        to: i32,
    ) -> [f32; 3] {
        let (Some(before), Some(after)) = (physics.mover(ground), self.mover(ground)) else {
            return origin;
        };
        let after = after.position.position(to);
        std::array::from_fn(|axis| origin[axis] + (after[axis] - before.origin[axis]))
    }

    fn mover(&self, ground: u16) -> Option<&Collider> {
        if ground == 0 || ground >= 1_022 {
            return None;
        }
        let index = self.by_entity.get(usize::from(ground)).copied().flatten()?;
        let mover = &self.colliders[index];
        mover.mover.then_some(mover)
    }

    /// Validate a snapshot's carried ground before using it for view offsets.
    /// A clear foot sweep proves that a reported pusher no longer supports us;
    /// ambiguous/embedded contact is left to ordinary command prediction.
    pub(crate) fn supported_ground(
        &self,
        bsp: &jkr_bsp::Bsp,
        origin: [f32; 3],
        ground: u16,
    ) -> u16 {
        if let Some(mover) = self.mover(ground)
            && !mover.rotated
            && !crate::movement_collision::mover_supports_feet(bsp, mover, origin)
        {
            return ENTITY_NUMBER_NONE;
        }
        ground
    }
}

impl super::LocalPrediction {
    pub(crate) fn set_smooth_clients(&mut self, enabled: bool) {
        self.physics_movers.set_smooth_clients(enabled);
        self.render_movers.set_smooth_clients(enabled);
    }
}

/// `CreateRotationMatrix` / `AngleVectors`: rows of world-to-model rotation.
pub(crate) fn rotation_axes(angles: [f32; 3]) -> [[f32; 3]; 3] {
    // AngleVectors multiplies by the double-precision M_PI expression,
    // then rounds to float before calling sinf/cosf.
    let [pitch, yaw, roll] =
        angles.map(|v| (f64::from(v) * (std::f64::consts::PI * 2.0 / 360.0)) as f32);
    let (sp, cp) = pitch.sin_cos();
    let (sy, cy) = yaw.sin_cos();
    let (sr, cr) = roll.sin_cos();
    [
        [cp * cy, cp * sy, -sp],
        [sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, sr * cp],
        [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp],
    ]
}
