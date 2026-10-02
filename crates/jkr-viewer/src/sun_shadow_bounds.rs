//! Conservative depth bounds for exact static-shadow rejection.
use wgpu::util::DeviceExt;

// Kept in step with the compute tile and receiver lookup in the two WGSL files.
const TILE_SIZE: u32 = 64;

/// Min/max depths for the static view, close and far maps, in that layer order.
/// Refresh a layer immediately after its source map, before any receiver reads it.
pub(super) struct Bounds {
    pub(super) view: wgpu::TextureView,
    groups: [wgpu::BindGroup; 3],
    pipeline: wgpu::ComputePipeline,
    dispatches: u32,
}
impl Bounds {
    /// Allocate one pair of bounds per tile, including partial tiles at map edges.
    pub(super) fn new(
        device: &wgpu::Device,
        resolution: u32,
        maps: [&wgpu::TextureView; 3],
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("JKR static shadow bounds"),
            size: wgpu::Extent3d {
                width: resolution.div_ceil(TILE_SIZE),
                height: resolution.div_ceil(TILE_SIZE),
                depth_or_array_layers: 3,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("JKR static shadow bounds"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sun_shadow_bounds.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("JKR static shadow bounds"),
            layout: None,
            module: &module,
            entry_point: Some("build"),
            compilation_options: Default::default(),
            cache: None,
        });
        let groups = std::array::from_fn(|index| {
            let parameters = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&[index as u32, 0u32, 0u32, 0u32]),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("JKR static shadow bounds"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(maps[index]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: parameters.as_entire_binding(),
                    },
                ],
            })
        });
        Self {
            view,
            groups,
            pipeline,
            dispatches: resolution.div_ceil(TILE_SIZE),
        }
    }
    /// Rebuild just the changed static cascade; moving casters never enter these bounds.
    pub(super) fn encode(&self, encoder: &mut wgpu::CommandEncoder, index: usize) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("JKR static shadow bounds"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.groups[index], &[]);
        pass.dispatch_workgroups(self.dispatches, self.dispatches, 1);
    }
}
/// Shared receiver binding for conservative static-shadow rejection.
pub(in crate::world_materials) fn layout_entry() -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding: 7,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    }
}
