//! The status HUD described by the game's own menu files.
//!
//! Retail Jedi Academy draws its health, armor, Force and ammo display from
//! `.menu` files: `cg_hudFiles` (default `ui/jahud.txt`) lists them and
//! OpenJK codemp `CG_DrawHUD` (`cg_draw.c`) draws named items of their
//! `lefthud` and `righthud` menus. Loading those files gives the original
//! HUD, and any PK3 that replaces `ui/hud.menu` or names another list in
//! `cg_hudFiles` gives a custom HUD, with no JKR-specific format. A nonzero
//! integer `cg_hudFiles` selects the stock text-only HUD instead.
//!
//! `cg_hudStyle` chooses between JKR's own modern and classic layouts and
//! this game-data HUD (`game`). In `game` mode the JKR status widgets hide
//! ([`crate::hud::HudVisibility::menu_hud`]) and everything else the JKR HUD
//! shows (crosshair, obituaries, timers, chat) stays. Files that cannot be
//! read, or that define no HUD menus, leave JKR's HUD in place.
//!
//! Files are parsed and pictures packed when the mode is first used and
//! again when `cg_hudFiles` changes; each frame only fills fixed storage.

mod frame;
mod gpu;
mod layout;
mod parse;

use crate::GpuState;
use frame::{Frame, Readout, Timers};
use jkr_shell::{CvarDefinition, CvarFlags, CvarRegistry};
use jkr_vfs::VirtualFileSystem;
use layout::{Layout, Side};

/// Which HUD the player sees.
pub(crate) const STYLE_CVAR: &str = "cg_hudStyle";
/// The menu list (or text-HUD switch) of the game-data HUD.
pub(crate) const FILES_CVAR: &str = "cg_hudFiles";
/// The retail `cg_hudFiles` default.
const DEFAULT_LIST: &str = "ui/jahud.txt";
/// `gfx/2d/numbers/t_*`, the `NUM_FONT_SMALL` digits, then the minus sign.
const DIGITS: [&str; 11] = [
    "gfx/2d/numbers/t_zero",
    "gfx/2d/numbers/t_one",
    "gfx/2d/numbers/t_two",
    "gfx/2d/numbers/t_three",
    "gfx/2d/numbers/t_four",
    "gfx/2d/numbers/t_five",
    "gfx/2d/numbers/t_six",
    "gfx/2d/numbers/t_seven",
    "gfx/2d/numbers/t_eight",
    "gfx/2d/numbers/t_nine",
    "gfx/2d/numbers/t_minus",
];
/// `#include` nesting the loader follows.
const INCLUDE_DEPTH: usize = 4;

/// `cg_hudStyle` values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HudStyle {
    /// JKR's own layout (or the classic one while `cg_classicHudFont` is on).
    Modern,
    /// JKR's classic layout in either font.
    Classic,
    /// The game-data HUD of `cg_hudFiles`.
    Game,
}

impl HudStyle {
    /// Settings choices, in [`HudStyle`] order; the first is the default.
    pub(crate) const NAMES: [&'static str; 3] = ["modern", "classic", "game"];

    /// Read the cvar; anything unknown is the default.
    pub(crate) fn from_cvar(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some(text) if text.eq_ignore_ascii_case("classic") => Self::Classic,
            Some(text) if text.eq_ignore_ascii_case("game") => Self::Game,
            _ => Self::Modern,
        }
    }

    pub(crate) fn read(console: Option<&crate::console::ViewerConsole>) -> Self {
        Self::from_cvar(console.and_then(|console| console.text_value(STYLE_CVAR)))
    }
}

/// What `cg_hudFiles` selects.
#[derive(Debug, Eq, PartialEq)]
enum Source<'a> {
    /// The stock text HUD (`cg_hudFiles.integer` nonzero in `CG_DrawHUD`).
    Text,
    /// A menu list or menu file.
    Menus(&'a str),
}

fn source(value: &str) -> Source<'_> {
    let value = value.trim();
    match value.parse::<i64>() {
        // EternalJK reads 0 as the default list and 3/4 as its bundled
        // elegance and JoF HUDs; 1 and 2 are its (and stock's) text HUDs.
        Ok(0) => Source::Menus(DEFAULT_LIST),
        Ok(3) => Source::Menus("ui/elegance_hud.txt"),
        Ok(4) => Source::Menus("ui/jof_hud.txt"),
        Ok(_) => Source::Text,
        Err(_) if value.is_empty() => Source::Menus(DEFAULT_LIST),
        Err(_) => Source::Menus(value),
    }
}

pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), jkr_shell::CvarError> {
    cvars.register(CvarDefinition::new(
        STYLE_CVAR,
        HudStyle::NAMES[0],
        CvarFlags::ARCHIVE,
        "HUD: modern, classic (JKR layouts) or game (the game's menu-file HUD, cg_hudFiles)",
    ))?;
    cvars.register(CvarDefinition::new(
        FILES_CVAR,
        DEFAULT_LIST,
        CvarFlags::ARCHIVE,
        "Game HUD menu list (ui/jahud.txt); 1 or 2 is the text HUD, 3/4 EternalJK's HUDs",
    ))?;
    Ok(())
}

enum Loaded {
    /// Nothing loaded, or the files were unusable.
    None,
    Text,
    Menus {
        layout: Box<Layout>,
        /// Picture index of the first digit.
        digits: u16,
    },
}

/// Game-data HUD state owned by [`GpuState`].
pub(crate) struct MenuHud {
    gpu: gpu::Gpu,
    loaded: Loaded,
    /// `cg_hudFiles` the current load was made for; `None` before any.
    loaded_for: Option<String>,
    frame: Frame,
    timers: Timers,
    score_label: String,
    /// Whether the game-data HUD stands in for JKR's status widgets.
    active: bool,
    /// Whether this frame's pictures and text are drawn.
    drawn: bool,
}

impl MenuHud {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self {
            gpu: gpu::Gpu::new(device, format),
            loaded: Loaded::None,
            loaded_for: None,
            frame: Frame::default(),
            timers: Timers::default(),
            score_label: String::from("Score"),
            active: false,
            drawn: false,
        }
    }

    /// Whether JKR's status widgets should hide this frame.
    pub(crate) fn active(&self) -> bool {
        self.active
    }

    /// Follow `cg_hudStyle` and `cg_hudFiles`, loading on first use or change.
    pub(crate) fn sync(gpu: &mut GpuState) {
        let style = HudStyle::read(gpu.console.as_ref());
        if style != HudStyle::Game {
            gpu.menu_hud.active = false;
            return;
        }
        let files = gpu
            .console
            .as_ref()
            .and_then(|console| console.text_value(FILES_CVAR))
            .unwrap_or(DEFAULT_LIST);
        if gpu.menu_hud.loaded_for.as_deref() != Some(files) {
            let files = files.to_owned();
            let Some(vfs) = gpu.vfs.clone() else {
                return;
            };
            gpu.menu_hud
                .load(&gpu.device, &gpu.queue, &vfs, &gpu.shaders, &files);
            if let Some(label) = gpu.localization.strings.get("SCORE") {
                gpu.menu_hud.score_label.clone_from(label);
            }
            gpu.menu_hud.loaded_for = Some(files);
        }
        gpu.menu_hud.active = !matches!(gpu.menu_hud.loaded, Loaded::None);
    }

    fn load(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &jkr_shader::ShaderCatalog,
        files: &str,
    ) {
        self.timers = Timers::default();
        self.gpu.unload();
        self.loaded = Loaded::None;
        let list = match source(files) {
            Source::Text => {
                self.loaded = Loaded::Text;
                crate::log::progress(format_args!("game HUD: text HUD (cg_hudFiles {files})"));
                return;
            }
            Source::Menus(list) => list,
        };
        // CG_LoadMenus falls back to the default list when a file is missing.
        let menus = read_menus(vfs, list)
            .filter(|menus| !menus.is_empty())
            .or_else(|| (list != DEFAULT_LIST).then(|| read_menus(vfs, DEFAULT_LIST))?);
        let mut layout = Layout::resolve(menus.as_deref().unwrap_or_default());
        if !layout.is_usable() {
            crate::log::progress(format_args!(
                "warning: game HUD {list} defines no lefthud/righthud menu; keeping the JKR HUD"
            ));
            return;
        }
        let digits = layout.pictures.len() as u16;
        let mut largest = vec![0.0_f32; layout.pictures.len()];
        layout.for_each_piece(|piece| {
            if let Some(picture) = piece.picture {
                let [_, _, w, h] = piece.rect;
                let side = &mut largest[usize::from(picture)];
                *side = side.max(w.abs()).max(h.abs());
            }
        });
        let digit_side = [&layout.health_amount, &layout.armor_amount]
            .into_iter()
            .chain([&layout.force_amount, &layout.ammo_amount])
            .flatten()
            .map(|piece| piece.rect[2].abs().max(piece.rect[3].abs()))
            .fold(12.0_f32, f32::max);
        layout
            .pictures
            .extend(DIGITS.iter().map(|name| (*name).to_owned()));
        largest.extend([digit_side; DIGITS.len()]);
        let found = self
            .gpu
            .load(device, queue, vfs, shaders, &layout.pictures, &largest);
        crate::log::progress(format_args!(
            "game HUD: {list}, {found} of {} pictures",
            layout.pictures.len()
        ));
        self.loaded = Loaded::Menus {
            layout: Box::new(layout),
            digits,
        };
    }

    /// Build and upload this frame. `visible` is retail's condition for
    /// `CG_DrawStats`: a living, non-spectating player with the status HUD
    /// on and no scoreboard.
    pub(crate) fn prepare(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        readout: Option<&Readout>,
        viewport: [f32; 2],
        scale: f32,
    ) {
        self.drawn = false;
        let readout = readout.filter(|_| self.active);
        let Some(readout) = readout else {
            self.gpu.prepare(queue, None, viewport, scale);
            return;
        };
        match &self.loaded {
            Loaded::None => {}
            Loaded::Text => {
                self.frame.simple_hud(readout, &mut self.timers);
                self.drawn = true;
            }
            Loaded::Menus { layout, digits } => {
                self.frame.menu_hud(
                    layout,
                    readout,
                    &mut self.timers,
                    *digits,
                    &self.score_label,
                );
                self.drawn = true;
            }
        }
        self.gpu
            .prepare(queue, self.drawn.then_some(&self.frame), viewport, scale);
    }

    /// Append this frame's text runs (score line, infinite-ammo mark, text
    /// HUD) in the HUD font.
    pub(crate) fn append_text(
        &self,
        vertices: &mut Vec<crate::text::TextVertex>,
        font: &crate::text::UiFont,
        viewport: [f32; 2],
        scale: f32,
    ) {
        if !self.drawn {
            return;
        }
        for run in &self.frame.texts[..self.frame.text_count] {
            let [x, y, _, size] = gpu::to_pixels(
                [run.x, run.y, 0.0, run.size],
                run.side.unwrap_or(Side::Left),
                viewport,
                scale,
            );
            let glyph_scale = size / font.height.max(1.0);
            let face = crate::text::TextFace::Regular;
            let x = if run.centred {
                x - crate::text::visible_text_width_style(font, &run.text, glyph_scale, face, 0.0)
                    * 0.5
            } else {
                x
            };
            crate::text::append_text_style(
                vertices,
                font,
                &run.text,
                [x, y],
                glyph_scale,
                viewport,
                face,
                run.color,
                0.0,
            );
        }
    }

    /// Draw this frame's pictures (before the retained UI shapes and text).
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.drawn {
            self.gpu.draw(pass);
        }
    }
}

/// The values the HUD shows, or `None` when retail would not draw it.
pub(crate) fn readout(gpu: &GpuState, time: i32) -> Option<Readout> {
    let (snapshot, game) = if let Some(session) = &gpu.live_session {
        (session.latest_snapshot(), session.game_state())
    } else if let Some(session) = &gpu.demo_session {
        (session.snapshot_at_or_before(time), session.game_state())
    } else {
        return None;
    };
    let player = &snapshot.player;
    // CG_Draw2D: no status for spectators, the dead or an open scoreboard;
    // CG_DrawHUD: none for PM_SPECTATOR.
    if player.team() == 3
        || player.is_spectator()
        || player.health() <= 0
        || gpu.gameplay_input.held(crate::input::GameButton::Scores)
    {
        return None;
    }
    let console = gpu.console.as_ref();
    let enabled = |name, fallback| {
        console
            .and_then(|console| console.bool_cvar(name))
            .unwrap_or(fallback)
    };
    if !(enabled("cg_draw2D", true)
        && enabled("cg_drawHud", true)
        && enabled("cg_drawStatus", true))
    {
        return None;
    }
    let predicted = gpu.local_prediction.predicted_state();
    let info = jkr_client::LegacyClientInfo::new(game.config_string(0).unwrap_or_default());
    let gametype = info.integer("g_gametype").unwrap_or(0);
    let mut ammo = [0; 10];
    for (slot, value) in ammo.iter_mut().enumerate() {
        *value = predicted.map_or(player.ammo[slot] as i32, |state| state.ammo[slot]);
    }
    Some(Readout {
        health: player.health(),
        max_health: player.max_health(),
        armor: player.armor(),
        force: i32::from(player.force_power()),
        weapon: predicted.map_or(player.weapon(), |state| state.weapon),
        ammo,
        saber_style: player.saber_draw_style(),
        weapon_state: predicted.map_or(player.weapon_state(), |state| state.weapon_state),
        weapon_time: predicted.map_or(player.weapon_time(), |state| state.weapon_time),
        double_ammo: player.entity_flags() & (1 << 20) != 0,
        score: player.persistent[0] as i32,
        duel: gametype == 3,
        power_duel: gametype == 4,
        fraglimit: info.integer("fraglimit").unwrap_or(0),
        time,
    })
}

/// Parse `list` and every menu file it loads or includes.
fn read_menus(vfs: &VirtualFileSystem, list: &str) -> Option<Vec<parse::MenuDef>> {
    let mut menus = Vec::new();
    read_file(vfs, list, 0, &mut menus)?;
    Some(menus)
}

fn read_file(
    vfs: &VirtualFileSystem,
    path: &str,
    depth: usize,
    menus: &mut Vec<parse::MenuDef>,
) -> Option<()> {
    let asset = vfs.read(path).ok().flatten()?;
    let parsed = parse::parse(&String::from_utf8_lossy(&asset.bytes));
    for include in &parsed.includes {
        if depth < INCLUDE_DEPTH {
            // Headers such as ui/menudef.h only define constants.
            let _ = read_file(vfs, include, depth + 1, menus);
        }
    }
    menus.extend(parsed.menus);
    for file in &parsed.load {
        if depth < INCLUDE_DEPTH && read_file(vfs, file, depth + 1, menus).is_none() {
            crate::log::progress(format_args!("warning: game HUD menu {file} not found"));
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hud_files_values_follow_retail_and_eternaljk() {
        assert_eq!(source(""), Source::Menus(DEFAULT_LIST));
        assert_eq!(source("0"), Source::Menus(DEFAULT_LIST));
        assert_eq!(source("ui/jahud.txt"), Source::Menus("ui/jahud.txt"));
        assert_eq!(
            source(" ui/elegance_hud.txt "),
            Source::Menus("ui/elegance_hud.txt")
        );
        assert_eq!(source("1"), Source::Text);
        assert_eq!(source("2"), Source::Text);
        assert_eq!(source("3"), Source::Menus("ui/elegance_hud.txt"));
        assert_eq!(source("4"), Source::Menus("ui/jof_hud.txt"));
    }

    #[test]
    fn style_defaults_to_modern() {
        assert_eq!(HudStyle::from_cvar(None), HudStyle::Modern);
        assert_eq!(HudStyle::from_cvar(Some("GAME")), HudStyle::Game);
        assert_eq!(HudStyle::from_cvar(Some("classic")), HudStyle::Classic);
        assert_eq!(HudStyle::from_cvar(Some("retro")), HudStyle::Modern);
        let parsed = HudStyle::NAMES.map(|name| HudStyle::from_cvar(Some(name)));
        assert_eq!(
            parsed,
            [HudStyle::Modern, HudStyle::Classic, HudStyle::Game]
        );
    }
}
