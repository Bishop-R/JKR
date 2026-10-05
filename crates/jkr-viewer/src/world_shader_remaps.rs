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
    /// The replacement drawn instead of the slot's own shader; `None` as loaded.
    pub applied: Option<Box<Applied>>,
}
/// A drawn replacement, and the slot's own compiled state kept aside for its restore.
pub(super) struct Applied {
    target: String,
    offset: f32,
    own: material::Look,
}
#[derive(Default)]
pub(super) struct State {
    pub sources: Vec<Source>,
    pub local: std::collections::BTreeMap<String, String>,
    pub applied: Option<(u64, u64, i64)>,
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
            let target = self
                .remaps
                .local
                .get(&name)
                .map(String::as_str)
                .unwrap_or_else(|| remaps.map_or(name.as_str(), |r| r.destination(&name)));
            let offset = remaps.map_or(0., |r| r.time_offset(target));
            let applied = original
                .applied
                .as_deref()
                .map(|a| (a.target.as_str(), a.offset));
            let next = step(&name, applied, target, offset);
            let target = target.to_owned();
            let result = match next {
                Step::Keep => continue,
                Step::Restore => Ok(self.restore_remapped_material(source)),
                Step::Replace => self.replace_remapped_material(
                    source,
                    &target,
                    offset,
                    device,
                    queue,
                    vfs,
                    shaders,
                    &mut images,
                ),
            };
            match result {
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
        sky_result
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

/// What a slot drawing `applied` needs in order to show `target` at `offset`.
#[derive(Debug, PartialEq)]
enum Step {
    Keep,
    Restore,
    Replace,
}

/// Load compiles every slot from its own name at shader time zero: only that state is
/// restored as kept. A native user retimed by its own shader's offset is recompiled.
fn step(name: &str, applied: Option<(&str, f32)>, target: &str, offset: f32) -> Step {
    if target == name && offset == 0. {
        if applied.is_some() {
            Step::Restore
        } else {
            Step::Keep
        }
    } else if applied == Some((target, offset)) {
        Step::Keep
    } else {
        Step::Replace
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_load_state_is_restored_without_compiling() {
        assert_eq!(step("a", None, "a", 0.), Step::Keep);
        assert_eq!(step("a", Some(("b", 0.)), "a", 0.), Step::Restore);
        assert_eq!(step("a", Some(("a", 2.)), "a", 0.), Step::Restore);
        assert_eq!(step("a", None, "a", 2.), Step::Replace);
        assert_eq!(step("a", Some(("a", 2.)), "a", 3.), Step::Replace);
        assert_eq!(step("a", Some(("b", 2.)), "b", 2.), Step::Keep);
        assert_eq!(step("a", Some(("b", 2.)), "b", 3.), Step::Replace);
        assert_eq!(step("a", Some(("b", 0.)), "c", 0.), Step::Replace);
    }
}
