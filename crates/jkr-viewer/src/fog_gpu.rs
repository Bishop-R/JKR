//! Map-lifetime volume uniforms, selectors and fog pipelines.
use super::*;
use crate::fog_volumes::{FOG_SLOTS, Mode, Table};
use jkr_shader::{FogPass, ShaderCull};

/// GPU resources allocated once, with selectors aligned for dynamic uniform offsets.
pub(super) struct FogGpu {
    pub(super) table: Table,
    group: wgpu::BindGroup,
    stride: u32,
    layout: wgpu::PipelineLayout,
    shader: wgpu::ShaderModule,
    keys: Vec<PipelineKey>,
    mover: Vec<wgpu::RenderPipeline>,
    entity: Vec<wgpu::RenderPipeline>,
}

impl FogGpu {
    /// Upload the immutable fog table and aligned selector block once.
    pub(super) fn new(device: &wgpu::Device, camera: &wgpu::BindGroupLayout, table: Table) -> Self {
        let entries = [uniform_entry(0, false), uniform_entry(1, true)];
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("JKR fog volume layout"),
            entries: &entries,
        });
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("JKR map fog table"),
            contents: bytemuck::cast_slice(&table.entries),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let stride = device.limits().min_uniform_buffer_offset_alignment.max(16);
        let mut selectors = vec![0_u8; stride as usize * FOG_SLOTS * 2];
        for index in 0..FOG_SLOTS * 2 {
            let offset = index * stride as usize;
            let fog = (index % FOG_SLOTS) as u32;
            let global_exp2 = (index / FOG_SLOTS) as u32;
            selectors[offset..offset + 4].copy_from_slice(&fog.to_le_bytes());
            selectors[offset + 4..offset + 8].copy_from_slice(&global_exp2.to_le_bytes());
        }
        let selector = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("JKR immutable fog selectors"),
            contents: &selectors,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("JKR fog volumes"),
            layout: &bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &selector,
                        offset: 0,
                        size: NonZeroU64::new(16),
                    }),
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("JKR fog pipeline layout"),
            bind_group_layouts: &[
                Some(camera),
                Some(&bind_layout),
                Some(&crate::shared_geometry::quads::layout(device)),
                Some(&crate::world_stage::geometry_uniform::layout(device)),
            ],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("JKR RB_FogPass"),
            source: wgpu::ShaderSource::Wgsl(crate::fog_volumes::SHADER.into()),
        });
        Self {
            table,
            group,
            stride,
            layout,
            shader,
            keys: Vec::new(),
            mover: Vec::new(),
            entity: Vec::new(),
        }
    }

    pub(super) fn pipeline_with_geometry(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        fog_pass: FogPass,
        cull: ShaderCull,
        geometry: u8,
    ) -> Option<usize> {
        if fog_pass == FogPass::None {
            return None;
        }
        // RB_FogPass, tr_shade.cpp:1120-1125: alpha blend, no depth write.
        let key = PipelineKey {
            geometry,
            source: wgpu::BlendFactor::SrcAlpha,
            destination: wgpu::BlendFactor::OneMinusSrcAlpha,
            depth_write: false,
            depth: if fog_pass == FogPass::Equal {
                wgpu::CompareFunction::Equal
            } else {
                wgpu::CompareFunction::LessEqual
            },
            cull: crate::world_stage::cull_face(cull),
        };
        if let Some(index) = self.keys.iter().position(|known| *known == key) {
            return Some(index);
        }
        let index = self.keys.len();
        self.keys.push(key);
        // Static world fog draws bind the identity instance and share the mover program:
        // one driver compile per key instead of two.
        self.mover.push(create_pipeline(
            device,
            &self.layout,
            &self.shader,
            format,
            key,
            true,
        ));
        self.entity.push(create_entity_pipeline(
            device,
            &self.layout,
            &self.shader,
            format,
            key,
            true,
        ));
        Some(index)
    }

    /// Select a table index without modifying or allocating a GPU buffer.
    pub(super) fn bind<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        fog: u32,
        mode: Mode,
    ) {
        let bank = u32::from(mode == Mode::GlobalExp2) * FOG_SLOTS as u32;
        pass.set_bind_group(1, &self.group, &[(fog + bank) * self.stride]);
    }

    /// Bind the world, mover or entity vertex variant for the selected material.
    pub(super) fn set_pipeline<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        index: usize,
        kind: usize,
    ) {
        let pipelines = match kind {
            0 | 1 => &self.mover,
            _ => &self.entity,
        };
        pass.set_pipeline(&pipelines[index]);
    }
}

fn uniform_entry(binding: u32, dynamic: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: dynamic,
            min_binding_size: None,
        },
        count: None,
    }
}
