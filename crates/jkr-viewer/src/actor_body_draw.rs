//! How much of a player's model is drawn, and how (`RF_FORCE_ENT_ALPHA` on its `legs`
//! refEntity): a player who mind-tricks the viewer fades out of its sight
//! ([`jkr_client::trick_fade`]), and the viewer's own player is as see-through as
//! `cg_thirdPersonAlpha` asks while the view is from behind (`CG_CheckThirdPersonAlpha`,
//! `cg_players.c:8363-8441`). Stock bolts the held weapon, the hilts and a jetpack into
//! that one Ghoul2 refEntity, so they fade and vanish with the body.

use crate::actor_instance::ActorInstance;
use crate::entity_materials::{OverrideInstance, OverrideMesh};
use jkr_client::cloak_fade::CloakDraw;
use jkr_client::trick_fade::BodyDraw;

/// `cgs.media.cloakedShader`. Stock's default draws a fully cloaked player through its
/// refraction pass (`cg_renderToTextureFX 1`), which JKR does not have; this is what stock
/// draws without it.
pub(crate) const CLOAK_SHADER: &str = "gfx/effects/cloakedShader";

/// How a cloak draws a player this frame ([`jkr_client::cloak_fade`]), its bolted parts with
/// it: the model faded to an alpha (`None` not at all, 255 whole), and the cloak's shell
/// (its material and level).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Cloak {
    model: Option<u8>,
    shell: Option<(usize, f32)>,
}

/// What `draw` asks of a player, `own` the viewer's, `sight` whether the viewer's Force
/// Sight is on; `None` when the cloak changes nothing ("just draw him").
pub(super) fn cloak(
    draw: CloakDraw,
    (own, third_person, sight): (bool, bool, bool),
    material: Option<usize>,
) -> Option<Cloak> {
    if sight && !own {
        return None;
    }
    match draw {
        CloakDraw::Plain => None,
        // Uncloaking, stock also draws the model whole, under which the faded copy shows
        // nothing; only the whole model is drawn.
        CloakDraw::Turning { level, cloaking } => Some(Cloak {
            model: Some(if cloaking {
                (255.0 * (1.0 - level)) as u8
            } else {
                255
            }),
            shell: material.map(|material| (material, level)),
        }),
        CloakDraw::Cloaked => Some(Cloak {
            model: None,
            shell: material
                .filter(|_| third_person || !own)
                .map(|material| (material, 1.0)),
        }),
    }
}

/// The passes `body` and `cloak` ask of one model (`mesh` at `instance`): the faded copy
/// and the cloak's shell (`RF_RGB_TINT` at its level) go to `overrides`; `true` when the
/// model is also drawn as its shaders say.
pub(super) fn passes(
    overrides: &mut Vec<OverrideInstance>,
    (body, cloak): (BodyDraw, Option<Cloak>),
    mesh: OverrideMesh,
    instance: ActorInstance,
) -> bool {
    let model = match (body, cloak) {
        (BodyDraw::Hidden, _) => return false,
        (BodyDraw::Opaque, None) => return true,
        (BodyDraw::Faded(alpha), None) => Some(alpha),
        (_, Some(cloak)) => cloak.model,
    };
    if let Some((material, level)) = cloak.and_then(|cloak| cloak.shell)
        && overrides.len() < overrides.capacity()
    {
        overrides.push(OverrideInstance {
            mesh,
            material: Some(material),
            instance: instance.with_rgb_tint([level, level, level, 1.0]),
            no_depth: false,
            forced_alpha: false,
            switched_off: None,
        });
    }
    match model {
        Some(255) if cloak.is_some() => true,
        Some(alpha) => {
            overrides.push(faded(mesh, instance, alpha));
            false
        }
        None => false,
    }
}

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
/// player's `body` and `cloak` are this frame.
pub(super) fn push_part(
    (groups, overrides): (&mut [Vec<ActorInstance>], &mut Vec<OverrideInstance>),
    draw: (BodyDraw, Option<Cloak>),
    mesh: usize,
    instance: ActorInstance,
) {
    if passes(overrides, draw, OverrideMesh::Object(mesh), instance) {
        groups[mesh].push(instance);
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

#[cfg(test)]
mod cloak_tests {
    use super::*;

    #[test]
    fn a_cloak_resolves_as_stock_draws_it() {
        let turning = CloakDraw::Turning {
            level: 0.25,
            cloaking: true,
        };
        // Force Sight sees another player whole.
        assert_eq!(cloak(turning, (false, false, true), Some(7)), None);
        assert_eq!(
            cloak(turning, (false, false, false), Some(7)),
            Some(Cloak {
                model: Some(191),
                shell: Some((7, 0.25))
            })
        );
        // Fully cloaked, the viewer's own first-person view shows nothing at all.
        let own = cloak(CloakDraw::Cloaked, (true, false, false), Some(7)).unwrap();
        assert_eq!((own.model, own.shell), (None, None));
        let seen = cloak(CloakDraw::Cloaked, (false, false, false), Some(7)).unwrap();
        assert_eq!(seen.shell, Some((7, 1.0)));
    }

    #[test]
    fn a_cloaked_model_is_only_its_shell() {
        let mut overrides = Vec::with_capacity(4);
        let instance = ActorInstance::new([0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0; 3]);
        let mesh = OverrideMesh::Object(3);
        assert!(passes(&mut overrides, (BodyDraw::Opaque, None), mesh, instance));
        assert!(overrides.is_empty());
        let cloaked = Cloak {
            model: None,
            shell: Some((7, 1.0)),
        };
        assert!(!passes(&mut overrides, (BodyDraw::Opaque, Some(cloaked)), mesh, instance));
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].material, Some(7));
    }
}
