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

use crate::menu::art::{ArtPiece, ArtSet, Decoded};

/// Per-piece bind groups of the classic menu artwork.
pub(super) struct ArtTextures {
    groups: [Option<wgpu::BindGroup>; ArtPiece::COUNT],
    ready: ArtSet,
    installed: bool,
}

impl ArtTextures {
    pub(super) fn new() -> Self {
        Self {
            groups: std::array::from_fn(|_| None),
            ready: ArtSet::default(),
            installed: false,
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

    /// Upload every decoded piece into its own texture.
    pub(super) fn install(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        layout: &wgpu::BindGroupLayout,
        decoded: &Decoded,
    ) {
        self.installed = true;
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("JKR classic menu art sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        for piece in ArtPiece::ALL {
            let Some(image) = decoded.image(piece) else {
                continue;
            };
            let (width, height) = image.dimensions();
            let view = crate::gpu_texture::create_rgba8_texture(
                device,
                queue,
                "JKR classic menu art",
                width,
                height,
                image.as_raw(),
                true,
            );
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
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                }));
            self.ready = self.ready.with(piece);
        }
    }
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
