//! Startup scene precision and a highlight-only, RGB-ratio-preserving display shoulder.
use jkr_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry};

/// No automatic exposure: camera contents never change the visibility of another player.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Settings {
    /// Zero preserves the display format; one selects RGBA16F.
    pub(crate) mode: u32,
    /// Fixed linear scene multiplier, before bloom and display mapping.
    pub(crate) exposure: f32,
}

// Neutral policy for the display-only resolve; scene defaults are sampled below.
impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: 0,
            exposure: 1.0,
        }
    }
}

impl Settings {
    /// Read the registered/archive-loaded values once, before creating scene resources.
    pub(crate) fn sample(console: Option<&crate::console::ViewerConsole>) -> Self {
        Self {
            mode: console
                .and_then(|c| c.integer_cvar("jkr_hdr"))
                .unwrap_or(1)
                .clamp(0, 1) as u32,
            exposure: exposure(
                console
                    .and_then(|c| c.float_cvar("jkr_hdrExposure"))
                    .unwrap_or(1.0),
            ),
        }
    }

    /// Scene-only format; the swapchain, HUD and captures retain their display format.
    pub(crate) fn format(self, display: wgpu::TextureFormat) -> wgpu::TextureFormat {
        match self.mode {
            1 => wgpu::TextureFormat::Rgba16Float,
            _ => display,
        }
    }
}

fn exposure(value: f64) -> f32 {
    if value.is_finite() {
        value.clamp(0.25, 4.0) as f32
    } else {
        1.0
    }
}

/// Own names: HDR is neither stock gamma nor the existing optional filmic LDR grade.
pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), CvarError> {
    cvars.register(CvarDefinition::new(
        "jkr_hdr",
        1_i64,
        CvarFlags::ARCHIVE,
        "Scene HDR: 0 off, 1 RGBA16F; restart required",
    ))?;
    cvars.register(CvarDefinition::new(
        "jkr_hdrExposure",
        1.0,
        CvarFlags::ARCHIVE,
        "Fixed HDR exposure 0.25..4 (never automatic); restart required",
    ))?;
    for name in ["jkr_hdr", "jkr_hdrExposure"] {
        cvars.on_change(name, |_| {
            crate::log::progress(format_args!(
                "HDR setting changed: restart viewer to rebuild scene targets"
            ))
        })?;
    }
    Ok(())
}
