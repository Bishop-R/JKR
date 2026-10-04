//! Multiplayer `animevents.cfg` audio cues, independent of movement and the wire.
use jkr_model::{AnimationConfig, AnimationSequence};
use jkr_vfs::VirtualFileSystem;

pub mod footsteps;
mod parser;

/// One model-authored audio action. Paths are normalized once during loading.
#[derive(Clone, Debug)]
pub enum Cue {
    /// A random registered sound on a legacy channel.
    Sound { paths: Vec<String>, channel: u8 },
    /// Ground contact from a named Ghoul2 foot bolt.
    Footstep { right: bool, heavy: bool },
}

/// Absolute animation frame and probability (`0` means always).
#[derive(Clone, Debug)]
pub struct Event {
    /// Absolute GLA frame, including the animation's configured first frame.
    pub frame: i32,
    /// Percentage chance; zero means always, as in codemp.
    pub probability: u8,
    /// Audio-only payload; gameplay animation events are not executed here.
    pub cue: Cue,
    kind: String,
    order: usize,
}

/// Upper and lower event tables from one mounted model's configuration.
#[derive(Default, Debug)]
pub struct Events {
    /// Legs first, torso second; sorted by absolute keyframe.
    pub tracks: [Vec<Event>; 2],
}

impl Events {
    /// Load the model's event file, falling back to its skeleton's shared table.
    /// Includes remain within the mounted VFS and are bounded against cycles.
    pub fn load(
        vfs: &VirtualFileSystem,
        model: &str,
        skeleton: &str,
        config: &AnimationConfig,
    ) -> Self {
        let mut result = Self::default();
        let model_path = format!("{model}/animevents.cfg");
        let path = if vfs.contains(&model_path).unwrap_or(false) {
            model_path
        } else {
            format!("{skeleton}/animevents.cfg")
        };
        parser::load(&mut result, vfs, &path, config, &mut Vec::new());
        for track in &mut result.tracks {
            track.sort_by_key(|event| event.frame);
        }
        result
    }

    /// Sound assets to read on the appearance loader, before frame playback.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.tracks
            .iter()
            .flatten()
            .flat_map(|event| match &event.cue {
                Cue::Sound { paths, .. } => paths.as_slice(),
                Cue::Footstep { .. } => &[],
            })
            .map(String::as_str)
    }
}

/// Small replay-stable random stream; advances per occurrence, never per frame.
#[derive(Clone, Debug)]
pub struct Random(u64);
impl Default for Random {
    fn default() -> Self {
        Self(0x853c49e6748fea9b)
    }
}
impl Random {
    /// Select a bounded variant without allocation or shared state.
    pub fn index(&mut self, count: usize) -> usize {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545f4914f6cdd1d) >> 32) as usize % count.max(1)
    }
}

/// Per-actor event latch. A stopped frame cannot replay a cue.
#[derive(Default)]
pub struct Cursor {
    previous: [Option<(usize, i32)>; 2],
    random: Random,
}
impl Cursor {
    /// Forget an absent actor, teleport or backwards seek without firing old cues.
    pub fn reset(&mut self) {
        self.previous = [None; 2];
    }

    /// Visit only nearby keyframes, matching codemp's three-frame skip tolerance.
    pub fn advance(
        &mut self,
        events: &Events,
        config: &AnimationConfig,
        frames: [Option<(usize, i32)>; 2],
        mut emit: impl FnMut(&Cue, usize),
    ) {
        for (track, current) in frames.into_iter().enumerate() {
            let previous = std::mem::replace(&mut self.previous[track], current);
            let (Some((clip, frame)), Some((old_clip, old))) = (current, previous) else {
                continue;
            };
            if frame == old {
                continue;
            }
            let sequence =
                crate::legacy_animation_name(clip).and_then(|name| config.get_exact(name));
            let entries = &events.tracks[track];
            let mut selected = [0usize; 42];
            let mut count = 0;
            // Every eligible event is within three frames of one endpoint.
            for (range, (lo, hi)) in [
                (old.saturating_sub(3), old.saturating_add(3)),
                (frame.saturating_sub(3), frame.saturating_add(3)),
            ]
            .into_iter()
            .enumerate()
            {
                let start = entries.partition_point(|event| event.frame < lo);
                for (offset, event) in entries[start..]
                    .iter()
                    .take_while(|event| event.frame <= hi)
                    .enumerate()
                {
                    if range == 1 && event.frame.abs_diff(old) <= 3 {
                        continue;
                    }
                    if matches_frame(event.frame, old, frame, clip == old_clip, sequence) {
                        // Two seven-frame intervals, at most three supported event kinds per key.
                        selected[count] = start + offset;
                        count += 1;
                    }
                }
            }
            selected[..count].sort_unstable_by_key(|&i| entries[i].order);
            for &i in &selected[..count] {
                let event = &entries[i];
                if event.probability == 0 || self.random.index(100) < usize::from(event.probability)
                {
                    emit(&event.cue, self.random.index(1 << 16));
                }
            }
        }
    }
}

// Separate predicate mirrors CG_PlayerAnimEvents and is reference-checkable.
/// Whether one keyframe is reached by a changed rendered animation frame.
pub fn matches_frame(
    key: i32,
    old: i32,
    frame: i32,
    same: bool,
    sequence: Option<&AnimationSequence>,
) -> bool {
    if frame == old {
        return false;
    }
    if key == frame {
        return true;
    }
    if !same || old.abs_diff(frame) <= 1 || (old.abs_diff(key) > 3 && frame.abs_diff(key) > 3) {
        return false;
    }
    let Some(sequence) = sequence else {
        return false;
    };
    let inside = key >= sequence.first_frame as i32
        && key < (sequence.first_frame + sequence.frame_count) as i32;
    if sequence.frames_per_second < 0.0 {
        old > key && (frame < key || (sequence.loop_frame != -1 && inside && frame > old))
    } else {
        old < key && (frame > key || (sequence.loop_frame != -1 && inside && frame < old))
    }
}
