//! The spheres round a player (`CG_DrawPlayerSphere`, `cg_players.c:6574-6658`, drawn from
//! `CG_Player`, `9911-9945`): the spawn bubble while it cannot be hurt (`EF_INVULNERABLE`),
//! the ysalamiri's (red or blue on a flag carrier in Capture the Ysalamiri), Force Boon's,
//! and the dark or light enlightenment's. Each is stock's half shield model nine units
//! above the player's origin, turned to face away from the viewer and scaled; none on a
//! dead player.

use super::{ActorInstance, Sinks};
use crate::entity_materials::{OverrideInstance, OverrideMesh};
use glam::Vec3;
use jkr_protocol::{EntityState, Snapshot};

/// `EF_DEAD`, `EF_INVULNERABLE`.
const EF_DEAD: u32 = 1 << 1;
const EF_INVULNERABLE: u32 = 1 << 27;
/// `PW_REDFLAG`, `PW_BLUEFLAG`, `PW_FORCE_ENLIGHTENED_LIGHT` .. `PW_YSALAMIRI`.
const PW_REDFLAG: u32 = 4;
const PW_BLUEFLAG: u32 = 5;
const PW_FORCE_ENLIGHTENED_LIGHT: u32 = 12;
const PW_FORCE_ENLIGHTENED_DARK: u32 = 13;
const PW_FORCE_BOON: u32 = 14;
const PW_YSALAMIRI: u32 = 15;
/// `GT_CTY`.
const GT_CTY: i64 = 9;

/// The spheres of a player at `origin` (`lerpOrigin`) with `flags` and `powerups`, in
/// game type `gametype`.
pub(super) fn submit(
    sinks: &mut Sinks<'_>,
    origin: [f32; 3],
    (flags, powerups): (u32, u32),
    gametype: i64,
) {
    if flags & EF_DEAD != 0 {
        return;
    }
    let has = |powerup: u32| powerups & (1 << powerup) != 0;
    let carries = |flag: u32| gametype == GT_CTY && has(flag);
    let ysalamiri = if carries(PW_REDFLAG) {
        Some("powerups/ysaliredshell")
    } else if carries(PW_BLUEFLAG) {
        Some("powerups/ysaliblueshell")
    } else if has(PW_YSALAMIRI) {
        Some("powerups/ysalimarishell")
    } else {
        None
    };
    let enlightenment = if has(PW_FORCE_ENLIGHTENED_DARK) {
        Some("powerups/endarkenmentshell")
    } else if has(PW_FORCE_ENLIGHTENED_LIGHT) {
        Some("powerups/enlightenmentshell")
    } else {
        None
    };
    let spheres = [
        (ysalamiri, 1.4),
        (has(PW_FORCE_BOON).then_some("powerups/boonshell"), 2.0),
        (enlightenment, 2.0),
        (
            (flags & EF_INVULNERABLE != 0).then_some("powerups/invulnerabilityshell"),
            1.0,
        ),
    ];
    for (shader, scale) in spheres {
        if let Some(shader) = shader {
            sphere(sinks, origin, scale, shader);
        }
    }
}

/// One sphere: the half shield model at `origin` raised nine units, its open side away
/// from the viewer (`vectoangles` of the view's way, rolled and pitched half a turn). Its
/// entity carries no render flags, so the player's own sphere shows in first person too.
fn sphere(sinks: &mut Sinks<'_>, origin: [f32; 3], scale: f32, shader: &str) {
    let (Some(mesh), Some(material)) = (sinks.shield_mesh, sinks.material_overrides.force(shader))
    else {
        return;
    };
    let center = Vec3::from_array(origin) + Vec3::Z * 9.0;
    let way = center - sinks.view_origin;
    if way.length() <= 0.1 {
        return;
    }
    let mut yaw = way.y.atan2(way.x).to_degrees();
    if yaw < 0.0 {
        yaw += 360.0;
    }
    let pitch = -way.z.atan2(way.truncate().length()).to_degrees();
    let rotation = jkr_client::legacy_angles_to_quaternion([pitch + 180.0, yaw, 180.0]);
    let instance = ActorInstance::new(center.to_array(), rotation, [scale; 3]);
    if sinks.overrides.len() < sinks.overrides.capacity() {
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

/// A player's `eFlags` and powerup bits: its entity's, or for the viewer's own player
/// (`state` `None`) its player state's, as `BG_PlayerStateToEntityState` makes them.
pub(super) fn player_bits(state: Option<&EntityState>, snapshot: Option<&Snapshot>) -> (u32, u32) {
    match (state, snapshot) {
        (Some(state), _) => (state.e_flags(), state.powerups()),
        (None, Some(snapshot)) => {
            let player = &snapshot.player;
            let flags = if player.health() <= 0 {
                player.entity_flags() | EF_DEAD
            } else {
                player.entity_flags() & !EF_DEAD
            };
            let powerups = player
                .powerups
                .iter()
                .enumerate()
                .filter(|(_, until)| **until != 0)
                .fold(0, |bits, (index, _)| bits | (1 << index));
            (flags, powerups)
        }
        (None, None) => (0, 0),
    }
}
