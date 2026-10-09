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
/// game type `gametype`; `own` for the viewer's player, `team` its team. jaPRO's
/// `cg_stylePlayer` can hide one's own ysalamiri shell and colour spawn bubbles by team
/// (EternalJK `cg_players.c:11610-11650`).
pub(super) fn submit(
    sinks: &mut Sinks<'_>,
    origin: [f32; 3],
    (flags, powerups): (u32, u32),
    (gametype, own, team): (i64, bool, i64),
) {
    use jkr_client::style_player::{HIDE_OWN_YSALAMIRI, TEAM_SPAWN_BUBBLE};
    if flags & EF_DEAD != 0 {
        return;
    }
    let has = |powerup: u32| powerups & (1 << powerup) != 0;
    let carries = |flag: u32| gametype == GT_CTY && has(flag);
    let ysalamiri = if carries(PW_REDFLAG) {
        Some("powerups/ysaliredshell")
    } else if carries(PW_BLUEFLAG) {
        Some("powerups/ysaliblueshell")
    } else if has(PW_YSALAMIRI) && !(own && sinks.style & HIDE_OWN_YSALAMIRI != 0) {
        Some("powerups/ysalimarishell")
    } else {
        None
    };
    // `GT_TEAM` and above, but not Capture the Ysalamiri; red is `TEAM_RED`.
    let bubble = if sinks.style & TEAM_SPAWN_BUBBLE != 0 && gametype >= 6 && gametype != GT_CTY {
        if team == 1 {
            ("powerups/ysaliredshell", 1.2)
        } else {
            ("powerups/ysaliblueshell", 1.2)
        }
    } else {
        ("powerups/invulnerabilityshell", 1.0)
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
        ((flags & EF_INVULNERABLE != 0).then_some(bubble.0), bubble.1),
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

/// The DEMP2's bursting alternate-fire shells (`effect_aux::demp2_shell`), the sphere model
/// under their shader, unturned, shaded as `RF_VOLUMETRIC`.
pub(super) fn submit_demp2_shells(sinks: &mut Sinks<'_>, now: std::time::Instant) {
    use crate::effect_aux::demp2_shell;
    let Some(material) = sinks.material_overrides.force(demp2_shell::SHADER) else {
        return;
    };
    let mut mesh = None;
    for shape in sinks.effect_aux.demp2_shells.frame(now) {
        let Some(mesh) = *mesh.get_or_insert_with(|| {
            sinks
                .object_meshes
                .iter()
                .position(|mesh| mesh.appearance.model == demp2_shell::MODEL)
        }) else {
            return;
        };
        let mut instance =
            ActorInstance::new(shape.origin.to_array(), [0.0, 0.0, 0.0, 1.0], shape.scale)
                .volumetric();
        instance.entity_color = [shape.level, shape.level, shape.level, 0.0];
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
}
