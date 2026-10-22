//! Live material-lighting diagnostics; no texture replacement or pipeline rebuild.
use jkr_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry, CvarValue};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use super::material_maps::DEBUG_SHIFT;
/// The bits of the material-map view in the mode word.
const DEBUG_BITS: u32 = 3 << DEBUG_SHIFT;

/// Two independent stock controls and the material-map view (`r_materialMapsDebug`,
/// bits [`DEBUG_SHIFT`]..+2) packed into the existing scene-light uniform.
#[derive(Clone, Default)]
pub(crate) struct Settings(Arc<AtomicU32>);

impl Settings {
    /// Follow EternalJK's archived, omit-default flags and seed before subscribing.
    pub(crate) fn bind(cvars: &mut CvarRegistry) -> Result<Self, CvarError> {
        let flags = CvarFlags::ARCHIVE | CvarFlags::OMIT_DEFAULT;
        cvars.register(CvarDefinition::new(
            "r_fullbright",
            0_i64,
            flags,
            "White lightmaps, vertex bake and model diffuse; live",
        ))?;
        cvars.register(CvarDefinition::new(
            "r_lightmap",
            0_i64,
            flags,
            "Show lightmap bundles without diffuse texture; live",
        ))?;
        // A diagnostic view: never archived, so a restart always shows the real scene.
        cvars.register(CvarDefinition::new(
            "r_materialMapsDebug",
            0_i64,
            CvarFlags::NONE,
            "Material-map surfaces (r_normalMapping/r_specularMapping): 1 mapped normal \
             as colour, 2 tint by maps found (green normal, blue specular, red parallax), \
             3 normal-map relief x4 on grey; other surfaces unchanged; live",
        ))?;
        Self::from_registered(cvars)
    }

    fn from_registered(cvars: &mut CvarRegistry) -> Result<Self, CvarError> {
        let settings = Self::default();
        for (name, bit) in [("r_fullbright", 1), ("r_lightmap", 2)] {
            settings.set(bit, &cvars.get(name).unwrap().value);
            let changed = settings.clone();
            cvars.on_change(name, move |change| changed.set(bit, &change.current))?;
        }
        settings.set_debug(&cvars.get("r_materialMapsDebug").unwrap().value);
        let changed = settings.clone();
        cvars.on_change("r_materialMapsDebug", move |change| {
            changed.set_debug(&change.current)
        })?;
        Ok(settings)
    }

    /// `r_materialMapsDebug` 0..3; other values show the scene unchanged.
    fn set_debug(&self, value: &CvarValue) {
        let view = match value {
            CvarValue::Integer(value @ 0..=3) => *value as u32,
            _ => 0,
        };
        let _ = self
            .0
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |bits| {
                Some(bits & !DEBUG_BITS | view << DEBUG_SHIFT)
            });
    }

    fn set(&self, bit: u32, value: &CvarValue) {
        if let CvarValue::Integer(value) = value {
            if *value != 0 {
                self.0.fetch_or(bit, Ordering::Relaxed);
            } else {
                self.0.fetch_and(!bit, Ordering::Relaxed);
            }
            if bit == 2 && *value == 2 {
                crate::log::progress(format_args!(
                    "r_lightmap 2: intensity heatmap unavailable; showing ordinary lightmap"
                ));
            }
        }
    }

    /// Sample once alongside the existing scene-light upload, not per material or fragment.
    pub(crate) fn bits(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_map_view_has_its_own_bits() {
        let mut cvars = CvarRegistry::new();
        let settings = Settings::bind(&mut cvars).expect("registers");
        assert_eq!(settings.bits(), 0);
        cvars.set_text("r_fullbright", "1").expect("set");
        cvars.set_text("r_materialMapsDebug", "2").expect("set");
        assert_eq!(settings.bits(), 1 | 2 << DEBUG_SHIFT);
        // The stock bits stay below the view's: `mode & 3` tests are unaffected.
        assert_eq!(settings.bits() & 3, 1);
        cvars.set_text("r_materialMapsDebug", "3").expect("set");
        assert_eq!(settings.bits() >> DEBUG_SHIFT, 3);
        // Out of range shows the scene unchanged rather than another view.
        cvars.set_text("r_materialMapsDebug", "7").expect("set");
        assert_eq!(settings.bits(), 1);
        cvars.set_text("r_fullbright", "0").expect("set");
        cvars.set_text("r_materialMapsDebug", "1").expect("set");
        assert_eq!(settings.bits(), 1 << DEBUG_SHIFT);
    }
}
