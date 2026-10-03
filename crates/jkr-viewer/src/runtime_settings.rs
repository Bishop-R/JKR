//! Application of shell cvars to the native window, renderer, and frame loop.

use super::pointer_input::MouseLook;
use super::{DepthTarget, GpuState};
use std::time::{Duration, Instant};
use winit::dpi::PhysicalSize;

impl GpuState {
    pub(crate) fn sync_runtime_cvars(&mut self) {
        self.sync_post_color();
        if let (Some(console), Some(window)) = (&mut self.console, &self.window) {
            console.apply_window_options(window);
        }
        let Some(console) = &self.console else {
            return; // evidence runs keep whatever accent they set
        };
        if let Some(menu) = &mut self.client_menu {
            menu.set_accent(ui_accent(console));
        }
        self.mouse_look = MouseLook {
            sensitivity: console.float_cvar("sensitivity").unwrap_or(5.0) as f32,
            yaw_scale: console.float_cvar("m_yaw").unwrap_or(0.022) as f32,
            pitch_scale: console.float_cvar("m_pitch").unwrap_or(0.022) as f32,
            invert: console.bool_cvar("m_invert").unwrap_or(false),
        };
        self.gameplay_input
            .set_always_run(console.bool_cvar("cl_run").unwrap_or(true));
        self.gameplay_input.motion.sync(console);
        // Retail defaults from `codemp/cgame/cg_xcvar.h`: cg_marks 1,
        // cg_shadows 1, cg_drawGun 1.
        self.effect_aux
            .decals
            .set_marks_enabled(console.bool_cvar("cg_marks").unwrap_or(true));
        self.effect_aux.saber_contacts.enabled =
            console.bool_cvar("cg_saberContact").unwrap_or(true);
        self.player_shadows.set_enabled(
            console.bool_cvar("cg_shadows").unwrap_or(true)
                && !self.world_materials.sun_shadows_active(),
        );
        self.first_person_weapon
            .set_visible(console.bool_cvar("cg_drawGun").unwrap_or(true));
        self.field_of_view = console.float_cvar("cg_fov").unwrap_or(90.0) as f32;
        self.local_prediction.set_error_decay_millis(
            console
                .float_cvar("cg_errorDecay")
                .unwrap_or(f64::from(jkr_client::DEFAULT_ERROR_DECAY_MILLIS)) as f32,
        );
        self.local_prediction
            .set_predict_items(console.bool_cvar("cg_predictItems").unwrap_or(true));
        if let Some(adapter) = &mut self.legacy_world_adapter {
            adapter.set_smooth_clients(console.smooth_clients());
        }
        self.local_prediction
            .set_smooth_clients(console.smooth_clients());
        if let Some(session) = &mut self.demo_session {
            session.set_smooth_clients(console.smooth_clients());
        }
        self.net_timing.set_enabled(console.show_timedelta());
        self.presentation_clock
            .set_time_nudge_millis(console.time_nudge_millis());
        if self.window.is_none() {
            return;
        }
        let fullscreen = console.bool_cvar("r_fullscreen").unwrap_or(false);
        let vsync = console.bool_cvar("r_vsync").unwrap_or(false);
        let resolution = console
            .text_value("r_resolution")
            .and_then(parse_resolution);
        if fullscreen != self.applied_fullscreen {
            if let Some(window) = &self.window {
                window.set_fullscreen(
                    fullscreen.then_some(winit::window::Fullscreen::Borderless(None)),
                );
            }
            self.applied_fullscreen = fullscreen;
        }
        if let Some([width, height]) = resolution
            && [width, height] != self.applied_resolution
        {
            if let Some(window) = &self.window {
                let _ = window.request_inner_size(PhysicalSize::new(width, height));
            }
            self.applied_resolution = [width, height];
        }
        let present_mode = preferred_present_mode(&self.present_modes, vsync);
        if present_mode != self.configuration.present_mode {
            self.configuration.present_mode = present_mode;
            // The frame in flight presents before its swapchain is reconfigured.
            self.frame_pacer.split.wait_previous();
            if let Some(surface) = &self.context.surface {
                surface.configure(&self.device, &self.configuration);
            }
            self.depth = DepthTarget::new(
                &self.device,
                self.configuration.width,
                self.configuration.height,
            );
        }
    }

    pub(crate) fn maximum_fps(&self) -> u32 {
        let normal = match self
            .console
            .as_ref()
            .and_then(|console| console.integer_cvar("com_maxfps"))
        {
            Some(value) if value >= 0 => u32::try_from(value).unwrap_or(u32::MAX),
            _ => self.refresh_rate_cap(),
        };
        self.console
            .as_ref()
            .map_or(normal, |console| console.window_fps(normal))
    }

    /// `com_maxfps -1`: the refresh rate of the window's monitor, rounded to whole
    /// hertz (59.94 Hz caps at 60), or stock's 125 when it cannot be read.
    fn refresh_rate_cap(&self) -> u32 {
        let now = Instant::now();
        if let Some((read, cap)) = self.refresh_cap.get()
            && now.duration_since(read) < REFRESH_RECHECK
        {
            return cap;
        }
        let cap = self
            .window
            .as_ref()
            .and_then(|window| window.current_monitor())
            .and_then(|monitor| monitor.refresh_rate_millihertz())
            .map_or(UNKNOWN_REFRESH_CAP, refresh_cap_from_millihertz);
        self.refresh_cap.set(Some((now, cap)));
        cap
    }

    pub(crate) fn finish_frame(&mut self) {
        let maximum_fps = self.maximum_fps();
        self.frame_pacer.frame_rendered(maximum_fps);
    }
}

/// Stock's `com_maxfps` default, used when the monitor reports no refresh rate.
const UNKNOWN_REFRESH_CAP: u32 = 125;
/// How long a monitor refresh-rate reading is reused; moving the window to
/// another monitor is picked up within this.
const REFRESH_RECHECK: Duration = Duration::from_secs(1);

fn refresh_cap_from_millihertz(millihertz: u32) -> u32 {
    match millihertz.saturating_add(500) / 1000 {
        0 => UNKNOWN_REFRESH_CAP,
        hertz => hertz,
    }
}

/// The player's `cg_hudScale`; 1.0 when the console is absent (evidence runs).
pub(crate) fn hud_scale(console: Option<&super::console::ViewerConsole>) -> f32 {
    console
        .and_then(|console| console.float_cvar("cg_hudScale"))
        .map_or(1.0, |value| value as f32)
}

fn parse_resolution(value: &str) -> Option<[u32; 2]> {
    let (width, height) = value.split_once('x')?;
    let width = width.parse().ok()?;
    let height = height.parse().ok()?;
    (width > 0 && height > 0).then_some([width, height])
}

pub(crate) fn preferred_present_mode(
    modes: &[wgpu::PresentMode],
    vsync: bool,
) -> wgpu::PresentMode {
    if modes.is_empty() {
        return wgpu::PresentMode::Fifo;
    }
    let preferences: &[wgpu::PresentMode] = if vsync {
        &[wgpu::PresentMode::Fifo]
    } else {
        &[
            wgpu::PresentMode::Immediate,
            wgpu::PresentMode::Mailbox,
            wgpu::PresentMode::AutoNoVsync,
        ]
    };
    preferences
        .iter()
        .copied()
        .find(|mode| modes.contains(mode))
        .unwrap_or(modes[0])
}

/// Named accent presets accepted by `ui_accent`, first entry is the default.
pub(crate) const ACCENT_PRESETS: [(&str, &str); 6] = [
    ("ember", "ff6a3d"),
    ("amber", "ffb340"),
    ("blue", "33c7ff"),
    ("green", "5ee08a"),
    ("violet", "c9a3ff"),
    ("neutral", "e8e8e8"),
];

/// The player's `ui_accent`; the default preset when absent or malformed, so
/// a typo never blanks the menu chrome.
pub(crate) fn ui_accent(console: &super::console::ViewerConsole) -> jkr_ui::Color {
    console
        .text_value("ui_accent")
        .and_then(parse_accent)
        .or_else(|| parse_accent(ACCENT_PRESETS[0].0))
        .unwrap_or(jkr_ui::Theme::default().accent)
}

/// Parse a preset name (`ember`, `amber`, ...) or `RRGGBB` / `#RRGGBB` into an
/// opaque sRGB colour.
pub(crate) fn parse_accent(text: &str) -> Option<jkr_ui::Color> {
    let text = text.trim();
    let hex = ACCENT_PRESETS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(text))
        .map_or(text, |(_, hex)| hex)
        .trim_start_matches('#');
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let channel = |index: usize| {
        u8::from_str_radix(&hex[index..index + 2], 16)
            .ok()
            .map(|value| f32::from(value) / 255.0)
    };
    Some(jkr_ui::Color::new(
        channel(0)?,
        channel(2)?,
        channel(4)?,
        1.0,
    ))
}

#[cfg(test)]
mod refresh_cap_tests {
    use super::refresh_cap_from_millihertz;

    #[test]
    fn rounds_reported_rates_to_whole_hertz() {
        assert_eq!(refresh_cap_from_millihertz(59_940), 60);
        assert_eq!(refresh_cap_from_millihertz(143_856), 144);
        assert_eq!(refresh_cap_from_millihertz(240_000), 240);
    }

    #[test]
    fn a_zero_rate_falls_back_to_stock() {
        assert_eq!(refresh_cap_from_millihertz(0), 125);
    }
}
