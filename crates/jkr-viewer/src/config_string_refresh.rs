//! `CG_ConfigStringModified` dispatch at the legacy viewer boundary.

use super::*;
use jkr_protocol::{ConfigStringDirty, MAX_CONFIGSTRINGS};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Consumer {
    ServerInfo,
    SystemInfo,
    Music,
    Sound,
    Model(i16),
    Player(u16),
    Effect,
}

fn consumer(index: usize) -> Option<Consumer> {
    Some(match index {
        0 => Consumer::ServerInfo,
        1 => Consumer::SystemInfo,
        2 => Consumer::Music,
        32 | 37..=292 | 811..=1066 => Consumer::Sound,
        298..=809 => Consumer::Model((index - 298) as i16),
        1131..=1162 => Consumer::Player((index - 1131) as u16),
        1355..=1418 => Consumer::Effect,
        _ => return None,
    })
}

/// Bytes installed in this world, allowing a replacement gamestate's `mark_all`
/// to skip assets already built by the world-install worker.
pub(crate) struct ConfigStringRefresh {
    values: Vec<Vec<u8>>,
}

impl ConfigStringRefresh {
    /// Seed from the exact gamestate used by the world builder.
    pub(crate) fn new(game_state: Option<&GameState>) -> Self {
        Self {
            values: (0..MAX_CONFIGSTRINGS)
                .map(|index| {
                    game_state
                        .and_then(|game| game.config_string(index))
                        .unwrap_or_default()
                        .to_vec()
                })
                .collect(),
        }
    }

    /// Remember changed bytes; untouched and already-built slots require no work.
    pub(crate) fn accept(&mut self, index: usize, bytes: &[u8]) -> bool {
        if self.values[index] == bytes {
            return false;
        }
        self.values[index].clear();
        self.values[index].extend_from_slice(bytes);
        true
    }
}

/// Load the initial effect graphs once while the world is being built.
pub(crate) fn preload_effects(
    vfs: &VirtualFileSystem,
    map: &LegacyMapEffects,
    missiles: &LegacyMissileEffects,
) -> EffectLibrary {
    let mut effects = EffectLibrary::default();
    effects.preload(vfs, effect_assets::STOCK_EFFECTS);
    effects.preload(vfs, map.effect_names().chain(missiles.effect_names()));
    let _ = effects.shader("gfx/effects/saberFlare");
    for shader in effect_assets::CODE_SHADERS {
        let _ = effects.shader(shader);
    }
    let _ = effects.code_primitive_definition();
    for icon in crate::pickups::simple::ICONS
        .iter()
        .chain(crate::pickups::simple::DISABLED_ICONS.iter())
        .filter(|s| !s.is_empty())
    {
        let _ = effects.shader(icon);
    }
    effects
}

impl GpuState {
    /// Drain once per frame. An idle session performs only fixed bitset work.
    pub(crate) fn refresh_config_strings(&mut self, audio: &mut Option<GameAudio>) {
        self.refresh_forced_models();
        if self.pending_map_reload
            || self.world_load_task.is_some()
            || self.world_install_task.is_some()
        {
            return;
        }
        let mut changes = ConfigStringDirty::default();
        if let Some(session) = &mut self.live_session {
            session.drain_config_string_changes(|index| changes.mark(index));
        } else if let Some(session) = &mut self.demo_session {
            session.drain_config_string_changes(|index| changes.mark(index));
        }
        changes.drain(|index| {
            let Some(target) = consumer(index) else {
                return;
            };
            let game = self
                .live_session
                .as_ref()
                .map(ClientSession::game_state)
                .or_else(|| {
                    self.demo_session
                        .as_ref()
                        .map(demo_playback::Session::game_state)
                });
            let Some(game) = game else { return };
            let bytes = game.config_string(index).unwrap_or_default();
            if !self.config_string_refresh.accept(index, bytes) {
                return;
            }
            match target {
                Consumer::SystemInfo => {
                    self.local_prediction.refresh_roll_rules(game);
                    if let Some(console) = &mut self.console {
                        console.refresh_movement_cvars(game);
                    }
                }
                Consumer::ServerInfo => {
                    self.local_prediction.refresh_roll_rules(game);
                    if let Some(console) = &mut self.console {
                        if let Some(session) = &self.live_session {
                            console.set_server_info(session);
                        } else {
                            console.set_demo_server_info(game);
                        }
                    }
                }
                Consumer::Music => {
                    if let (Some(audio), Some(vfs)) = (audio.as_mut(), &self.vfs) {
                        audio.refresh_music(game, vfs);
                    }
                }
                Consumer::Sound => {
                    if let (Some(audio), Some(vfs)) = (audio.as_mut(), &self.vfs) {
                        audio.refresh_sound_table(index, game, vfs);
                    }
                }
                Consumer::Model(slot) => {
                    let appearance = legacy_model_appearance(game, slot);
                    if let Some(appearance) = appearance {
                        if let Err(error) = self.load_config_model(&appearance) {
                            log::progress(format_args!("cs {index}: model load failed: {error}"));
                        }
                    }
                }
                Consumer::Player(client) => {
                    if let Some(vfs) = &self.vfs {
                        self.local_prediction.refresh_sabers(game, vfs, client);
                    }
                    if let Err(error) = self.apply_clientinfo(client) {
                        log::progress(format_args!("cs {index}: clientinfo failed: {error}"));
                    }
                }
                Consumer::Effect => {
                    self.missile_effects.refresh_config_string(index, game);
                    let name = self
                        .map_effects
                        .refresh_config_string(index, game)
                        .map(str::to_owned);
                    if let Some(name) = name {
                        self.preload_config_effect(&name, audio);
                    }
                }
            }
        });
        self.refresh_npc_actors();
    }

    fn load_config_model(&mut self, appearance: &Appearance) -> Result<(), Box<dyn Error>> {
        if appearance.model.starts_with(['*', '$', '@'])
            || jkr_client::legacy_npc_body_model(&appearance.model)
            || self
                .object_meshes
                .iter()
                .any(|mesh| mesh.appearance == *appearance)
        {
            return Ok(());
        }
        let vfs = self.vfs.as_ref().ok_or("no VFS")?;
        let mut scene = FlattenedScene::default();
        let mut mesh = object_meshes::load_one(vfs, appearance, &mut scene)?;
        let materials = self.world_materials.append_entity_materials(
            &self.device,
            &self.queue,
            vfs,
            &self.shaders,
            &scene.materials,
        )?;
        let placement =
            self.geometry
                .append(&self.device, &self.queue, &scene.vertices, &scene.indices)?;
        shared_geometry::relocate(&mut mesh.draws, &mut [], placement, materials);
        self.world_materials.bind_geometry(&self.geometry);
        self.emitter_model_catalog
            .insert(&appearance.model, self.object_meshes.len());
        self.object_meshes.push(mesh);
        self.object_groups.push(Vec::with_capacity(32));
        log::progress(format_args!(
            "loaded configstring model {} mid-match",
            appearance.model
        ));
        Ok(())
    }

    fn preload_config_effect(&mut self, name: &str, audio: &mut Option<GameAudio>) {
        let Some(vfs) = self.vfs.clone() else { return };
        self.effects.preload(&vfs, [name]);
        if let Err(error) = self.refresh_effect_atlas() {
            log::progress(format_args!("effect {name}: texture load failed: {error}"));
        }
        // Recursive EFX dependencies include rigid models and sound assets.
        for model in effect_assets::required_models(&vfs, std::iter::once(name)) {
            let appearance = Appearance {
                model,
                variant: String::new(),
            };
            if let Err(error) = self.load_config_model(&appearance) {
                log::progress(format_args!("effect {name}: model load failed: {error}"));
            }
        }
        if let Some(audio) = audio {
            let mut paths = Vec::new();
            self.effects.append_sound_paths(&mut paths);
            audio.register_effect_paths(&vfs, paths.iter().map(String::as_str));
        }
    }
}
