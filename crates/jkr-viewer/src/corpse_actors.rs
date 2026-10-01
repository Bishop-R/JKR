//! Reliable body copies have independent geometry, even after the owner changes.

use super::*;
use jkr_client::BaseServerCommandEvent;

impl GpuState {
    /// Process copies once per death, never by allocating in the entity loop.
    pub(crate) fn apply_body_commands(&mut self) {
        if self.clientinfo_watch.body_commands.is_empty() {
            return;
        }
        let mut commands = std::mem::take(&mut self.clientinfo_watch.body_commands);
        for command in commands.drain(..) {
            match command {
                BaseServerCommandEvent::CopyBody(body) => {
                    if let Err(error) = self.copy_body_actor(body) {
                        crate::log::progress(format_args!("body copy failed: {error}"));
                    }
                }
                BaseServerCommandEvent::KillGhoul2(number) => {
                    let id = EntityId::new(u64::from(number) + 1);
                    for mesh in &mut self.actor_meshes {
                        if mesh.corpse_pool && mesh.entity_id == Some(id) {
                            mesh.entity_id = None;
                            mesh.body_identity = None;
                        }
                    }
                }
                _ => {}
            }
        }
        self.clientinfo_watch.body_commands = commands;
    }

    fn copy_body_actor(&mut self, body: jkr_client::BodyIdentity) -> Result<(), Box<dyn Error>> {
        let id = EntityId::new(u64::from(body.entity_num) + 1);
        for name in body.sabers.iter().flatten() {
            self.load_hilt(name)?;
        }
        // Reuse only storage with the same appearance. Each active body needs
        // its own vertex ranges; sharing the live mesh overwrites its pose.
        let reusable = self.actor_meshes.iter().position(|mesh| {
            mesh.corpse_pool
                && mesh.appearance == body.appearance
                && (mesh.entity_id.is_none() || mesh.entity_id == Some(id))
        });
        let mut mesh = if let Some(index) = reusable {
            let mesh = &mut self.actor_meshes[index];
            mesh.entity_id = Some(id);
            mesh.saber_names = body.sabers.clone();
            mesh.body_identity = Some(body);
            mesh.animator = crate::actor_pose::storage(&mesh.preview.animation)?;
            return Ok(());
        } else {
            let preview = self
                .actor_meshes
                .iter()
                .find(|mesh| !mesh.corpse_pool && mesh.appearance == body.appearance)
                .map(|mesh| mesh.preview.clone());
            if let Some(preview) = preview {
                self.upload_actor(preview, &body.appearance, id, body.sabers.clone())?
            } else {
                self.build_live_actor(&body.appearance, id, body.sabers.clone())?
            }
        };
        mesh.corpse_pool = true;
        mesh.body_identity = Some(body);
        // The body slot's previous corpse goes back to the pool under its own appearance.
        // Replacing it here threw its uploaded geometry away, so the next death of that
        // model uploaded it again: on a map where a crowd dies every second (one spawn
        // point, telefrags) that was a full actor upload several times a second, into
        // shared buffers that only grow.
        for old in &mut self.actor_meshes {
            if old.corpse_pool && old.entity_id == Some(id) {
                old.entity_id = None;
                old.body_identity = None;
            }
        }
        self.actor_meshes.push(mesh);
        self.actor_groups.push(Vec::with_capacity(4));
        Ok(())
    }
}
