//! Retained display/scene policy. Callbacks run on edits, not during HUD construction.
use jkr_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry, CvarValue};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

/// Scene grading and display gamma, independent of simulation and asset formats.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Policy {
    /// Stock display exponent control, clamped to 0.5–3.0.
    pub(crate) gamma: f32,
    /// Apply an LDR filmic grade before HUD composition.
    pub(crate) tonemap: bool,
    /// Add conservative scene brightness bloom, never to HUD elements.
    pub(crate) bloom: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            gamma: 1.0,
            tonemap: false,
            bloom: false,
        }
    }
}

/// Snapshot-safe policy shared with console-free world-install workers.
#[derive(Clone)]
pub(crate) struct Settings(Arc<[AtomicU32; 3]>);

impl Default for Settings {
    fn default() -> Self {
        Self(Arc::new([
            AtomicU32::new(1.0_f32.to_bits()),
            AtomicU32::new(0),
            AtomicU32::new(0),
        ]))
    }
}

impl Settings {
    /// Subscribe before loading archived config. No frame-time name lookup or formatting.
    pub(crate) fn bind(cvars: &mut CvarRegistry) -> Result<Self, CvarError> {
        cvars.register(CvarDefinition::new(
            "jkr_tonemap",
            0_i64,
            CvarFlags::ARCHIVE,
            "Optional LDR filmic scene curve (0 off, 1 on); applies immediately",
        ))?;
        // The default look keeps authored glow without adding scene-wide bloom.
        // The optional pass remains available through jkr_bloom 1.
        cvars.register(CvarDefinition::new(
            "jkr_bloom",
            0_i64,
            CvarFlags::ARCHIVE,
            "Scene bloom (0 off, 1 on); applies immediately, never blooms HUD",
        ))?;
        let settings = Self::default();
        let gamma = settings.clone();
        cvars.on_change("r_gamma", move |change| {
            if let CvarValue::Float(value) = change.current {
                gamma.0[0].store(clamp_gamma(value).to_bits(), Ordering::Relaxed);
            }
        })?;
        let tone = settings.clone();
        cvars.on_change("jkr_tonemap", move |change| {
            if let CvarValue::Integer(value) = change.current {
                tone.0[1].store(u32::from(value != 0), Ordering::Relaxed);
            }
        })?;
        let bloom = settings.clone();
        cvars.on_change("jkr_bloom", move |change| {
            if let CvarValue::Integer(value) = change.current {
                bloom.0[2].store(u32::from(value != 0), Ordering::Relaxed);
            }
        })?;
        // Seed the retained scalars from the registered defaults. `on_change` only fires on an
        // edit, so without this a cvar whose default is on would never reach the renderer until
        // the player toggled it — the runtime state and the cvar default are separate sources of
        // truth and this is the only place they are reconciled.
        for (name, slot) in [("jkr_tonemap", 1_usize), ("jkr_bloom", 2)] {
            if let Some(&CvarValue::Integer(value)) = cvars.get(name).map(|cvar| &cvar.value) {
                settings.0[slot].store(u32::from(value != 0), Ordering::Relaxed);
            }
        }
        if let Some(&CvarValue::Float(value)) = cvars.get("r_gamma").map(|cvar| &cvar.value) {
            settings.0[0].store(clamp_gamma(value).to_bits(), Ordering::Relaxed);
        }
        Ok(settings)
    }

    /// Read retained scalars without allocation or locking.
    pub(crate) fn policy(&self) -> Policy {
        Policy {
            gamma: f32::from_bits(self.0[0].load(Ordering::Relaxed)),
            tonemap: self.0[1].load(Ordering::Relaxed) != 0,
            bloom: self.0[2].load(Ordering::Relaxed) != 0,
        }
    }
}

/// OpenJK tr_image.cpp:1438–1464; non-finite input cannot poison the shader.
pub(crate) fn clamp_gamma(value: f64) -> f32 {
    if value.is_nan() {
        1.0
    } else {
        value.clamp(0.5, 3.0) as f32
    }
}

impl crate::GpuState {
    /// Apply edited policy; resources change only on enable/disable, never on steady frames.
    pub(crate) fn sync_post_color(&mut self) {
        let policy = self.context.post_color.policy();
        if self.post_aa.as_ref().is_some_and(|aa| aa.policy == policy) {
            return;
        }
        if let Some(aa) = &mut self.post_aa
            && aa.policy.tonemap == policy.tonemap
            && aa.policy.bloom == policy.bloom
            && aa.policy.gamma != 1.0
            && policy.gamma != 1.0
        {
            aa.set_gamma(&self.queue, policy.gamma);
            return;
        }
        self.post_aa = super::Runtime::configured(
            &self.device,
            self.configuration.format,
            [self.configuration.width, self.configuration.height],
            self.context.fxaa,
            policy,
            self.context.hdr,
            Some(self.scene_size()),
        );
    }
}
