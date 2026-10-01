//! Cached CG_ForcePushBodyBlur joint indices; no second pose evaluation.
use glam::Vec3;
use jkr_model::Gla;

const NAMES: [&str; 8] = [
    "cranium",
    "lower_lumbar",
    "rhand",
    "lhand",
    "ltibia",
    "rtibia",
    "lradius",
    "rradius",
];

/// Fixed joint-origin storage populated alongside the existing saber bolts.
pub(crate) struct ForceBones {
    indices: [Option<usize>; 8],
    /// Model-space joints from the latest evaluated skin pose; missing bones stay absent.
    pub(crate) origins: [Option<Vec3>; 8],
    /// Raw lower-lumbar bolt from this pose, shared by carried-flag submission.
    pub(crate) lumbar: Option<crate::bolt::BoltMatrix>,
}

impl ForceBones {
    /// Resolve names once while loading an actor (cg_players.c:4995-5006).
    pub(crate) fn new(animation: &Gla) -> Self {
        Self {
            indices: NAMES.map(|name| {
                animation
                    .bones
                    .iter()
                    .position(|bone| bone.name.eq_ignore_ascii_case(name))
            }),
            origins: [None; 8],
            lumbar: None,
        }
    }

    /// Undo the inverse bind pose, exactly as the existing generic joint query does.
    pub(crate) fn update(&mut self, animation: &Gla, matrices: &[[[f32; 4]; 3]]) {
        // G2_GetBoltMatrixLow, tr_ghoul2.cpp:3132: skin matrix * base pose.
        self.lumbar = self.indices[1].and_then(|index| {
            let matrix = matrices.get(index)?;
            let bind = animation.bones.get(index)?.base_pose;
            Some(std::array::from_fn(|row| {
                std::array::from_fn(|column| {
                    (0..3)
                        .map(|axis| matrix[row][axis] * bind[axis][column])
                        .sum::<f32>()
                        + if column == 3 { matrix[row][3] } else { 0.0 }
                })
            }))
        });
        for (output, index) in self.origins.iter_mut().zip(self.indices) {
            *output = index.and_then(|index| {
                let matrix = matrices.get(index)?;
                let bind = animation.bones.get(index)?.base_pose;
                Some(Vec3::from_array(std::array::from_fn(|row| {
                    matrix[row][3]
                        + (0..3)
                            .map(|axis| matrix[row][axis] * bind[axis][3])
                            .sum::<f32>()
                })))
            });
        }
    }
}
