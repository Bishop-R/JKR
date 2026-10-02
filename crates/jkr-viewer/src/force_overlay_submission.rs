//! Allocation-free actor custom-shader resubmission.

use crate::entity_materials::{OverrideInstance, OverrideMesh};
use crate::{ActorInstance, EntityId, model_materials};
use jkr_client::{
    LegacyActorOverlayState, LegacyForceOverlayContext, LegacyForceOverlayTracker,
    LegacyOverlayRandom, LegacyOverlayTint, legacy_force_overlays,
};
use jkr_protocol::{GameState, Snapshot};

/// Fixed instance storage shared with pickup overrides.
pub(crate) const CAPACITY: usize = 1_024;

/// Submit every cgame force overlay for one actor using the same pose instance.
#[allow(clippy::too_many_arguments)]
pub(crate) fn submit(
    output: &mut Vec<OverrideInstance>,
    mesh: usize,
    instance: ActorInstance,
    entity_id: EntityId,
    snapshot: &Snapshot,
    game_state: &GameState,
    tracker: &LegacyForceOverlayTracker,
    materials: model_materials::Overrides,
    now: i32,
    third_person: bool,
    aura_shell: bool,
    predicted_force_powers_active: Option<u32>,
    shield_mesh: Option<usize>,
) -> usize {
    let number = u16::try_from(entity_id.get().saturating_sub(1)).unwrap_or(u16::MAX);
    let actor = if number == snapshot.player.client_num() {
        LegacyActorOverlayState::from_player(&snapshot.player, game_state, now)
    } else if let Some(entity) = snapshot
        .entities
        .iter()
        .find(|entity| entity.number() == number)
    {
        LegacyActorOverlayState::from_entity(entity, game_state)
    } else {
        return 0;
    };
    let mut context = LegacyForceOverlayContext::from_player(
        game_state,
        &snapshot.player,
        now,
        third_person,
        aura_shell,
    );
    // `CG_Player` uses `cg.predictedPlayerState` for the local absorb gate
    // (`codemp/cgame/cg_players.c:10953-10958`). Other overlay inputs remain
    // snapshot/currentState driven.
    if let Some(active) = predicted_force_powers_active {
        context.local_force_powers_active = active;
    }
    let mut random = FrameRandom::new(number, now);
    let requests = legacy_force_overlays(actor, tracker.effect(number), context, &mut random);
    let start = output.len();
    // CG_PlayerHitFX: no first-person local shell, dead shell, or vehicle shell.
    let local = number == snapshot.player.client_num();
    let visible = if local {
        third_person && snapshot.player.entity_flags() & 1 == 0
    } else {
        snapshot
            .entities
            .iter()
            .find(|e| e.number() == number)
            .is_some_and(|e| e.e_flags() & 1 == 0 && e.npc_class() != 53)
    };
    if visible
        && output.len() < output.capacity()
        && let Some(mesh) = shield_mesh
        && let Some(material) = materials.force("halfShieldShell")
        && let Some((brightness, scale)) = tracker.shield(number).sample(now, random.unit())
    {
        let direction = glam::Vec3::from_array(tracker.shield(number).direction);
        let yaw = direction.y.atan2(direction.x);
        let pitch = (-direction.z).atan2(direction.truncate().length());
        let rotation = glam::Quat::from_rotation_z(yaw) * glam::Quat::from_rotation_y(pitch);
        let mut origin = instance.position;
        origin[2] += 10.0;
        let mut shell = ActorInstance::new(origin, rotation.to_array(), [scale; 3])
            .with_entity_color([brightness, brightness, brightness, 255]);
        shell.view_flags = instance.view_flags;
        output.push(OverrideInstance {
            mesh: OverrideMesh::Object(mesh),
            material: Some(material),
            instance: shell,
            no_depth: false,
            forced_alpha: false,
        });
    }
    for request in requests.iter() {
        if output.len() == output.capacity() {
            break;
        }
        let Some(material) = materials.force(request.shader) else {
            continue;
        };
        let color = request.rgba.map(|value| f32::from(value) / 255.0);
        let instance = match request.tint {
            LegacyOverlayTint::Shader => instance.with_entity_color(request.rgba),
            LegacyOverlayTint::Rgb => instance.with_rgb_tint(color),
            LegacyOverlayTint::Rgba => instance.with_rgba_tint(color),
        };
        output.push(OverrideInstance {
            mesh: OverrideMesh::Actor(mesh),
            material: Some(material),
            instance,
            no_depth: request.no_depth,
            forced_alpha: false,
        });
    }
    output.len() - start
}

/// Deterministic presentation RNG scoped to entity and rendered millisecond.
struct FrameRandom(u32);

impl FrameRandom {
    fn new(entity: u16, time: i32) -> Self {
        let seed = (time as u32)
            .wrapping_mul(0x9e37_79b9)
            .wrapping_add(u32::from(entity).wrapping_mul(0x85eb_ca6b))
            .max(1);
        Self(seed)
    }

    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
}

impl LegacyOverlayRandom for FrameRandom {
    fn unit(&mut self) -> f32 {
        self.next() as f32 / u32::MAX as f32
    }

    fn bit(&mut self) -> bool {
        self.next() & 1 != 0
    }

    fn byte_1_255(&mut self) -> u8 {
        (self.next() % 255 + 1) as u8
    }
}
