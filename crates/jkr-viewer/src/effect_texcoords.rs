//! Fixed affine texture-coordinate transforms for effect shader stages.

use jkr_shader::TextureModification;

pub(crate) fn compile(modifications: &[TextureModification]) -> ([f32; 2], [f32; 2]) {
    let mut scale = [1.0_f32; 2];
    let mut scroll = [0.0_f32; 2];
    for modification in modifications {
        match (
            modification.kind.as_str(),
            modification.arguments.as_slice(),
        ) {
            ("scale", [x, y, ..]) => {
                scale[0] *= x;
                scale[1] *= y;
                scroll[0] *= x;
                scroll[1] *= y;
            }
            ("scroll", [x, y, ..]) => {
                scroll[0] += x;
                scroll[1] += y;
            }
            _ => {}
        }
    }
    (scale, scroll)
}

pub(crate) fn sample(scale: [f32; 2], scroll: [f32; 2], time: f32) -> [f32; 4] {
    [
        scale[0],
        scale[1],
        (scroll[0] * time).rem_euclid(1.0),
        (scroll[1] * time).rem_euclid(1.0),
    ]
}
