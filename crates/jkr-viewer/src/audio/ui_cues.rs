//! Menu sound cues: hover, click and back posted by every menu canvas as it
//! routes input, the stage model's saber throw and catch posted by the
//! menu stage, all played once per frame by the audio owner.
//!
//! The posters live inside screens that have no audio access, so cues go
//! through one process-wide atomic mailbox. A cue is a bit, not a queue:
//! ten hovers in one frame play one tick, which is what a menu wants.

use super::GameAudio;
use jkr_audio::{ChannelId, SourceId};
use std::sync::atomic::{AtomicU8, Ordering};

/// One interface sound; each maps to a sound file below.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Cue {
    /// The pointer or keyboard focus arrived on a new control.
    Hover = 1,
    /// A control was activated.
    Click = 2,
    /// A screen was cancelled or closed.
    Back = 4,
    /// The stage model threw its saber (the Saber tab opened).
    Throw = 8,
    /// The thrown saber landed back in the stage model's hand.
    Catch = 16,
}

/// `(cue, sound path, volume)` — the game's own sounds, looked up through
/// the VFS like every other sound. Throw and catch are what a thrown saber
/// plays in game: the flight loop `saberspin.wav` (`codemp/game/w_saber.c`
/// `saberent->s.loopSound = saberSpinSound`) and `saber_catch.wav` on
/// return (`saberCheckRadiusDamage` → `G_Sound(saberent, CHAN_AUTO, …)`).
pub(crate) const CUE_SOUNDS: [(Cue, &str, f32); 5] = [
    (Cue::Hover, "sound/interface/menuroam.mp3", 0.6),
    (Cue::Click, "sound/interface/button1.mp3", 0.9),
    (Cue::Back, "sound/interface/esc.mp3", 0.9),
    (Cue::Throw, "sound/weapons/saber/saberspin.wav", 0.8),
    (Cue::Catch, "sound/weapons/saber/saber_catch.wav", 0.8),
];

/// Interface cues never share a channel with world sounds.
const UI_SOURCE: SourceId = SourceId(u32::MAX);

static PENDING: AtomicU8 = AtomicU8::new(0);

/// Post `cue` for the next frame's playback.
pub(crate) fn post(cue: Cue) {
    PENDING.fetch_or(cue as u8, Ordering::Relaxed);
}

/// Play every cue posted since the last call.
pub(crate) fn play_pending(audio: &mut GameAudio) {
    let pending = PENDING.swap(0, Ordering::Relaxed);
    if pending == 0 {
        return;
    }
    for (index, (cue, path, volume)) in CUE_SOUNDS.iter().enumerate() {
        if pending & *cue as u8 != 0 {
            audio.play_local(path, *volume, UI_SOURCE, ChannelId(index as u32));
        }
    }
}
