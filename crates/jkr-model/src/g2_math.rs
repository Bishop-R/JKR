//! The small vector and matrix arithmetic Ghoul2's model-space queries share
//! (`q_math.c` and `G2_misc.cpp`), in single precision as Ghoul2 does it.

pub(crate) fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) fn scale(a: [f32; 3], by: f32) -> [f32; 3] {
    [a[0] * by, a[1] * by, a[2] * by]
}

pub(crate) fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

/// `VectorNormalize`.
pub(crate) fn normalize(a: &mut [f32; 3]) {
    let length = length(*a);
    if length != 0.0 {
        let inverse = 1.0 / length;
        for value in a {
            *value *= inverse;
        }
    }
}

/// `G2_GenerateWorldMatrix`: `Create_Matrix` (the entity's `AnglesToAxis` axes as
/// columns) and the origin.
pub(crate) fn world_matrix(axes: [[f32; 3]; 3], origin: [f32; 3]) -> [[f32; 4]; 3] {
    std::array::from_fn(|row| [axes[0][row], axes[1][row], axes[2][row], origin[row]])
}

/// `Inverse_Matrix`: the transposed rotation and the translation brought back through it.
pub(crate) fn inverse(matrix: &[[f32; 4]; 3]) -> [[f32; 4]; 3] {
    let mut out = [[0.0; 4]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = matrix[j][i];
        }
    }
    for i in 0..3 {
        out[i][3] = 0.0;
        for j in 0..3 {
            out[i][3] -= out[i][j] * matrix[j][3];
        }
    }
    out
}

/// `TransformAndTranslatePoint`.
pub(crate) fn transform_and_translate(point: [f32; 3], matrix: &[[f32; 4]; 3]) -> [f32; 3] {
    std::array::from_fn(|i| {
        point[0] * matrix[i][0] + point[1] * matrix[i][1] + point[2] * matrix[i][2] + matrix[i][3]
    })
}

/// `TransformPoint`: the rotation only.
pub(crate) fn transform(point: [f32; 3], matrix: &[[f32; 4]; 3]) -> [f32; 3] {
    std::array::from_fn(|i| {
        point[0] * matrix[i][0] + point[1] * matrix[i][1] + point[2] * matrix[i][2]
    })
}
