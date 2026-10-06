//! Retained HUD icon policy and submissions, using the existing UI texture atlas.
use super::*;
use jkr_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign, TextOverflow, TextureId};
#[path = "icon_assets.rs"]
pub(super) mod assets;

/// Register only the implemented flat-icon path and explicitly partial model selector.
pub(crate) fn register(cvars: &mut jkr_shell::CvarRegistry) -> Result<(), jkr_shell::CvarError> {
    for (name, default, help) in [
        (
            "cg_drawIcons",
            1,
            "Status weapon, ammo and carried-flag icons",
        ),
        (
            "cg_draw3DIcons",
            0,
            "Flag slot: 0 flat; 1 model variant currently unavailable",
        ),
        (
            "cg_drawPowerUpIcons",
            1,
            "Timed powerup icons and countdowns",
        ),
        (
            "cg_killfeedColors",
            0,
            "Tint obituary icons by cause of death",
        ),
    ] {
        cvars.register(jkr_shell::CvarDefinition::new(
            name,
            default as i64,
            jkr_shell::CvarFlags::ARCHIVE,
            help,
        ))?;
    }
    cvars.register(jkr_shell::CvarDefinition::new(
        "cg_killfeedIconSize",
        12.0,
        jkr_shell::CvarFlags::ARCHIVE,
        "Obituary icon size; zero uses 18 virtual units",
    ))?;
    Ok(())
}

/// Install map-specific image handles without a second texture loader or render pass.
pub(crate) fn install(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    format: wgpu::TextureFormat,
    vfs: &jkr_vfs::VirtualFileSystem,
    shaders: &jkr_shader::ShaderCatalog,
) -> (ui_renderer::ShapeRenderer, HudOverlay) {
    let renderer =
        ui_renderer::ShapeRenderer::new(device, queue, format, crate::DepthTarget::FORMAT);
    let mut hud = HudOverlay::new();
    hud.icons.handles = assets::load(vfs, shaders, |id, rgba| {
        renderer.upload_icon(queue, id, rgba)
    });
    (renderer, hud)
}

/// Sampled options and scalar snapshot data; all texture handles are map-lifetime values.
pub(crate) struct Icons {
    handles: [Option<TextureId>; assets::COUNT],
    enabled: bool,
    models: bool,
    powers: bool,
    colors: bool,
    size: f32,
    weapon: u8,
    alive: bool,
    seconds: [i32; 16],
    cty: bool,
    method: usize,
    warned_model: bool,
}

impl Default for Icons {
    fn default() -> Self {
        Self {
            handles: [None; assets::COUNT],
            enabled: true,
            models: false,
            powers: true,
            colors: false,
            size: 18.0,
            weapon: 0,
            alive: false,
            seconds: [-1; 16],
            cty: false,
            method: 0,
            warned_model: false,
        }
    }
}

impl Icons {
    /// Use the same predicted weapon selection as the existing status text.
    pub(super) fn weapon(&mut self, weapon: u8) {
        {
            self.weapon = weapon;
        }
    }

    /// Cache settings once per HUD update; format no strings and perform no asset lookup.
    pub(super) fn update(
        &mut self,
        snapshot: &jkr_protocol::Snapshot,
        game: &GameState,
        console: Option<&ViewerConsole>,
    ) {
        self.enabled = family::integer(console, "cg_drawicons", 1) != 0;
        self.models = family::integer(console, "cg_draw3dicons", 0) != 0;
        self.powers = family::integer(console, "cg_drawpowerupicons", 1) != 0;
        self.colors = family::integer(console, "cg_killfeedcolors", 0) != 0;
        let size = console
            .and_then(|c| c.float_cvar("cg_killfeediconsize"))
            .unwrap_or(0.0);
        self.size = if size == 0.0 {
            18.0
        } else {
            size.clamp(1.0, 96.0) as f32
        };
        if self.models && !self.warned_model {
            eprintln!("3D HUD flag icons unavailable; use cg_draw3DIcons 0 for flat icons");
            self.warned_model = true;
        }

        self.weapon = snapshot.player.weapon();
        self.alive = snapshot.player.health() > 0 && snapshot.player.movement_type() != 4;
        self.cty = game
            .config_string(0)
            .and_then(|s| jkr_client::LegacyClientInfo::new(s).integer("g_gametype"))
            == Some(8);
        for (seconds, end) in self.seconds.iter_mut().zip(snapshot.player.powerups) {
            let remaining = i64::from(end) - i64::from(snapshot.server_time);
            *seconds = if remaining > 0 {
                (remaining / 1000) as i32
            } else {
                -1
            };
        }
    }

    /// Preserve the cause from the existing shared feed, never a second event observer.
    pub(super) fn obituary(&mut self, feed: &jkr_client::KillFeed) {
        {
            self.method = feed.newest(0).map_or(0, |e| usize::from(e.means_of_death));
        }
    }

    fn quad(&self, list: &mut DrawList, index: usize, rect: Rect, color: Color) -> bool {
        let Some(texture) = self.handles.get(index).copied().flatten() else {
            return false;
        };
        let _ = list.push(DrawCommand::TexturedQuad {
            rect,
            texture,
            color,
        });
        true
    }

    /// Emit the status slots and the independently gated powerup column.
    pub(super) fn emit(
        &self,
        list: &mut DrawList,
        viewport: [f32; 2],
        visibility: HudVisibility,
        upper: bool,
    ) {
        let [w, h] = viewport;
        let s = crate::ui_scale::height_scale(h);
        let white = Color::new(1.0, 1.0, 1.0, 1.0);
        if self.enabled && visibility.status && self.alive {
            let item = assets::WEAPONS
                .get(self.weapon as usize)
                .copied()
                .unwrap_or(0);
            self.quad(
                list,
                item,
                Rect::new(w / 2.0 - 32.0 * s, h - 88.0 * s, 64.0 * s, 64.0 * s),
                white,
            );
            self.quad(
                list,
                assets::ammo(self.weapon),
                Rect::new(w / 2.0 + 48.0 * s, h - 88.0 * s, 64.0 * s, 64.0 * s),
                white,
            );
            if !self.models {
                for power in 4..=6 {
                    if self.seconds[power] >= 0 {
                        self.quad(
                            list,
                            assets::POWERS[power],
                            Rect::new(420.0 * s, h - 128.0 * s, 64.0 * s, 64.0 * s),
                            white,
                        );
                    }
                }
            }
        }
        if !self.powers || !upper {
            return;
        }
        let mut y = 200.0 * s;
        for (power, seconds) in self.seconds.iter().copied().enumerate() {
            if seconds < 0 || assets::POWERS[power] == 0 {
                continue;
            }
            let index = if self.cty && matches!(power, 4 | 5) {
                90 + power
            } else {
                assets::POWERS[power]
            };
            if self.quad(
                list,
                index,
                Rect::new(w - 100.0 * s, y, 64.0 * s, 64.0 * s),
                white,
            ) {
                if !matches!(power, 4 | 5) && seconds < 999 {
                    let _ = list.push(DrawCommand::Text {
                        rect: Rect::new(w - 100.0 * s, y + 56.0 * s, 64.0 * s, 28.0 * s),
                        text: TextId(400 + power as u32),
                        size: 22.0 * s,
                        color: white,
                        align: TextAlign::Center,
                        overflow: TextOverflow::Clip,
                        weight: FontWeight::Semibold,
                        letter_spacing: 0.0,
                    });
                }
                y += 86.0 * s;
            }
        }
    }

    /// Add an icon beside the existing obituary text and reserve its space.
    pub(super) fn feed(&self, list: &mut DrawList, rect: &mut Rect, alpha: f32, scale: f32) {
        let index = 51 + self.method.min(42);
        let index = if self.handles[index].is_some() {
            index
        } else {
            51
        };
        let size = self.size * scale;
        let [r, g, b] = if self.colors {
            assets::tint(self.method)
        } else {
            [1.0; 3]
        };
        if self.quad(
            list,
            index,
            Rect::new(rect.right() - size, rect.y, size, size),
            Color::new(r, g, b, alpha),
        ) {
            rect.width -= size + 8.0 * scale;
        }
    }

    /// Borrow a precomputed decimal string; countdown updates allocate and format nothing.
    pub(super) fn text(&self, power: usize) -> &str {
        let value = self.seconds[power].clamp(0, 998) as usize;
        let start = if value >= 100 {
            0
        } else if value >= 10 {
            1
        } else {
            2
        };
        std::str::from_utf8(&NUMBERS[value][start..]).unwrap_or("")
    }
}

const NUMBERS: [[u8; 3]; 999] = {
    let mut values = [[b'0'; 3]; 999];
    let mut i = 0;
    while i < values.len() {
        values[i] = [
            b'0' + (i / 100) as u8,
            b'0' + ((i / 10) % 10) as u8,
            b'0' + (i % 10) as u8,
        ];
        i += 1;
    }
    values
};
