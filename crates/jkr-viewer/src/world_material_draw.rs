//! Allocation-free shared colour draw traversal.
use super::*;

/// Marks the stage table's group and its pipelines in a pass's last-bound state.
const TABLE_GROUP: PassRef = PassRef {
    material: usize::MAX,
    stage: usize::MAX,
};
const TABLE_PIPELINE: usize = 1 << (usize::BITS - 1);

impl Runtime {
    /// The visible sky faces for a sky portal's composite; the caller binds the pipeline.
    pub(crate) fn draw_sky_faces<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        vertices: &'pass wgpu::Buffer,
        indices: &'pass wgpu::Buffer,
        source: Option<usize>,
        visibility: Option<&'pass Visibility>,
    ) {
        self.sky.draw_faces(
            pass,
            vertices,
            indices,
            &self.forge.identity_instance,
            source,
            visibility,
            &self.areas,
        );
    }
    /// Bind the current shared geometry after map upload or an asset-buffer growth.
    pub(crate) fn bind_geometry(&mut self, geometry: &crate::shared_geometry::SharedGeometry) {
        self.forge.geometry = geometry.deform_binding.clone();
    }

    /// Fixed map fog data, sampled alongside entity lighting.
    pub(crate) fn fogs(&self) -> &crate::fog_volumes::Table {
        &self.fog.table
    }

    /// Draw the Q3 sky before ordinary opaque world geometry.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_sky<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        vertices: &'pass wgpu::Buffer,
        indices: &'pass wgpu::Buffer,
        source_cluster: Option<usize>,
        visibility: Option<&'pass Visibility>,
    ) {
        self.sky.draw(
            pass,
            camera,
            vertices,
            indices,
            source_cluster,
            visibility,
            &self.areas,
        );
    }

    /// The main view's Q3 sky on its visible sky faces, after the opaque world.
    pub(crate) fn draw_main_sky_faces<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        vertices: &'pass wgpu::Buffer,
        indices: &'pass wgpu::Buffer,
        source_cluster: Option<usize>,
        visibility: Option<&'pass Visibility>,
    ) {
        self.sky.draw_faces_shaded(
            pass,
            camera,
            vertices,
            indices,
            source_cluster,
            visibility,
            &self.areas,
        );
    }

    pub(crate) fn material_sort(&self, material: usize) -> f32 {
        self.material(material)
            .map_or(SORT_OPAQUE, |entry| entry.sort)
    }

    pub(crate) fn material_blended(&self, material: usize) -> bool {
        self.material(material).is_some_and(|entry| entry.blended)
    }

    pub(crate) fn entity_pipeline_index(&self, material: usize) -> usize {
        self.material(material)
            .and_then(|entry| entry.stages.first())
            .map_or(0, |stage| stage.pipeline)
    }

    pub(super) fn material(&self, source: usize) -> Option<&Material> {
        self.source_to_runtime
            .get(source)
            .copied()
            .filter(|index| *index != usize::MAX)
            .and_then(|index| self.materials.get(index))
    }

    /// Publish one lighting-mode word in the already allocated scene block.
    pub(crate) fn update_scene_lighting_mode(
        &self,
        queue: &wgpu::Queue,
        lights: &PointLightList,
        model_pixels: bool,
        mode: u32,
    ) {
        let mut block: GpuPointLightBlock = lights.gpu_block();
        block.metadata[1] = u32::from(model_pixels);
        block.metadata[2] = mode;
        self.lighting_mode.set(mode);
        queue.write_buffer(&self.dynamic_light_buffer, 0, bytemuck::bytes_of(&block));
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_opaque<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        vertices: &'pass wgpu::Buffer,
        indices: &'pass wgpu::Buffer,
        instances: &'pass wgpu::Buffer,
        mover_ranges: &'pass [Range<u32>],
        source_cluster: Option<usize>,
        visibility: Option<&'pass Visibility>,
    ) {
        self.draw_order(
            pass,
            camera,
            vertices,
            indices,
            instances,
            mover_ranges,
            source_cluster,
            visibility,
            &self.opaque_order,
            false,
        );
    }

    /// Main-view world surfaces may use the current sun and probes; other views never do.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_main_opaque<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        vertices: &'pass wgpu::Buffer,
        indices: &'pass wgpu::Buffer,
        instances: &'pass wgpu::Buffer,
        mover_ranges: &'pass [Range<u32>],
        source_cluster: Option<usize>,
        visibility: Option<&'pass Visibility>,
    ) {
        self.draw_order(
            pass,
            camera,
            vertices,
            indices,
            instances,
            mover_ranges,
            source_cluster,
            visibility,
            &self.opaque_order,
            true,
        );
    }

    /// Main-view blended world surfaces; see `draw_main_opaque`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_main_blended<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        vertices: &'pass wgpu::Buffer,
        indices: &'pass wgpu::Buffer,
        instances: &'pass wgpu::Buffer,
        mover_ranges: &'pass [Range<u32>],
        source_cluster: Option<usize>,
        visibility: Option<&'pass Visibility>,
    ) {
        self.draw_order(
            pass,
            camera,
            vertices,
            indices,
            instances,
            mover_ranges,
            source_cluster,
            visibility,
            &self.blended_order,
            true,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_blended<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        vertices: &'pass wgpu::Buffer,
        indices: &'pass wgpu::Buffer,
        instances: &'pass wgpu::Buffer,
        mover_ranges: &'pass [Range<u32>],
        source_cluster: Option<usize>,
        visibility: Option<&'pass Visibility>,
    ) {
        self.draw_order(
            pass,
            camera,
            vertices,
            indices,
            instances,
            mover_ranges,
            source_cluster,
            visibility,
            &self.blended_order,
            false,
        );
    }

    /// Draw a pre-sorted entity queue with the same fragment stages, material
    /// bind groups, and pipeline states used by BSP surfaces.
    pub(crate) fn draw_entities<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        vertices: &'pass wgpu::Buffer,
        indices: &'pass wgpu::Buffer,
        instances: &'pass wgpu::Buffer,
        draws: &'pass [crate::entity_materials::Draw],
    ) {
        self.draw_entity_view(pass, camera, vertices, indices, instances, draws, false);
    }

    /// Main-view entities may use the current sun; secondary and detached views never do.
    pub(crate) fn draw_main_entities<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        vertices: &'pass wgpu::Buffer,
        indices: &'pass wgpu::Buffer,
        instances: &'pass wgpu::Buffer,
        draws: &'pass [crate::entity_materials::Draw],
    ) {
        self.draw_entity_view(pass, camera, vertices, indices, instances, draws, true);
    }

    fn draw_entity_view<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        vertices: &'pass wgpu::Buffer,
        indices: &'pass wgpu::Buffer,
        instances: &'pass wgpu::Buffer,
        draws: &'pass [crate::entity_materials::Draw],
        main_view: bool,
    ) {
        if draws.is_empty() {
            return;
        }

        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(2, &self.forge.geometry, &[]);
        if let Some(sun) = &self.forge.model_sun {
            pass.set_bind_group(3, sun.binding(main_view), &[]);
        }
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_vertex_buffer(1, instances.slice(..));
        pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
        let live_emission =
            main_view && self.lighting_mode.get() & 3 == 0 && self.realtime_materials_active();
        let mut last_pipeline = None;
        let mut last_bind_group = None;
        // Opaque, depth-tested draws run stage-major: every first stage, then every second
        // stage, and so on. A later stage only lands where its own surface won the depth
        // test, so the image is the draw-major one, but a pipeline is set once per run of
        // draws instead of once per stage of every draw (a pipeline switch is the most
        // expensive state change to encode). Blended and depth-less draws keep their order.

        let stage_major = |draw: &crate::entity_materials::Draw| {
            !draw.no_depth
                && !draw.forced_alpha
                && self
                    .material(draw.material)
                    .is_some_and(|material| !material.blended)
        };
        let deepest = draws
            .iter()
            .filter(|draw| stage_major(draw))
            .filter_map(|draw| self.material(draw.material))
            .map(|m| m.stages.len())
            .max();
        for level in 0..deepest.unwrap_or(0) {
            for draw in draws.iter().filter(|draw| stage_major(draw)) {
                self.draw_entity_stages(
                    pass,
                    draw,
                    level..level + 1,
                    live_emission,
                    &mut last_pipeline,
                    &mut last_bind_group,
                );
            }
        }
        for draw in draws.iter().filter(|draw| !stage_major(draw)) {
            self.draw_entity_stages(
                pass,
                draw,
                0..usize::MAX,
                live_emission,
                &mut last_pipeline,
                &mut last_bind_group,
            );
        }
    }

    /// Draw the stages of one entity draw that fall in `levels`.
    fn draw_entity_stages<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        draw: &crate::entity_materials::Draw,
        levels: Range<usize>,
        live_emission: bool,
        last_pipeline: &mut Option<(usize, bool)>,
        last_bind_group: &mut Option<PassRef>,
    ) {
        let Some(runtime_material_index) = self
            .source_to_runtime
            .get(draw.material)
            .copied()
            .filter(|index| *index != usize::MAX)
        else {
            return;
        };
        let Some(material) = self.materials.get(runtime_material_index) else {
            return;
        };
        // Depth-tested stages on the stage table: the table group stays bound and an
        // immediate names the record, in place of a bind-group switch per draw.
        // Forced-alpha draws take the per-stage bind groups with their own pipelines.
        let table = self
            .stage_table
            .as_ref()
            .filter(|_| !draw.no_depth && !draw.forced_alpha);
        for (stage_index, stage) in material
            .stages
            .iter()
            .enumerate()
            .skip(levels.start)
            .take(levels.end.saturating_sub(levels.start))
        {
            let index = match (draw.forced_alpha, live_emission) {
                (false, false) => stage.pipeline,
                (false, true) => stage.live_pipeline,
                (true, live) => stage.forced_alpha_pipelines[usize::from(live)],
            };
            if let Some((table, record)) = table
                .and_then(|table| Some((table, table.record(runtime_material_index, stage_index)?)))
            {
                if let Some(tabled) = table.pipeline(&self.forge, index, true) {
                    let pipeline = (index | TABLE_PIPELINE, false);
                    if *last_pipeline != Some(pipeline) {
                        pass.set_pipeline(tabled);
                        *last_pipeline = Some(pipeline);
                    }
                    if *last_bind_group != Some(TABLE_GROUP) {
                        pass.set_bind_group(1, table.group(), &[]);
                        *last_bind_group = Some(TABLE_GROUP);
                    }
                    pass.set_immediates(0, &record.to_le_bytes());
                    pass.draw_indexed(draw.indices.clone(), 0, draw.instances.clone());

                    continue;
                }
            }
            let pipeline = (index, draw.no_depth);
            if *last_pipeline != Some(pipeline) {
                pass.set_pipeline(self.entity_pipeline(index, !draw.no_depth));

                *last_pipeline = Some(pipeline);
            }
            let bind = PassRef {
                material: runtime_material_index,
                stage: stage_index,
            };
            if *last_bind_group != Some(bind) {
                pass.set_bind_group(1, &stage.bind_group, &[]);

                *last_bind_group = Some(bind);
            }
            pass.draw_indexed(draw.indices.clone(), 0, draw.instances.clone());
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_order<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass wgpu::BindGroup,
        vertices: &'pass wgpu::Buffer,
        indices: &'pass wgpu::Buffer,
        instances: &'pass wgpu::Buffer,
        mover_ranges: &'pass [Range<u32>],
        source_cluster: Option<usize>,
        visibility: Option<&'pass Visibility>,
        order: &[PassRef],
        main_view: bool,
    ) {
        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(2, &self.forge.geometry, &[]);
        if let Some(sun) = &self.forge.model_sun {
            pass.set_bind_group(3, sun.binding(main_view), &[]);
        }
        let live_emission =
            main_view && self.lighting_mode.get() & 3 == 0 && self.realtime_materials_active();
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
        let mut last_pipeline = None;
        let mut last_bind_group = None;
        let active = self.active_materials(source_cluster, visibility);
        // Stage-table statics first: one multi-draw per run of draws sharing a pipeline,
        // every stage named by its draw's first instance. Opaque, depth-tested passes do
        // not depend on the order between materials.
        let tabled = self
            .stage_table
            .as_ref()
            .zip(self.indirect.as_ref())
            .filter(|_| {
                std::ptr::eq(order, self.opaque_order.as_slice()) && self.forge.model_sun.is_some()
            });
        let listed = tabled.and_then(|(table, indirect)| {
            let runs = indirect.runs(&self.forge.queue, |push| {
                for &reference in order {
                    if !active.flags[reference.material] {
                        continue;
                    }
                    let Some(record) = table.record(reference.material, reference.stage) else {
                        continue;
                    };
                    let material = &self.materials[reference.material];
                    let stage = &material.stages[reference.stage];
                    let index = if live_emission {
                        stage.live_pipeline
                    } else {
                        stage.pipeline
                    };
                    for range in self.visible_static_ranges(material, source_cluster, visibility) {
                        push(index, range, record);
                    }
                }
            })?;
            pass.set_bind_group(1, table.group(), &[]);
            for run in runs.iter() {
                let Some(pipeline) = table.pipeline(&self.forge, run.pipeline, false) else {
                    continue;
                };
                pass.set_pipeline(pipeline);
                pass.multi_draw_indexed_indirect(&indirect.buffer, run.offset, run.count);
            }
            Some(table)
        });
        for &reference in order {
            if !active.flags[reference.material] {
                continue;
            }
            let material = &self.materials[reference.material];
            if reference.stage == material.stages.len() {
                let input = FrameDraw {
                    camera,
                    vertices,
                    indices,
                    instances,
                    mover_ranges,
                    source_cluster,
                    visibility,
                    entities: &[],
                };
                self.draw_fog_ranges(pass, &input, &material.fog_draws);
                // The fog pass binds its own groups (its geometry stage sits at 3): restore
                // the material groups before the next stage draws.
                pass.set_bind_group(0, camera, &[]);
                pass.set_bind_group(2, &self.forge.geometry, &[]);
                if let Some(sun) = &self.forge.model_sun {
                    pass.set_bind_group(3, sun.binding(main_view), &[]);
                }
                last_pipeline = None;
                last_bind_group = None;
                continue;
            }
            let stage = &material.stages[reference.stage];
            let index = if live_emission {
                stage.live_pipeline
            } else {
                stage.pipeline
            };
            let on_table = listed
                .is_some_and(|table| table.record(reference.material, reference.stage).is_some());
            // Statics drawn by the stage table need no range walk here.
            let statics = !on_table;
            let mut ranges = statics
                .then(|| self.visible_static_ranges(material, source_cluster, visibility))
                .into_iter()
                .flatten()
                .peekable();
            if ranges.peek().is_some() {
                let pipeline = (false, index);
                if last_pipeline != Some(pipeline) {
                    pass.set_pipeline(self.world_pipeline(index));
                    pass.set_vertex_buffer(1, self.forge.identity_instance.slice(..));

                    last_pipeline = Some(pipeline);
                }
                if last_bind_group != Some(reference) {
                    pass.set_bind_group(1, &stage.bind_group, &[]);

                    last_bind_group = Some(reference);
                }
                for range in ranges {
                    pass.draw_indexed(range, 0, 0..1);
                }
            }
            if !material.mover_draws.is_empty() {
                let pipeline = (true, index);
                if last_pipeline != Some(pipeline) {
                    pass.set_pipeline(self.world_pipeline(index));

                    last_pipeline = Some(pipeline);
                }
                if last_bind_group != Some(reference) {
                    pass.set_bind_group(1, &stage.bind_group, &[]);

                    last_bind_group = Some(reference);
                }
                pass.set_vertex_buffer(1, instances.slice(..));
                for draw in &material.mover_draws {
                    if let Some(range) = mover_ranges
                        .get(draw.mesh)
                        .filter(|range| !range.is_empty())
                    {
                        pass.draw_indexed(draw.indices.clone(), 0, range.clone());
                    }
                }
            }
        }
    }
}

impl Material {
    pub(super) fn static_draws_for(
        &self,
        source: Option<usize>,
        visibility: Option<&Visibility>,
    ) -> &[StaticDraw] {
        if visibility.is_some()
            && let Some(source) = source
        {
            if let Some(spans) = self.static_draws_by_cluster.get(source) {
                return spans;
            }
        }
        &self.static_draws
    }
}
