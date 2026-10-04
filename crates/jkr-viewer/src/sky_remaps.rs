//! Retarget authored sky images without replacing day/night resources.
use super::*;
impl Runtime {
    pub(crate) fn refresh_remaps(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        remaps: Option<&jkr_client::ShaderRemapTable>,
        local: &std::collections::BTreeMap<String, String>,
    ) -> Result<(), Box<dyn Error>> {
        for material in &mut self.materials {
            let Some(name) = jkr_client::shader_name(&material.name) else {
                continue;
            };
            if !remaps.is_some_and(|r| r.affects(&name))
                && !local.contains_key(&name)
                && material.remapped.is_none()
            {
                continue;
            }
            let target = local
                .get(&name)
                .map(String::as_str)
                .unwrap_or_else(|| remaps.map_or(name.as_str(), |r| r.destination(&name)));
            if material.remapped.as_deref() == Some(target) {
                continue;
            }
            let Some(sky) = shaders.get(target).and_then(|d| d.sky.as_ref()) else {
                continue;
            };
            let (bind_group, vertex_buffer, vertex_count, missing_faces) =
                sky.outer_box.as_deref().map_or_else(
                    || Ok((None, None, 0, Vec::new())),
                    |prefix| {
                        load_box(
                            device,
                            queue,
                            &gpu::texture_layout(device),
                            vfs,
                            shaders,
                            prefix,
                        )
                    },
                )?;
            material.bind_group = bind_group;
            material.vertex_buffer = vertex_buffer;
            material.vertex_count = vertex_count;
            material.missing_faces = missing_faces;
            material.remapped = Some(target.to_owned());
        }
        Ok(())
    }
}
