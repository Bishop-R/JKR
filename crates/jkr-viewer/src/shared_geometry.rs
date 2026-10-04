//! The combined vertex/index buffers every world, actor and object draw reads
//! from. They are uploaded once at map load and can grow when a match needs
//! geometry the load did not know about (a player's new model, a hilt nobody
//! carried yet): the buffers are reallocated with the old contents copied on
//! the GPU and the new mesh appended behind them. That happens per change, not
//! per frame, so the hot path only ever sees a plain buffer handle.

use super::{ActorDraw, GpuVertex, PreviewVertexRange};
use wgpu::util::DeviceExt;

#[path = "geometry_environment.rs"]
pub(crate) mod environment;
#[path = "quad_geometry.rs"]
pub(crate) mod quads;

pub(crate) struct SharedGeometry {
    pub(crate) vertex_buffer: wgpu::Buffer,
    pub(crate) index_buffer: wgpu::Buffer,
    pub(crate) deform_binding: wgpu::BindGroup,
    quad_buffer: wgpu::Buffer,
    vertex_count: u32,
    index_count: u32,
    environment_buffer: wgpu::Buffer,
    pub(crate) environment: environment::State,
    /// Static skin inputs and per-actor joint palettes, independent of CPU trace storage.
    pub(crate) skinning: crate::actor_pose::gpu_skinning::Buffers,
}

/// Where an appended mesh landed in the shared buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Placement {
    pub(crate) vertex_base: u32,
    pub(crate) index_base: u32,
}

const VERTEX_USAGE: wgpu::BufferUsages = wgpu::BufferUsages::VERTEX
    .union(wgpu::BufferUsages::STORAGE)
    .union(wgpu::BufferUsages::COPY_DST)
    .union(wgpu::BufferUsages::COPY_SRC);
const INDEX_USAGE: wgpu::BufferUsages = wgpu::BufferUsages::INDEX
    .union(wgpu::BufferUsages::COPY_DST)
    .union(wgpu::BufferUsages::COPY_SRC);

impl SharedGeometry {
    /// Upload the load-time scene.
    pub(crate) fn upload(device: &wgpu::Device, vertices: &[GpuVertex], indices: &[u32]) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("JKR world vertices"),
            contents: if vertices.is_empty() {
                &[0; std::mem::size_of::<GpuVertex>()]
            } else {
                bytemuck::cast_slice(vertices)
            },
            usage: VERTEX_USAGE,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("JKR world indices"),
            contents: if indices.is_empty() {
                &[0; 4]
            } else {
                bytemuck::cast_slice(indices)
            },
            usage: INDEX_USAGE,
        });
        let quad_buffer = quads::upload(device, vertices.len(), indices, 0);
        let environment = environment::State::default();
        let environment_buffer = environment::buffer(device, &environment.data);
        let deform_binding =
            quads::bind_with_environment(device, &vertex_buffer, &quad_buffer, &environment_buffer);
        Self {
            vertex_buffer,
            index_buffer,
            quad_buffer,
            deform_binding,
            vertex_count: vertices.len() as u32,
            index_count: indices.len() as u32,
            environment_buffer,
            environment,
            skinning: crate::actor_pose::gpu_skinning::Buffers::empty(device),
        }
    }

    /// Append a mesh whose `indices` are relative to its own first vertex;
    /// they are rebased onto the shared buffer here.
    pub(crate) fn append(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        vertices: &[GpuVertex],
        indices: &[u32],
    ) -> Result<Placement, Box<dyn std::error::Error>> {
        let placement = Placement {
            vertex_base: self.vertex_count,
            index_base: self.index_count,
        };
        let rebased = indices
            .iter()
            .map(|index| {
                placement
                    .vertex_base
                    .checked_add(*index)
                    .ok_or("shared geometry index overflow")
            })
            .collect::<Result<Vec<u32>, _>>()?;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("JKR shared geometry growth"),
        });
        self.vertex_buffer = grow(
            device,
            queue,
            &mut encoder,
            &self.vertex_buffer,
            u64::from(self.vertex_count) * std::mem::size_of::<GpuVertex>() as u64,
            "JKR world vertices",
            VERTEX_USAGE,
            bytemuck::cast_slice(vertices),
        );
        self.index_buffer = grow(
            device,
            queue,
            &mut encoder,
            &self.index_buffer,
            u64::from(self.index_count) * 4,
            "JKR world indices",
            INDEX_USAGE,
            bytemuck::cast_slice(&rebased),
        );
        let refs = quads::references(vertices.len(), indices, placement.vertex_base);
        self.quad_buffer = grow(
            device,
            queue,
            &mut encoder,
            &self.quad_buffer,
            u64::from(self.vertex_count) * std::mem::size_of::<quads::QuadRef>() as u64,
            "JKR quad lookup",
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            bytemuck::cast_slice(&refs),
        );
        self.rebind(device);
        queue.submit(std::iter::once(encoder.finish()));
        self.vertex_count += vertices.len() as u32;
        self.index_count += rebased.len() as u32;
        Ok(placement)
    }

    pub(crate) fn update_environment(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        time: i32,
        projection: glam::Mat4,
        controls: Option<&environment::Cvars>,
    ) {
        self.environment.update(time, projection, controls);
        queue.write_buffer(
            &self.environment_buffer,
            0,
            bytemuck::bytes_of(&self.environment.data),
        );
    }

    /// Rebind shared buffers after load-time skin installation or geometry growth.
    /// Give a mesh already appended to these buffers its skinning palette, then rebind.
    pub(crate) fn append_actor_skin(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        mesh: &mut crate::ActorMesh,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let skinned = self
            .skinning
            .append_actor(device, queue, self.vertex_count, mesh)?;
        if skinned {
            self.rebind(device);
        }
        Ok(skinned)
    }

    pub(crate) fn rebind(&mut self, device: &wgpu::Device) {
        self.deform_binding = quads::bind_with_skin(
            device,
            &self.vertex_buffer,
            &self.quad_buffer,
            &self.environment_buffer,
            &self.skinning,
        );
    }
}

/// A copy of `old` with `appended` behind it. The GPU copy and the queued
/// write touch disjoint byte ranges, so their order does not matter.
fn grow(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    encoder: &mut wgpu::CommandEncoder,
    old: &wgpu::Buffer,
    old_size: u64,
    label: &str,
    usage: wgpu::BufferUsages,
    appended: &[u8],
) -> wgpu::Buffer {
    let new = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (old_size + appended.len() as u64).max(32),
        usage,
        mapped_at_creation: false,
    });
    if old_size != 0 {
        encoder.copy_buffer_to_buffer(old, 0, &new, 0, old_size);
    }
    if !appended.is_empty() {
        queue.write_buffer(&new, old_size, appended);
    }
    new
}

/// Move `draws` and `ranges` from a mesh's private scene onto the shared
/// buffers at `placement`, with its materials starting at `material_base`.
pub(crate) fn relocate(
    draws: &mut [ActorDraw],
    ranges: &mut [PreviewVertexRange],
    placement: Placement,
    material_base: usize,
) {
    for draw in draws {
        draw.indices =
            draw.indices.start + placement.index_base..draw.indices.end + placement.index_base;
        draw.material += material_base;
    }
    for range in ranges {
        let base = placement.vertex_base as usize;
        range.vertices = range.vertices.start + base..range.vertices.end + base;
    }
}
