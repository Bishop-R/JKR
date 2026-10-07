//! Which surfaces of a Ghoul2 model are drawn: the model's own flags, a skin's `*off`
//! entries (`G2_SetSurfaceOnOffFromSkin`), the switches a game makes while it plays
//! (`G2_SetSurfaceOnOff`: a cut limb off with everything below it, its stump's cap on), and
//! the renderer's walk down from the root surface (`RenderSurfaces`,
//! `tr_ghoul2.cpp:2398-2600`): a surface is drawn only while none of its flags is set, and
//! nothing below a surface marked `G2SURFACEFLAG_NODESCENDANTS` is visited.

use crate::{Glm, SKIN_SHADER_OFF, Skin};

/// `G2SURFACEFLAG_OFF`: a surface that is not drawn, the caps among them from the start.
pub const SURFACE_OFF: u32 = 0x2;
/// `G2SURFACEFLAG_NODESCENDANTS`: neither the surface nor any below it is drawn.
pub const SURFACE_NO_DESCENDANTS: u32 = 0x100;
/// The bits `G2_SetSurfaceOnOff` changes.
const SWITCH_BITS: u32 = SURFACE_OFF | SURFACE_NO_DESCENDANTS;
/// `MAX_VARIANTS` (`bg_g2_utils.c:100`).
const VARIANTS: u8 = 8;

/// An instance's surface overrides (`surfaceInfo_v`): hierarchy index and flags.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SurfaceOverrides {
    entries: Vec<(usize, u32)>,
}

impl SurfaceOverrides {
    /// What a skin switches (`G2_SetSurfaceOnOffFromSkin`, `G2_surfaces.cpp`): every surface
    /// it gives `*off` is switched off; the others it names keep the model's flags.
    pub fn from_skin(model: &Glm, skin: &Skin) -> Self {
        let mut overrides = Self::default();
        for (index, surface) in model.hierarchy.iter().enumerate() {
            if skin.shader(&surface.name) == Some(SKIN_SHADER_OFF) {
                overrides.set_index(model, index, SURFACE_OFF);
            }
        }
        overrides
    }

    /// `G2_SetSurfaceOnOff(name, offFlags)`: only the off and no-descendants bits change,
    /// and a surface left as the model has it needs no entry. `false` where the model has
    /// no such surface.
    pub fn set(&mut self, model: &Glm, name: &str, off_flags: u32) -> bool {
        let Some(index) = model.surface_index(name) else {
            return false;
        };
        self.set_index(model, index, off_flags);
        true
    }

    fn set_index(&mut self, model: &Glm, index: usize, off_flags: u32) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.0 == index) {
            entry.1 = (entry.1 & !SWITCH_BITS) | (off_flags & SWITCH_BITS);
            return;
        }
        let flags = model.hierarchy[index].flags;
        let switched = (flags & !SWITCH_BITS) | (off_flags & SWITCH_BITS);
        if switched != flags {
            self.entries.push((index, switched));
        }
    }

    /// Surface `index`'s flags as the renderer reads them.
    pub fn flags(&self, model: &Glm, index: usize) -> u32 {
        self.entries
            .iter()
            .find(|entry| entry.0 == index)
            .map_or(model.hierarchy[index].flags, |entry| entry.1)
    }

    /// Whether anything is switched.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Glm {
    /// The hierarchy index of surface `name` (`G2_IsSurfaceLegal`; names compare without
    /// case).
    pub fn surface_index(&self, name: &str) -> Option<usize> {
        self.hierarchy
            .iter()
            .position(|surface| surface.name.eq_ignore_ascii_case(name))
    }

    /// For each surface, whether `RenderSurfaces` draws it when the walk starts at `root`
    /// (the model's first surface, or a cut limb's with `G2API_SetRootSurface`).
    pub fn drawn_surfaces(&self, root: usize, overrides: &SurfaceOverrides) -> Vec<bool> {
        let mut drawn = vec![false; self.hierarchy.len()];
        let mut pending = vec![root];
        while let Some(index) = pending.pop() {
            let Some(surface) = self.hierarchy.get(index) else {
                continue;
            };
            let flags = overrides.flags(self, index);
            drawn[index] = flags == 0;
            if flags & SURFACE_NO_DESCENDANTS == 0 {
                pending.extend(surface.children.iter().rev());
            }
        }
        drawn
    }

    /// `G2API_GetSurfaceRenderStatus` (`G2_IsSurfaceRendered`): the surface's own flags,
    /// `SURFACE_OFF` added when a surface above it hides what is below, and its override
    /// read only while the model leaves it unflagged; `None` for no such surface.
    pub fn surface_render_status(&self, name: &str, overrides: &SurfaceOverrides) -> Option<u32> {
        let index = self.surface_index(name)?;
        let mut flags = self.hierarchy[index].flags;
        let mut parent = self.hierarchy[index].parent;
        while let Some(above) = parent {
            if overrides.flags(self, above) & SURFACE_NO_DESCENDANTS != 0 {
                flags |= SURFACE_OFF;
                break;
            }
            parent = self.hierarchy[above].parent;
        }
        if flags == 0 {
            flags = overrides.flags(self, index);
        }
        Some(flags)
    }

    /// `BG_GetRootSurfNameWithVariant`: `name` while it is drawn, else its first species
    /// variant (`name` with `a` to `h`) that is; `name` when none is.
    pub fn surface_with_variant(&self, name: &str, overrides: &SurfaceOverrides) -> String {
        let shown = |surface: &str| self.surface_render_status(surface, overrides) == Some(0);
        if shown(name) {
            return name.to_owned();
        }
        (0..VARIANTS)
            .map(|variant| format!("{name}{}", char::from(b'a' + variant)))
            .find(|surface| shown(surface))
            .unwrap_or_else(|| name.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GlmLod, GlmSurfaceHierarchy};

    /// hips → torso → (head, l_arm → l_hand), plus a cap under the torso that starts off.
    fn body() -> Glm {
        let surface = |name: &str, flags, parent, children: &[usize]| GlmSurfaceHierarchy {
            name: name.to_owned(),
            flags,
            shader: String::new(),
            parent,
            children: children.to_vec(),
        };
        Glm {
            name: String::new(),
            animation_name: String::new(),
            bone_count: 0,
            hierarchy: vec![
                surface("hips", 0, None, &[1]),
                surface("torso", 0, Some(0), &[2, 3, 5]),
                surface("head", 0, Some(1), &[]),
                surface("l_arm", 0, Some(1), &[4]),
                surface("l_hand", 0, Some(3), &[]),
                surface("torso_cap_l_arm", SURFACE_OFF, Some(1), &[]),
            ],
            lods: vec![GlmLod {
                surfaces: Vec::new(),
            }],
        }
    }

    #[test]
    fn a_cut_arm_goes_with_its_hand_and_its_stump_shows_a_cap() {
        let model = body();
        let mut overrides = SurfaceOverrides::default();
        assert_eq!(
            model.drawn_surfaces(0, &overrides),
            [true, true, true, true, true, false]
        );
        // `CG_General`: the arm off with all below it, the cap on.
        assert!(overrides.set(&model, "L_ARM", SURFACE_NO_DESCENDANTS));
        assert!(overrides.set(&model, "torso_cap_l_arm", 0));
        assert_eq!(
            model.drawn_surfaces(0, &overrides),
            [true, true, true, false, false, true]
        );
        // The limb itself, walked from its own root.
        assert_eq!(
            model.drawn_surfaces(3, &SurfaceOverrides::default()),
            [false, false, false, true, true, false]
        );
        assert_eq!(
            model.surface_render_status("l_hand", &overrides),
            Some(SURFACE_OFF)
        );
    }

    #[test]
    fn a_skin_names_a_cap_by_its_off_line_and_the_first_line_wins() {
        let skin = Skin::parse(
            b"hips,models/a.tga\nr_leg_cap_hips_off,models/caps.tga\nhead_off,*off\nhips,models/b.tga\ntag_weapon,models/c.tga\n",
        )
        .unwrap();
        assert_eq!(skin.shader("r_leg_cap_hips"), Some("models/caps.tga"));
        assert_eq!(skin.shader("hips"), Some("models/a.tga"));
        assert_eq!(skin.shader("head"), None);
        assert_eq!(skin.shader("tag_weapon"), None);
    }

    #[test]
    fn a_hidden_surface_gives_way_to_its_first_shown_variant() {
        let mut model = body();
        model.hierarchy[2].name = "heada".to_owned();
        model.hierarchy.push(GlmSurfaceHierarchy {
            name: "head".to_owned(),
            flags: SURFACE_OFF,
            shader: String::new(),
            parent: Some(1),
            children: Vec::new(),
        });
        let overrides = SurfaceOverrides::default();
        assert_eq!(model.surface_with_variant("head", &overrides), "heada");
        assert_eq!(model.surface_with_variant("l_arm", &overrides), "l_arm");
        assert_eq!(model.surface_with_variant("tail", &overrides), "tail");
    }
}
