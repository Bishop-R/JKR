//! How much of a player's model is drawn, and how (`RF_FORCE_ENT_ALPHA` on its `legs`
//! refEntity): a player who mind-tricks the viewer fades out of its sight
//! ([`jkr_client::trick_fade`]), and the viewer's own player is as see-through as
//! `cg_thirdPersonAlpha` asks while the view is from behind (`CG_CheckThirdPersonAlpha`,
//! `cg_players.c:8363-8441`). Stock bolts the held weapon, the hilts and a jetpack into
//! that one Ghoul2 refEntity, so they fade and vanish with the body.

use crate::actor_instance::ActorInstance;
use crate::entity_materials::{OverrideInstance, OverrideMesh};
use jkr_client::trick_fade::BodyDraw;

/// The viewer's own player: `cg_thirdPersonAlpha` while the view is from behind and the
/// player rides nothing. (A ridden vehicle's camera alpha sets `shaderRGBA` without
/// forcing it, which no stock vehicle shader reads, so nothing shows it.)
pub(super) fn own(third_person: bool, riding: u16, alpha: f32) -> BodyDraw {
    if third_person && riding == 0 {
        BodyDraw::see_through(alpha)
    } else {
        BodyDraw::Opaque
    }
}

/// Adds one of a player's bolted parts (the held weapon, a hilt, a jetpack), drawn as the
/// player's `body` is this frame.
pub(super) fn push_part(
    (groups, overrides): (&mut [Vec<ActorInstance>], &mut Vec<OverrideInstance>),
    body: BodyDraw,
    mesh: usize,
    instance: ActorInstance,
) {
    match body {
        BodyDraw::Opaque => groups[mesh].push(instance),
        BodyDraw::Faded(alpha) => {
            overrides.push(faded(OverrideMesh::Object(mesh), instance, alpha))
        }
        BodyDraw::Hidden => {}
    }
}

/// `instance` of `mesh` blended at `alpha` through its own materials.
pub(super) fn faded(mesh: OverrideMesh, instance: ActorInstance, alpha: u8) -> OverrideInstance {
    OverrideInstance {
        mesh,
        material: None,
        instance: instance.with_forced_alpha(alpha),
        no_depth: false,
        forced_alpha: true,
        switched_off: None,
    }
}
