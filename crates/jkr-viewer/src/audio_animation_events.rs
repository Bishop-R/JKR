//! Animation-event sounds of posed actors (`CG_TriggerAnimSounds`,
//! `codemp/cgame/cg_players.c:3048-3090`): once per rendered frame each
//! actor's legs and torso frames are checked against its skeleton's
//! `animevents.cfg`, which is what voices the fast-style taunt's saber spins,
//! saber kicks and katas, melee punches and similar moves.
//!
//! The events file is the one beside the skeleton (`models/players/_humanoid/`
//! for every humanoid), as `CG_G2EvIndexForModel` picks it. Tables are read
//! once per skeleton and their sounds registered then; the humanoid one is
//! read with the map's sounds so the first actor does not stall a frame.

use super::GameAudio;
use crate::ActorMesh;
use crate::audio_output::AudioCommand;
use jkr_client::{LegacyAnimationEventTracker, LegacyAnimationEvents};
use jkr_model::AnimationConfig;
use jkr_runtime::World;
use jkr_vfs::VirtualFileSystem;
use std::collections::HashMap;
use std::sync::Arc;

/// Entity numbers an actor may carry (`MAX_GENTITIES`).
const ENTITIES: usize = 1024;

/// The skeleton every humanoid player shares.
const HUMANOID_SKELETON: &str = "models/players/_humanoid/_humanoid";

/// Parsed tables by skeleton and the per-entity frame memory.
pub(super) struct AnimationEventSounds {
    /// By `Glm::animation_name`; `None` for a skeleton without a file.
    tables: HashMap<String, Option<Arc<LegacyAnimationEvents>>>,
    tracker: LegacyAnimationEventTracker,
}

impl Default for AnimationEventSounds {
    fn default() -> Self {
        Self {
            tables: HashMap::with_capacity(8),
            tracker: LegacyAnimationEventTracker::new(ENTITIES),
        }
    }
}

impl AnimationEventSounds {
    /// Forget the tables, so a new map's content is read again.
    pub(super) fn clear(&mut self) {
        self.tables.clear();
    }
}

/// Read `skeleton`'s `animevents.cfg` (and its includes) against `config`.
fn read_table(
    vfs: &VirtualFileSystem,
    skeleton: &str,
    config: &AnimationConfig,
) -> Option<LegacyAnimationEvents> {
    let directory = skeleton
        .rsplit_once('/')
        .map_or("", |(directory, _)| directory);
    let read = |path: &str| {
        let bytes = vfs.read(path).ok().flatten()?.bytes;
        Some(String::from_utf8_lossy(&bytes).into_owned())
    };
    let text = read(&format!("{directory}/animevents.cfg"))?;
    Some(LegacyAnimationEvents::parse(&text, config, &mut |name| {
        read(&format!("models/players/{name}/animevents.cfg"))
    }))
}

impl GameAudio {
    /// Read the humanoid skeleton's events and register their sounds while a
    /// map loads.
    pub(crate) fn preload_animation_events(&mut self, vfs: &VirtualFileSystem) {
        self.animation_events.clear();
        let directory = HUMANOID_SKELETON
            .rsplit_once('/')
            .map_or("", |(directory, _)| directory);
        let config = vfs
            .read(&format!("{directory}/animation.cfg"))
            .ok()
            .flatten()
            .and_then(|asset| AnimationConfig::parse(&asset.bytes).ok());
        if let Some(config) = config {
            self.load_animation_events(vfs, HUMANOID_SKELETON, &config);
        }
    }

    /// Parse, register and remember one skeleton's table.
    fn load_animation_events(
        &mut self,
        vfs: &VirtualFileSystem,
        skeleton: &str,
        config: &AnimationConfig,
    ) -> Option<Arc<LegacyAnimationEvents>> {
        let table = read_table(vfs, skeleton, config).map(Arc::new);
        if let Some(table) = &table {
            table.for_each_sound_path(|path| {
                let _ = self.register_vfs_async(vfs, path);
            });
        }
        self.animation_events
            .tables
            .insert(skeleton.to_owned(), table.clone());
        table
    }

    /// Start the sounds whose frames each posed actor reached since the last
    /// rendered frame. `local` is the local player's entity number, whose
    /// sounds play without falloff.
    pub(crate) fn play_animation_events(
        &mut self,
        meshes: &[ActorMesh],
        world: &World,
        local: Option<u16>,
        presentation_time: i64,
    ) {
        self.animation_events.tracker.begin_frame();
        for mesh in meshes {
            if mesh.corpse_pool || mesh.animator.requested.is_none() {
                continue;
            }
            let Some(entity_id) = mesh.entity_id else {
                continue;
            };
            let Some(entity) = entity_id
                .get()
                .checked_sub(1)
                .and_then(|number| u16::try_from(number).ok())
            else {
                continue;
            };
            let skeleton = mesh.preview.mesh.animation_name.as_str();
            let table = match self.animation_events.tables.get(skeleton) {
                Some(table) => table.clone(),
                None => match self.legacy_vfs.clone() {
                    Some(vfs) => self.load_animation_events(&vfs, skeleton, &mesh.preview.config),
                    None => None,
                },
            };
            let Some(table) = table else {
                continue;
            };
            let origin = (Some(entity) != local).then(|| {
                world.entity(entity_id).map_or([0.0; 3], |entity| {
                    entity.sample(presentation_time).translation
                })
            });
            let frames = mesh
                .animator
                .track_frames(&mesh.preview.animation, presentation_time);
            let (tracker, handles, output) = (
                &mut self.animation_events.tracker,
                &self.handles,
                &mut self.output,
            );
            for (torso, track) in frames.into_iter().enumerate() {
                let Some((clip, frame)) = track else {
                    continue;
                };
                tracker.observe(
                    usize::from(entity),
                    torso == 1,
                    clip,
                    frame,
                    &table,
                    &mesh.preview.config,
                    |sound| {
                        if let Some(handle) = handles.get(sound.path) {
                            output.send(AudioCommand::Play(*handle, sound.request(entity, origin)));
                        }
                    },
                );
            }
        }
    }
}
