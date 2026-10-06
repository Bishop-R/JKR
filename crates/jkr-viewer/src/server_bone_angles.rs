//! Bones a server turns for the clients (`CG_G2ServerBoneAngles` on the client,
//! `cg_players.c:3960-4011`): a droid's head toward what it looks at (`NPC_SetBoneAngles`,
//! reached from `CG_G2PlayerAngles` for every skeleton that is not the humanoid one, `:4376`),
//! a turret's or an E-Web's cannon (`CG_General`, `cg_ents.c:989-992`). The entity carries
//! up to four `CS_G2BONES` indices and their angles; each named bone is turned after its
//! animation.
use crate::actor_mesh::ActorMesh;
use jkr_client::LegacyGhoul2PosePolicy;
use jkr_model::{BoneAngleCommand, Gla};
use jkr_protocol::{EntityState, GameState};

/// `CS_G2BONES` (`bg_public.h:141`).
const CS_G2BONES: usize = 1_163;
/// The entity's bone slots (`boneIndex1`..`boneIndex4`).
const SLOTS: usize = 4;

/// The turns `state` asks of `animation`'s skeleton, the bone names read from
/// `game_state`'s `CS_G2BONES` strings. A slot of index zero, a name the configstrings lack
/// or a bone the skeleton lacks is skipped, as `CG_G2ServerBoneAngles` skips it; so is a
/// turn about an axis no skeleton has. A turn once installed stays until the server sends
/// another, as it stays on the client's Ghoul2 instance: nothing here clears one.
pub(crate) fn turns<'a>(
    state: &'a EntityState,
    game_state: &'a GameState,
    animation: &'a Gla,
) -> impl Iterator<Item = (usize, BoneAngleCommand)> + 'a {
    let orient = state.bone_orient();
    (0..SLOTS).filter_map(move |slot| {
        let index = usize::from(state.bone_index(slot));
        if index == 0 {
            return None;
        }
        let name = game_state
            .config_string(CS_G2BONES + index)
            .filter(|name| !name.is_empty())?;
        let bone = animation
            .bones
            .iter()
            .position(|bone| bone.name.as_bytes().eq_ignore_ascii_case(name))?;
        let command = LegacyGhoul2PosePolicy::server_bone_angles(state.bone_angles(slot), orient)?;
        Some((bone, command))
    })
}

/// [`turns`] installed on an actor's skeleton.
pub(crate) fn apply(mesh: &mut ActorMesh, state: &EntityState, game_state: &GameState) {
    for (bone, command) in turns(state, game_state, &mesh.preview.animation) {
        // A bone of the skeleton always takes a command; nothing to report otherwise.
        let _ = mesh
            .animator
            .set_bone_angles(&mesh.preview.animation, bone, command);
    }
}
