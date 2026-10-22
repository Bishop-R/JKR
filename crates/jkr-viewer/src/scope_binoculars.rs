//! The electrobinoculars' view (`CG_DrawZoomMask`, `cg_draw.c:197-286`) on the 640x480
//! screen: the scrolling window, a black strip behind green compass numbers, the numbers'
//! overlay, a pulsing circle, the mask, the zoom arrow and a top triangle that now and
//! then flips. Each picture is drawn stage by stage as its shader says (`zoom.shader`):
//! texture, `blendFunc`, colour from the vertex or not, `tcMod scale` and `scroll`.

use super::{Blend, Quad};
use image::RgbaImage;
use jkr_shader::{ShaderCatalog, StageBlend};
use jkr_vfs::VirtualFileSystem;

/// The pictures in drawing order (`cgs.media.binocular*`, `cg_main.c:1299-1304`).
const PICTURES: [&str; 6] = [
    "gfx/2d/binocularWindow",
    "gfx/2d/binocularNumOverlay",
    "gfx/2d/binCircle",
    "gfx/2d/binMask",
    "gfx/2d/binSideArrow",
    "gfx/2d/binTopTri",
];
const WINDOW: usize = 0;
const NUMBERS: usize = 1;
const CIRCLE: usize = 2;
const MASK: usize = 3;
const ARROW: usize = 4;
const TRIANGLE: usize = 5;
/// `chunkyNumberShaders` (`cg_main.c:1089-1100`).
const DIGITS: [&str; 10] = [
    "gfx/2d/numbers/c_zero",
    "gfx/2d/numbers/c_one",
    "gfx/2d/numbers/c_two",
    "gfx/2d/numbers/c_three",
    "gfx/2d/numbers/c_four",
    "gfx/2d/numbers/c_five",
    "gfx/2d/numbers/c_six",
    "gfx/2d/numbers/c_seven",
    "gfx/2d/numbers/c_eight",
    "gfx/2d/numbers/c_nine",
];
/// Textures the view loads: the pictures' nine stages, the digits and white.
pub(super) const TEXTURES: usize = 20;
/// Stages a picture keeps; `binocularNumOverlay` has the most.
const MAX_STAGES: usize = 3;
/// `CG_DrawNumField`'s chunky digits: 24 by 14, each `24 / 1.2 + 2` along.
const DIGIT_SIZE: [f32; 2] = [24.0, 14.0];
const DIGIT_ADVANCE: i32 = 22;

#[derive(Clone, Copy, Default)]
struct Stage {
    texture: usize,
    blend: Blend,
    /// Colour from the vertex (`rgbGen vertex` or `exactVertex`) rather than white, and
    /// alpha likewise (`alphaGen vertex`, which `rgbGen vertex` implies).
    vertex_rgb: bool,
    vertex_alpha: bool,
    scale: [f32; 2],
    scroll: [f32; 2],
}

#[derive(Clone, Copy, Default)]
struct Picture {
    stages: [Stage; MAX_STAGES],
    count: usize,
}

impl Picture {
    /// A picture with no shader of its own (`R_RegisterShader`'s 2D default): blended by
    /// alpha, coloured by the vertex.
    fn plain(texture: usize) -> Self {
        let mut picture = Self::default();
        picture.stages[0] = Stage {
            texture,
            blend: Blend::Alpha,
            vertex_rgb: true,
            vertex_alpha: true,
            scale: [1.0; 2],
            scroll: [0.0; 2],
        };
        picture.count = 1;
        picture
    }
}

/// The loaded pictures.
pub(super) struct Media {
    pictures: [Picture; 6],
    digits: [Picture; 10],
    /// `cgs.media.whiteShader`.
    white: Picture,
}

/// What a frame of the view reads.
#[derive(Clone, Copy, Debug)]
pub(crate) struct View {
    /// `cg.time`, in milliseconds.
    pub(crate) time: i32,
    /// The view's yaw in degrees, -180 to 180 (`cg.refdef.viewangles[YAW]`).
    pub(crate) yaw: f32,
    /// `cg.predictedPlayerState.zoomFov`.
    pub(crate) zoom_fov: f32,
}

impl Media {
    /// Load every picture through `upload`, which keeps an image and returns its texture;
    /// `None`, and a line in the log, when the game data lacks one.
    pub(super) fn load(
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        mut upload: impl FnMut(&str, &RgbaImage) -> usize,
    ) -> Option<Self> {
        let mut picture = |name: &str| {
            let loaded = load_picture(vfs, shaders, name, &mut upload);
            if loaded.is_none() {
                eprintln!("binocular view unavailable: {name} did not load");
            }
            loaded
        };
        let mut pictures = [Picture::default(); 6];
        for (slot, name) in pictures.iter_mut().zip(PICTURES) {
            *slot = picture(name)?;
        }
        let mut digits = [Picture::default(); 10];
        for (slot, name) in digits.iter_mut().zip(DIGITS) {
            *slot = picture(name)?;
        }
        let white = Picture::plain(upload(
            "white",
            &RgbaImage::from_pixel(1, 1, image::Rgba([255; 4])),
        ));
        Some(Self {
            pictures,
            digits,
            white,
        })
    }

    /// `CG_DrawZoomMask` with `zoomMode` 2, one [`Quad`] per picture stage. `flip` is the
    /// triangle's state between frames, `dice` the random numbers' state.
    pub(super) fn lay_out(
        &self,
        view: View,
        (flip, dice): (&mut bool, &mut u32),
        mut quad: impl FnMut(Quad),
    ) {
        let seconds = f64::from(view.time) / 1000.0;
        let mut picture = |picture: &Picture, [x, y, width, height]: [f32; 4], color: [f32; 4]| {
            for stage in &picture.stages[..picture.count] {
                let offset = |rate: f32| (f64::from(rate) * seconds).rem_euclid(1.0) as f32;
                let rgb = |value: f32| if stage.vertex_rgb { value } else { 1.0 };
                quad(Quad {
                    texture: stage.texture,
                    blend: stage.blend,
                    rect: [x + width * 0.5, y + height * 0.5, width, height],
                    degrees: 0.0,
                    color: [
                        rgb(color[0]),
                        rgb(color[1]),
                        rgb(color[2]),
                        if stage.vertex_alpha { color[3] } else { 1.0 },
                    ],
                    uv: [
                        stage.scale[0],
                        stage.scale[1],
                        offset(stage.scroll[0]),
                        offset(stage.scroll[1]),
                    ],
                });
            }
        };
        let level = ((80.0 - view.zoom_fov) / 80.0).clamp(0.0, 1.0) * 162.0;
        picture(&self.pictures[WINDOW], [34.0, 48.0, 570.0, 362.0], [1.0; 4]);
        picture(
            &self.white,
            [212.0, 367.0, 200.0, 40.0],
            [0.0, 0.0, 0.0, 1.0],
        );
        let green = [0.2, 0.4, 0.2, 0.3];
        for (x, value, tick) in compass(view.yaw) {
            for (place, digit) in digits(value).into_iter().enumerate() {
                let left = (x + DIGIT_ADVANCE * place as i32) as f32;
                picture(
                    &self.digits[usize::from(digit)],
                    [left, 374.0, DIGIT_SIZE[0], DIGIT_SIZE[1]],
                    green,
                );
            }
            picture(&self.white, [tick, 376.0, 6.0, 6.0], green);
        }
        picture(&self.pictures[NUMBERS], [212.0, 367.0, 200.0, 28.0], green);
        let pulse = ((view.time as f32 * 0.01).sin() * 0.5 + 0.5).powi(2);
        picture(
            &self.pictures[CIRCLE],
            [82.0, 94.0, 16.0, 16.0],
            [pulse, pulse, pulse, 1.0],
        );
        let mut spread = || unit(dice) * 0.2 - 0.1;
        let flicker = [0.7 + spread(), 0.8 + spread(), 0.7 + spread(), 1.0];
        picture(&self.pictures[MASK], [0.0, 0.0, 640.0, 480.0], flicker);
        picture(
            &self.pictures[ARROW],
            [4.0, 282.0 - level, 16.0, 16.0],
            flicker,
        );
        let triangle = if *flip {
            [330.0, 60.0, -26.0, -30.0]
        } else {
            [307.0, 40.0, 26.0, 30.0]
        };
        picture(&self.pictures[TRIANGLE], triangle, flicker);
        if unit(dice) > 0.98 && view.time & 1024 != 0 {
            *flip = !*flip;
        }
    }
}

/// The compass: for each ten degrees left of the view's, its number's place, the number
/// (200 up) and its tick's left edge; the leftmost only once the view has turned more
/// than three degrees past its ten.
fn compass(yaw: f32) -> impl Iterator<Item = (i32, i32, f32)> {
    let turned = yaw + 180.0;
    let mut value = (turned / 10.0) as i32 * 10;
    let off = turned - value as f32;
    [-10, 0, 10, 20].into_iter().filter_map(move |step: i32| {
        value -= 10;
        if value < 0 {
            value += 360;
        }
        ((off > 3.0 && step == -10) || step > -10).then(|| {
            (
                (155.0 + step as f32 * 10.0 + off * 10.0) as i32,
                value + 200,
                245.0 + (step - 1) as f32 * 10.0 + off * 10.0,
            )
        })
    })
}

/// A number's three digits, as `CG_DrawNumField` with width 3 draws one from 0 to 999.
fn digits(value: i32) -> [u8; 3] {
    let value = value.clamp(0, 999);
    [value / 100, value / 10 % 10, value % 10].map(|digit| digit as u8)
}

/// The next random number from 0 to 1 (`Q_flrand`'s part here).
fn unit(state: &mut u32) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state >> 8) as f32 / (1 << 24) as f32
}

/// A stage's pipeline.
fn blend(stage: &StageBlend) -> Blend {
    match stage {
        StageBlend::Add => Blend::Add,
        StageBlend::Filter => Blend::Filter,
        StageBlend::Replace => Blend::Replace,
        StageBlend::Custom {
            source,
            destination,
        } if source == "gl_dst_color" && destination == "gl_src_color" => Blend::Filter2x,
        _ => Blend::Alpha,
    }
}

fn load_picture(
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    name: &str,
    upload: &mut impl FnMut(&str, &RgbaImage) -> usize,
) -> Option<Picture> {
    let image = |image: &str| crate::shader_image::load_shader_image(vfs, shaders, image).ok();
    let Some(definition) = shaders.get(name) else {
        return Some(Picture::plain(upload(name, &image(name)?)));
    };
    let mut picture = Picture::default();
    for stage in definition
        .stages
        .iter()
        .filter(|stage| !stage.images.is_empty())
        .take(MAX_STAGES)
    {
        let path = &stage.images[0];
        let (scale, scroll) = crate::effect_texcoords::compile(&stage.texture_modifications);
        let rgb = stage.rgb_generator.as_deref();
        picture.stages[picture.count] = Stage {
            texture: upload(path, &image(path)?),
            blend: blend(&stage.blend),
            vertex_rgb: matches!(rgb, Some("vertex" | "exactvertex")),
            vertex_alpha: match stage.alpha_generator.as_deref() {
                Some(alpha) => alpha == "vertex",
                None => rgb == Some("vertex"),
            },
            scale,
            scroll,
        };
        picture.count += 1;
    }
    (picture.count > 0).then_some(picture)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_compass_counts_down_by_ten_from_the_view() {
        // Facing yaw 0, 180 past -180: 160, 150 and 140 (drawn 360, 350, 340); 170 is
        // left out while the view sits on its ten.
        let numbers: Vec<_> = compass(0.0).collect();
        assert_eq!(
            numbers,
            [(155, 360, 235.0), (255, 350, 335.0), (355, 340, 435.0)]
        );
        // Five degrees on, all four, each fifty units right.
        let numbers: Vec<_> = compass(5.0).collect();
        assert_eq!(
            numbers,
            [
                (105, 370, 185.0),
                (205, 360, 285.0),
                (305, 350, 385.0),
                (405, 340, 485.0)
            ]
        );
        // Wrapping below zero.
        assert_eq!(compass(-175.0).next(), Some((105, 550, 185.0)));
        assert_eq!(digits(370), [3, 7, 0]);
    }

    #[test]
    fn a_dst_src_blend_doubles_and_the_rest_map_through() {
        let custom = StageBlend::Custom {
            source: "gl_dst_color".to_owned(),
            destination: "gl_src_color".to_owned(),
        };
        assert_eq!(blend(&custom), Blend::Filter2x);
        assert_eq!(blend(&StageBlend::Add), Blend::Add);
        assert_eq!(blend(&StageBlend::Alpha), Blend::Alpha);
    }

    #[test]
    fn the_dice_stay_within_one() {
        let mut state = 0x9e37_79b9;
        for _ in 0..1_000 {
            let value = unit(&mut state);
            assert!((0.0..1.0).contains(&value));
        }
    }
}
