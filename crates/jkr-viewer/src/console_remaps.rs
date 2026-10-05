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
        if name == "clearremaps" {
            if !args.is_empty() {
                return Err("usage: clearRemaps".into());
            }
            // EternalJK's renderer command: local only, nothing reaches the server.
            if let Some(session) = &mut self.live_session {
                session.clear_shader_remaps();
            } else if let Some(session) = &mut self.demo_session {
                session.clear_shader_remaps();
            } else {
                self.config_string_refresh.clear_remaps();
            }
            self.world_materials.clear_local_remaps();
            return Ok(vec!["Shader remaps cleared".into()]);
        }
        if !args.is_empty() {
            return Err("usage: listRemaps".into());
        }
        let mut lines = Vec::new();
        let mode = self.console.as_ref().map_or(1, |c| c.remap_mode());
        let state = self
            .live_session
            .as_ref()
            .map(|s| s.shader_remaps())
            .or_else(|| self.demo_session.as_ref().map(|s| s.shader_remaps()));
        if let Some(table) = state.and_then(|s| s.table(mode)) {
            lines.extend(
                table
                    .entries()
                    .map(|(a, b)| format!("server: {a} -> {b} (time {})", table.time_offset(b))),
            );
        }
        lines.extend(
            self.world_materials
                .local_remaps()
                .map(|(a, b)| format!("local: {a} -> {b}")),
        );
        if lines.is_empty() {
            lines.push("No active shader remaps".into());
        }
        Ok(lines)
    }
}
