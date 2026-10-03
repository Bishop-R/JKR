//! Fixed-size texture atlas used by generic retained UI textured quads:
//! a grid of [`ICON_SIZE`] cells for icons, plus one wide banner strip along
//! the bottom for the menu wordmark and, beside it, one map-preview slot
//! (the Create game screen's levelshot).

use super::ShapeVertex;
use jkr_ui::{Color, Rect, TextureId};

const ATLAS_SIZE: u32 = 2_048;
const ATLAS_HEIGHT: u32 = ATLAS_SIZE + 768;
/// Edge of one atlas cell; icons are uploaded at exactly this size.
pub(crate) const ICON_SIZE: u32 = 128;
const COLUMNS: u32 = ATLAS_SIZE / ICON_SIZE;
/// Pixel size of the banner strip (rows of cells it takes: 3).
pub(crate) const BANNER_SIZE: [u32; 2] = [1_536, 384];
const BANNER_Y: u32 = ATLAS_HEIGHT - BANNER_SIZE[1];
/// Original menu-cell reservation; HUD cells follow without reducing menu capacity.
pub(crate) const ICON_CELLS: u32 = COLUMNS * ((ATLAS_SIZE - BANNER_SIZE[1]) / ICON_SIZE);
const TOTAL_CELLS: u32 = ICON_CELLS + 96;
/// `TexturedQuad` texture naming the banner strip.
pub(crate) const BANNER_TEXTURE: TextureId = TextureId(u32::MAX);
/// Pixel size of the map-preview slot right of the banner (4:3, as the
/// stock UI shows levelshots).
pub(crate) const LEVELSHOT_SIZE: [u32; 2] = [ATLAS_SIZE - BANNER_SIZE[0], BANNER_SIZE[1]];
/// `TexturedQuad` texture naming the map-preview slot.
pub(crate) const LEVELSHOT_TEXTURE: TextureId = TextureId(u32::MAX - 1);

pub(super) struct IconAtlas {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

impl IconAtlas {
    pub(super) fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("JKR retained UI icon atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("JKR retained UI icon sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("JKR retained UI icon atlas bind group"),
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
        });
        Self {
            texture,
            bind_group,
        }
    }

    pub(super) fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub(super) fn upload(
        &self,
        queue: &crate::frame_queue::FrameQueue,
        texture: TextureId,
        rgba: &[u8],
    ) {
        if rgba.len() != (ICON_SIZE * ICON_SIZE * 4) as usize {
            return;
        }
        let index = texture.0.min(TOTAL_CELLS - 1);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: (index % COLUMNS) * ICON_SIZE,
                    y: (index / COLUMNS) * ICON_SIZE,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ICON_SIZE * 4),
                rows_per_image: Some(ICON_SIZE),
            },
            wgpu::Extent3d {
                width: ICON_SIZE,
                height: ICON_SIZE,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Upload the [`BANNER_SIZE`] RGBA banner drawn by [`BANNER_TEXTURE`].
    pub(super) fn upload_banner(&self, queue: &crate::frame_queue::FrameQueue, rgba: &[u8]) {
        self.upload_region(queue, [0, BANNER_Y], BANNER_SIZE, rgba);
    }

    /// Upload the [`LEVELSHOT_SIZE`] RGBA map preview drawn by
    /// [`LEVELSHOT_TEXTURE`].
    pub(super) fn upload_levelshot(&self, queue: &crate::frame_queue::FrameQueue, rgba: &[u8]) {
        self.upload_region(queue, [BANNER_SIZE[0], BANNER_Y], LEVELSHOT_SIZE, rgba);
    }

    fn upload_region(
        &self,
        queue: &crate::frame_queue::FrameQueue,
        origin: [u32; 2],
        size: [u32; 2],
        rgba: &[u8],
    ) {
        let [width, height] = size;
        if rgba.len() != (width * height * 4) as usize {
            return;
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: origin[0],
                    y: origin[1],
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
}

/// Atlas UV corners of `texture`.
pub(super) fn uv_range(texture: TextureId) -> ([f32; 2], [f32; 2]) {
    let atlas = ATLAS_SIZE as f32;
    if texture == BANNER_TEXTURE {
        let [width, height] = BANNER_SIZE;
        let uv0 = [0.0, BANNER_Y as f32 / ATLAS_HEIGHT as f32];
        return (
            uv0,
            [
                width as f32 / atlas,
                uv0[1] + height as f32 / ATLAS_HEIGHT as f32,
            ],
        );
    }
    if texture == LEVELSHOT_TEXTURE {
        // Half a texel in, so filtering never picks up the banner beside it.
        let [width, height] = LEVELSHOT_SIZE;
        let uv = |x: f32, y: f32| [x / atlas, y / ATLAS_HEIGHT as f32];
        let (x, y) = (BANNER_SIZE[0] as f32 + 0.5, BANNER_Y as f32 + 0.5);
        return (
            uv(x, y),
            uv(x + width as f32 - 1.0, y + height as f32 - 1.0),
        );
    }
    let index = texture.0.min(TOTAL_CELLS - 1);
    let x = (index % COLUMNS) * ICON_SIZE;
    let y = (index / COLUMNS) * ICON_SIZE;
    let uv = |x: u32, y: u32| [x as f32 / atlas, y as f32 / ATLAS_HEIGHT as f32];
    (uv(x, y), uv(x + ICON_SIZE, y + ICON_SIZE))
}

/// Push one textured quad sampling `uv` (top-left and bottom-right corners)
/// of whatever texture its draw run binds.
pub(super) fn push_quad(
    vertices: &mut Vec<ShapeVertex>,
    rect: Rect,
    (uv0, uv1): ([f32; 2], [f32; 2]),
    color: Color,
    opacity: f32,
    viewport: [f32; 2],
    capacity: usize,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 || vertices.len() + 6 > capacity {
        return;
    }
    let position = |x: f32, y: f32| [x / viewport[0] * 2.0 - 1.0, 1.0 - y / viewport[1] * 2.0];
    let tint = [color.r, color.g, color.b, color.a * opacity];
    let points = [
        ([rect.x, rect.y], [uv0[0], uv0[1]]),
        ([rect.right(), rect.y], [uv1[0], uv0[1]]),
        ([rect.right(), rect.bottom()], [uv1[0], uv1[1]]),
        ([rect.x, rect.y], [uv0[0], uv0[1]]),
        ([rect.right(), rect.bottom()], [uv1[0], uv1[1]]),
        ([rect.x, rect.bottom()], [uv0[0], uv1[1]]),
    ];
    vertices.extend(points.map(|(pixel, uv)| ShapeVertex {
        position: position(pixel[0], pixel[1]),
        local: [0.0, 0.0],
        size: [rect.width, rect.height],
        start_color: tint,
        end_color: tint,
        parameters: [0.0, 2.0],
        uv,
    }));
}
