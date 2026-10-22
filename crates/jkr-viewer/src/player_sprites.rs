//! `CG_PlayerSprites` / `CG_PlayerFloatSprite` (`codemp/cgame/cg_players.c`): the
//! connection icon or talk balloon over a player's head, as a frame billboard in the
//! effect pool that `pickups::simple::append_frame` clears on the next frame.
use super::*;

/// `ET_PLAYER`: the local player is drawn from its own (predicted) player state.
const ET_PLAYER: u8 = 1;

/// Float the stock sprite over one submitted actor, without growing the pool.
#[allow(clippy::too_many_arguments)]
pub(super) fn submit(
    sinks: &mut Sinks<'_>,
    kind: EntityKind,
    origin: [f32; 3],
    snapshot: &Snapshot,
    state: Option<&jkr_protocol::EntityState>,
    local: bool,
    draw_actor: bool,
    now: Instant,
) {
    // The local first-person model is `RF_THIRD_PERSON`, and so is its sprite; no
    // player is drawn during intermission (`CG_Player`), sprites included.
    if !draw_actor
        || kind != EntityKind::Actor
        || snapshot.player.movement_type() == jkr_client::PM_INTERMISSION
    {
        return;
    }
    let (flags, entity_type, mind_tricked) = if local {
        let flags = sinks
            .predicted_local_state
            .map_or(snapshot.player.entity_flags(), |state| state.entity_flags);
        (flags, ET_PLAYER, false)
    } else if let Some(state) = state {
        (
            state.e_flags(),
            state.entity_type(),
            state.client_bitflag(snapshot.player.client_num()),
        )
    } else {
        return;
    };
    let client = if local {
        snapshot.player.client_num()
    } else {
        state.map_or(u16::MAX, |s| s.number())
    };
    let speaking = sinks
        .effect_aux
        .feedback
        .speaking(client, sinks.presentation_time as i32);
    let Some(sprite) =
        jkr_client::legacy_player_sprite_with_voice(flags, entity_type, mind_tricked, speaking)
    else {
        return;
    };
    if sinks.particles.len() == sinks.particles.capacity() {
        return;
    }
    let origin = Vec3::from_array(origin) + Vec3::Z * jkr_client::LEGACY_PLAYER_SPRITE_HEIGHT;
    sinks.particles.push(Particle::frame_sprite(
        sinks.effects,
        origin,
        jkr_client::LEGACY_PLAYER_SPRITE_RADIUS,
        sprite.shader(),
        ([1.0; 3], 1.0),
        now,
    ));
}
