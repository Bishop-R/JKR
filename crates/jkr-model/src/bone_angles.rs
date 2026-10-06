//! Engine-generic skeletal angle overrides.
//!
//! An adapter supplies Euler angles, signed axis mappings, and composition
//! mode. The model layer precomputes the override matrix once per update
//! (base-pose-conjugated for a postmultiplied one) and applies it while walking
//! the hierarchy; it contains no game bone names or policy constants.

use super::{GlaBone, multiply_3x4, quake_angles_matrix};

/// Signed skeleton axis selected for one Euler rotation component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoneAxis {
    NegativeX,
    PositiveX,
    NegativeY,
    PositiveY,
    NegativeZ,
    PositiveZ,
}

/// Point at which an angle override composes with the animated local matrix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoneAngleMode {
    /// Compose the base-pose-conjugated command after the animated local.
    PostMultiply,
    /// Replace the bone's model-space orientation with the command's, keeping the
    /// animated bone's position and scale; descendants follow the replaced bone.
    Replace,
}

/// One adapter-authored bone-angle command.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoneAngleCommand {
    pub angles_degrees: [f32; 3],
    pub mode: BoneAngleMode,
    pub up: BoneAxis,
    pub left: BoneAxis,
    pub forward: BoneAxis,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct BoneAngleOverride {
    pub(super) matrix: [[f32; 4]; 3],
    pub(super) mode: BoneAngleMode,
}

pub(super) fn compile(bone: &GlaBone, command: BoneAngleCommand) -> BoneAngleOverride {
    let matrix = match command.mode {
        BoneAngleMode::PostMultiply => {
            let mapped = map_angles(
                command.angles_degrees,
                command.up,
                command.left,
                command.forward,
            );
            let rotation = quake_angles_matrix(mapped);
            multiply_3x4(
                bone.base_pose,
                multiply_3x4(rotation, bone.inverse_base_pose),
            )
        }
        BoneAngleMode::Replace => {
            let mut angles = command.angles_degrees;
            // Ghoul2 turns a positive-Y left axis half round (`G2_Generate_Matrix`).
            if command.left == BoneAxis::PositiveY {
                angles[0] += 180.0;
            }
            multiply_3x4(quake_angles_matrix(angles), permutation(command))
        }
    };
    BoneAngleOverride {
        matrix,
        mode: command.mode,
    }
}

/// The signed axes a replacing command's forward, left and up columns land on.
fn permutation(command: BoneAngleCommand) -> [[f32; 4]; 3] {
    let mut matrix = [[0.0; 4]; 3];
    for (column, axis) in [command.forward, command.left, command.up]
        .into_iter()
        .enumerate()
    {
        let (row, sign) = match axis {
            BoneAxis::NegativeX => (0, -1.0),
            BoneAxis::PositiveX => (0, 1.0),
            BoneAxis::NegativeY => (1, -1.0),
            BoneAxis::PositiveY => (1, 1.0),
            BoneAxis::NegativeZ => (2, -1.0),
            BoneAxis::PositiveZ => (2, 1.0),
        };
        matrix[row][column] = sign;
    }
    matrix
}

fn map_angles(angles: [f32; 3], up: BoneAxis, left: BoneAxis, forward: BoneAxis) -> [f32; 3] {
    let yaw = match up {
        BoneAxis::NegativeX => angles[2] + 180.0,
        BoneAxis::PositiveX => angles[2],
        BoneAxis::NegativeY | BoneAxis::PositiveY => angles[0],
        BoneAxis::NegativeZ => angles[1] + 180.0,
        BoneAxis::PositiveZ => angles[1],
    };
    let pitch = match left {
        BoneAxis::NegativeX => angles[2],
        BoneAxis::PositiveX => angles[2] + 180.0,
        BoneAxis::NegativeY => angles[0],
        BoneAxis::PositiveY => angles[0] + 180.0,
        BoneAxis::NegativeZ | BoneAxis::PositiveZ => angles[1],
    };
    let roll = match forward {
        BoneAxis::NegativeX | BoneAxis::PositiveX => angles[2],
        BoneAxis::NegativeY => angles[0],
        BoneAxis::PositiveY => angles[0] + 180.0,
        BoneAxis::NegativeZ => angles[1],
        BoneAxis::PositiveZ => angles[1] + 180.0,
    };
    [pitch, yaw, roll]
}
