//! In-match reaction to clientinfo changes — JKR's `CG_NewClientInfo`
//! (`codemp/cgame/cg_players.c`, called from `CG_ConfigStringModified` in
//! `cg_servercmds.c:894-896` whenever a `CS_PLAYERS` string changes).
//!
//! The server rewrites a player's clientinfo when their model changes
//! (`ClientUserinfoChanged`, at once) or their sabers change (`ClientSpawn`,
//! on the next spawn). The load-time actor meshes only know the roster of
//! the first gamestate, so a player who changes model kept the old mesh and
//! a saber nobody carried at load fell back to `single_1`. This module
//! receives dirty client indices from the session and rebuilds that player's
//! mesh and missing hilts on a change, appending
//! the geometry and materials to the shared buffers.

use super::*;
use crate::actor_load::{build_actor_mesh, client_saber_names};
use crate::player_assets::{GlaCache, load_player_appearance_with};
use crate::shared_geometry::relocate;
use jkr_client::legacy_client_appearance_forced;

#[path = "corpse_actors.rs"]
mod corpse_actors;
#[path = "clientinfo_forcing.rs"]
mod forcing;

/// Per-world appearance loading caches used by dirty clientinfo notifications.
pub(crate) struct ClientInfoWatch {
    pub(crate) body_commands: Vec<jkr_client::BaseServerCommandEvent>,
    gla_cache: GlaCache,
    forced: bool,
    forced_model: String,
    overrides: [String; 3],
    local_identity: (i32, i32),
    /// Appearances whose load failed; not retried until the string changes.
    pub(crate) failed: BTreeSet<Appearance>,
}

impl ClientInfoWatch {
    pub(crate) fn new() -> Self {
        Self {
            body_commands: Vec::new(),
            gla_cache: GlaCache::default(),
            forced: false,
            forced_model: String::new(),
            overrides: std::array::from_fn(|_| String::new()),
            local_identity: (-1, -1),
            failed: BTreeSet::new(),
        }
    }
}

impl GpuState {
    /// Rebuild live player meshes only when the forced-model policy changes.
    pub(crate) fn refresh_forced_models(&mut self) {
        let forced = self
            .console
            .as_ref()
            .and_then(|c| c.bool_cvar("cg_forcemodel"))
            .unwrap_or(false);
        let model = self
            .console
            .as_ref()
            .and_then(|c| c.text_cvar("model").ok())
            .unwrap_or("kyle/default");
        let overrides = [
            "cg_forceenemymodel",
            "cg_forceallymodel",
            "cg_forceownsaber",
        ]
        .map(|name| {
            self.console
                .as_ref()
                .and_then(|c| c.text_cvar(name).ok())
                .unwrap_or("none")
        });
        let game = self
            .live_session
            .as_ref()
            .map(ClientSession::game_state)
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .map(demo_playback::Session::game_state)
            });
        let snapshot = self
            .live_session
            .as_ref()
            .map(ClientSession::latest_snapshot)
            .or_else(|| {
                self.demo_session
                    .as_ref()
                    .map(demo_playback::Session::latest_snapshot)
            });
        let has_game = game.is_some();
        let local = game.map_or((-1, -1), |game| {
            (
                game.client_num,
                snapshot.map_or(0, |snap| i32::from(snap.player.team())),
            )
        });
        if self.clientinfo_watch.forced == forced
            && (!forced || self.clientinfo_watch.forced_model == model)
            && self
                .clientinfo_watch
                .overrides
                .iter()
                .map(String::as_str)
                .eq(overrides)
            && self.clientinfo_watch.local_identity == local
        {
            return;
        }
        self.clientinfo_watch.forced = forced;
        self.clientinfo_watch.forced_model.clear();
        self.clientinfo_watch.forced_model.push_str(model);
        for (stored, value) in self.clientinfo_watch.overrides.iter_mut().zip(overrides) {
            stored.clear();
            stored.push_str(value);
        }
        self.clientinfo_watch.local_identity = local;
        // Without a game state there is no player to rebuild; the first one
        // changes `local` and comes back here.
        if !has_game {
            return;
        }
        for client in 0..32 {
            if let Err(error) = self.apply_clientinfo(client) {
                log::progress(format_args!("force-model client {client}: {error}"));
            }
        }
    }

    /// Apply one dirty player slot using the existing actor and hilt caches.
    pub(crate) fn apply_clientinfo(&mut self, client: u16) -> Result<(), Box<dyn Error>> {
        let (appearance, saber_names) = {
            let game_state = self
                .live_session
                .as_ref()
                .map(ClientSession::game_state)
                .or_else(|| {
                    self.demo_session
                        .as_ref()
                        .map(demo_playback::Session::game_state)
                })
                .ok_or("no game state")?;
            let forced = self
                .clientinfo_watch
                .forced
                .then_some(self.clientinfo_watch.forced_model.as_str());
            let override_model = forcing::model(
                forced,
                &self.clientinfo_watch.overrides,
                client,
                self.clientinfo_watch.local_identity,
                forcing::team(game_state, client),
            );
            let Some(appearance) = legacy_client_appearance_forced(
                game_state,
                client,
                override_model.as_deref().or(forced),
            ) else {
                return Ok(());
            };
            let mut names = client_saber_names(Some(game_state), client);
            if i32::from(client) == game_state.client_num {
                forcing::sabers(&mut names, &self.clientinfo_watch.overrides[2]);
            }
            (appearance, names)
        };
        for name in saber_names.iter().flatten() {
            self.load_hilt(name)?;
        }
        let entity_id = EntityId::new(u64::from(client) + 1);
        let index = self
            .actor_meshes
            .iter()
            .position(|mesh| !mesh.corpse_pool && mesh.entity_id == Some(entity_id));
        if let Some(index) = index {
            self.actor_meshes[index].saber_names = saber_names.clone();
            if self.actor_meshes[index].appearance == appearance {
                return Ok(());
            }
        }
        if self.clientinfo_watch.failed.contains(&appearance) {
            return Ok(());
        }
        let mesh = match self.build_live_actor(&appearance, entity_id, saber_names) {
            Ok(mesh) => mesh,
            Err(error) => {
                self.clientinfo_watch.failed.insert(appearance.clone());
                return Err(error);
            }
        };
        match index {
            Some(index) => self.actor_meshes[index] = mesh,
            None => {
                self.actor_meshes.push(mesh);
                self.actor_groups.push(Vec::with_capacity(4));
            }
        }
        log::progress(format_args!(
            "client {client} now wears {}/{}",
            appearance.model, appearance.variant
        ));
        Ok(())
    }

    /// Load `appearance` for `entity_id` and append it to the shared buffers.
    pub(crate) fn build_live_actor(
        &mut self,
        appearance: &Appearance,
        entity_id: EntityId,
        saber_names: [Option<String>; 2],
    ) -> Result<ActorMesh, Box<dyn Error>> {
        let vfs = self.vfs.clone().ok_or("no VFS")?;
        let preview = load_player_appearance_with(
            &vfs,
            &appearance.model,
            &appearance.variant,
            [0.0; 3],
            0.0,
            &mut self.clientinfo_watch.gla_cache,
        )?;
        self.upload_actor(preview, appearance, entity_id, saber_names)
    }

    /// Upload a captured preview into independent pose storage and vertex ranges.
    pub(crate) fn upload_actor(
        &mut self,
        preview: PlayerPreview,
        appearance: &Appearance,
        entity_id: EntityId,
        saber_names: [Option<String>; 2],
    ) -> Result<ActorMesh, Box<dyn Error>> {
        let vfs = self.vfs.clone().ok_or("no VFS")?;
        let mut scene = FlattenedScene::default();
        let mut mesh = build_actor_mesh(
            &mut scene,
            preview,
            Some(entity_id),
            false,
            appearance.clone(),
            saber_names,
        )?;
        let material_base = self.world_materials.append_entity_materials(
            &self.device,
            &self.queue,
            &vfs,
            &self.shaders,
            &scene.materials,
        )?;
        let placement =
            self.geometry
                .append(&self.device, &self.queue, &scene.vertices, &scene.indices)?;
        self.world_materials.bind_geometry(&self.geometry);
        relocate(
            &mut mesh.draws,
            &mut mesh.vertex_ranges,
            placement,
            material_base,
        );
        // Palette-skin the new mesh like the actors known at world load; on a topology
        // mismatch this one mesh keeps the CPU path.
        match self
            .geometry
            .append_actor_skin(&self.device, &self.queue, &mut mesh)
        {
            Ok(true) => self.world_materials.bind_geometry(&self.geometry),
            Ok(false) => {}
            Err(error) => log::progress(format_args!(
                "GPU skinning unavailable for {}: {error}",
                appearance.model
            )),
        }
        Ok(mesh)
    }

    /// Make sure the hilt catalog can answer for `name`, loading the `.sab`
    /// model into the shared buffers if it never was.
    pub(crate) fn load_hilt(&mut self, name: &str) -> Result<(), Box<dyn Error>> {
        let Some(catalog) = &mut self.saber_hilts else {
            return Ok(());
        };
        if catalog.is_resolved(name) {
            return Ok(());
        }
        let vfs = self.vfs.clone().ok_or("no VFS")?;
        let mut scene = FlattenedScene::default();
        if !catalog.load(&vfs, name, &mut scene, &mut self.object_meshes) {
            return Ok(());
        }
        // The catalog appended one object mesh; give it a group slot even if
        // the upload fails, so `mesh_index` stays valid (an empty draw list
        // is an invisible hilt, not a crash).
        self.object_groups.push(Vec::with_capacity(32));
        let materials = &scene.materials;
        let uploaded = self
            .world_materials
            .append_entity_materials(&self.device, &self.queue, &vfs, &self.shaders, materials)
            .and_then(|material_base| {
                let placement = self.geometry.append(
                    &self.device,
                    &self.queue,
                    &scene.vertices,
                    &scene.indices,
                )?;
                self.world_materials.bind_geometry(&self.geometry);
                Ok((placement, material_base))
            });
        let mesh = self.object_meshes.last_mut().ok_or("hilt mesh vanished")?;
        match uploaded {
            Ok((placement, material_base)) => {
                relocate(&mut mesh.draws, &mut [], placement, material_base);
                log::progress(format_args!("loaded saber hilt {name} mid-match"));
                Ok(())
            }
            Err(error) => {
                mesh.draws.clear();
                Err(error)
            }
        }
    }
}
