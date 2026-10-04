//! Compile a replacement without touching the material until its uploads succeed.
use super::*;
impl Runtime {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn replace_remapped_material(
        &mut self,
        source: usize,
        target: &str,
        offset: f32,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        images: &mut ImageCache,
    ) -> Result<bool, Box<dyn Error>> {
        let original = &self.remaps.sources[source];
        // A failed registration must not replace usable geometry with the checkerboard.
        if shaders.get(target).is_none() && shaders.resolve_image(vfs, target)?.is_none() {
            crate::log::progress(format_args!("shader remap ignored: missing {target}"));
            return Ok(false);
        }
        let index = self.source_to_runtime[source];
        if index == usize::MAX {
            return Ok(false);
        }
        // R_RemapShader selects an already registered target by name, or
        // registers it with LIGHTMAP_NONE. It does not borrow the source's
        // lightmap when the replacement has never been used by this world.
        let target_source = self
            .remaps
            .sources
            .iter()
            .rev()
            .find(|s| s.name.as_deref() == Some(target));
        let (lightmap_index, lightmap) = if original.name.as_deref() == Some(target) {
            (original.key.lightmap, &original.lightmap)
        } else {
            target_source.map_or(
                (
                    crate::world_stage::LIGHTMAP_NONE,
                    &self.forge.fallback_lightmap,
                ),
                |s| (s.key.lightmap, &s.lightmap),
            )
        };
        let key = ViewerMaterial {
            shader: target.to_owned(),
            lightmap: lightmap_index,
        };
        let definition = shaders.get(target);
        let mut compiled = compile_material(vfs, shaders, &key, lightmap, true, images, false)?;
        let hidden = definition.is_some_and(|d| d.sky.is_some() || !d.has_color_pass());
        if hidden {
            compiled.stages.clear();
        }
        for stage in &mut compiled.stages {
            stage.gpu.emission[2] = offset;
            stage.gpu.emission[3] = if definition.is_some_and(|d| {
                d.stages
                    .iter()
                    .any(|s| s.texture_generator == jkr_shader::TextureGenerator::Environment)
            }) {
                0.7
            } else {
                0.12
            };
        }
        let stages = build_passes(
            device,
            queue,
            &mut self.forge,
            &self.dynamic_light_buffer,
            compiled.stages,
        );
        // Upload failure can still intern earlier stages' pipeline keys.
        // Complete the slots even on error, before a subsequent load uses them.
        while self.entity_pipelines.len() < self.forge.pipeline_keys.len() {
            let key = self.forge.pipeline_keys[self.entity_pipelines.len()];
            self.push_pipelines(device, key);
        }
        let stages = stages?;
        let fog_pass = if hidden {
            FogPass::None
        } else {
            definition.map_or(FogPass::Equal, |d| d.fog_pass())
        };
        let deform = definition.is_some_and(|d| !d.deforms.is_empty());
        let fog_pipeline = self.fog.pipeline_with_geometry(
            device,
            self.forge.format,
            fog_pass,
            definition.map_or(ShaderCull::Front, |d| d.cull),
            u8::from(deform),
        );
        let material = &mut self.materials[index];
        material.view_bounded = !deform;
        material.camera_ranges = Default::default();
        material.blended = compiled.sort > SORT_OPAQUE;
        material.sort = compiled.sort;
        material.light_buffered = (!material.blended
            && !material.flare
            && stages.first().is_some_and(|s| s.shadow_caster))
        .then(|| match self.forge.pipeline_keys[stages[0].pipeline].cull {
            Some(wgpu::Face::Front) => 0,
            Some(wgpu::Face::Back) => 1,
            None => 2,
        });
        material.light_cutout = !material.blended && stages.first().is_some_and(|s| s.light_cutout);
        material.fog_pass = fog_pass;
        material.fog_pipeline = fog_pipeline;
        material.stages = stages;
        self.source_order[source] = (
            material.sort,
            material.stages.first().map_or(0, |s| s.pipeline),
        );
        self.remaps.sources[source].applied = Some((target.to_owned(), offset));
        Ok(true)
    }
}
