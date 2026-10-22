//! One retained leader/opponent portrait in the existing HUD icon atlas.
use jkr_protocol::GameState;
use jkr_ui::{Color, DrawCommand, DrawList, Rect, TextureId};

// The HUD's bg_itemlist slot zero has no item/image; menu cells precede it.
const TEXTURE: TextureId = TextureId(crate::ui_renderer::ICON_CELLS);

/// Cached selected-client art; an unresolved icon deliberately emits nothing.
pub(super) struct Portrait {
    model: Vec<u8>,
    ready: bool,
    /// The atlas cell it is drawn from.
    texture: TextureId,
}

impl Default for Portrait {
    fn default() -> Self {
        Self::at(TEXTURE)
    }
}

fn model(game: &GameState, client: Option<u16>) -> &[u8] {
    client
        .and_then(|id| game.config_string(1131 + usize::from(id)))
        .and_then(|bytes| jkr_client::LegacyClientInfo::new(bytes).bytes("model"))
        .unwrap_or_default()
}

fn icon_path(model: &[u8]) -> Option<String> {
    if model.is_empty() {
        return None;
    }
    // Only the asset field is interpreted, never the enclosing (byte-string) clientinfo.
    let value = String::from_utf8_lossy(model);
    let (model, skin) = value.split_once('/').unwrap_or((&value, "default"));
    // Customizable species identify their head icon before torso/legs selections.
    let skin = skin
        .split('|')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("default");
    Some(format!("models/players/{model}/icon_{skin}"))
}

impl Portrait {
    /// A portrait kept in atlas cell `texture`.
    pub(super) fn at(texture: TextureId) -> Self {
        Self {
            model: Vec::new(),
            ready: false,
            texture,
        }
    }

    /// Resolve only when selected model bytes change, retaining misses until then as well.
    pub(super) fn update(
        &mut self,
        game: &GameState,
        client: Option<u16>,
        vfs: &jkr_vfs::VirtualFileSystem,
        shaders: &jkr_shader::ShaderCatalog,
        renderer: &crate::ui_renderer::ShapeRenderer,
        queue: &crate::frame_queue::FrameQueue,
    ) {
        let texture = self.texture;
        self.set_model(model(game, client), |path| {
            let Some(pixels) = super::icons::assets::decode(vfs, shaders, path) else {
                crate::log::progress(format_args!("HUD portrait absent, not drawn: {path}"));
                return false;
            };
            renderer.upload_icon(queue, texture, pixels.as_raw());
            true
        });
    }

    fn set_model(&mut self, value: &[u8], resolve: impl FnOnce(&str) -> bool) {
        if self.model == value {
            return;
        }
        self.model.clear();
        self.model.extend_from_slice(value);
        self.ready = false;
        let Some(path) = icon_path(value) else {
            return;
        };
        self.ready = resolve(&path);
    }

    /// Height this portrait occupies above the leader/opponent text, zero when unresolved.
    pub(super) const HEIGHT: f32 = 92.0;

    /// Whether an image resolved, and so whether [`Portrait::HEIGHT`] applies to the layout.
    pub(super) fn drawn(&self) -> bool {
        self.ready
    }

    /// Emit the resolved portrait above the leader/opponent text, sharing its right edge.
    pub(super) fn emit(&self, list: &mut DrawList, viewport: [f32; 2], top: f32) {
        if !self.ready {
            return;
        }
        let s = (viewport[1] / 1080.0).clamp(0.6, 2.5);
        self.emit_at(
            list,
            Rect::new(viewport[0] - 128.0 * s, top * s, 88.0 * s, 88.0 * s),
        );
    }

    /// Emit the resolved portrait into `rect`.
    pub(super) fn emit_at(&self, list: &mut DrawList, rect: Rect) {
        if !self.ready {
            return;
        }
        let _ = list.push(DrawCommand::TexturedQuad {
            rect,
            texture: self.texture,
            color: Color::new(1.0, 1.0, 1.0, 1.0),
        });
    }
}
