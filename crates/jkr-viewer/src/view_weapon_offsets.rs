//! TaystJK cg_weapons.c:879-881: offsets along the unbobbed camera axes; and OpenJK's
//! view-model field of view (`CG_AddViewWeapon`, `cg_weapons.c:789-853`): the gun lowered
//! at a wide field of view (`cg_fovViewmodelAdjust`) and drawn as at its own
//! (`cg_fovViewmodel`).
use crate::cgame_options::scalar;
use crate::console::ViewerConsole;
use glam::Vec3;

/// Move the hand rig and its muzzle together in camera space.
pub(super) fn apply(
    origin: [f32; 3],
    yaw: f32,
    pitch: f32,
    console: Option<&ViewerConsole>,
) -> [f32; 3] {
    let mut values = ["cg_gunx", "cg_guny", "cg_gunz"].map(|name| scalar(console, name, 0.0));
    // `cg_fovViewmodelAdjust`: the gun drops lower as the field of view widens past 90.
    let fov = viewmodel_fov(console);
    if scalar(console, "cg_fovViewmodelAdjust", 1.0) as i32 != 0 && fov > 90.0 {
        values[2] += -0.2 * (fov - 90.0);
    }
    let forward = Vec3::new(
        yaw.cos() * pitch.cos(),
        yaw.sin() * pitch.cos(),
        pitch.sin(),
    );
    let left = Vec3::new(-yaw.sin(), yaw.cos(), 0.0);
    let up = forward.cross(left);
    (Vec3::from_array(origin) + forward * values[0] + left * values[1] + up * values[2]).to_array()
}

/// `cg_fovViewmodel`'s stretch of the hand's forward axis in a frame whose horizontal field
/// of view is `fov_x` degrees, so the gun looks as it would at its own; 1 while it is off.
pub(super) fn stretch(console: Option<&ViewerConsole>, fov_x: f32) -> f32 {
    if scalar(console, "cg_fovViewmodel", 0.0) as i32 == 0 {
        return 1.0;
    }
    (viewmodel_fov(console).to_radians() * 0.5).tan() / (fov_x.to_radians() * 0.5).tan()
}

/// `cgFov`: `cg_fovViewmodel` when set, else `cg_fov`, held to 1-130.
fn viewmodel_fov(console: Option<&ViewerConsole>) -> f32 {
    let own = scalar(console, "cg_fovViewmodel", 0.0);
    let fov = if own as i32 != 0 {
        own
    } else {
        scalar(console, "cg_fov", 80.0)
    };
    fov.clamp(1.0, 130.0)
}
