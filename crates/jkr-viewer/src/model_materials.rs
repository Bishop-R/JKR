//! Thin compatibility adapter for cgame's entity-level material overrides.
//!
//! All shader parsing, stage compilation, texture upload, and pipeline state
//! live in `world_materials`/`world_stage`. This module only ensures that the
//! shaders selected directly by cgame are present in that shared material
//! table. Item overrides follow `cg_ents.c:2162-2197`; player force shells
//! follow `cg_players.c:9673-9723,10857-11119`.

use super::ViewerMaterial;

/// Shared-table indices for the custom shaders selected by `CG_Item`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Overrides {
    pub(crate) placeholder: usize,
    pub(crate) light_disabled: usize,
    pub(crate) dark_disabled: usize,
    force: [usize; 9],
}

/// Append or reuse the cgame entity-override materials before the shared Q3
/// stage runtime is built.
pub(crate) fn append_overrides(materials: &mut Vec<ViewerMaterial>) -> Overrides {
    let mut result = Overrides {
        placeholder: ensure(materials, "powerups/placeholder"),
        light_disabled: ensure(materials, "gfx/misc/mp_light_enlight_disable"),
        dark_disabled: ensure(materials, "gfx/misc/mp_dark_enlight_disable"),
        force: [0; 9],
    };
    for (slot, shader) in result.force.iter_mut().zip(FORCE_SHADERS) {
        *slot = ensure(materials, shader);
    }
    result
}

const FORCE_SHADERS: [&str; 9] = [
    "halfShieldShell",
    "gfx/misc/electric",
    "gfx/misc/fullbodyelectric2",
    "gfx/misc/sightbubble",
    "gfx/misc/forceprotect",
    "gfx/misc/personalshield",
    "powerups/forceshell",
    "powerups/sightshell",
    "powerups/ysalimarishell",
];

impl Overrides {
    pub(crate) fn force(&self, shader: &str) -> Option<usize> {
        FORCE_SHADERS
            .iter()
            .position(|candidate| candidate.eq_ignore_ascii_case(shader))
            .map(|index| self.force[index])
    }
}

fn ensure(materials: &mut Vec<ViewerMaterial>, shader: &str) -> usize {
    if let Some(index) = materials
        .iter()
        .position(|material| material.lightmap < 0 && material.shader.eq_ignore_ascii_case(shader))
    {
        return index;
    }
    let index = materials.len();
    materials.push(ViewerMaterial {
        shader: shader.to_owned(),
        lightmap: -1,
    });
    index
}
