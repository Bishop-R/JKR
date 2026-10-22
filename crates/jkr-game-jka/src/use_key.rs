//! The use key (`TryUse`, OpenJK `codemp/game/g_utils.c:1588-1760`): what a player
//! pressing `BUTTON_USE` reaches, and whether that thing answers.
//!
//! This is one of the shared pieces of the game rather than any one entity's: siege
//! objectives, `func_usable`, the buttons and every `PLAYER_USE` door and lift all wait
//! on it, which is why it lives on its own rather than inside one of them.

/// `USE_DISTANCE` (`g_utils.c:1588`): how far in front of its eyes a player can reach.
pub const USE_DISTANCE: f32 = 64.0;
/// `SVF_PLAYER_USABLE`, the flag that says the use key may reach a thing at all.
pub const SVF_PLAYER_USABLE: u32 = 0x0010;
/// `FL_INACTIVE`, which `target_deactivate` sets.
pub const FL_INACTIVE: u32 = 0x10000;
/// `BUTTON_USE`.
pub const BUTTON_USE: u16 = 32;
/// `MASK_OPAQUE | CONTENTS_SOLID | CONTENTS_BODY | CONTENTS_ITEM | CONTENTS_CORPSE`, the
/// mask `TryUse` traces with — so the reach is stopped by a wall, a player or a body.
pub const USE_MASK: u32 = 0x1 | 0x40 | 0x2000_0000 | 0x100 | 0x4000_0000;

/// `G_ValidUseEnt` (`g_utils.c:1428-1444`): whether the thing a player is looking at
/// answers the use key at all. Three separate refusals, in the reference's own order: it
/// has nothing to do when used, something deactivated it, or it was never usable.
pub fn answers_use(has_use: bool, flags: u32, svflags: u32) -> bool {
    if !has_use {
        return false;
    }
    if flags & FL_INACTIVE != 0 {
        return false;
    }
    svflags & SVF_PLAYER_USABLE != 0
}

/// Where `TryUse`'s trace runs from and to: a player's eyes, and `USE_DISTANCE` along
/// the way it is looking. The angles are degrees, as a player state keeps them.
pub fn reach(eyes: [f32; 3], view_angles: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let (yaw_sin, yaw_cos) = view_angles[1].to_radians().sin_cos();
    let (pitch_sin, pitch_cos) = view_angles[0].to_radians().sin_cos();
    // `AngleVectors`' forward: pitch is measured downwards, which is why it is negated.
    let forward = [pitch_cos * yaw_cos, pitch_cos * yaw_sin, -pitch_sin];
    (
        eyes,
        std::array::from_fn(|axis| eyes[axis] + forward[axis] * USE_DISTANCE),
    )
}
