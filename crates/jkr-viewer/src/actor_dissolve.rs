//! Stock's disintegration of an actor (`EF_DISINTEGRATION`: a disruptor's alternate fire
//! that killed it). From the first frame the client sees the flag (`dustTrailTime`), a
//! player's pose holds the legs frame it had then on `model_root`, `lower_lumbar` and
//! `Motion` (`cg_players.c:10635-10660`); it is drawn without its weapon or Force shells
//! (`CG_Player` returns before them), with the disruptor's smoke at its waist for the first
//! second (`CG_Disintegration`, `cg_ents.c:709-727`), and after a second and a half not at
//! all. A corpse that carries the flag burns the same way but keeps being drawn
//! (`CG_General`, `cg_ents.c:1488-1496`); its pose is its death frame already.
//!
//! The burn itself — the hole ripped from the hit point and the glowing edge,
//! `RF_DISINTEGRATE1` and `RF_DISINTEGRATE2` — is not drawn yet: the body is drawn whole
//! until it goes.

use crate::actor_mesh::ActorMesh;
use glam::Vec3;
use jkr_runtime::EntityId;

/// `EF_DISINTEGRATION` (`bg_public.h`).
pub(crate) const EF_DISINTEGRATION: u32 = jkr_game_jka::disruptor::EF_DISINTEGRATION;
/// After this long a disintegrating player is drawn no more (`cg_players.c:10643-10647`).
const GONE_AFTER_MS: i64 = 1_500;
/// How long the smoke rises (`cg.time - ent->endTime < 1000`).
const SMOKE_MS: i64 = 1_000;
/// Stock rolls for smoke once a frame, which at its own frame rates is about every 16 ms;
/// rolling at that pace keeps the amount the same at any frame rate.
const SMOKE_STEP_MS: i64 = 16;

/// One actor's disintegration as this client saw it begin.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Dissolve {
    /// The entity it is about; anything else starts afresh.
    entity: Option<EntityId>,
    /// `dustTrailTime`: when the flag was first seen.
    since: Option<i64>,
    /// `miscTime`: the legs frame then, which the pose holds.
    frame: Option<u32>,
    /// The last time the smoke was rolled for, and the puffs this frame starts.
    smoke_rolled: i64,
    puffs: u32,
}

/// What a disintegration asks of the drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    /// Burning since the time given: drawn without weapon or Force shells.
    Burning { since: i64 },
    /// Gone (a player a second and a half on): not drawn at all.
    Gone,
}

impl Dissolve {
    /// Whether `entity`'s disintegration has yet to begin here; the legs frame is read
    /// for [`Self::observe`] only then.
    pub(crate) fn starting(&self, entity: Option<EntityId>) -> bool {
        self.entity != entity || self.since.is_none()
    }

    /// The pose's part each frame: whether `entity` carries the flag now, and the legs
    /// frame it is at when it starts (`legs_frame`); the smoke due is rolled with
    /// `random`. Returns the frame the pose holds, if any.
    pub(crate) fn observe(
        &mut self,
        entity: Option<EntityId>,
        flagged: bool,
        now: i64,
        legs_frame: Option<u32>,
        random: impl FnMut() -> f32,
    ) -> Option<u32> {
        if self.entity != entity {
            *self = Self {
                entity,
                ..Self::default()
            };
        }
        if !flagged {
            (self.since, self.frame, self.puffs) = (None, None, 0);
            return None;
        }
        if self.since.is_none() {
            self.since = Some(now);
            self.frame = legs_frame;
            self.smoke_rolled = now - SMOKE_STEP_MS;
        }
        self.puffs = self.smoke_rolls(now, random);
        self.frame
    }

    /// What the drawing does with the actor at `now`; `corpse` for a body, which is never
    /// taken away by the client.
    pub(crate) fn phase(&self, now: i64, corpse: bool) -> Option<Phase> {
        let since = self.since?;
        Some(if !corpse && now - since > GONE_AFTER_MS {
            Phase::Gone
        } else {
            Phase::Burning { since }
        })
    }

    /// The puffs of smoke this frame starts.
    pub(crate) fn puffs(&self) -> u32 {
        self.puffs
    }

    /// The smoke rolls due since the last frame, for the first second: each a
    /// `Q_flrand(0, 1) > 0.05` for one puff and another half the time.
    fn smoke_rolls(&mut self, now: i64, mut random: impl FnMut() -> f32) -> u32 {
        let Some(since) = self.since else {
            return 0;
        };
        let mut puffs = 0;
        while self.smoke_rolled + SMOKE_STEP_MS <= now {
            self.smoke_rolled += SMOKE_STEP_MS;
            if self.smoke_rolled - since >= SMOKE_MS {
                continue;
            }
            if random() > 0.05 {
                puffs += 1;
                if random() > 0.5 {
                    puffs += 1;
                }
            }
        }
        puffs
    }
}

/// Where a puff of smoke starts: the waist (`lower_lumbar`) in the world, 18 toward the
/// camera, raised or lowered by up to 20 (`cg_ents.c:714-722`).
pub(crate) fn smoke_origin(waist: Vec3, camera_forward: Vec3, random: f32) -> Vec3 {
    let mut origin = waist - camera_forward * 18.0;
    origin.z += random * 20.0;
    origin
}

/// Where `mesh`'s waist (`lower_lumbar`) is in the world, drawn with `transform`.
pub(crate) fn waist(mesh: &ActorMesh, transform: jkr_runtime::Transform) -> Option<Vec3> {
    let mut raw = mesh.force_bones.lumbar?;
    // G2API_GetBoltMatrix normalizes the three basis rows before the world multiplication.
    for row in &mut raw {
        let length = Vec3::new(row[0], row[1], row[2]).length();
        if length != 0.0 {
            for component in &mut row[..3] {
                *component /= length;
            }
        }
    }
    let game = crate::bolt::game_facing(raw);
    let rotation = crate::weapon_view::actor_world_rotation(transform.rotation);
    let offset =
        Vec3::from_array(crate::bolt::column(&game, 3)) * Vec3::from_array(transform.scale);
    Some(Vec3::from_array(transform.translation) + rotation * offset)
}

/// The camera's forward axis (`cg.refdef.viewaxis[0]`) from its yaw and its pitch up, in
/// radians.
pub(crate) fn camera_forward(yaw: f32, pitch_up: f32) -> Vec3 {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch_up.sin_cos();
    Vec3::new(cp * cy, cp * sy, sp)
}

/// The entity flags of the actor `entity` draws: the local player's predicted ones, else
/// what the snapshot says.
pub(crate) fn entity_flags(
    entity: Option<EntityId>,
    local: Option<EntityId>,
    predicted: Option<&jkr_client::pmove::MovementState>,
    snapshot: Option<&jkr_protocol::Snapshot>,
) -> u32 {
    let is_local = entity.is_some() && entity == local;
    if let Some(predicted) = predicted.filter(|_| is_local) {
        return predicted.entity_flags;
    }
    let (Some(id), Some(snapshot)) = (entity, snapshot) else {
        return 0;
    };
    if is_local {
        return snapshot.player.entity_flags();
    }
    let Some(number) = id
        .get()
        .checked_sub(1)
        .and_then(|number| u16::try_from(number).ok())
    else {
        return 0;
    };
    snapshot
        .entities
        .binary_search_by_key(&number, |state| state.number())
        .map_or(0, |index| snapshot.entities[index].e_flags())
}

/// `prepare`'s part: the disintegration observed with this frame's `flags`, and a player's
/// pose held at the legs frame it had when it began (a corpse is at its death frame
/// already). The smoke is rolled from a generator seeded by the actor and the time.
pub(crate) fn hold_pose(
    mesh: &mut ActorMesh,
    flags: u32,
    now: i64,
    state: &mut jkr_runtime::AnimationState,
) {
    let flagged = flags & EF_DISINTEGRATION != 0;
    let legs_frame = (flagged && mesh.dissolve.starting(mesh.entity_id))
        .then(|| legs_frame(mesh, now))
        .flatten();
    // xorshift32, never seeded with zero: presentation only, as stock's `Q_flrand` is.
    let mut seed = (mesh.entity_id.map_or(1, |id| id.get() as u32) ^ (now as u32)) | 1;
    let random = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed >> 8) as f32 / (1 << 24) as f32
    };
    let entity = mesh.entity_id;
    if let Some(frame) = mesh
        .dissolve
        .observe(entity, flagged, now, legs_frame, random)
        && !mesh.corpse_pool
    {
        state.lower.forced_frame = Some(frame as usize);
        state.upper.forced_frame = Some(frame as usize);
        state.lower.transition = None;
        state.upper.transition = None;
    }
}

/// The legs frame `mesh` is at now (`legs.frame`), for the pose to hold.
pub(crate) fn legs_frame(mesh: &ActorMesh, now: i64) -> Option<u32> {
    let [legs, _] = mesh.animator.event_frames(&mesh.preview.animation, now);
    legs.and_then(|(_, frame)| u32::try_from(frame).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: u64) -> Option<EntityId> {
        Some(EntityId::new(value))
    }

    #[test]
    fn a_disintegration_holds_its_first_frame_and_ends_for_a_player_only() {
        let mut dissolve = Dissolve::default();
        let quiet = || 0.0;
        assert_eq!(dissolve.observe(id(3), false, 900, Some(7), quiet), None);
        assert_eq!(dissolve.phase(900, false), None);
        assert!(dissolve.starting(id(3)));
        assert_eq!(
            dissolve.observe(id(3), true, 1_000, Some(42), quiet),
            Some(42)
        );
        assert!(!dissolve.starting(id(3)));
        // The frame is taken once.
        assert_eq!(dissolve.observe(id(3), true, 1_200, None, quiet), Some(42));
        assert_eq!(
            dissolve.phase(2_500, false),
            Some(Phase::Burning { since: 1_000 })
        );
        assert_eq!(dissolve.phase(2_501, false), Some(Phase::Gone));
        assert_eq!(
            dissolve.phase(9_000, true),
            Some(Phase::Burning { since: 1_000 })
        );
        // Another entity in the slot starts afresh; the flag gone, nothing is held.
        assert!(dissolve.starting(id(4)));
        assert_eq!(
            dissolve.observe(id(4), true, 3_000, Some(5), quiet),
            Some(5)
        );
        assert_eq!(
            dissolve.phase(3_000, false),
            Some(Phase::Burning { since: 3_000 })
        );
        assert_eq!(dissolve.observe(id(4), false, 3_100, None, quiet), None);
        assert_eq!(dissolve.phase(3_100, false), None);
    }

    #[test]
    fn smoke_rises_for_a_second_at_a_steady_pace() {
        let mut dissolve = Dissolve::default();
        let always = || 1.0;
        // Two puffs a roll when every draw lands: one roll at the start, then one every
        // 16 ms (16 and 32), none twice.
        let _ = dissolve.observe(id(1), true, 0, None, always);
        assert_eq!(dissolve.puffs(), 2);
        let _ = dissolve.observe(id(1), true, 32, None, always);
        assert_eq!(dissolve.puffs(), 4);
        let _ = dissolve.observe(id(1), true, 32, None, always);
        assert_eq!(dissolve.puffs(), 0);
        let mut total = 0;
        for step in 3..200 {
            let _ = dissolve.observe(id(1), true, step * 16, None, always);
            total += dissolve.puffs();
        }
        // 0..=992 in 16 ms steps is 63 rolls, three of them made before.
        assert_eq!(total, 2 * (63 - 3));
        let mut calm = Dissolve::default();
        let _ = calm.observe(id(1), true, 500, None, || 0.0);
        assert_eq!(calm.puffs(), 0);
    }
}
