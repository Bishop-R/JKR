//! The last leg of a join: once the gate stands fully open and the server
//! world is built, the camera leaves the browser vantage and glides through
//! the doorway into the map being joined. The world is adopted only after
//! the crossing, so the in-game cut happens with the destination already on
//! screen — and because it was built before the glide, nothing competes with
//! the render loop for the GPU on the way through, and the camera never
//! stands waiting beyond the doorway.

use super::Sample;
use crate::portal::Frame;
use jkr_ui::{Easing, Tween};

/// How long the glide from the browser vantage to beyond the doorway takes.
const PASSAGE_MILLIS: u32 = 900;
/// How far past the doorway plane the glide ends, so the crossing is
/// unambiguous and the menu world is fully behind the camera.
const PASSAGE_BEYOND: f32 = 48.0;

/// The glide through the map's gate, if the map has one.
pub(crate) struct Passage {
    doorway: Option<Frame>,
    /// 0 = at the browser vantage, 1 = beyond the doorway.
    progress: Tween,
}

impl Passage {
    pub(crate) fn new(doorway: Option<Frame>) -> Self {
        Self {
            doorway,
            progress: Tween::settled(0.0),
        }
    }

    /// Start the glide once the gate is fully open, the server world stands
    /// ready to be adopted and the join still wants it; glide back when the
    /// join is abandoned mid-way. A glide already under way is never held
    /// back by `world_ready`.
    pub(crate) fn drive(
        &mut self,
        wanted: bool,
        gate_fully_open: bool,
        world_ready: bool,
        millis: u64,
    ) {
        if self.doorway.is_none() {
            return;
        }
        let may_start = world_ready || self.under_way(millis);
        let target = if wanted && gate_fully_open && may_start {
            1.0
        } else {
            0.0
        };
        if self.progress.target() != target {
            let remaining = (target - self.progress.sample(millis)).abs();
            let duration = (PASSAGE_MILLIS as f32 * remaining) as u32;
            crate::log::progress(format_args!(
                "gate passage: toward {target} over {duration} ms at {millis} ms \
                 (wanted={wanted}, gate fully open={gate_fully_open}, \
                 world ready={world_ready})"
            ));
            self.progress
                .retarget(target, millis, duration, Easing::SmoothStep);
        }
    }

    /// Back to the browser vantage at once, no glide.
    pub(crate) fn reset(&mut self) {
        self.progress = Tween::settled(0.0);
    }

    /// 0 = not under way, 1 = beyond the doorway.
    pub(crate) fn progress(&self, millis: u64) -> f32 {
        self.progress.sample(millis)
    }

    /// Whether the camera has left the browser vantage at all: the gate
    /// must stay open while it is anywhere along the glide.
    pub(crate) fn under_way(&self, millis: u64) -> bool {
        self.progress(millis) > 0.0
    }

    /// Whether the glide is finished: the camera stands beyond the doorway
    /// (or the map has no gate, in which case there is nothing to wait for).
    pub(crate) fn crossed(&self, millis: u64) -> bool {
        self.doorway.is_none() || self.progress(millis) >= 1.0
    }

    /// The camera along the glide: `parked` at 0, beyond the doorway at 1,
    /// looking straight through it.
    pub(crate) fn pose(&self, parked: Sample, millis: u64) -> Sample {
        let Some(doorway) = self.doorway else {
            return parked;
        };
        let t = self.progress(millis);
        if t <= 0.0 {
            return parked;
        }
        // The doorway frame sits on the floor; the camera keeps its height.
        let end = (doorway.origin + doorway.forward() * PASSAGE_BEYOND).with_z(parked.origin.z);
        Sample {
            origin: parked.origin.lerp(end, t),
            yaw: parked.yaw + shortest_turn(parked.yaw, doorway.yaw) * t,
            pitch: parked.pitch * (1.0 - t),
        }
    }
}

/// Signed radians from `from` to `to` the short way round.
fn shortest_turn(from: f32, to: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (to - from + PI).rem_euclid(TAU) - PI
}
