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

/// `func_usable` (`SP_func_usable`, `g_mover.c:3160-3215`): a brush a player uses to
/// fire its targets, and which a map may start hidden and bring back solid.
#[derive(Clone, Debug, PartialEq)]
pub struct Usable {
    /// The inline model it is made of, and that model's box.
    pub model: usize,
    pub bounds: ([f32; 3], [f32; 3]),
    /// What it fires when used.
    pub target: String,
    pub targetname: String,
    pub spawnflags: u32,
    /// `count`: whether it is there at all. A START_OFF brush is not.
    pub present: bool,
    pub contents: u32,
    pub svflags: u32,
    pub eflags: u32,
    /// `wait`, in milliseconds, before a hidden one comes back.
    pub wait: i32,
}

/// `SP_func_usable`'s own spawnflags.
const USABLE_START_OFF: u32 = 1;
/// `ALWAYS_ON` (8): using it fires its targets and it never goes away.
pub const USABLE_ALWAYS_ON: u32 = 8;
/// `SVF_NOCLIENT` and `EF_NODRAW`, which a hidden one wears.
const SVF_NOCLIENT: u32 = 0x0001;
const EF_NODRAW: u32 = 1 << 7;
/// `CONTENTS_SOLID`.
const CONTENTS_SOLID: u32 = 1;

/// `SP_func_usable`. A brush that starts off is not in the world at all — no contents,
/// not drawn, not sent — until something brings it back.
pub fn spawn_usable(entity: &jkr_entity::Entity, bounds: ([f32; 3], [f32; 3])) -> Option<Usable> {
    if entity.get("classname") != Some("func_usable") {
        return None;
    }
    let model = entity
        .get("model")
        .and_then(|name| name.strip_prefix('*'))
        .and_then(|index| index.parse().ok())?;
    let number = |key: &str| {
        entity
            .get(key)
            .and_then(|text| text.trim().parse::<f32>().ok())
            .unwrap_or(0.0)
    };
    let spawnflags = number("spawnflags") as u32;
    let off = spawnflags & USABLE_START_OFF != 0;
    Some(Usable {
        model,
        bounds,
        target: entity.get("target").unwrap_or_default().to_owned(),
        targetname: entity.get("targetname").unwrap_or_default().to_owned(),
        spawnflags,
        present: !off,
        contents: if off { 0 } else { CONTENTS_SOLID },
        svflags: if off { SVF_NOCLIENT } else { SVF_PLAYER_USABLE },
        eflags: if off { EF_NODRAW } else { 0 },
        wait: (number("wait") * 1_000.0) as i32,
    })
}
