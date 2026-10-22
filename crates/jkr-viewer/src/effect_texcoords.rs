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
            // `tcMod transform` along the axes, as the left footprints' mirror
            // `-1 0 0 1 1 0`. The coordinates wrap, so a whole translation changes
            // nothing; a fractional one, or a turn or shear, is not carried.
            ("transform", [m00, m01, m10, m11, ..]) if *m01 == 0.0 && *m10 == 0.0 => {
                scale[0] *= m00;
                scale[1] *= m11;
                scroll[0] *= m00;
                scroll[1] *= m11;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_axis_transform_mirrors_and_a_turn_is_left_alone() {
        let modification = |arguments: &[f32]| TextureModification {
            kind: "transform".to_owned(),
            arguments: arguments.to_vec(),
            wave: None,
        };
        let mirror = [modification(&[-1.0, 0.0, 0.0, 1.0, 1.0, 0.0])];
        assert_eq!(compile(&mirror), ([-1.0, 1.0], [0.0, 0.0]));
        let turn = [modification(&[0.0, 1.0, 1.0, 0.0, 0.0, 0.0])];
        assert_eq!(compile(&turn), ([1.0, 1.0], [0.0, 0.0]));
    }
}
