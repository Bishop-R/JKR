//! Asynchronous construction of legacy gamestate sound tables.

use super::*;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

pub(super) struct PreparedLegacyAudio {
    pub(super) adapter: LegacySoundAdapter,
    pub(super) sounds: Vec<(String, SoundHandle, Box<[u8]>)>,
    pub(super) elapsed: Duration,
    cue_handles: [Option<SoundHandle>; 9],
}

pub(super) struct LegacyLoadTask {
    receiver: Receiver<Result<PreparedLegacyAudio, String>>,
}

impl LegacyLoadTask {
    pub(super) fn start(vfs: Arc<VirtualFileSystem>, game_state: GameState) -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        thread::Builder::new()
            .name("jkr-legacy-audio-load".into())
            .spawn(move || {
                let started = Instant::now();
                let result = (|| {
                    let mut sounds = Vec::with_capacity(512);
                    let mut next_handle = 0;
                    let adapter = LegacySoundAdapter::new(&game_state, &vfs, |path, bytes| {
                        let handle = SoundHandle(next_handle);
                        next_handle = next_handle.wrapping_add(1);
                        sounds.push((path.to_owned(), handle, bytes.into()));
                        Some(handle)
                    });
                    for path in feedback::KILL_SOUNDS {
                        if let Ok(Some(asset)) = vfs.read(path) {
                            let handle = SoundHandle(next_handle);
                            next_handle = next_handle.wrapping_add(1);
                            sounds.push((path.to_owned(), handle, asset.bytes.into()));
                        } else {
                            crate::log::progress(format_args!("optional kill cue missing: {path}"));
                        }
                    }
                    let cue_handles = transitions::CUES.map(|requested| {
                        let Some((path, asset)) = assets::resolve_sound(&vfs, requested) else {
                            crate::log::progress(format_args!(
                                "optional feedback missing: {requested}"
                            ));
                            return None;
                        };
                        let handle = SoundHandle(next_handle);
                        next_handle = next_handle.wrapping_add(1);
                        sounds.push((path, handle, asset.bytes.into()));
                        Some(handle)
                    });
                    Ok(PreparedLegacyAudio {
                        adapter,
                        sounds,
                        cue_handles,
                        elapsed: started.elapsed(),
                    })
                })();
                let _ = sender.send(result);
            })
            .expect("legacy audio loader thread creation failed");
        Self { receiver }
    }

    pub(super) fn poll(&self) -> Option<Result<PreparedLegacyAudio, String>> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                Some(Err("legacy audio loader stopped unexpectedly".into()))
            }
        }
    }
}

impl GameAudio {
    /// Stop old-map audio before new snapshots can reach the old configstring resolver.
    pub(crate) fn begin_map_change(&mut self) {
        self.transitions.reset();
        self.output.send(AudioCommand::StopEffects);
        self.output.stop_music();
        self.legacy = None;
        self.legacy_load = None;
        self.sound_table_refresh = None;
        self.deferred_snapshots.clear();
        self.map_music = None;
        self.music_value = None;
        self.dynamic = None;
    }

    /// Rebuild legacy tables after a mid-session gamestate/map change.
    pub(crate) fn install_gamestate(
        &mut self,
        game_state: &GameState,
        vfs: Arc<VirtualFileSystem>,
    ) {
        self.begin_map_change();
        self.sound_table_refresh = Some(SoundTableRefresh::new(game_state));
        self.legacy_vfs = Some(Arc::clone(&vfs));
        self.deferred_snapshots.clear();
        self.legacy_load = Some(LegacyLoadTask::start(vfs, game_state.clone()));
    }

    /// Integrate worker handles into the current bank before replaying queued events.
    pub(super) fn poll_legacy_load(&mut self) {
        let Some(result) = self.legacy_load.as_ref().and_then(LegacyLoadTask::poll) else {
            return;
        };
        self.legacy_load = None;
        match result {
            Ok(mut prepared) => {
                let elapsed = prepared.elapsed;
                let mut remap = Vec::with_capacity(prepared.sounds.len());
                for (path, _, bytes) in prepared.sounds {
                    let key = path.to_ascii_lowercase();
                    let extension = path.rsplit('.').next().unwrap_or("wav");
                    let handle = if let Some(handle) = self.handles.get(&key) {
                        *handle
                    } else {
                        let handle = SoundHandle(self.next_handle);
                        self.next_handle = self.next_handle.wrapping_add(1);
                        self.output.decode(handle, &bytes, extension);
                        self.handles.insert(key, handle);
                        handle
                    };
                    remap.push(handle);
                }
                prepared
                    .adapter
                    .remap_sound_handles(|handle| remap[handle.0 as usize]);
                self.transitions.handles =
                    prepared.cue_handles.map(|h| h.map(|h| remap[h.0 as usize]));
                self.legacy = Some(prepared.adapter);
                crate::log::progress(format_args!(
                    "legacy audio tables ready in {:.1}ms",
                    elapsed.as_secs_f64() * 1_000.0
                ));
                self.play_deferred_snapshots();
                self.apply_pending_sound_tables();
            }
            Err(error) => {
                crate::log::progress(format_args!("legacy audio load failed: {error}"));
            }
        }
    }
}
