//! Event-time replacement of compiled materials; draw-time indices stay stable.
use super::*;
#[path = "world_remap_material.rs"]
mod material;

/// Original identity and binding retained when its visible shader changes.
pub(super) struct Source {
    pub key: ViewerMaterial,
    pub name: Option<String>,
    pub lightmap: wgpu::TextureView,
    pub fog: Vec<FogDraw>,
    pub applied: Option<(String, f32)>,
}
#[derive(Default)]
pub(super) struct State {
    pub sources: Vec<Source>,
    pub local: std::collections::BTreeMap<String, String>,
    pub applied: Option<(u64, u64, i64)>,
    pub generation: u64,
}

/// Visible shader and destination clock offset for an original shader name.
pub(crate) fn remap_target<'a>(
    server: Option<&'a jkr_client::ShaderRemapTable>,
    local: &'a std::collections::BTreeMap<String, String>,
    name: &'a str,
) -> (&'a str, f32) {
    let target = local
        .get(name)
        .map(String::as_str)
        .unwrap_or_else(|| server.map_or(name, |r| r.destination(name)));
    (target, server.map_or(0., |r| r.time_offset(target)))
}

impl Runtime {
    /// Apply only on a server change or after registering new entity materials.
    pub(crate) fn refresh_remaps(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        remaps: &jkr_client::ShaderRemaps,
        mode: i64,
        visibility: Option<&Visibility>,
    ) -> Result<(), Box<dyn Error>> {
        let stamp = (remaps.stamp().0, remaps.stamp().1, mode);
        if self
            .remaps
            .applied
            .is_some_and(|previous| previous.0 != stamp.0)
        {
            self.remaps.local.clear();
        }
        let remaps = remaps.table(mode);
        if self.remaps.applied == Some(stamp) {
            return Ok(());
        }
        let started = Instant::now();
        let mut changed = 0;
        let mut images = ImageCache::new();
        for source in 0..self.remaps.sources.len() {
            let original = &self.remaps.sources[source];
            let Some(name) = original.name.clone() else {
                continue;
            };
            if !remaps.is_some_and(|r| r.affects(&name))
                && !self.remaps.local.contains_key(&name)
                && original.applied.is_none()
            {
                continue;
            }
            let (target, offset) = remap_target(remaps, &self.remaps.local, &name);
            if original
                .applied
                .as_ref()
                .is_some_and(|(n, t)| n == target && *t == offset)
            {
                continue;
            }
            let target = target.to_owned();
            match self.replace_remapped_material(
                source,
                &target,
                offset,
                device,
                queue,
                vfs,
                shaders,
                &mut images,
            ) {
                Ok(true) => changed += 1,
                Ok(false) => {}
                Err(error) => {
                    crate::log::progress(format_args!("shader remap {name} -> {target}: {error}"))
                }
            }
        }
        if changed != 0 {
            self.rebuild_remapped_fog(visibility);
            self.opaque_order = build_draw_order(&self.materials, false);
            self.blended_order = build_draw_order(&self.materials, true);
            self.prepare_camera_ranges();
            self.active.get_mut().invalidate();
            self.caster_runs.take();
            // Keep the map's sealed BSP shadow boundaries: dropping all hulls for
            // a recolor would reintroduce light leaking through unrelated walls.
            if let Some(table) = &mut self.stage_table {
                table.rebuild(device, &self.forge, &self.materials);
            }
            self.ssao = ssao::AmbientOcclusion::new(
                device,
                &self.forge.camera_layout,
                self.forge.format,
                &self.materials,
                &self.forge.pipeline_keys,
            );
            crate::log::progress(format_args!(
                "shader remaps: {changed} materials updated in {:.1}ms",
                started.elapsed().as_secs_f64() * 1000.
            ));
        }
        let sky_result =
            self.sky
                .refresh_remaps(device, queue, vfs, shaders, remaps, &self.remaps.local);
        self.remaps.applied = Some(stamp);
        self.remaps.generation = self.remaps.generation.wrapping_add(1);
        sky_result
    }

    /// Counts applied remap states, including local edits and late materials.
    pub(crate) fn remap_generation(&self) -> u64 {
        self.remaps.generation
    }
    /// This world's remap target for a shader name, shared by effects.
    pub(crate) fn remap_target<'a>(
        &'a self,
        server: Option<&'a jkr_client::ShaderRemapTable>,
        name: &'a str,
    ) -> (&'a str, f32) {
        remap_target(server, &self.remaps.local, name)
    }

    fn rebuild_remapped_fog(&mut self, visibility: Option<&Visibility>) {
        self.opaque_fog.clear();
        self.opaque_fog_by_cluster.clear();
        for (source, original) in self.remaps.sources.iter().enumerate() {
            let Some(material) = self.materials.get_mut(self.source_to_runtime[source]) else {
                continue;
            };
            material.fog_draws.clear();
            if let Some(pipeline) = material.fog_pipeline {
                material.fog_draws = original.fog.clone();
                for draw in &mut material.fog_draws {
                    draw.rebind(pipeline, (!material.view_bounded).then_some(source));
                }
                if material.sort <= SORT_OPAQUE && material.fog_pass == FogPass::Equal {
                    self.opaque_fog.append(&mut material.fog_draws);
                }
            }
        }
        fog_draws::compact(&mut self.opaque_fog);
        self.prepare_fog_visibility(visibility);
    }
}

impl Runtime {
    /// A local remap is transient and never sent to the server or saved in config.
    pub(crate) fn local_remap(
        &mut self,
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        old: &str,
        new: &str,
    ) -> Result<(), String> {
        let old = jkr_client::shader_name(old).ok_or("Invalid source shader name")?;
        let new = jkr_client::shader_name(new).ok_or("Invalid destination shader name")?;
        for name in [&old, &new] {
            if shaders.get(name).is_none()
                && shaders
                    .resolve_image(vfs, name)
                    .map_err(|e| e.to_string())?
                    .is_none()
            {
                return Err(format!("Shader not found: {name}"));
            }
        }
        if self.remaps.local.len() >= 1024 && !self.remaps.local.contains_key(&old) {
            return Err("Local remap limit reached".into());
        }
        self.remaps.local.insert(old, new);
        self.remaps.applied = None;
        Ok(())
    }
    pub(crate) fn local_remaps(&self) -> impl Iterator<Item = (&str, &str)> {
        self.remaps
            .local
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
    }
}
