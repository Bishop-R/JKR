//! Live material-lighting diagnostics; no texture replacement or pipeline rebuild.
use jkr_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry, CvarValue};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

/// Two independent stock controls packed into the existing scene-light uniform.
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
        Self::from_registered(cvars)
    }

    fn from_registered(cvars: &mut CvarRegistry) -> Result<Self, CvarError> {
        let settings = Self::default();
        for (name, bit) in [("r_fullbright", 1), ("r_lightmap", 2)] {
            settings.set(bit, &cvars.get(name).unwrap().value);
            let changed = settings.clone();
            cvars.on_change(name, move |change| changed.set(bit, &change.current))?;
        }
        Ok(settings)
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
