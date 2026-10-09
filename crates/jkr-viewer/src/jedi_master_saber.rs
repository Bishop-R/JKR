//! A saber missile, the Jedi Master's while nobody holds it (`CG_Missile`,
//! `cg_ents.c:2452-2483,2585-2727`): an `ET_MISSILE` of `WP_SABER` drawn with the model at its
//! `modelindex` in `CS_MODELS` (stock's own saber when that is empty) and turned as a missile
//! is. In Jedi Master it glows as a saber pulled back does and, inside a yellow
//! `forceSightBubble` drawn over the world (`RF_DEPTHHACK`), shows through walls.

use super::*;
use crate::actor_world_submission::thrown_saber;
use crate::entity_materials::{OverrideInstance, OverrideMesh};

/// `CS_MODELS`, `ET_MISSILE`, `WP_SABER`, `EF_NODRAW`, `GT_JEDIMASTER`.
const CS_MODELS: usize = 298;
const ET_MISSILE: u8 = 3;
const WP_SABER: u8 = 3;
const EF_NODRAW: u32 = 1 << 8;
const GT_JEDIMASTER: i64 = 2;
/// `DEFAULT_SABER_MODEL`.
const DEFAULT_MODEL: &str = "models/weapons2/saber/saber_w.glm";
/// `cgs.media.forceSightBubble`.
const SIGHT_BUBBLE: &str = "gfx/misc/sightbubble";

/// Every saber missile in `snapshot`.
pub(super) fn submit(
    sinks: &mut Sinks<'_>,
    snapshot: &Snapshot,
    game_state: &jkr_protocol::GameState,
    now: Instant,
) {
    for state in snapshot.entities.iter().filter(|state| {
        state.entity_type() == ET_MISSILE
            && state.weapon() == WP_SABER
            && state.e_flags() & EF_NODRAW == 0
    }) {
        let model = usize::try_from(state.model_index())
            .ok()
            .and_then(|index| game_state.config_string(CS_MODELS + index))
            .and_then(|name| std::str::from_utf8(name).ok())
            .filter(|name| !name.is_empty())
            .unwrap_or(DEFAULT_MODEL);
        let Some(mesh) = sinks
            .object_meshes
            .iter()
            .position(|mesh| mesh.appearance.model.eq_ignore_ascii_case(model))
        else {
            continue;
        };
        let missile = crate::projectiles::present(state, sinks.presentation_time as i32);
        let rotation = if sinks.object_meshes[mesh].ghoul2 {
            weapon_view::actor_world_rotation(missile.rotation).to_array()
        } else {
            missile.rotation
        };
        let pose = jkr_runtime::Transform {
            translation: missile.origin,
            rotation,
            scale: [1.0; 3],
        };
        sinks.object_groups[mesh].push(ActorInstance::new(pose.translation, rotation, pose.scale));
        if sinks.gametype != GT_JEDIMASTER {
            continue;
        }
        thrown_saber::saber_glow(sinks, pose, mesh, now);
        if let Some(material) = sinks.material_overrides.force(SIGHT_BUBBLE)
            && sinks.overrides.len() < sinks.overrides.capacity()
        {
            let mut instance = ActorInstance::new(pose.translation, rotation, pose.scale)
                .with_rgb_tint([1.0, 1.0, 0.0, 1.0]);
            instance.depth_hack = 1.0;
            sinks.overrides.push(OverrideInstance {
                mesh: OverrideMesh::Object(mesh),
                material: Some(material),
                instance,
                no_depth: false,
                forced_alpha: false,
                switched_off: None,
            });
        }
    }
}
