//! Optional game fonts for menus and chat, read from the player's game data.
//!
//! `ui_gameFont` swaps the bundled Inter for Jedi Academy's own bitmap fonts:
//! `ergoec`, the medium font that the retail menus draw nearly all items with
//! (`assetGlobalDef` in `ui/main.menu` and `ui/ingame.menu`), for menus, and
//! `ocr_a`, the cgame small font the chat box paints with (OpenJK `codemp`
//! `cg_main.c` registers it as `qhSmallFont`; `CG_ChatBox_DrawStrings` uses
//! `FONT_SMALL`), for chat. The fonts come from the mounted game data, so an
//! HD replacement atlas in a later PK3 is used automatically; nothing is
//! bundled. A font that is missing or unreadable leaves its surface on Inter.
//!
//! The fonts load the first time the option is on and stay resident for the
//! world. A world installed while the option is on loads them on its install
//! worker ([`GameFonts::preload`]), so a map change does not decode the atlases
//! on the frame thread. Each font has its own vertex buffer and atlas bind
//! group, drawn before the Inter text so console and HUD text stay on top.

use crate::GpuState;
use crate::gpu_texture;
use crate::text::{self, MAX_TEXT_VERTICES, TextVertex, UiFont};
use jkr_vfs::VirtualFileSystem;

/// The cvar that turns the game fonts on.
pub(crate) const CVAR: &str = "ui_gameFont";
/// Retail menu font (`FONT_MEDIUM`).
const MENU_FONT: &str = "ergoec";
/// Retail chat-box font (`FONT_SMALL`).
const CHAT_FONT: &str = "ocr_a";
/// Long side of the retail font atlases. HD replacements are mipmapped down
/// to this size and no further: below it, tightly packed neighbouring glyphs
/// would bleed into each other.
const RETAIL_ATLAS_SIZE: u32 = 512;

/// One loaded font with its per-frame vertices and GPU resources.
struct Layer {
    font: UiFont,
    vertices: Vec<TextVertex>,
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    count: u32,
}

/// Device resources the font layers are created with.
pub(crate) struct Device<'a> {
    pub(crate) device: &'a wgpu::Device,
    pub(crate) queue: &'a wgpu::Queue,
    /// The text pipeline's atlas bind group layout.
    pub(crate) layout: &'a wgpu::BindGroupLayout,
    pub(crate) sampler: &'a wgpu::Sampler,
}

impl Layer {
    fn load(vfs: &VirtualFileSystem, name: &str, gpu: &Device<'_>) -> Option<Self> {
        let (fontdat, image) = match text::fontdat::read(vfs, name) {
            Ok(loaded) => loaded,
            Err(error) => {
                crate::log::progress(format_args!(
                    "warning: game font {name} unavailable, keeping Inter: {error}"
                ));
                return None;
            }
        };
        crate::log::progress(format_args!(
            "game font {name}: {}x{} atlas",
            image.width(),
            image.height()
        ));
        let view = gpu_texture::create_rgba8_texture_mipmapped(
            gpu.device,
            gpu.queue,
            "JKR game font atlas",
            &image,
            true,
            mip_levels(image.width(), image.height()),
        );
        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("JKR game font bind group"),
            layout: gpu.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(gpu.sampler),
                },
            ],
        });
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("JKR game font text vertices"),
            size: (MAX_TEXT_VERTICES * std::mem::size_of::<TextVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Some(Self {
            font: fontdat.into_typographic_font(),
            vertices: Vec::with_capacity(4_096),
            buffer,
            bind_group,
            count: 0,
        })
    }
}

/// Mip levels for an atlas of `width` x `height`: one per halving down to the
/// retail atlas size, so HD atlases minify cleanly to UI text sizes.
fn mip_levels(width: u32, height: u32) -> u32 {
    1 + (width.max(height) / RETAIL_ATLAS_SIZE).max(1).ilog2()
}

/// Lazily loaded game fonts and this frame's on/off decision.
#[derive(Default)]
pub(crate) struct GameFonts {
    enabled: bool,
    attempted: bool,
    menu: Option<Layer>,
    chat: Option<Layer>,
}

impl GameFonts {
    /// Fonts for a new world: loaded now when `enabled`, else on first use.
    pub(crate) fn preload(enabled: bool, vfs: &VirtualFileSystem, gpu: &Device<'_>) -> Self {
        let mut fonts = Self::default();
        if enabled {
            fonts.load(vfs, gpu);
        }
        fonts
    }

    fn load(&mut self, vfs: &VirtualFileSystem, gpu: &Device<'_>) {
        self.attempted = true;
        self.menu = Layer::load(vfs, MENU_FONT, gpu);
        self.chat = Layer::load(vfs, CHAT_FONT, gpu);
    }

    /// Text target for menus: the game menu font when it is on and loaded,
    /// otherwise the given Inter `vertices` and `font`.
    pub(crate) fn menu<'a>(
        &'a mut self,
        vertices: &'a mut Vec<TextVertex>,
        font: &'a UiFont,
    ) -> (&'a mut Vec<TextVertex>, &'a UiFont) {
        target(self.enabled, &mut self.menu, vertices, font)
    }

    /// Text target for chat, with the same fallback as [`Self::menu`].
    pub(crate) fn chat<'a>(
        &'a mut self,
        vertices: &'a mut Vec<TextVertex>,
        font: &'a UiFont,
    ) -> (&'a mut Vec<TextVertex>, &'a UiFont) {
        target(self.enabled, &mut self.chat, vertices, font)
    }

    fn layers_mut(&mut self) -> impl Iterator<Item = &mut Layer> {
        self.menu.iter_mut().chain(self.chat.iter_mut())
    }

    /// Copy this frame's vertices to the GPU.
    pub(crate) fn upload(&mut self, queue: &wgpu::Queue) {
        for layer in self.layers_mut() {
            layer.count = u32::try_from(layer.vertices.len()).unwrap_or(0);
            if layer.count != 0 {
                queue.write_buffer(&layer.buffer, 0, bytemuck::cast_slice(&layer.vertices));
            }
        }
    }

    /// Draw the uploaded text with the shared text `pipeline`.
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, pipeline: &wgpu::RenderPipeline) {
        for layer in self.menu.iter().chain(self.chat.iter()) {
            if layer.count != 0 {
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &layer.bind_group, &[]);
                pass.set_vertex_buffer(0, layer.buffer.slice(..));
                pass.draw(0..layer.count, 0..1);
            }
        }
    }
}

fn target<'a>(
    enabled: bool,
    layer: &'a mut Option<Layer>,
    vertices: &'a mut Vec<TextVertex>,
    font: &'a UiFont,
) -> (&'a mut Vec<TextVertex>, &'a UiFont) {
    match layer {
        Some(layer) if enabled => (&mut layer.vertices, &layer.font),
        _ => (vertices, font),
    }
}

/// Read the option, load the fonts on first use and clear last frame's text.
/// Call before any menu or chat text is appended.
pub(crate) fn prepare(gpu: &mut GpuState) {
    let enabled = enabled(gpu.console.as_ref());
    let fonts = &mut gpu.game_fonts;
    fonts.enabled = enabled;
    for layer in fonts.layers_mut() {
        layer.vertices.clear();
    }
    if enabled
        && !fonts.attempted
        && let Some(vfs) = &gpu.vfs
    {
        let device = Device {
            device: &gpu.device,
            queue: &gpu.queue,
            layout: &gpu.text_layout,
            sampler: &gpu.text_sampler,
        };
        fonts.load(vfs, &device);
    }
}

/// Whether the option is on in `console`, for preloading a world's fonts.
pub(crate) fn enabled(console: Option<&crate::console::ViewerConsole>) -> bool {
    console
        .and_then(|console| console.bool_cvar(CVAR))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retail_atlases_get_no_mips() {
        assert_eq!(mip_levels(512, 256), 1);
        assert_eq!(mip_levels(256, 256), 1);
    }

    #[test]
    fn hd_atlases_mip_down_to_retail_size() {
        // JoF-style HD packs: ocr_a at 2x, ergoec at 8x.
        assert_eq!(mip_levels(1_024, 512), 2);
        assert_eq!(mip_levels(4_096, 2_048), 4);
    }

    #[test]
    fn falls_back_to_inter_when_off_or_missing() {
        let inter = text::fontdat::Fontdat::parse(&[0; 28 * 256 + 10])
            .unwrap()
            .into_typographic_font();
        let mut vertices = Vec::new();
        let mut missing = None;
        let (chosen, font) = target(true, &mut missing, &mut vertices, &inter);
        assert!(std::ptr::eq(font, &inter));
        chosen.push(bytemuck::Zeroable::zeroed());
        assert_eq!(vertices.len(), 1);
    }
}
