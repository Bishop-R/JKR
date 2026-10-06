//! Optional game fonts, read from the player's game data.
//!
//! `ui_gameFont` swaps the bundled Inter for Jedi Academy's own fonts on every
//! surface the retail game drew with them ([`RetailFont`]):
//!
//! - `ergoec`, the medium font (`FONT_MEDIUM`): menus (`assetGlobalDef` in
//!   `ui/main.menu` and `ui/ingame.menu`), the crosshair name, centre prints,
//!   the warmup text and match timer, enemy info, and scoreboard names and
//!   headings;
//! - `ocr_a`, the small font (`FONT_SMALL`; OpenJK `codemp` `cg_main.c`
//!   registers it as `qhSmallFont`): the chat box (`CG_ChatBox_DrawStrings`),
//!   weapon, Force and inventory selection names (`UI_SMALLFONT`) and
//!   scoreboard numbers;
//! - the console character set ([`text::charset`]): the console and its notify
//!   lines, what the cgame drew with `CG_DrawBigString`, `CG_DrawSmallString`
//!   or `CG_DrawStringExt` (FPS, snapshot, vote, team overlay, connection
//!   interrupted), and obituaries, which the cgame printed to the console.
//!
//! The fonts come from the mounted game data, so an HD replacement atlas in a
//! later PK3 is used automatically; nothing is bundled. A font that is missing
//! or unreadable leaves its surfaces on Inter. Retail-size atlases are uploaded
//! as signed distance fields ([`text::sdf`]) and drawn with the text pipeline's
//! distance-field fragment, so magnified text keeps sharp edges instead of the
//! bitmap's bilinear blur; large HD atlases are drawn from their own coverage.
//! `cg_classicHudFont` keeps its own scope, the status HUD's `arialnb`.
//!
//! The fonts load the first time the option is on and stay resident for the
//! world. A world installed while the option is on loads them on its install
//! worker ([`GameFonts::preload`]), so a map change does not decode the atlases
//! on the frame thread. Each font has its own vertex buffer and atlas bind
//! group. The medium and small fonts draw before the Inter text, as when every
//! surface shared one buffer; the console font draws after all other text so
//! the console stays on top ([`GameFonts::draw_console`]).

use crate::GpuState;
use crate::gpu_texture;
use crate::text::{self, MAX_TEXT_VERTICES, TextVertex, UiFont};
use jkr_ui::{DrawList, TextId};
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

/// The retail font a text surface was drawn with.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RetailFont {
    /// `FONT_MEDIUM`, `ergoec`.
    Medium,
    /// `FONT_SMALL`, `ocr_a`.
    Small,
    /// The console character set.
    Console,
}

impl RetailFont {
    const ALL: [Self; 3] = [Self::Medium, Self::Small, Self::Console];

    const fn index(self) -> usize {
        match self {
            Self::Medium => 0,
            Self::Small => 1,
            Self::Console => 2,
        }
    }
}

/// One loaded font with its per-frame vertices and GPU resources.
struct Layer {
    font: UiFont,
    vertices: Vec<TextVertex>,
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// The atlas was uploaded as a distance field.
    distance_field: bool,
    count: u32,
}

/// Device resources the font layers are created with.
pub(crate) struct Device<'a> {
    pub(crate) device: &'a wgpu::Device,
    pub(crate) queue: &'a crate::frame_queue::FrameQueue,
    /// The text pipeline's atlas bind group layout.
    pub(crate) layout: &'a wgpu::BindGroupLayout,
    pub(crate) sampler: &'a wgpu::Sampler,
}

impl Layer {
    fn load(vfs: &VirtualFileSystem, name: &str, gpu: &Device<'_>) -> Option<Self> {
        match text::fontdat::read(vfs, name) {
            Ok((fontdat, image)) => Some(Self::upload(
                name,
                fontdat.into_typographic_font(),
                image,
                gpu,
            )),
            Err(error) => {
                crate::log::progress(format_args!(
                    "warning: game font {name} unavailable, keeping Inter: {error}"
                ));
                None
            }
        }
    }

    fn load_charset(vfs: &VirtualFileSystem, gpu: &Device<'_>) -> Option<Self> {
        match text::charset::read(vfs) {
            Ok(image) => Some(Self::upload(
                text::charset::PATH,
                text::charset::font(),
                image,
                gpu,
            )),
            Err(error) => {
                crate::log::progress(format_args!(
                    "warning: console character set unavailable, keeping Inter: {error}"
                ));
                None
            }
        }
    }

    fn upload(name: &str, font: UiFont, image: image::RgbaImage, gpu: &Device<'_>) -> Self {
        let started = std::time::Instant::now();
        let (width, height) = image.dimensions();
        let (field, distance_field) = text::sdf::for_atlas(image);
        if distance_field {
            crate::log::progress(format_args!(
                "game font {name}: {width}x{height} atlas, {}x{} distance field in {:.1} ms",
                field.width(),
                field.height(),
                started.elapsed().as_secs_f64() * 1_000.0
            ));
        } else {
            crate::log::progress(format_args!(
                "game font {name}: {width}x{height} HD atlas, drawn from coverage"
            ));
        }
        let view = gpu_texture::create_rgba8_texture_mipmapped(
            gpu.device,
            gpu.queue,
            "JKR game font atlas",
            &field,
            true,
            mip_levels(field.width(), field.height()),
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
        Self {
            font,
            vertices: Vec::with_capacity(4_096),
            buffer,
            bind_group,
            distance_field,
            count: 0,
        }
    }

    fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipeline: &wgpu::RenderPipeline,
        sdf_pipeline: &wgpu::RenderPipeline,
    ) {
        if self.count != 0 {
            pass.set_pipeline(if self.distance_field {
                sdf_pipeline
            } else {
                pipeline
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.buffer.slice(..));
            pass.draw(0..self.count, 0..1);
        }
    }
}

/// Mip levels for an atlas or its distance field of `width` x `height`: one per
/// halving down to the retail atlas size, so HD atlases minify cleanly to UI
/// text sizes.
pub(crate) fn mip_levels(width: u32, height: u32) -> u32 {
    1 + (width.max(height) / RETAIL_ATLAS_SIZE).max(1).ilog2()
}

/// Lazily loaded game fonts and this frame's on/off decision.
#[derive(Default)]
pub(crate) struct GameFonts {
    enabled: bool,
    attempted: bool,
    menu: Option<Layer>,
    chat: Option<Layer>,
    console: Option<Layer>,
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
        self.console = Layer::load_charset(vfs, gpu);
    }

    fn slot(&self, font: RetailFont) -> &Option<Layer> {
        match font {
            RetailFont::Medium => &self.menu,
            RetailFont::Small => &self.chat,
            RetailFont::Console => &self.console,
        }
    }

    fn slot_mut(&mut self, font: RetailFont) -> &mut Option<Layer> {
        match font {
            RetailFont::Medium => &mut self.menu,
            RetailFont::Small => &mut self.chat,
            RetailFont::Console => &mut self.console,
        }
    }

    /// Whether `font` is on and loaded this frame.
    pub(crate) fn active(&self, font: RetailFont) -> bool {
        self.enabled && self.slot(font).is_some()
    }

    /// Layout metrics of `font` when it is on and loaded.
    pub(crate) fn font(&self, font: RetailFont) -> Option<&UiFont> {
        self.slot(font)
            .as_ref()
            .filter(|_| self.enabled)
            .map(|layer| &layer.font)
    }

    /// Text target for a surface retail drew with `font`: that font when it is
    /// on and loaded, otherwise the given Inter `vertices` and `inter`.
    pub(crate) fn target<'a>(
        &'a mut self,
        font: RetailFont,
        vertices: &'a mut Vec<TextVertex>,
        inter: &'a UiFont,
    ) -> (&'a mut Vec<TextVertex>, &'a UiFont) {
        let enabled = self.enabled;
        target(enabled, self.slot_mut(font), vertices, inter)
    }

    /// Text target for menus ([`RetailFont::Medium`]).
    pub(crate) fn menu<'a>(
        &'a mut self,
        vertices: &'a mut Vec<TextVertex>,
        font: &'a UiFont,
    ) -> (&'a mut Vec<TextVertex>, &'a UiFont) {
        self.target(RetailFont::Medium, vertices, font)
    }

    /// Append one draw list's text, each command in the font `font_of` names
    /// for it when that font is on and loaded, and everything else to
    /// `fallback`. With the option off this is the single pass it replaces.
    pub(crate) fn append_routed<'s>(
        &mut self,
        draw_list: &DrawList,
        resolve: impl Fn(TextId) -> &'s str + Copy,
        font_of: impl Fn(TextId, &str) -> Option<RetailFont> + Copy,
        fallback: (&mut Vec<TextVertex>, &UiFont),
        viewport: [f32; 2],
    ) {
        let active = RetailFont::ALL.map(|font| self.active(font));
        for font in RetailFont::ALL {
            if !active[font.index()] {
                continue;
            }
            if let Some(layer) = self.slot_mut(font) {
                crate::ui_renderer::append_text_commands_where(
                    draw_list,
                    resolve,
                    |id, text| font_of(id, text) == Some(font),
                    &mut layer.vertices,
                    &layer.font,
                    viewport,
                );
            }
        }
        crate::ui_renderer::append_text_commands_where(
            draw_list,
            resolve,
            |id, text| !font_of(id, text).is_some_and(|font| active[font.index()]),
            fallback.0,
            fallback.1,
            viewport,
        );
    }

    fn layers_mut(&mut self) -> impl Iterator<Item = &mut Layer> {
        self.menu
            .iter_mut()
            .chain(self.chat.iter_mut())
            .chain(self.console.iter_mut())
    }

    /// Copy this frame's vertices to the GPU.
    pub(crate) fn upload(&mut self, queue: &crate::frame_queue::FrameQueue) {
        for layer in self.layers_mut() {
            layer.count = u32::try_from(layer.vertices.len()).unwrap_or(0);
            if layer.count != 0 {
                queue.write_buffer(&layer.buffer, 0, bytemuck::cast_slice(&layer.vertices));
            }
        }
    }

    /// Draw the uploaded medium and small font text, which sits under the Inter
    /// text: distance-field atlases with `sdf_pipeline`, HD coverage atlases
    /// with the plain text `pipeline`.
    pub(crate) fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipeline: &wgpu::RenderPipeline,
        sdf_pipeline: &wgpu::RenderPipeline,
    ) {
        for layer in self.menu.iter().chain(self.chat.iter()) {
            layer.draw(pass, pipeline, sdf_pipeline);
        }
    }

    /// Draw the uploaded console font text, after all other text.
    pub(crate) fn draw_console(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipeline: &wgpu::RenderPipeline,
        sdf_pipeline: &wgpu::RenderPipeline,
    ) {
        if let Some(layer) = &self.console {
            layer.draw(pass, pipeline, sdf_pipeline);
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

/// Text `size` (a line-box height in physical pixels) as a glyph scale for
/// `font`, for callers that place text with the bare text functions.
pub(crate) fn scale_for(font: &UiFont, size: f32) -> f32 {
    size / font.height.max(1.0)
}

/// Read the option, load the fonts on first use and clear last frame's text.
/// Call before any text is appended.
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

    #[test]
    fn routing_with_no_font_loaded_sends_everything_to_the_fallback() {
        let inter = text::charset::font();
        let mut list = DrawList::new(8);
        for id in 0..3 {
            let _ = list.push(jkr_ui::DrawCommand::Text {
                rect: jkr_ui::Rect::new(0.0, 0.0, 100.0, 16.0),
                text: TextId(id),
                size: 16.0,
                color: jkr_ui::Color::new(1.0, 1.0, 1.0, 1.0),
                align: jkr_ui::TextAlign::Start,
                overflow: jkr_ui::TextOverflow::Clip,
                weight: jkr_ui::FontWeight::Regular,
                letter_spacing: 0.0,
            });
        }
        let mut fonts = GameFonts {
            enabled: true,
            ..GameFonts::default()
        };
        let mut vertices = Vec::new();
        fonts.append_routed(
            &list,
            |_| "ab",
            |id, _| (id.0 == 1).then_some(RetailFont::Medium),
            (&mut vertices, &inter),
            [640.0, 480.0],
        );
        // Three runs of two glyphs, each a shadow and a glyph quad.
        assert_eq!(vertices.len(), 3 * 2 * 2 * 6);
    }

    #[test]
    fn size_becomes_a_glyph_scale() {
        assert_eq!(scale_for(&text::charset::font(), 24.0), 1.5);
    }
}
