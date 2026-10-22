//! Force Speed's afterimages drawn ([`jkr_client::speed_trail`]): copies of the player's
//! body as posed this frame, blended at their alphas whatever their shaders say
//! (`RF_FORCE_ENT_ALPHA`), with its view flags (so the viewer's own stays out of its eyes).

use super::{ActorInstance, Sinks, body_draw};
use crate::entity_materials::OverrideMesh;
use jkr_protocol::{EntityState, Snapshot};

/// `PW_SPEED`.
const PW_SPEED: u32 = 10;

/// The afterimages of the actor `mesh` drawn as `instance` at `time`. `state` is its
/// entity's state, `None` for the viewer's own player, whose snapshot and predicted state
/// say what it has and how it moves; `tricking` holds while it fades from a mind trick.
pub(super) fn submit(
    sinks: &mut Sinks<'_>,
    mesh: usize,
    instance: ActorInstance,
    (state, snapshot): (Option<&EntityState>, Option<&Snapshot>),
    tricking: bool,
    time: i64,
) {
    let (powerups, delta) = match state {
        Some(state) => (state.powerups(), state.trajectory_delta()),
        None => {
            let Some(player) = snapshot.map(|snapshot| &snapshot.player) else {
                return;
            };
            // `BG_PlayerStateToEntityState`: a powerup's bit for every one held.
            let powerups = player
                .powerups
                .iter()
                .enumerate()
                .filter(|(_, until)| **until != 0)
                .fold(0, |bits, (index, _)| bits | (1 << index));
            let delta = sinks
                .predicted_local_state
                .map_or(player.velocity(), |state| state.velocity);
            (powerups, delta)
        }
    };
    let speeding = sinks.speed_trails && powerups & (1 << PW_SPEED) != 0 && !tricking;
    let actor = &sinks.actor_meshes[mesh];
    let mut trail = actor.speed_trail.get();
    let images = trail.frame(speeding, instance.position, delta, time);
    actor.speed_trail.set(trail);
    for (origin, alpha) in images.into_iter().flatten() {
        let mut copy = instance;
        copy.position = origin;
        sinks
            .overrides
            .push(body_draw::faded(OverrideMesh::Actor(mesh), copy, alpha));
    }
}
