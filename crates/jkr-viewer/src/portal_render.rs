//! Destination world rendering through a menu doorway.
use super::*;

impl GpuState {
    /// Keep the preview in step with the join: start it when the target map
    /// becomes known, drop it when the shell leaves the connecting states,
    /// and tell the menu whether the gate may open.
    pub(crate) fn drive_portal(&mut self) {
        let map = self.destination_map();
        if let (Some(map), Some(session)) = (map.as_deref(), self.resident.session.as_ref()) {
            self.portal.aim_session(
                map,
                &self.game_data,
                session.game_state(),
                Some(session.latest_snapshot()),
            );
        } else {
            self.portal.aim(map.as_deref(), self.vfs.as_ref());
        }
        let size = [self.size.width, self.size.height];
        self.portal.poll(&self.context, size, &self.game_data);
        if let Some(menu) = &mut self.client_menu {
            menu.set_destination_ready(self.portal.ready());
        }
    }

    /// `maps/<map>.bsp` the client is joining, if it is joining and the map
    /// is known: the server's gamestate first, else the browser row.
    fn destination_map(&self) -> Option<String> {
        let menu = self.client_menu.as_ref()?;
        // Only the menu world has a gate to look through; a map change on
        // a server has nothing to preview.
        if !self.is_menu_world || !menu.is_connecting() {
            return None;
        }
        if self.resident.session.is_some() {
            return self.pending_map_path().ok();
        }
        if !self.world_load_map.is_empty() {
            return Some(self.world_load_map.clone());
        }
        menu.destination_map().map(|map| format!("maps/{map}.bsp"))
    }

    /// Draw the destination preview into `view` from the menu camera mirrored
    /// through `doorway`, and say how the menu world is to be drawn over it.
    pub(crate) fn draw_portal(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        doorway: Frame,
        shader_time: f32,
    ) -> View {
        let size = self.scene_size();
        let Some(world) = &mut self.portal.world else {
            return View::Absent;
        };
        let camera = Camera {
            position: self.camera_position,
            yaw: self.camera_yaw,
            pitch: self.camera_pitch,
        };
        let mirrored = camera.through(doorway, self.portal.frame);
        let past = camera.is_past(doorway);
        // In front of the doorway the far camera stands behind the far one:
        // clip the destination to the far doorway's plane and cull from it.
        let clip = (!past).then_some(self.portal.frame);
        world.fit_offscreen(winit::dpi::PhysicalSize::new(size[0], size[1]));
        world.draw_static_world(
            encoder,
            view,
            &self.depth.view,
            mirrored,
            clip,
            self.field_of_view,
            shader_time,
        );
        if past { View::Inside } else { View::Behind }
    }

    /// Whether the server world's install must wait: the menu camera is
    /// still on its way through the gate, so the cut to the game would land
    /// before the destination is on screen.
    pub(crate) fn holds_world_install(&self, now: std::time::Instant) -> bool {
        self.is_menu_world && !crate::menu_backdrop::gate_crossed(self, now)
    }

    /// Match the host's target size without touching the shared surface.
    fn fit_offscreen(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        if self.size == size {
            return;
        }
        self.size = size;
        self.configuration.width = size.width;
        self.configuration.height = size.height;
    }

    /// One pass of sky and static world geometry from `camera`, clearing
    /// colour and depth. Nothing dynamic: no entities, movers, effects or UI.
    fn draw_static_world(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        camera: Camera,
        clip: Option<Frame>,
        field_of_view: f32,
        shader_time: f32,
    ) {
        self.camera_position = camera.position;
        self.camera_yaw = camera.yaw;
        self.camera_pitch = camera.pitch;
        let forward = Vec3::new(
            camera.yaw.cos() * camera.pitch.cos(),
            camera.yaw.sin() * camera.pitch.cos(),
            camera.pitch.sin(),
        );
        let view_matrix = look_at_mat4(camera.position, camera.position + forward, Vec3::Z);
        let aspect = self.configuration.width as f32 / self.configuration.height as f32;
        let mut projection = perspective(field_of_view.to_radians(), aspect, 2.0, self.far_plane);
        if let Some(doorway) = clip {
            // A hair short of the plane: the destination reaches under the
            // menu world's sill, which draws over it, instead of leaving a
            // strip of nothing between the two floors.
            let plane_point = doorway.origin - doorway.forward() * CLIP_OVERLAP;
            projection =
                clip::oblique_projection(projection, view_matrix, plane_point, doorway.forward());
        }
        self.update_fog_setting();
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&CameraUniform {
                view_projection: (projection * view_matrix).to_cols_array_2d(),
                camera_position: camera.position.to_array(),
                view_forward: forward.to_array(),
                _padding: 0.0,
                shader_time,
            }),
        );
        // What shows through a doorway is what is visible from the doorway.
        let eye = clip.map_or(camera.position, |doorway| {
            doorway.origin + Vec3::Z * EYE_ABOVE_FLOOR
        });
        let leaf = self.bsp.leaf_at(eye.to_array());
        let source_cluster = usize::try_from(self.bsp.leaves()[leaf].cluster).ok();
        let visibility = self.bsp.render().visibility();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("JKR portal pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.world_materials.draw_sky(
            &mut pass,
            &self.camera_bind_group,
            &self.geometry.vertex_buffer,
            &self.geometry.index_buffer,
            source_cluster,
            visibility,
        );
        self.world_materials.draw_opaque(
            &mut pass,
            &self.camera_bind_group,
            &self.geometry.vertex_buffer,
            &self.geometry.index_buffer,
            &self.actor_instance_buffer,
            &self.mover_instance_ranges,
            source_cluster,
            visibility,
        );
        self.draw_world_fog(&mut pass, source_cluster, visibility, false);
        self.world_materials.draw_blended(
            &mut pass,
            &self.camera_bind_group,
            &self.geometry.vertex_buffer,
            &self.geometry.index_buffer,
            &self.actor_instance_buffer,
            &self.mover_instance_ranges,
            source_cluster,
            visibility,
        );
    }
}
