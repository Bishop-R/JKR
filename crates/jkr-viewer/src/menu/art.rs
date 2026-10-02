//! The player's own retail menu artwork for the classic menu style.
//!
//! The classic menus draw the original `gfx/menus` images (backgrounds,
//! window frames, the game logo, the in-game bar and boxes) when the mounted
//! game data has them. Nothing is bundled: a piece that is missing or fails
//! to decode is simply absent, and the classic views fall back to JKR's own
//! vector drawing for it.
//!
//! The images are decoded once per process on a worker thread, the first
//! time the classic style is in use ([`request`]), and kept as RGBA so each
//! world's UI renderer can upload them without decoding again
//! ([`decoded`]). The renderer gives every piece its own texture and bind
//! group (`ui_renderer/art.rs`) rather than a slot in the shared UI icon
//! atlas, which has no room for full-screen art.

use image::RgbaImage;
use jkr_ui::TextureId;
use jkr_vfs::VirtualFileSystem;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

/// First `TextureId` naming a piece; far from the atlas cell indices and
/// the atlas' reserved ids at the top of the range.
const TEXTURE_BASE: u32 = 0x4000_0000;
/// Longest side kept for a piece; larger HD replacements are scaled down.
const MAX_SIDE: u32 = 4_096;

/// How the retail shader blends a piece over the screen.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Blend {
    /// `GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA`, or an opaque image.
    Alpha,
    /// `GL_ONE GL_ONE` (or the near-identical screen blend of the in-game
    /// bar): black is transparent and light adds. Converted to alpha at
    /// decode time so the shared alpha-blended UI pipeline can draw it.
    Additive,
}

/// One retail menu image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArtPiece {
    /// `main_background`: the full-screen backdrop.
    Background,
    /// `menu_side_text`: the glyph column down the left edge.
    SideLeft,
    /// `menu_side_text_right`: the glyph column down the right edge.
    SideRight,
    /// `main_ring`: the ring around the centre of the main menu.
    Ring,
    /// `main_centerwindow`: the centre window of the main menu.
    CenterWindow,
    /// `main_leftwindow`: the frame behind Play and Profile.
    LeftWindow,
    /// `main_rightwindow`: the frame behind Controls and Setup.
    RightWindow,
    /// `jediacademy`: the game logo across the top.
    Logo,
    /// `main_centerblue`: the panel behind the sub-pages' centre area.
    CenterBlue,
    /// `menu_boxes_left`: the sub-pages' top-left frame.
    BoxesLeft,
    /// `menu_boxes_right`: the sub-pages' top-right frame.
    BoxesRight,
    /// `menu_buttonback`: the glow behind a focused button.
    ButtonBack,
    /// `menu_blendbox`: the band behind page titles.
    BlendBox,
    /// `menu_top_mp`: the in-game menu's top bar.
    TopBar,
    /// `menu_box_dark` (shader `menu_box_ingame`): the in-game pop-up box.
    PopupBox,
}

impl ArtPiece {
    /// Every piece, in [`ArtPiece`] order.
    pub(crate) const ALL: [Self; 15] = [
        Self::Background,
        Self::SideLeft,
        Self::SideRight,
        Self::Ring,
        Self::CenterWindow,
        Self::LeftWindow,
        Self::RightWindow,
        Self::Logo,
        Self::CenterBlue,
        Self::BoxesLeft,
        Self::BoxesRight,
        Self::ButtonBack,
        Self::BlendBox,
        Self::TopBar,
        Self::PopupBox,
    ];
    pub(crate) const COUNT: usize = Self::ALL.len();

    /// Image path in game data, without extension.
    fn path(self) -> &'static str {
        match self {
            Self::Background => "gfx/menus/main_background",
            Self::SideLeft => "gfx/menus/menu_side_text",
            Self::SideRight => "gfx/menus/menu_side_text_right",
            Self::Ring => "gfx/menus/main_ring",
            Self::CenterWindow => "gfx/menus/main_centerwindow",
            Self::LeftWindow => "gfx/menus/main_leftwindow",
            Self::RightWindow => "gfx/menus/main_rightwindow",
            Self::Logo => "gfx/menus/jediacademy",
            Self::CenterBlue => "gfx/menus/main_centerblue",
            Self::BoxesLeft => "gfx/menus/menu_boxes_left",
            Self::BoxesRight => "gfx/menus/menu_boxes_right",
            Self::ButtonBack => "gfx/menus/menu_buttonback",
            Self::BlendBox => "gfx/menus/menu_blendbox",
            Self::TopBar => "gfx/menus/menu_top_mp",
            Self::PopupBox => "gfx/menus/menu_box_dark",
        }
    }

    /// Blend of the piece's retail shader (`shaders/ui.shader`).
    fn blend(self) -> Blend {
        match self {
            Self::ButtonBack | Self::BlendBox | Self::TopBar => Blend::Additive,
            _ => Blend::Alpha,
        }
    }

    /// Position in [`Self::ALL`].
    pub(crate) fn index(self) -> usize {
        self as usize
    }

    /// The `TexturedQuad` texture that draws this piece.
    pub(crate) fn texture(self) -> TextureId {
        TextureId(TEXTURE_BASE + self as u32)
    }

    /// The piece a `TexturedQuad` texture names, if it names one.
    pub(crate) fn from_texture(texture: TextureId) -> Option<Self> {
        let index = texture.0.checked_sub(TEXTURE_BASE)?;
        Self::ALL.get(index as usize).copied()
    }
}

/// Which pieces are ready to draw.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ArtSet(u32);

impl ArtSet {
    /// Whether `piece` can be drawn.
    pub(crate) fn has(self, piece: ArtPiece) -> bool {
        self.0 & (1 << piece.index()) != 0
    }

    /// This set with `piece` added.
    pub(crate) fn with(self, piece: ArtPiece) -> Self {
        Self(self.0 | 1 << piece.index())
    }
}

/// The decoded pieces, indexed by [`ArtPiece::index`].
pub(crate) struct Decoded {
    images: [Option<RgbaImage>; ArtPiece::COUNT],
}

impl Decoded {
    /// The RGBA image of `piece`, ready to upload, if game data has it.
    pub(crate) fn image(&self, piece: ArtPiece) -> Option<&RgbaImage> {
        self.images[piece.index()].as_ref()
    }
}

static DECODED: OnceLock<Decoded> = OnceLock::new();
static REQUESTED: AtomicBool = AtomicBool::new(false);

/// Start decoding the artwork from `vfs` on a worker thread, once per
/// process; later calls do nothing.
pub(crate) fn request(vfs: &Arc<VirtualFileSystem>) {
    if REQUESTED.swap(true, Ordering::AcqRel) {
        return;
    }
    let vfs = Arc::clone(vfs);
    let spawned = std::thread::Builder::new()
        .name("jkr-menu-art".into())
        .spawn(move || {
            let decoded = decode_all(&vfs);
            let found = decoded.images.iter().flatten().count();
            crate::log::progress(format_args!(
                "classic menu art: {found} of {} pieces found in game data",
                ArtPiece::COUNT
            ));
            let _ = DECODED.set(decoded);
        });
    if let Err(error) = spawned {
        crate::log::progress(format_args!(
            "warning: classic menu art not loaded, drawing without it: {error}"
        ));
    }
}

/// The decoded artwork, once the worker has finished.
pub(crate) fn decoded() -> Option<&'static Decoded> {
    DECODED.get()
}

fn decode_all(vfs: &VirtualFileSystem) -> Decoded {
    Decoded {
        images: ArtPiece::ALL.map(|piece| decode(vfs, piece)),
    }
}

/// Read and decode one piece, trying the extensions retail resolves an
/// extensionless image name with.
fn decode(vfs: &VirtualFileSystem, piece: ArtPiece) -> Option<RgbaImage> {
    let mut image = ["tga", "jpg", "png"].iter().find_map(|extension| {
        let path = format!("{}.{extension}", piece.path());
        let asset = vfs.read(&path).ok()??;
        crate::decode_image(&asset.bytes, &path)
            .ok()
            .map(image::DynamicImage::into_rgba8)
    })?;
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    if width.max(height) > MAX_SIDE {
        let scale = MAX_SIDE as f32 / width.max(height) as f32;
        image = image::imageops::resize(
            &image,
            ((width as f32 * scale) as u32).max(1),
            ((height as f32 * scale) as u32).max(1),
            image::imageops::FilterType::Triangle,
        );
    }
    if piece.blend() == Blend::Additive {
        additive_to_alpha(&mut image);
    }
    Some(image)
}

/// Re-express an additively blended image for alpha blending: the
/// brightest channel becomes the alpha and the colour is divided by it, so
/// `colour * alpha + under * (1 - alpha)` adds the original colour and only
/// slightly darkens what lies under bright texels.
fn additive_to_alpha(image: &mut RgbaImage) {
    for pixel in image.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        let peak = r.max(g).max(b);
        if peak == 0 {
            pixel.0 = [0, 0, 0, 0];
            continue;
        }
        let scale = |channel: u8| {
            ((u16::from(channel) * 255 + u16::from(peak) / 2) / u16::from(peak)) as u8
        };
        let alpha = (u16::from(peak) * u16::from(a) / 255) as u8;
        pixel.0 = [scale(r), scale(g), scale(b), alpha];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textures_round_trip_and_stay_clear_of_the_atlas() {
        for piece in ArtPiece::ALL {
            assert_eq!(ArtPiece::from_texture(piece.texture()), Some(piece));
            assert_eq!(ArtPiece::ALL[piece.index()], piece);
        }
        assert_eq!(ArtPiece::from_texture(TextureId(0)), None);
        assert_eq!(ArtPiece::from_texture(TextureId(u32::MAX)), None);
        assert_eq!(ArtPiece::from_texture(TextureId(u32::MAX - 1)), None);
    }

    #[test]
    fn art_set_tracks_pieces() {
        let set = ArtSet::default().with(ArtPiece::Ring).with(ArtPiece::Logo);
        assert!(set.has(ArtPiece::Ring) && set.has(ArtPiece::Logo));
        assert!(!set.has(ArtPiece::Background));
    }

    #[test]
    fn additive_black_is_transparent_and_light_keeps_its_colour() {
        let mut image = RgbaImage::from_raw(
            3,
            1,
            vec![0, 0, 0, 255, 200, 100, 0, 255, 255, 255, 255, 255],
        )
        .expect("3x1 image");
        additive_to_alpha(&mut image);
        assert_eq!(image.get_pixel(0, 0).0, [0, 0, 0, 0]);
        // Alpha-blending the converted texel over black gives the original.
        let [r, g, b, a] = image.get_pixel(1, 0).0;
        let over_black = |c: u8| (u16::from(c) * u16::from(a) / 255) as u8;
        assert_eq!(a, 200);
        assert!(over_black(r).abs_diff(200) <= 1);
        assert!(over_black(g).abs_diff(100) <= 1);
        assert_eq!(over_black(b), 0);
        assert_eq!(image.get_pixel(2, 0).0, [255, 255, 255, 255]);
    }
}
