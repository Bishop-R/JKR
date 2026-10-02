//! Viewer-side GPU/text adapter for renderer-neutral UI draw commands.

use crate::text::{self, TextVertex, UiFont};
use bytemuck::{Pod, Zeroable};
use jkr_ui::{Color, DrawCommand, DrawList, FontWeight, Rect, TextAlign, TextId};

mod icons;
use icons::IconAtlas;
pub(crate) use icons::{
    BANNER_SIZE, BANNER_TEXTURE, ICON_CELLS, ICON_SIZE, LEVELSHOT_SIZE, LEVELSHOT_TEXTURE,
};

/// Main-menu wordmark: the Jedi Knight saber emblem laid horizontal, white
/// on transparent, tinted by the player's accent at draw time.
const MENU_WORDMARK: &[u8] = include_bytes!("../assets/menu/jk-wordmark.png");

const MAX_SHAPE_VERTICES: usize = 4_096;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct ShapeVertex {
    position: [f32; 2],
    local: [f32; 2],
    size: [f32; 2],
    start_color: [f32; 4],
    end_color: [f32; 4],
    parameters: [f32; 2],
    uv: [f32; 2],
}

impl ShapeVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x2,
        3 => Float32x4,
        4 => Float32x4,
        5 => Float32x2,
        6 => Float32x2
    ];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// Fixed-capacity WGPU shape renderer. GPU ownership never leaks into `jkr-ui`.
pub(crate) struct ShapeRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    vertices: Vec<ShapeVertex>,
    icons: IconAtlas,
}

impl ShapeRenderer {
    /// Create the one process-lifetime pipeline and fixed vertex storage.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("JKR retained UI shape shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ui_shapes.wgsl").into()),
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("JKR retained UI texture layout"),
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
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("JKR retained UI shape pipeline layout"),
            bind_group_layouts: &[Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("JKR retained UI shape pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(ShapeVertex::layout())],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("JKR retained UI shape vertices"),
            size: (MAX_SHAPE_VERTICES * std::mem::size_of::<ShapeVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let icons = IconAtlas::new(device, &texture_layout);
        match image::load_from_memory(MENU_WORDMARK) {
            Ok(wordmark) => icons.upload_banner(queue, &wordmark.into_rgba8()),
            Err(error) => eprintln!("menu wordmark: {error}"),
        }
        Self {
            pipeline,
            vertex_buffer,
            vertices: Vec::with_capacity(MAX_SHAPE_VERTICES),
            icons,
        }
    }

    /// Translate every active retained layer and upload one shared vertex batch.
    pub(crate) fn prepare_layers<'a>(
        &mut self,
        queue: &wgpu::Queue,
        draw_lists: impl IntoIterator<Item = &'a DrawList>,
        viewport: [f32; 2],
    ) {
        self.vertices.clear();
        let mut opacity = [1.0_f32; 8];
        let mut opacity_depth = 0_usize;
        let mut clips = [Rect::new(0.0, 0.0, viewport[0], viewport[1]); 8];
        let mut clip_depth = 0_usize;
        for command in draw_lists.into_iter().flat_map(DrawList::commands) {
            match *command {
                DrawCommand::SolidRect { rect, color }
                | DrawCommand::RoundedRect {
                    rect,
                    radius: 0.0,
                    color,
                } => {
                    self.push_rect(
                        clipped(rect, clips[clip_depth]),
                        color,
                        color,
                        0.0,
                        false,
                        opacity[opacity_depth],
                        viewport,
                    );
                }
                DrawCommand::RoundedRect {
                    rect,
                    radius,
                    color,
                } => {
                    self.push_rect(
                        clipped(rect, clips[clip_depth]),
                        color,
                        color,
                        radius,
                        false,
                        opacity[opacity_depth],
                        viewport,
                    );
                }
                DrawCommand::GradientRect {
                    rect,
                    radius,
                    gradient,
                } => {
                    self.push_rect(
                        clipped(rect, clips[clip_depth]),
                        gradient.start,
                        gradient.end,
                        radius,
                        gradient.vertical,
                        opacity[opacity_depth],
                        viewport,
                    );
                }
                DrawCommand::Border {
                    rect,
                    width,
                    color,
                    radius,
                } if radius > 0.0 => {
                    self.push_ring(
                        clipped(rect, clips[clip_depth]),
                        color,
                        radius,
                        width,
                        opacity[opacity_depth],
                        viewport,
                    );
                }
                DrawCommand::Border {
                    rect, width, color, ..
                } => {
                    for edge in border_rects(rect, width) {
                        self.push_rect(
                            clipped(edge, clips[clip_depth]),
                            color,
                            color,
                            0.0,
                            false,
                            opacity[opacity_depth],
                            viewport,
                        );
                    }
                }
                DrawCommand::PushClip(rect) if clip_depth + 1 < clips.len() => {
                    clip_depth += 1;
                    clips[clip_depth] = clipped(clips[clip_depth - 1], rect);
                }
                DrawCommand::PopClip => clip_depth = clip_depth.saturating_sub(1),
                DrawCommand::PushOpacity(value) if opacity_depth + 1 < opacity.len() => {
                    opacity_depth += 1;
                    opacity[opacity_depth] = opacity[opacity_depth - 1] * value.clamp(0.0, 1.0);
                }
                DrawCommand::PopOpacity => opacity_depth = opacity_depth.saturating_sub(1),
                DrawCommand::TexturedQuad {
                    rect,
                    texture,
                    color,
                } => icons::push_textured(
                    &mut self.vertices,
                    clipped(rect, clips[clip_depth]),
                    texture,
                    color,
                    opacity[opacity_depth],
                    viewport,
                    MAX_SHAPE_VERTICES,
                ),
                DrawCommand::Text { .. }
                | DrawCommand::PushClip(_)
                | DrawCommand::PushOpacity(_) => {}
            }
        }
        if !self.vertices.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&self.vertices));
        }
    }

    /// Draw every retained shape in one pipeline/buffer submission.
    pub(crate) fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        if self.vertices.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, self.icons.bind_group(), &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertices.len() as u32, 0..1);
    }

    fn push_rect(
        &mut self,
        rect: Rect,
        start: Color,
        end: Color,
        radius: f32,
        vertical: bool,
        opacity: f32,
        viewport: [f32; 2],
    ) {
        self.push_shape(
            rect,
            start,
            end,
            radius,
            f32::from(vertical),
            opacity,
            viewport,
        );
    }

    /// Rounded outline `width` pixels thick, drawn as one quad the shader
    /// hollows out (negative gradient parameter = ring width).
    fn push_ring(
        &mut self,
        rect: Rect,
        color: Color,
        radius: f32,
        width: f32,
        opacity: f32,
        viewport: [f32; 2],
    ) {
        self.push_shape(
            rect,
            color,
            color,
            radius,
            -width.max(0.5),
            opacity,
            viewport,
        );
    }

    fn push_shape(
        &mut self,
        rect: Rect,
        start: Color,
        end: Color,
        radius: f32,
        mode: f32,
        opacity: f32,
        viewport: [f32; 2],
    ) {
        if rect.width <= 0.0 || rect.height <= 0.0 || self.vertices.len() + 6 > MAX_SHAPE_VERTICES {
            return;
        }
        let position = |x: f32, y: f32| [x / viewport[0] * 2.0 - 1.0, 1.0 - y / viewport[1] * 2.0];
        let color = |value: Color| [value.r, value.g, value.b, value.a * opacity];
        let points = [
            ([rect.x, rect.y], [0.0, 0.0]),
            ([rect.right(), rect.y], [1.0, 0.0]),
            ([rect.right(), rect.bottom()], [1.0, 1.0]),
            ([rect.x, rect.y], [0.0, 0.0]),
            ([rect.right(), rect.bottom()], [1.0, 1.0]),
            ([rect.x, rect.bottom()], [0.0, 1.0]),
        ];
        self.vertices
            .extend(points.map(|(pixel, local)| ShapeVertex {
                position: position(pixel[0], pixel[1]),
                local,
                size: [rect.width, rect.height],
                start_color: color(start),
                end_color: color(end),
                parameters: [radius.min(rect.width.min(rect.height) * 0.5), mode],
                uv: local,
            }));
    }

    /// Upload one decoded [`ICON_SIZE`]-square RGBA icon into a stable atlas
    /// cell, sampled by `TexturedQuad` commands naming `texture`.
    pub(crate) fn upload_icon(&self, queue: &wgpu::Queue, texture: jkr_ui::TextureId, rgba: &[u8]) {
        self.icons.upload(queue, texture, rgba);
    }

    /// Upload the one [`LEVELSHOT_SIZE`] RGBA map preview sampled by
    /// `TexturedQuad` commands naming [`LEVELSHOT_TEXTURE`].
    pub(crate) fn upload_levelshot(&self, queue: &wgpu::Queue, rgba: &[u8]) {
        self.icons.upload_levelshot(queue, rgba);
    }
}

fn clipped(left: Rect, right: Rect) -> Rect {
    let x = left.x.max(right.x);
    let y = left.y.max(right.y);
    Rect::new(
        x,
        y,
        left.right().min(right.right()) - x,
        left.bottom().min(right.bottom()) - y,
    )
}

fn border_rects(rect: Rect, width: f32) -> [Rect; 4] {
    [
        Rect::new(rect.x, rect.y, rect.width, width),
        Rect::new(rect.x, rect.bottom() - width, rect.width, width),
        Rect::new(rect.x, rect.y, width, rect.height),
        Rect::new(rect.right() - width, rect.y, width, rect.height),
    ]
}

/// Append all text commands without allocating; HUD rectangles are consumed by
/// the existing fullscreen quad pass. `style` scales and tracks every command
/// ([`text::TextStyle::NEUTRAL`] draws the layout as authored).
pub(crate) fn append_text_commands<'a>(
    draw_list: &DrawList,
    resolve: impl Fn(TextId) -> &'a str,
    vertices: &mut Vec<TextVertex>,
    font: &UiFont,
    viewport: [f32; 2],
    style: text::TextStyle,
) {
    let mut opacity = [1.0_f32; 8];
    let mut opacity_depth = 0_usize;
    let mut clips = [Rect::new(0.0, 0.0, viewport[0], viewport[1]); 8];
    let mut clip_depth = 0_usize;
    for command in draw_list.commands() {
        let DrawCommand::Text {
            rect,
            text: id,
            size,
            color,
            align,
            weight,
            letter_spacing,
            overflow,
        } = command
        else {
            match command {
                DrawCommand::PushOpacity(value) if opacity_depth + 1 < opacity.len() => {
                    opacity_depth += 1;
                    opacity[opacity_depth] = opacity[opacity_depth - 1] * value.clamp(0.0, 1.0);
                }
                DrawCommand::PopOpacity => opacity_depth = opacity_depth.saturating_sub(1),
                DrawCommand::PushClip(rect) if clip_depth + 1 < clips.len() => {
                    clip_depth += 1;
                    clips[clip_depth] = clipped(clips[clip_depth - 1], *rect);
                }
                DrawCommand::PopClip => clip_depth = clip_depth.saturating_sub(1),
                _ => {}
            }
            continue;
        };
        let value = resolve(*id);
        let placement = style.place(*rect, *size, *letter_spacing);
        let rect = placement.bounds;
        let scale = placement.size / font.height.max(1.0);
        let face = match weight {
            FontWeight::Regular => text::TextFace::Regular,
            FontWeight::Semibold => text::TextFace::Semibold,
        };
        let measured =
            text::visible_text_width_style(font, value, scale, face, placement.letter_spacing);
        let width = if *overflow == jkr_ui::TextOverflow::Ellipsis {
            measured.min(rect.width)
        } else {
            measured
        };
        let x = match align {
            TextAlign::Start => rect.x,
            TextAlign::Center => rect.x + (rect.width - width) * 0.5,
            TextAlign::End => rect.right() - width,
        };
        text::append_bounded(
            vertices,
            font,
            value,
            [x, placement.y],
            rect,
            clipped(rect, clips[clip_depth]),
            scale,
            viewport,
            face,
            [color.r, color.g, color.b, color.a * opacity[opacity_depth]],
            placement.letter_spacing,
            *overflow,
        );
    }
}
