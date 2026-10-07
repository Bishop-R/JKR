//! A cut limb drawn from its copy of the owner's model ([`crate::actor_dismember`]): at the
//! limb's place and angles, the copy moved so that its rotate bone is the limb's origin
//! (`G2API_SetNewOrigin`, which moves the model and does not turn it; `RootMatrix`,
//! `tr_ghoul2.cpp:3164-3196`).

use super::{ActorInstance, Sinks, weapon_view};
use glam::Vec3;

/// Draws `entity` through its limb copy at `transform`; `false` when it has none.
pub(super) fn submit(
    sinks: &mut Sinks<'_>,
    entity: &jkr_runtime::SceneEntity,
    transform: jkr_runtime::Transform,
) -> bool {
    let Some(mesh) = sinks
        .actor_meshes
        .iter()
        .position(|mesh| mesh.cuts.limb.is_some() && mesh.entity_id == Some(entity.id))
    else {
        return false;
    };
    let rotation = weapon_view::actor_world_rotation(transform.rotation);
    let pivot = crate::actor_dismember::pivot(&sinks.actor_meshes[mesh]).unwrap_or(Vec3::ZERO);
    let origin = Vec3::from_array(transform.translation) - rotation * pivot;
    let mut instance = ActorInstance::new(origin.to_array(), rotation.to_array(), transform.scale)
        .with_entity_color(entity.color());
    instance.view_flags = sinks.entity_view_flags;
    sinks.actor_groups[mesh].push(instance);
    true
}
