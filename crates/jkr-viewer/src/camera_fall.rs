//! Falling to death (`gCGFallVector`: set in `CG_Draw2D`, `cg_draw.c:8114-8148`, used by
//! `CG_CalcIdealThirdPersonViewTarget`, `cg_view.c:274-285`, and `CG_DrawActiveFrame`,
//! `cg_view.c:2680-2690`): while the player falls to death, the third-person camera frames
//! the point the fall began at (the snapshot's origin then, at eye height), and the view
//! is then set at that point, looking the way the camera looked at the falling player.

use glam::Vec3;

/// The point the fall began at, while the player falls to death.
#[derive(Default)]
pub(crate) struct Fall {
    origin: Option<Vec3>,
}

impl Fall {
    /// Follow the presented player: whether it falls to death and where it is.
    pub(crate) fn update(&mut self, falling: bool, origin: Vec3) -> Option<Vec3> {
        if !falling {
            self.origin = None;
        } else if self.origin.is_none() {
            self.origin = Some(origin);
        }
        self.origin
    }

    /// The point the fall began at, if the player falls to death.
    pub(crate) fn origin(&self) -> Option<Vec3> {
        self.origin
    }
}

/// The view from the fall's point `fall`, looking from the camera's `position` toward the
/// falling player at `player`; the camera's own view where that has no direction.
pub(crate) fn view(fall: Vec3, (position, target): (Vec3, Vec3), player: Vec3) -> (Vec3, Vec3) {
    let direction = (player - position).normalize_or_zero();
    if direction == Vec3::ZERO {
        return (position, target);
    }
    (fall, fall + direction * 256.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_view_holds_where_the_fall_began() {
        let mut fall = Fall::default();
        assert_eq!(fall.update(false, Vec3::ZERO), None);
        let start = Vec3::new(100.0, 0.0, 64.0);
        assert_eq!(fall.update(true, start), Some(start));
        assert_eq!(
            fall.update(true, Vec3::new(100.0, 0.0, -300.0)),
            Some(start)
        );
        let camera = Vec3::new(20.0, 0.0, 100.0);
        let (eye, target) = view(start, (camera, Vec3::ZERO), Vec3::new(100.0, 0.0, -300.0));
        assert_eq!(eye, start);
        let look = (target - eye).normalize();
        assert!((look - (Vec3::new(100.0, 0.0, -300.0) - camera).normalize()).length() < 1e-5);
        assert_eq!(fall.update(false, start), None);
    }
}
