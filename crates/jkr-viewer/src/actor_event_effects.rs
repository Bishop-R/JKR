//! Bolt placement for animation EFX (`CG_PlayerAnimEventDo`, multiplayer path).

use glam::{Quat, Vec3};

pub(super) struct Placement<'a> {
    pub(super) preview: &'a crate::PlayerPreview,
    pub(super) matrices: &'a [[[f32; 4]; 3]],
    pub(super) attachment: Option<crate::saber::Attachment>,
    pub(super) hilt: Option<crate::saber_hilts::Hilt>,
    pub(super) gun_flash: Option<crate::bolt::BoltMatrix>,
    pub(super) origin: Vec3,
    pub(super) rotation: Quat,
    pub(super) scale: Vec3,
}

impl Placement<'_> {
    pub(super) fn origin(&self, name: Option<&str>) -> Vec3 {
        let Some(name) = name else {
            return self.origin;
        };
        let weapon_point = if name.starts_with("*blade") || name == "*flash" {
            self.attachment.and_then(|attachment| {
                let socket = if let Some(hilt) = self.hilt {
                    hilt.event_bolt(name)
                } else if name == "*flash" {
                    self.gun_flash.map(|m| crate::bolt::column(&m, 3))
                } else {
                    None
                }?;
                Some(
                    Vec3::from_array(attachment.grip)
                        + Quat::from_array(attachment.rotation) * Vec3::from_array(socket),
                )
            })
        } else {
            None
        };
        let point = weapon_point.or_else(|| {
            // Ghoul2 accepts both named bones and surface tags. Bone bolts use
            // EvalUnsmooth * BasePoseMat; only its translation is needed here.
            if let Some((index, bone)) = self
                .preview
                .animation
                .bones
                .iter()
                .enumerate()
                .find(|(_, b)| b.name.eq_ignore_ascii_case(name))
            {
                let matrix = self.matrices.get(index)?;
                return Some(Vec3::from_array(std::array::from_fn(|r| {
                    matrix[r][3]
                        + matrix[r][0] * bone.base_pose[0][3]
                        + matrix[r][1] * bone.base_pose[1][3]
                        + matrix[r][2] * bone.base_pose[2][3]
                })));
            }
            self.preview
                .mesh
                .surface_bolt_matrix(name, 0, self.matrices)
                .ok()
                .flatten()
                .map(|m| Vec3::from_array(crate::bolt::column(&m, 3)))
        });
        point.map_or(self.origin, |point| {
            self.origin + self.rotation * (point * self.scale)
        })
    }
}
