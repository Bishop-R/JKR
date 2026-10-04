//! GPU side of the classic menu artwork ([`crate::menu::art`]): one texture
//! and bind group per retail image, outside the shared icon atlas.
//!
//! The pieces are full-screen backgrounds and frames (up to 1024 square in
//! retail, more in HD packs); the atlas is sized for 128-pixel icons, a
//! banner and one map preview, and its cells are spoken for. A texture per
//! piece keeps the atlas untouched and lets HD replacements keep their
//! resolution. The shape renderer switches bind groups between draw runs
//! only where a textured quad names a different source, so the draw order
//! of every layer is preserved.
//!
//! Animated pieces ([`ArtPiece::dynamic`]) keep their texture writable: a
//! flickering glow is recomposed on the CPU and the main page's video
//! decodes its next frames, at most once per frame and only on frames that
//! draw the piece ([`ArtTextures::animate`]). Pieces whose texture scrolls
//! are sampled with wrapping ([`ArtPiece::wraps`]).

use crate::menu::art::{ArtPiece, ArtSet, Decoded, motion};
use crate::menu::roq::RoqDecoder;
use image::RgbaImage;

/// Longest gap between two drawn frames the video plays through; after a
/// longer pause (the page was hidden) it simply carries on.
const VIDEO_MAX_STEP: f64 = 0.25;
/// Most video frames decoded in one drawn frame; a longer backlog is skipped.
const VIDEO_CATCH_UP: u32 = 3;

/// How an animated piece rewrites its texture.
enum Motion {
    /// A glow recomposed with the static noise ([`motion::compose_flicker`]).
    Flicker {
        base: &'static RgbaImage,
        noise: &'static RgbaImage,
        speeds: [f32; 4],
        pixels: Vec<u8>,
    },
    /// The main page's logo video.
    Video {
        decoder: RoqDecoder<'static>,
        /// Seconds of video played.
        clock: f64,
        /// Frames shown since the start (or the last loop).
        shown: u64,
        /// Menu clock of the last frame that drew the video.
        last: Option<f64>,
    },
}

impl Motion {
    /// The picture to upload.
    fn pixels(&self) -> &[u8] {
        match self {
            Self::Flicker { pixels, .. } => pixels,
            Self::Video { decoder, .. } => decoder.frame(),
        }
    }

    /// Step to menu time `now`; true when the picture changed.
    fn step(&mut self, now: f64) -> bool {
        match self {
            Self::Flicker {
                base,
                noise,
                speeds,
                pixels,
            } => {
                motion::compose_flicker(base, noise, *speeds, now, pixels);
                true
            }
            Self::Video {
                decoder,
                clock,
                shown,
                last,
            } => {
                let step = last.map_or(0.0, |last| (now - last).clamp(0.0, VIDEO_MAX_STEP));
                *last = Some(now);
                *clock += step;
                let due = (*clock * f64::from(decoder.fps())) as u64;
                let mut decoded = false;
                let mut budget = VIDEO_CATCH_UP;
                while *shown < due && budget > 0 {
                    decoded |= decoder.next_frame();
                    *shown += 1;
                    budget -= 1;
                }
                *shown = (*shown).max(due);
                decoded
            }
        }
    }
}

/// A writable piece texture and how it animates.
struct Animated {
    piece: ArtPiece,
    texture: wgpu::Texture,
    size: [u32; 2],
    motion: Motion,
}

/// Per-piece bind groups of the classic menu artwork.
pub(super) struct ArtTextures {
    groups: [Option<wgpu::BindGroup>; ArtPiece::COUNT],
    ready: ArtSet,
    installed: bool,
    animated: Vec<Animated>,
    /// Pieces already animated this frame.
    stepped: ArtSet,
}

impl ArtTextures {
    pub(super) fn new() -> Self {
        Self {
            groups: std::array::from_fn(|_| None),
            ready: ArtSet::default(),
            installed: false,
            animated: Vec::new(),
            stepped: ArtSet::default(),
        }
    }

    /// Whether [`Self::install`] has run (whatever it found).
    pub(super) fn installed(&self) -> bool {
        self.installed
    }

    /// Pieces that can be drawn.
    pub(super) fn ready(&self) -> ArtSet {
        self.ready
    }

    /// The bind group sampling `piece`, if it was installed.
    pub(super) fn group(&self, piece: ArtPiece) -> Option<&wgpu::BindGroup> {
        self.groups[piece.index()].as_ref()
    }

    /// Start a frame: every animated piece may step once again.
    pub(super) fn begin_frame(&mut self) {
        self.stepped = ArtSet::default();
    }

    /// Step `piece`'s animation to menu time `now` and upload its new
    /// picture, once per frame; static pieces are left alone.
    pub(super) fn animate(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        piece: ArtPiece,
        now: f64,
    ) {
        if !piece.dynamic() || self.stepped.has(piece) {
            return;
        }
        self.stepped = self.stepped.with(piece);
        let Some(animated) = self.animated.iter_mut().find(|item| item.piece == piece) else {
            return;
        };
        if animated.motion.step(now) {
            write(
                queue,
                &animated.texture,
                animated.size,
                animated.motion.pixels(),
            );
        }
    }

    /// Upload every decoded piece into its own texture.
    pub(super) fn install(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        layout: &wgpu::BindGroupLayout,
        decoded: &'static Decoded,
    ) {
        self.installed = true;
        let sampler = |address_mode, label| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some(label),
                address_mode_u: address_mode,
                address_mode_v: address_mode,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            })
        };
        let clamped = sampler(
            wgpu::AddressMode::ClampToEdge,
            "JKR classic menu art sampler",
        );
        let wrapping = sampler(
            wgpu::AddressMode::Repeat,
            "JKR classic menu art wrapping sampler",
        );
        let now = motion::seconds();
        for piece in ArtPiece::ALL {
            let animated = animated_piece(piece, decoded, now);
            let (pixels, size): (&[u8], [u32; 2]) = match &animated {
                Some((motion, size)) => (motion.pixels(), *size),
                None => {
                    let Some(image) = decoded.image(piece) else {
                        continue;
                    };
                    (image.as_raw(), [image.width(), image.height()])
                }
            };
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("JKR classic menu art"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            write(queue, &texture, size, pixels);
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let sampler = if piece.wraps() { &wrapping } else { &clamped };
            self.groups[piece.index()] =
                Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("JKR classic menu art bind group"),
                    layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(sampler),
                        },
                    ],
                }));
            if let Some((motion, size)) = animated {
                self.animated.push(Animated {
                    piece,
                    texture,
                    size,
                    motion,
                });
            }
            self.ready = self.ready.with(piece);
        }
    }
}

/// The motion of `piece`, with its first picture made, if it animates and
/// game data has what it needs.
fn animated_piece(
    piece: ArtPiece,
    decoded: &'static Decoded,
    now: f64,
) -> Option<(Motion, [u32; 2])> {
    if piece == ArtPiece::Video {
        let mut decoder = RoqDecoder::new(decoded.video()?)?;
        if !decoder.next_frame() {
            return None;
        }
        let size = [decoder.width(), decoder.height()];
        return Some((
            Motion::Video {
                decoder,
                clock: 0.0,
                shown: 0,
                last: None,
            },
            size,
        ));
    }
    let speeds = motion::flicker(piece)?;
    let (base, noise) = decoded.flicker_base(piece)?;
    let mut pixels = vec![0_u8; (base.width() * base.height() * 4) as usize];
    motion::compose_flicker(base, noise, speeds, now, &mut pixels);
    Some((
        Motion::Flicker {
            base,
            noise,
            speeds,
            pixels,
        },
        [base.width(), base.height()],
    ))
}

/// Replace the whole of `texture` (`size`, RGBA) with `pixels`.
fn write(
    queue: &crate::frame_queue::FrameQueue,
    texture: &wgpu::Texture,
    size: [u32; 2],
    pixels: &[u8],
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size[0] * 4),
            rows_per_image: Some(size[1]),
        },
        wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
    );
}

/// Texture source of one run of shape vertices.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Source {
    /// The shared icon atlas (also bound for untextured shapes).
    Atlas,
    /// One classic menu art piece.
    Art(ArtPiece),
}

/// A run of consecutive vertices drawn with one bind group.
#[derive(Clone, Copy, Debug)]
pub(super) struct Run {
    pub(super) start: u32,
    pub(super) source: Source,
}

/// Most bind-group switches one frame may make; art quads past it are
/// dropped rather than growing the run list on the frame path.
pub(super) const MAX_RUNS: usize = 48;

/// Begin a new run for `source` at vertex `start` unless the current run
/// already samples it. Untextured shapes never call this: they draw the same
/// under any bind group. Returns false when the run list is full.
pub(super) fn switch(runs: &mut Vec<Run>, start: usize, source: Source) -> bool {
    if let Some(run) = runs.last_mut() {
        if run.source == source {
            return true;
        }
        // An empty run can simply change source.
        if run.start as usize == start {
            run.source = source;
            return true;
        }
    }
    if runs.len() >= MAX_RUNS {
        return false;
    }
    runs.push(Run {
        start: start as u32,
        source,
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_switch_only_on_a_new_source() {
        let mut runs = Vec::with_capacity(MAX_RUNS);
        runs.push(Run {
            start: 0,
            source: Source::Atlas,
        });
        // A source change before any vertex reuses the empty run.
        assert!(switch(&mut runs, 0, Source::Art(ArtPiece::Background)));
        assert_eq!(runs.len(), 1);
        assert!(switch(&mut runs, 6, Source::Art(ArtPiece::Background)));
        assert_eq!(runs.len(), 1);
        assert!(switch(&mut runs, 12, Source::Atlas));
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[1].start, 12);
        while runs.len() < MAX_RUNS {
            // Alternate against the current run so every call starts one.
            let source = if runs.last().expect("run").source == Source::Atlas {
                Source::Art(ArtPiece::Ring)
            } else {
                Source::Atlas
            };
            let start = 100 + runs.len() * 6;
            assert!(switch(&mut runs, start, source));
        }
        let last = runs.last().expect("run").source;
        let other = if last == Source::Atlas {
            Source::Art(ArtPiece::Logo)
        } else {
            Source::Atlas
        };
        assert!(!switch(&mut runs, 10_000, other));
        assert!(switch(&mut runs, 10_000, last));
    }
}
