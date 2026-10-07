//! An entity a player carries (`boltToPlayer`; in multiplayer, a Siege item its
//! `SiegeItemThink` keeps on the carrier), drawn as `CG_General` draws it
//! (`cg_ents.c:897-925`, `CG_RenderTimeEntBolt` `732-774`): in the carrier's left hand,
//! facing the way the hand does, upright; not at all while the carrier mind-tricks the
//! viewer, nor where its model is missing; and not in the world for a carrier that sees
//! from its own eyes, whose HUD shows it instead (`CG_DrawSiegeHUDItem`, not drawn by this
//! client yet).

use super::Sinks;
use glam::Vec3;
use jkr_protocol::Snapshot;
use jkr_runtime::{EntityId, SceneEntity, Transform};

/// `ET_GENERAL`: other entity types reuse the field (an NPC packs its saber colours there).
const ET_GENERAL: u8 = 0;
/// The bone `CG_RenderTimeEntBolt` bolts to.
const LEFT_HAND: &str = "lhand";

/// Where a carried entity is drawn this frame.
pub(super) enum Placement {
    /// Nobody carries it: where it is.
    Free,
    /// Nowhere.
    Hidden,
    /// In its carrier's hand.
    Held(Transform),
}

/// `CG_General`'s `boltToPlayer` branch for `entity` (drawn at `transform`) at `time`.
pub(super) fn place(
    sinks: &Sinks<'_>,
    snapshot: Option<&Snapshot>,
    entity: &SceneEntity,
    transform: Transform,
    local_entity_id: Option<u64>,
    time: i64,
) -> Placement {
    let Some(snapshot) = snapshot else {
        return Placement::Free;
    };
    let number = entity.id.get().wrapping_sub(1);
    let Some(state) = u16::try_from(number)
        .ok()
        .and_then(|number| state_of(snapshot, number))
    else {
        return Placement::Free;
    };
    let bolt = state.bolt_to_player();
    if bolt == 0 || state.entity_type() != ET_GENERAL {
        return Placement::Free;
    }
    let carrier = u16::from(bolt) - 1;
    // `CG_IsMindTricked(pl->currentState.trickedentindex…, cg.predictedPlayerState.clientNum)`.
    let viewer = snapshot.player.client_num();
    if state_of(snapshot, carrier).is_some_and(|state| state.client_bitflag(viewer)) {
        return Placement::Hidden;
    }
    let id = EntityId::new(u64::from(carrier) + 1);
    let local = local_entity_id == Some(id.get());
    if local && !sinks.third_person {
        return Placement::Hidden;
    }
    // `cl->ghoul2`, `cl->lerpOrigin`, `cl->turAngles`.
    let Some(mesh) = sinks
        .actor_meshes
        .iter()
        .find(|mesh| mesh.entity_id == Some(id))
    else {
        return Placement::Hidden;
    };
    let origin = if local {
        sinks.camera_position - Vec3::Z * sinks.view_height
    } else {
        match sinks.world.entity(id) {
            Some(carrier) => Vec3::from_array(carrier.sample(time).translation),
            None => return Placement::Hidden,
        }
    };
    let yaw = mesh.render_yaw_degrees.unwrap_or(0.0);
    let roll = mesh.angle_controller.root_roll_degrees();
    let raw = jkr_game_jka::ghoul2_bolt::model_bolt(
        &mesh.preview.mesh,
        &mesh.preview.animation,
        LEFT_HAND,
        mesh.animator.matrices(),
    )
    .ok()
    .flatten();
    let Some(raw) = raw else {
        // The engine answers a missing bolt with the model's origin.
        return Placement::Held(Transform {
            translation: origin.to_array(),
            rotation: jkr_client::legacy_angles_to_quaternion([0.0; 3]),
            ..transform
        });
    };
    let matrix =
        jkr_game_jka::ghoul2_bolt::world_bolt(raw, [0.0, yaw, roll], origin.to_array(), [0.0; 3]);
    // `BG_GiveMeVectorFromMatrix`'s ORIGIN and NEGATIVE_Y, then `vectoangles` with the
    // pitch and roll dropped.
    let point = [matrix[0][3], matrix[1][3], matrix[2][3]];
    let facing = [-matrix[0][1], -matrix[1][1]];
    let mut hand_yaw = facing[1].atan2(facing[0]).to_degrees();
    if hand_yaw < 0.0 {
        hand_yaw += 360.0;
    }
    Placement::Held(Transform {
        translation: point,
        rotation: jkr_client::legacy_angles_to_quaternion([0.0, hand_yaw, 0.0]),
        ..transform
    })
}

/// Entity `number`'s state in `snapshot`, whose entities are in number order.
fn state_of(snapshot: &Snapshot, number: u16) -> Option<&jkr_protocol::EntityState> {
    snapshot
        .entities
        .binary_search_by_key(&number, |state| state.number())
        .ok()
        .map(|index| &snapshot.entities[index])
}
