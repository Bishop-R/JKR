//! Geometry for codemp entities whose model index is a gameplay tag, not a model.
//! `FX_DrawPortableShield`, `CG_Beam` and `RB_SurfaceBeam` define these shapes.

use jkr_protocol::EntityState;

/// Shader names selected by the shield's team and damage state.
pub const SHIELD_SHADERS: [&str; 4] = [
    "gfx/misc/blue_portashield",
    "gfx/misc/blue_dmgshield",
    "gfx/misc/red_portashield",
    "gfx/misc/red_dmgshield",
];

/// One oriented shield face, in the reference oriented-line vertex order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShieldFace {
    /// Top/start, bottom/start, top/end, bottom/end.
    pub points: [[f32; 3]; 4],
    /// Authored team/damage shader.
    pub shader: &'static str,
}

/// Decode `ET_SPECIAL / HI_SHIELD` without interpreting its tag as `CS_MODELS`.
pub fn shield_face(state: &EntityState, origin: [f32; 3]) -> Option<ShieldFace> {
    if state.entity_type() != 4 || state.model_index() != 2 || state.e_flags() & 256 != 0 {
        return None;
    }
    // time2 packs axis, height, positive and negative widths in that order.
    let packed = state.raw_field(61).unwrap_or(0);
    let axis = if packed >> 24 & 1 != 0 { 0 } else { 1 };
    let height = (packed >> 16 & 255) as f32;
    let mut start = origin;
    let mut end = origin;
    start[axis] -= (packed & 255) as f32;
    end[axis] += (packed >> 8 & 255) as f32;
    // The C source adds integer height/2 before the floating half-width.
    start[2] += ((packed >> 16 & 255) / 2) as f32;
    end[2] = start[2];
    let mut points = [start, start, end, end];
    for (index, point) in points.iter_mut().enumerate() {
        point[2] += if index % 2 == 0 {
            height * 0.5
        } else {
            -height * 0.5
        };
    }
    Some(ShieldFace {
        points,
        shader: SHIELD_SHADERS[usize::from(state.other_entity_num2() == 1) * 2
            + usize::from(state.tracked_entity_num() != 0)],
    })
}

/// The six four-unit-radius sides of the stock untextured red beam.
pub fn beam_faces(state: &EntityState) -> Option<[[[f32; 3]; 4]; 6]> {
    if state.entity_type() != 7 || state.e_flags() & 256 != 0 {
        return None;
    }
    let start = state.trajectory_base();
    let end = state.origin2();
    if !start.iter().chain(&end).all(|v| v.is_finite()) {
        return None;
    }
    let delta: [f32; 3] = std::array::from_fn(|i| end[i] - start[i]);
    let length = dot(delta, delta).sqrt();
    if length <= 0.0 || !length.is_finite() {
        return None;
    }
    let direction = delta.map(|v| v / length);
    let axis = (0..3).min_by(|a, b| direction[*a].abs().total_cmp(&direction[*b].abs()))?;
    let perpendicular: [f32; 3] =
        std::array::from_fn(|i| f32::from(i == axis) - direction[i] * direction[axis]);
    let scale = 4.0 / dot(perpendicular, perpendicular).sqrt();
    let perpendicular = perpendicular.map(|v| v * scale);
    let cross: [f32; 3] = std::array::from_fn(|i| {
        direction[(i + 1) % 3] * perpendicular[(i + 2) % 3]
            - direction[(i + 2) % 3] * perpendicular[(i + 1) % 3]
    });
    let ring: [[f32; 3]; 6] = std::array::from_fn(|i| {
        let (sin, cos) = (i as f32 * 60.0).to_radians().sin_cos();
        std::array::from_fn(|a| perpendicular[a] * cos + cross[a] * sin)
    });
    Some(std::array::from_fn(|i| {
        let next = (i + 1) % 6;
        [
            (start, ring[i]),
            (end, ring[i]),
            (start, ring[next]),
            (end, ring[next]),
        ]
        .map(|(origin, offset)| std::array::from_fn(|a| origin[a] + offset[a]))
    }))
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
