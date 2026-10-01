//! TaystJK cg_weapons.c:879-881: offsets along the unbobbed camera axes.
use crate::console::ViewerConsole;
use glam::Vec3;

/// Move the hand rig and its muzzle together in camera space.
pub(super) fn apply(
    origin: [f32; 3],
    yaw: f32,
    pitch: f32,
    console: Option<&ViewerConsole>,
) -> [f32; 3] {
    let values = ["cg_gunx", "cg_guny", "cg_gunz"]
        .map(|name| crate::cgame_options::scalar(console, name, 0.0));
    let forward = Vec3::new(
        yaw.cos() * pitch.cos(),
        yaw.sin() * pitch.cos(),
        pitch.sin(),
    );
    let left = Vec3::new(-yaw.sin(), yaw.cos(), 0.0);
    let up = forward.cross(left);
    (Vec3::from_array(origin) + forward * values[0] + left * values[1] + up * values[2]).to_array()
}
