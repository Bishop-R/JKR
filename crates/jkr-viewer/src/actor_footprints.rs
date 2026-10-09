//! What a footstep leaves besides its sound (`_PlayerFootStep`, `cg_players.c:2039-2232`):
//! from `cg_footsteps 2` dust where the ground is mud, dirt, sand, snow or gravel, and
//! from 3 a footprint on mud, dirt, sand and snow, on every ground at 4. A print is a
//! mark of radius 6 turned with the legs, which cgame keeps for ten seconds
//! (`CG_ImpactMark`, see `decal_cgame_marks`).

use crate::decal_marks::DecalRequest;
use glam::Vec3;
use jkr_client::animation_events::footsteps;
use std::sync::{Arc, LazyLock};

/// The print shaders, left and right foot, walking and then running
/// (`cgs.media.fs*MarkShader`); the effect atlas loads them.
pub(crate) const SHADERS: [&str; 4] = [
    "footstep_l",
    "footstep_r",
    "footstep_heavy_l",
    "footstep_heavy_r",
];
/// `_PlayerFootStep`'s `radius`.
const PRINT_RADIUS: f32 = 6.0;

static PRINTS: LazyLock<[Arc<str>; 4]> = LazyLock::new(|| SHADERS.map(Arc::from));

/// One step that found the ground.
pub(super) struct Step {
    /// `cg_footsteps`.
    pub(super) level: i64,
    /// The ground's [`footsteps::material`] family.
    pub(super) family: usize,
    pub(super) right: bool,
    pub(super) heavy: bool,
    /// Where the trace stopped, and the ground's normal there.
    pub(super) ground: Vec3,
    pub(super) normal: Option<Vec3>,
    /// The legs' yaw in degrees (`cent->pe.legs.yawAngle`).
    pub(super) yaw: f32,
}

/// The dust to raise for `step`, if any.
pub(super) fn dust(step: &Step) -> Option<&'static str> {
    (step.level >= 2)
        .then(|| footsteps::effect(step.family))
        .flatten()
}

/// The print `step` leaves, if any; stock leaves none where the trace found no plane.
pub(super) fn print(step: &Step) -> Option<DecalRequest> {
    let shown = step.level >= 4 || (step.level >= 3 && footsteps::takes_print(step.family));
    let normal = step.normal.filter(|normal| *normal != Vec3::ZERO)?;
    shown.then(|| DecalRequest {
        origin: step.ground,
        direction: normal,
        orientation: step.yaw,
        color: [1.0; 4],
        radius: PRINT_RADIUS,
        shader: Arc::clone(&PRINTS[usize::from(step.heavy) * 2 + usize::from(step.right)]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(level: i64, family: usize) -> Step {
        Step {
            level,
            family,
            right: true,
            heavy: true,
            ground: Vec3::new(1.0, 2.0, 3.0),
            normal: Some(Vec3::Z),
            yaw: 90.0,
        }
    }

    #[test]
    fn the_levels_add_dust_and_then_prints() {
        // Sand (family 3), stone (0).
        assert_eq!(dust(&step(1, 3)), None);
        assert_eq!(dust(&step(2, 3)), Some("materials/sand"));
        assert!(print(&step(2, 3)).is_none());
        let mark = print(&step(3, 3)).unwrap();
        assert_eq!(&*mark.shader, "footstep_heavy_r");
        assert_eq!((mark.radius, mark.orientation), (6.0, 90.0));
        assert!(print(&step(3, 0)).is_none());
        assert!(print(&step(4, 0)).is_some());
        assert_eq!(dust(&step(4, 0)), None);
    }

    #[test]
    fn no_plane_no_print() {
        let mut step = step(3, 3);
        step.normal = None;
        assert!(print(&step).is_none());
    }
}
