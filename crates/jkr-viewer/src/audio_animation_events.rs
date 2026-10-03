//! Animation-event sounds of posed actors (`CG_TriggerAnimSounds`,
//! `codemp/cgame/cg_players.c:3048-3090`): once per rendered frame each
//! actor's legs and torso frames are checked against its skeleton's
//! `animevents.cfg`, which is what voices the fast-style taunt's saber spins,
//! saber kicks and katas, melee punches and similar moves.
//!
//! Each actor's table is parsed with its model
//! ([`crate::player_assets::PlayerPreview::animation_events`]) and its sounds
//! registered when the actor list changes (map load, clientinfo and NPC
//! refresh), so the per-frame pass only compares frames and starts sounds:
//! it never reads or parses a file.

use super::GameAudio;
use crate::ActorMesh;
use crate::audio_output::AudioCommand;
use jkr_client::{LegacyAnimationEventTracker, LegacyAnimationEvents};
use jkr_runtime::World;
use jkr_vfs::VirtualFileSystem;
use std::sync::Arc;

/// Entity numbers an actor may carry (`MAX_GENTITIES`).
const ENTITIES: usize = 1024;

/// Tables whose sounds are registered and the per-entity frame memory.
pub(super) struct AnimationEventSounds {
    /// Held so a table's address stays unique while it is listed; one per
    /// distinct skeleton, so a handful at most.
    registered: Vec<Arc<LegacyAnimationEvents>>,
    tracker: LegacyAnimationEventTracker,
}

impl Default for AnimationEventSounds {
    fn default() -> Self {
        Self {
            registered: Vec::with_capacity(8),
            tracker: LegacyAnimationEventTracker::new(ENTITIES),
        }
    }
}

impl AnimationEventSounds {
    fn is_registered(&self, table: &Arc<LegacyAnimationEvents>) -> bool {
        self.registered
            .iter()
            .any(|registered| Arc::ptr_eq(registered, table))
    }
}

impl GameAudio {
    /// Forget the registered tables while a map loads; the new map's actors
    /// are registered again by [`Self::register_animation_events`].
    pub(crate) fn clear_animation_events(&mut self) {
        self.animation_events.registered.clear();
    }

    /// Register the sounds of every actor table not seen yet. Without new
    /// tables this compares a few pointers per actor and reads nothing.
    pub(crate) fn register_animation_events(
        &mut self,
        vfs: &VirtualFileSystem,
        meshes: &[ActorMesh],
    ) {
        for mesh in meshes {
            let Some(table) = &mesh.preview.animation_events else {
                continue;
            };
            if self.animation_events.is_registered(table) {
                continue;
            }
            table.for_each_sound_path(|path| {
                let _ = self.register_vfs_async(vfs, path);
            });
            self.animation_events.registered.push(Arc::clone(table));
        }
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
            let Some(table) = mesh.preview.animation_events.as_deref() else {
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
                    table,
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
