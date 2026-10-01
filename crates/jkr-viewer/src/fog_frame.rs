//! Frame composition hook shared by the main and portal views.
use crate::{GpuState, world_materials::FrameDraw};

impl GpuState {
    /// Submit destination-world fog using the same runtime as the main view.
    pub(crate) fn draw_world_fog<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        source_cluster: Option<usize>,
        visibility: Option<&'pass jkr_bsp::Visibility>,
        entities: bool,
    ) {
        self.world_materials.draw_fog(
            pass,
            &FrameDraw {
                camera: &self.camera_bind_group,
                vertices: &self.geometry.vertex_buffer,
                indices: &self.geometry.index_buffer,
                instances: &self.actor_instance_buffer,
                mover_ranges: &self.mover_instance_ranges,
                source_cluster,
                visibility,
                entities: if entities {
                    self.entity_draw_queue.opaque()
                } else {
                    &[]
                },
            },
        );
    }

    /// Read the cached console toggle without allocating or locking.
    pub(crate) fn update_fog_setting(&mut self) {
        self.world_materials.fog_mode = self
            .console
            .as_ref()
            .map_or_else(Default::default, |console| console.draw_fog_mode());
    }
}
