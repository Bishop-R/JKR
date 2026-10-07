//! Which of an actor's draws are shown. Its model's switched-off surfaces (the caps a cut
//! limb leaves) are drawn after the others and start hidden; a cut switches surfaces as
//! the game does ([`jkr_model::g2_surfaces`]): the limb off on its owner with all below it
//! and the stump's cap on, or, on the limb's own copy, only what lies below the limb's root
//! surface, its cap on.

use crate::player_animation::PreviewVertexRange;
use jkr_model::g2_surfaces::{SURFACE_OFF, SurfaceOverrides};
use jkr_model::{Glm, Skin};

/// An actor's surface switches and the draws they hide.
pub(crate) struct Surfaces {
    /// Each draw's surface (hierarchy index): the skinned ones in order, then the caps.
    draw_surfaces: Vec<usize>,
    /// The caps' vertex ranges, `surface_index` holding the hierarchy index; skinned with
    /// the other ranges every frame.
    pub(crate) caps: Vec<PreviewVertexRange>,
    switches: Switches,
    /// Hidden draws, a bit each.
    hidden: Vec<u64>,
}

/// The skin's switches and the game's, and the surface the renderer's walk starts at;
/// what a copy of the model (`G2API_DuplicateGhoul2Instance`) takes along.
#[derive(Clone)]
pub(crate) struct Switches {
    overrides: SurfaceOverrides,
    root: usize,
    /// Whether the game switched anything or moved the root.
    switched: bool,
}

impl Surfaces {
    /// `ranges` run with the model's draws in skinning order (surfaces that start
    /// switched off left out); `caps` follow them.
    pub(crate) fn new(
        model: &Glm,
        skin: &Skin,
        ranges: &[PreviewVertexRange],
        caps: Vec<PreviewVertexRange>,
    ) -> Self {
        let skinned: Vec<usize> = model
            .hierarchy
            .iter()
            .enumerate()
            .filter(|(_, surface)| surface.flags & SURFACE_OFF == 0)
            .map(|(index, _)| index)
            .collect();
        let draw_surfaces = ranges
            .iter()
            .map(|range| {
                skinned
                    .get(range.surface_index)
                    .copied()
                    .unwrap_or(usize::MAX)
            })
            .chain(caps.iter().map(|cap| cap.surface_index))
            .collect();
        let mut surfaces = Self {
            draw_surfaces,
            caps,
            switches: Switches {
                overrides: SurfaceOverrides::from_skin(model, skin),
                root: 0,
                switched: false,
            },
            hidden: Vec::new(),
        };
        surfaces.refresh(model);
        surfaces
    }

    /// `G2API_SetSurfaceOnOff(name, flags)`; `false` where the model has no such surface.
    /// [`Self::refresh`] applies it.
    pub(crate) fn switch(&mut self, model: &Glm, name: &str, flags: u32) -> bool {
        let found = self.switches.overrides.set(model, name, flags);
        self.switches.switched |= found;
        found
    }

    /// `G2API_SetRootSurface(name)`; `false` where the model has no such surface.
    pub(crate) fn set_root(&mut self, model: &Glm, name: &str) -> bool {
        let Some(root) = model.surface_index(name) else {
            return false;
        };
        self.switches.root = root;
        self.switches.switched = true;
        true
    }

    /// Back to what the skin switches alone (`G2API_SetSkin`, `CG_ReattachLimb`).
    pub(crate) fn reset(&mut self, model: &Glm, skin: &Skin) {
        self.switches = Switches {
            overrides: SurfaceOverrides::from_skin(model, skin),
            root: 0,
            switched: false,
        };
        self.refresh(model);
    }

    /// The switches as they stand, for a copy of the same model.
    pub(crate) fn switches(&self) -> Switches {
        self.switches.clone()
    }

    /// Takes over `switches` from another copy of the same model; [`Self::refresh`]
    /// applies them.
    pub(crate) fn set_switches(&mut self, switches: Switches) {
        self.switches = switches;
    }

    /// `BG_GetRootSurfNameWithVariant` under the current switches.
    pub(crate) fn with_variant(&self, model: &Glm, name: &str) -> String {
        model.surface_with_variant(name, &self.switches.overrides)
    }

    /// Works out the hidden draws after switches. Untouched, every skinned surface shows
    /// and every cap hides; switched, the renderer's walk from the root decides.
    pub(crate) fn refresh(&mut self, model: &Glm) {
        self.hidden.clear();
        self.hidden.resize(self.draw_surfaces.len().div_ceil(64), 0);
        let switches = &self.switches;
        let drawn = switches
            .switched
            .then(|| model.drawn_surfaces(switches.root, &switches.overrides));
        for (draw, &surface) in self.draw_surfaces.iter().enumerate() {
            let shown = match &drawn {
                Some(drawn) => drawn.get(surface).copied().unwrap_or(false),
                None => model
                    .hierarchy
                    .get(surface)
                    .is_some_and(|surface| surface.flags & SURFACE_OFF == 0),
            };
            if !shown {
                self.hidden[draw / 64] |= 1 << (draw % 64);
            }
        }
    }

    /// The hidden draws, a bit each (`hidden[draw / 64] >> (draw % 64)`).
    pub(crate) fn hidden(&self) -> &[u64] {
        &self.hidden
    }
}

/// Whether draw `draw` is hidden by `mask` (see [`Surfaces::hidden`]).
pub(crate) fn is_hidden(mask: &[u64], draw: usize) -> bool {
    mask.get(draw / 64)
        .is_some_and(|word| word & (1 << (draw % 64)) != 0)
}
