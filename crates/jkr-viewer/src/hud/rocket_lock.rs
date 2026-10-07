//! The rocket launcher's lock round its target ([`jkr_client::rocket_lock`]): stock's red
//! wedges closing round a point half the lock's edge above the target's origin, and the
//! lock picture pulsing over them once it holds. The pictures are the radar's; stock draws
//! the lock twice a frame and so does this, square and sized by the screen's height.
//!
//! The wedges are blended over the view by their alpha, a tenth more for each
//! (`0.1 * i + 0.2`), although their shader adds them: that is how stock's rotated pictures
//! reach the screen — Raven's `RB_RotatePic` drew them in the 2D blend state, ignoring the
//! shader's `blendFunc`, and both of TaystJK's renderers show them so (under a wedge the
//! view's green and blue fall, never rise). The lock picture is added, as its shader says.

use super::*;
use jkr_client::rocket_lock::{self, LockInput, LockState};
use jkr_ui::{DrawList, Rect};
use std::cell::Cell;

/// `ps.rocketLockIndex`, `ps.rocketLockTime`; `GT_SIEGE`.
const PS_ROCKET_LOCK_INDEX: usize = 24;
const PS_ROCKET_LOCK_TIME: usize = 79;
const GT_SIEGE: i32 = 7;

/// One frame's lock, in pixels.
#[derive(Clone, Copy)]
struct Reticle {
    /// Where the target's origin falls, and the lock's edge.
    point: [f32; 2],
    edge: f32,
    steps: u8,
    locked: bool,
    /// The lock picture's grey.
    grey: f32,
}

/// The lock between frames.
#[derive(Default)]
pub(super) struct State {
    lock: LockState,
    reticle: Option<Reticle>,
    /// The sound the lock started, and where, for the audio's next frame.
    sound: Cell<Option<(&'static str, [f32; 3])>>,
}

/// What one frame of the lock reads.
pub(crate) struct RocketLockInputs<'a> {
    pub(crate) snapshot: &'a jkr_protocol::Snapshot,
    pub(crate) game: &'a GameState,
    /// Where the locked entity is drawn now (`cent->lerpOrigin`), when it is.
    pub(crate) target: Option<glam::Vec3>,
    pub(crate) camera: identification::Camera,
    /// `cg.time`.
    pub(crate) time: i32,
}

impl HudOverlay {
    /// One frame of the lock (`CG_DrawRocketLocking`), read from the snapshot's player.
    pub(crate) fn update_rocket_lock(&mut self, input: RocketLockInputs<'_>) {
        let state = &mut self.rocket_lock;
        state.reticle = None;
        let player = &input.snapshot.player;
        let index = player.raw_field(PS_ROCKET_LOCK_INDEX).unwrap_or(1_023) as u16;
        let gametype = input
            .game
            .config_string(0)
            .and_then(|info| jkr_client::LegacyClientInfo::new(info).integer("g_gametype"))
            .unwrap_or(0);
        let lock = LockInput {
            index,
            time: f32::from_bits(player.raw_field(PS_ROCKET_LOCK_TIME).unwrap_or(0)),
            now: input.time,
            siege: gametype == GT_SIEGE,
            spectator: rocket_lock::spectating(input.game, player.client_num()),
            teammate: rocket_lock::teammate(input.game, input.snapshot, player.client_num(), index),
        };
        let projected = input
            .target
            .and_then(|target| Some((input.camera.project_ahead(target)?, target)));
        let Some(frame) = state.lock.frame(&lock, projected.is_some()) else {
            return;
        };
        let Some((point, target)) = projected else {
            return;
        };
        let distance = target.distance(input.camera.eye);
        state.reticle = Some(Reticle {
            point,
            edge: rocket_lock::edge(distance) * input.camera.viewport[1] / 480.0,
            steps: frame.steps,
            locked: frame.locked,
            grey: rocket_lock::pulse(input.time),
        });
        if let Some(sound) = frame.sound {
            state.sound.set(Some((sound, target.to_array())));
        }
    }

    /// The sound the lock started and where, once, for the audio's frame.
    pub(crate) fn take_lock_sound(&self) -> Option<(&'static str, [f32; 3])> {
        self.rocket_lock.sound.take()
    }
}

impl State {
    /// The wedges, eight a full turn apart, and the lock picture once it holds — twice.
    pub(super) fn emit(&self, list: &mut DrawList, radar: &radar::State) {
        let Some(reticle) = self.reticle else {
            return;
        };
        let [wedge, lock] = radar.lock_pictures();
        let wedge = wedge.map(|layers| layers.map(|layer| layer.map(radar::Layer::blended)));
        let edge = reticle.edge;
        let (x, y) = (reticle.point[0], reticle.point[1] + edge * 0.5);
        for _ in 0..rocket_lock::DRAWS {
            if let Some(wedge) = &wedge {
                for step in 0..reticle.steps {
                    let degrees = f32::from(step) * 45.0;
                    let rect =
                        turned_about_corner(Rect::new(x - edge, y - edge, edge, edge), degrees);
                    let alpha = rocket_lock::wedge_alpha(step);
                    radar::draw(list, wedge, rect, degrees, [1.0, 0.0, 0.0, alpha], 0.0);
                }
            }
            if reticle.locked
                && let Some(lock) = &lock
            {
                let grey = reticle.grey;
                let rect = Rect::new(x - edge, y - edge * 2.0, edge * 2.0, edge * 2.0);
                radar::draw(list, lock, rect, 0.0, [grey, grey, grey, 1.0], 0.0);
            }
        }
    }
}

/// `CG_DrawRotatePic`'s picture as one turned about its own centre: rd-vanilla turns it
/// about its top-right corner (`RB_RotatePic`).
fn turned_about_corner(rect: Rect, degrees: f32) -> Rect {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let (dx, dy) = (-rect.width * 0.5, rect.height * 0.5);
    let centre = [
        rect.x + rect.width + dx * cos - dy * sin,
        rect.y + dx * sin + dy * cos,
    ];
    Rect::new(
        centre[0] - rect.width * 0.5,
        centre[1] - rect.height * 0.5,
        rect.width,
        rect.height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wedge_turns_about_its_top_right_corner() {
        let rect = Rect::new(10.0, 20.0, 8.0, 8.0);
        let still = turned_about_corner(rect, 0.0);
        assert_eq!((still.x, still.y), (10.0, 20.0));
        // A half turn puts it up and right of the corner it turned about.
        let half = turned_about_corner(rect, 180.0);
        assert!((half.x - 18.0).abs() < 1e-4 && (half.y - 12.0).abs() < 1e-4);
    }
}
