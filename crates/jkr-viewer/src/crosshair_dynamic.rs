//! On-foot CG_CalcMuzzlePoint and CG_ScanForCrosshairEntity (TaystJK codemp).
use crate::console::ViewerConsole;
use crate::hud::identification::Camera;
use glam::Vec3;
use jkr_client::pmove::MovementState;
use jkr_protocol::{EntityState, GameState, PlayerState};

/// `WP_EMPLACED_GUN`.
const WP_EMPLACED_GUN: u8 = 17;
/// `EF_NODRAW`: a player inside a vehicle is not drawn.
const EF_NODRAW: u32 = 1 << 8;
/// How far above its origin an emplaced gun's muzzle is, and the steepest pitch down it
/// aims at (`CG_CalcMuzzlePoint`).
const EMPLACED_HEIGHT: f32 = 46.0;
const EMPLACED_PITCH: f32 = 40.0;

/// One display-only trace, shared with target identification rather than repeated.
#[derive(Clone, Copy)]
pub(crate) struct Ray {
    /// World-space start, from the ordinary weapon muzzle when enabled.
    pub(crate) start: Vec3,
    /// Normalized firing direction.
    pub(crate) forward: Vec3,
    /// Trace distance in world units.
    pub(crate) distance: f32,
    /// Whether its endpoint moves the HUD crosshair.
    pub(crate) projected: bool,
    /// An entity the trace passes through besides the player: the emplaced gun it mans.
    pub(crate) ignore: Option<u16>,
}

/// Read scalar settings once and select the same ordinary muzzle offsets as cgame.
#[allow(clippy::too_many_arguments)]
pub(crate) fn ray(
    console: Option<&ViewerConsole>,
    player: &PlayerState,
    predicted: Option<&MovementState>,
    game: &GameState,
    entities: &[EntityState],
    camera: Camera,
    third_person: bool,
    weapon: u8,
) -> Ray {
    let mode = console
        .and_then(|c| c.integer_cvar("cg_dynamiccrosshair"))
        .unwrap_or(1);
    let helper = console
        .and_then(|c| c.integer_cvar("cg_strafehelper"))
        .unwrap_or(0) as u32;
    let race = game.config_string(0).is_some_and(|bytes| {
        jkr_client::LegacyClientInfo::new(bytes)
            .bytes("gamename")
            .is_some_and(|name| {
                name.windows(5)
                    .any(|part| part.eq_ignore_ascii_case(b"japro"))
            })
    }) && player.stats[11] != 0;
    // cg_draw.c:9184-9203: mode 1 forces dynamic even for melee; mode 2 is selective.
    let selective_static = mode == 2
        && (weapon <= 3 || race || helper & ((1 << 1) | (1 << 2) | (1 << 3) | (1 << 13)) != 0);
    // A pilot inside a vehicle aims from the vehicle's muzzle and an E-Web's user from its
    // cannon's bolt (`CG_CalcVehicleMuzzlePoint`, `CG_CalcEWebMuzzlePoint`); this display
    // path poses neither and keeps the camera's centre. A rider on top aims as on foot.
    let inside_vehicle = player.vehicle_entity_num() != 0 && player.entity_flags() & EF_NODRAW != 0;
    let emplaced = (weapon == WP_EMPLACED_GUN && player.emplaced_index() != 0)
        .then(|| {
            entities
                .binary_search_by_key(&player.emplaced_index(), EntityState::number)
                .ok()
                .map(|index| &entities[index])
        })
        .flatten();
    let eweb = emplaced.is_some_and(|gun| gun.weapon() == 0);
    let projected =
        mode != 0 && !selective_static && !inside_vehicle && !eweb && weapon <= WP_EMPLACED_GUN;
    let forward = (camera.target - camera.eye).normalize_or_zero();
    if !projected {
        return Ray {
            start: camera.eye,
            forward,
            distance: 131_072.0,
            projected: false,
            ignore: None,
        };
    }
    let view_angles = predicted.map_or(player.view_angles(), |ps| ps.view_angles);
    if let Some(gun) = emplaced {
        // `CG_CalcMuzzlePoint`'s emplaced gun: its origin raised 46 units, the view's pitch
        // held to 40 down, and the gun itself out of the trace.
        let [pitch, yaw] = if third_person {
            [view_angles[0], view_angles[1]]
        } else {
            [
                (-forward.z).asin().to_degrees(),
                forward.y.atan2(forward.x).to_degrees(),
            ]
        };
        return Ray {
            start: Vec3::from_array(gun.trajectory_base()) + Vec3::Z * EMPLACED_HEIGHT,
            forward: direction(pitch.min(EMPLACED_PITCH), yaw),
            distance: 6_000.0,
            projected: true,
            ignore: Some(gun.number()),
        };
    }
    let (origin, direction) = if third_person {
        let origin = predicted.map_or(Vec3::from_array(player.origin()), |ps| ps.origin.into());
        (
            origin + Vec3::Z * player.view_height() as f32,
            direction(view_angles[0], view_angles[1]),
        )
    } else {
        (camera.eye, forward)
    };
    Ray {
        start: muzzle(origin, direction, weapon),
        forward: direction,
        distance: 6_000.0,
        projected: true,
        ignore: None,
    }
}

/// `AngleVectors`' forward for `pitch` and `yaw` in degrees.
fn direction(pitch: f32, yaw: f32) -> Vec3 {
    let (pitch, yaw) = (pitch.to_radians(), yaw.to_radians());
    Vec3::new(
        pitch.cos() * yaw.cos(),
        pitch.cos() * yaw.sin(),
        -pitch.sin(),
    )
}

/// Cgame's ordinary muzzle is an offset table, not the rendered viewmodel's flash bolt.
pub(crate) fn muzzle(eye: Vec3, forward: Vec3, weapon: u8) -> Vec3 {
    // cg_weapons.c:3058-3063 zeroes melee/disruptor offsets; bg_weapons.c:30-49.
    let offset = match weapon {
        4 | 5 | 9 | 10 | 15 | 16 => [12.0, 6.0, -6.0],
        7 => [12.0, 2.0, -6.0],
        8 => [12.0, 4.5, -6.0],
        11 => [12.0, 8.0, -4.0],
        12 | 14 => [12.0, 0.0, -4.0],
        13 => [12.0, 0.0, -10.0],
        _ => [0.0; 3],
    };
    let yaw = forward.y.atan2(forward.x);
    let right = Vec3::new(yaw.sin(), -yaw.cos(), 0.0);
    eye + forward * offset[0] + right * offset[1] + Vec3::Z * offset[2]
}

/// Convert the hit point into the existing shader's upward-positive UV offset.
pub(crate) fn offset(camera: Camera, endpoint: Vec3) -> Option<[f32; 2]> {
    camera.project(endpoint).map(|point| {
        [
            point[0] / camera.viewport[0] - 0.5,
            0.5 - point[1] / camera.viewport[1],
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ps.weapon`, `ps.emplacedIndex`; `s.weapon`, `s.pos.trBase`.
    const PS_WEAPON: usize = 47;
    const PS_EMPLACED_INDEX: usize = 112;
    const ES_WEAPON: usize = 14;
    const ES_BASE: [usize; 3] = [2, 1, 4];

    fn manning(gun_weapon: u32) -> (PlayerState, Vec<EntityState>) {
        let mut player = PlayerState::zero();
        player.set_raw_field(PS_WEAPON, u32::from(WP_EMPLACED_GUN));
        player.set_raw_field(PS_EMPLACED_INDEX, 70);
        player.set_view_angles([60.0, 90.0, 0.0]);
        let mut gun = EntityState::zero(70, &jkr_protocol::LEGACY_ENTITY_FIELDS);
        gun.set_raw_field(ES_WEAPON, gun_weapon);
        for (index, value) in ES_BASE.into_iter().zip([100.0_f32, 200.0, 10.0]) {
            gun.set_raw_field(index, value.to_bits());
        }
        (player, vec![gun])
    }

    fn camera() -> Camera {
        Camera {
            eye: Vec3::new(0.0, 0.0, 64.0),
            target: Vec3::new(0.0, 1.0, 64.0),
            up: Vec3::Z,
            fov: 90.0,
            viewport: [640.0, 480.0],
        }
    }

    #[test]
    fn an_emplaced_gun_aims_from_above_its_origin_held_to_forty_degrees() {
        let (player, entities) = manning(u32::from(WP_EMPLACED_GUN));
        let game = GameState::empty_local(0);
        let ray = ray(None, &player, None, &game, &entities, camera(), true, 17);
        assert!(ray.projected);
        assert_eq!(ray.ignore, Some(70));
        assert_eq!(ray.start, Vec3::new(100.0, 200.0, 56.0));
        let expected = direction(40.0, 90.0);
        assert!((ray.forward - expected).length() < 1e-5);
    }

    #[test]
    fn an_eweb_keeps_the_camera_centre() {
        let (player, entities) = manning(0);
        let game = GameState::empty_local(0);
        let ray = ray(None, &player, None, &game, &entities, camera(), true, 17);
        assert!(!ray.projected);
        assert_eq!(ray.ignore, None);
    }
}
