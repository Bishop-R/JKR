//! Tayst-compatible local shader commands, owned by the displayed map.
impl crate::GpuState {
    pub(super) fn remap_command(
        &mut self,
        name: &str,
        args: &[String],
    ) -> Result<Vec<String>, String> {
        if name == "remapshader" {
            let [old, new] = args else {
                return Err("usage: remapShader <old> <new>".into());
            };
            self.world_materials.local_remap(
                self.vfs.as_deref().ok_or("No loaded map")?,
                &self.shaders,
                old,
                new,
            )?;
            return Ok(vec![format!("{old} -> {new}")]);
        }
        if !args.is_empty() {
            return Err("usage: listRemaps".into());
        }
        let mode = self.console.as_ref().map_or(1, |c| c.remap_mode());
        let state = self
            .live_session
            .as_ref()
            .map(|s| s.shader_remaps())
            .or_else(|| self.demo_session.as_ref().map(|s| s.shader_remaps()));
        let mut lines = self
            .world_materials
            .remap_listing(state.and_then(|s| s.table(mode)));
        if lines.is_empty() {
            lines.push("No active shader remaps".into());
        }
        Ok(lines)
    }
}
