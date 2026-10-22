//! Fixed-capacity stock scope quads; textures are resolved once at world construction.
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[path = "scope_binoculars.rs"]
mod binoculars;
pub(crate) use binoculars::View as BinocularView;
#[cfg(test)]
#[path = "scope_gpu_evidence.rs"]
mod evidence;

const PATHS: [&str; 5] = [
    "gfx/misc/scanline",
    "gfx/2d/cropcircle2.tga",
    "gfx/2d/cropCircle",
    "gfx/2d/insertTick",
    "gfx/2d/crop_charge",
];
/// Quads one frame can draw; the binoculars' view is the largest at 26.
const QUADS: usize = 32;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];

/// A stage's `blendFunc`, one pipeline each.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Blend {
    /// `GL_SRC_ALPHA GL_ONE_MINUS_SRC_ALPHA`.
    #[default]
    Alpha = 0,
    /// `GL_ONE GL_ONE`.
    Add = 1,
    /// `GL_DST_COLOR GL_SRC_COLOR`: twice the colour below times the picture's.
    Filter2x = 2,
    /// `GL_DST_COLOR GL_ZERO`.
    Filter = 3,
    /// `GL_ONE GL_ZERO`.
    Replace = 4,
}

/// One quad: a picture's place on the 640x480 screen as `[centre x, centre y, width,
/// height]` (a negative size mirrors it), its turn, colour, and texture coordinates as
/// `[scale u, scale v, offset u, offset v]`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Quad {
    texture: usize,
    blend: Blend,
    rect: [f32; 4],
    degrees: f32,
    color: [f32; 4],
    uv: [f32; 4],
}

/// Map-owned stock textures and a bounded batch of scope quads.
pub(crate) struct Mask {
    pipeline: [wgpu::RenderPipeline; 5],
    textures: Vec<wgpu::BindGroup>,
    buffer: wgpu::Buffer,
    vertices: [Vertex; QUADS * 6],
    quads: [(usize, Blend); QUADS],
    count: usize,
    /// The binoculars' pictures, when the game data has them.
    binoculars: Option<binoculars::Media>,
    /// Their top triangle's flip and the dice that throw it (`CG_DrawZoomMask`'s
    /// `static qboolean flip`).
    flip: bool,
    dice: u32,
}

impl Mask {
    /// Load retail shader images without requiring scope assets in custom-only installations.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        format: wgpu::TextureFormat,
        vfs: &jkr_vfs::VirtualFileSystem,
        shaders: &jkr_shader::ShaderCatalog,
    ) -> Option<Self> {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scope textures"),
            entries: &[
                crate::render_helpers::texture_layout_entry(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind = |name: &str, image: &image::RgbaImage| {
            let texture = device.create_texture_with_data(
                queue.raw(),
                &wgpu::TextureDescriptor {
                    label: Some(name),
                    size: wgpu::Extent3d {
                        width: image.width(),
                        height: image.height(),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                image.as_raw(),
            );
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(name),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &texture.create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            })
        };
        let mut textures = Vec::with_capacity(PATHS.len() + binoculars::TEXTURES);
        for name in PATHS {
            let loaded = if name.ends_with(".tga") {
                vfs.read(name)
                    .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
                    .and_then(|asset| asset.ok_or_else(|| format!("missing {name}").into()))
                    .and_then(|asset| {
                        crate::decode_image(&asset.bytes, name)
                            .map(|image| image.into_rgba8())
                            .map_err(Into::into)
                    })
            } else {
                crate::shader_image::load_shader_image(vfs, shaders, name)
            };
            let image = match loaded {
                Ok(image) => image,
                Err(error) => {
                    eprintln!("scope unavailable: {error}");
                    return None;
                }
            };
            textures.push(bind(name, &image));
        }
        let binoculars = binoculars::Media::load(vfs, shaders, |name, image| {
            textures.push(bind(name, image));
            textures.len() - 1
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scope"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scope.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scope"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let color = |src_factor, dst_factor| wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor,
                dst_factor,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        };
        use wgpu::BlendFactor::{Dst, One, Src, Zero};
        let pipeline = [
            wgpu::BlendState::ALPHA_BLENDING,
            color(One, One),
            color(Dst, Src),
            color(Dst, Zero),
            wgpu::BlendState::REPLACE,
        ]
        .map(|blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("scope"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &ATTRIBUTES,
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: crate::DepthTarget::FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scope quads"),
            size: (QUADS * 6 * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Some(Self {
            pipeline,
            textures,
            buffer,
            vertices: [Vertex::zeroed(); QUADS * 6],
            quads: [(0, Blend::Alpha); QUADS],
            count: 0,
            binoculars,
            flip: true,
            dice: 0x9e37_79b9,
        })
    }

    /// Rebuild the original 640x480 mask, rotating FOV insert, ammo ticks and charge readout.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        visible: bool,
        style: i64,
        fov: f32,
        time: i32,
        ammo: f32,
        charge: Option<f32>,
    ) {
        self.count = 0;
        if !visible {
            return;
        }
        let screen = [320.0, 240.0, 640.0, 480.0];
        let plain = [1.0, 1.0, 0.0, 0.0];
        let [mask, insert] = crate::cgame_options::scope_layers(style);
        if mask {
            self.quad(Quad {
                texture: 0,
                blend: Blend::Filter2x,
                rect: screen,
                degrees: 0.0,
                color: [1.0; 4],
                uv: [1.0, 10.5, 0.0, 0.0],
            });
            self.quad(Quad {
                texture: 1,
                blend: Blend::Alpha,
                rect: screen,
                degrees: 0.0,
                color: [1.0; 4],
                uv: plain,
            });
        }
        let level = ((50.0 - fov) / 50.0).clamp(0.0, 1.0) * 103.0;
        let alpha = if level >= 99.0 {
            0.7 + (time as f32 * 0.01).sin() * 0.3
        } else {
            1.0
        };
        if insert {
            self.quad(Quad {
                texture: 2,
                blend: Blend::Alpha,
                rect: screen,
                degrees: -level,
                color: [1.0, 1.0, 1.0, alpha],
                uv: plain,
            });
        }
        let ammo = ammo.clamp(0.0, 1.0);
        let color = if ammo < 0.15 && time & 512 != 0 {
            [0.0, 0.0, 0.0, 1.0]
        } else {
            [
                ((1.0 - ammo) * 2.0).min(1.0),
                (ammo * 1.5).min(1.0),
                0.0,
                1.0,
            ]
        };
        let mut angle = 18.5;
        while angle <= 18.5 + ammo * 58.0 {
            let radians: f32 = (angle + 90.0) / 57.296;
            self.quad(Quad {
                texture: 3,
                blend: Blend::Add,
                rect: [
                    320.0 + radians.sin() * 190.0,
                    240.0 + radians.cos() * 190.0,
                    12.0,
                    24.0,
                ],
                degrees: 90.0 - angle,
                color,
                uv: plain,
            });
            angle += 3.0;
        }
        if let Some(charge) = charge {
            let charge = charge.clamp(0.0, 1.0);
            self.quad(Quad {
                texture: 4,
                blend: Blend::Alpha,
                rect: [257.0 + 67.0 * charge, 452.0, 134.0 * charge, 34.0],
                degrees: 0.0,
                color: [1.0; 4],
                uv: [charge, 1.0, 0.0, 0.0],
            });
        }
        self.upload(queue);
    }

    /// The electrobinoculars' view in place of the scope; nothing when the game data
    /// lacks its pictures.
    pub(crate) fn prepare_binoculars(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        view: BinocularView,
    ) {
        self.count = 0;
        let Some(media) = self.binoculars.take() else {
            return;
        };
        let (mut flip, mut dice) = (self.flip, self.dice);
        media.lay_out(view, (&mut flip, &mut dice), |quad| self.quad(quad));
        (self.flip, self.dice) = (flip, dice);
        self.binoculars = Some(media);
        self.upload(queue);
    }

    fn upload(&mut self, queue: &crate::frame_queue::FrameQueue) {
        queue.write_buffer(
            &self.buffer,
            0,
            bytemuck::cast_slice(&self.vertices[..self.count * 6]),
        );
    }

    fn quad(&mut self, quad: Quad) {
        if self.count == QUADS {
            return;
        }
        let rect = quad.rect;
        let (sin, cos) = quad.degrees.to_radians().sin_cos();
        let [scale_u, scale_v, offset_u, offset_v] = quad.uv;
        for (index, [u, v]) in [
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
        ]
        .into_iter()
        .enumerate()
        {
            let x = (u - 0.5) * rect[2];
            let y = (v - 0.5) * rect[3];
            self.vertices[self.count * 6 + index] = Vertex {
                position: [
                    (rect[0] + x * cos - y * sin) / 320.0 - 1.0,
                    1.0 - (rect[1] + x * sin + y * cos) / 240.0,
                ],
                uv: [u * scale_u + offset_u, v * scale_v + offset_v],
                color: quad.color,
            };
        }
        self.quads[self.count] = (quad.texture, quad.blend);
        self.count += 1;
    }

    /// Draw after scene geometry and before text with no per-frame allocation.
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.count == 0 {
            return;
        }
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        for (index, (texture, blend)) in self.quads[..self.count].iter().enumerate() {
            pass.set_pipeline(&self.pipeline[*blend as usize]);
            pass.set_bind_group(0, &self.textures[*texture], &[]);
            pass.draw(index as u32 * 6..index as u32 * 6 + 6, 0..1);
        }
    }
}
