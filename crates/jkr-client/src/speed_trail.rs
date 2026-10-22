//! Force Speed's afterimages (`cg_speedTrail`; `CG_Player`, `cg_players.c:9326-9390` and
//! `10783-10794`): while a player runs with Force Speed, two see-through copies of it trail
//! behind, the first a speed-scaled step back along the way it came (alpha 100), the
//! second as far again behind the first (alpha 50).

/// `SPEED_TRAIL_DISTANCE`.
const STEP: f64 = 6.0;
/// The copies' alphas.
const ALPHAS: [u8; 2] = [100, 50];

/// One afterimage: where it stands and its alpha.
pub type Afterimage = Option<([f32; 3], u8)>;

/// `centity_t::frame_minus1`/`frame_minus2` and whether each was set the frame before.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpeedTrail {
    minus: [[f32; 3]; 2],
    fresh: [bool; 2],
    /// The frame these answer, and their images, for a second look in the same frame.
    at: i64,
    images: [Afterimage; 2],
}

impl SpeedTrail {
    /// One frame at `time` for a player drawn at `origin` moving at `delta` (`pos.trDelta`);
    /// `speeding` holds while it has Force Speed, is not fading from a trick and
    /// `cg_speedTrail` is on. The afterimages to draw this frame.
    pub fn frame(
        &mut self,
        speeding: bool,
        origin: [f32; 3],
        delta: [f32; 3],
        time: i64,
    ) -> [Afterimage; 2] {
        if self.at == time && time != 0 {
            return self.images;
        }
        self.at = time;
        if !speeding {
            self.fresh = [false; 2];
        }
        let mut images = [None; 2];
        if self.fresh[0] || self.fresh[1] {
            // `distVelBase`, an int: six units for every 250 of speed.
            let length = (delta.iter().map(|axis| axis * axis).sum::<f32>()).sqrt();
            let step = (STEP * (f64::from(length) * 0.004)) as i32 as f32;
            if self.fresh[0] {
                self.minus[0] = step_from(origin, self.minus[0], step);
                images[0] = Some((self.minus[0], ALPHAS[0]));
            }
            if self.fresh[1] {
                self.minus[1] = step_from(self.minus[0], self.minus[1], step);
                images[1] = Some((self.minus[1], ALPHAS[1]));
            }
        }
        // The end of `CG_Player`: the trail moves up a place behind this frame's origin.
        self.minus[1] = self.minus[0];
        self.fresh[1] = self.fresh[0];
        self.minus[0] = origin;
        self.fresh[0] = true;
        self.images = images;
        images
    }
}

/// `from` moved `step` units towards `toward` (`VectorNormalize` leaves no way as none).
fn step_from(from: [f32; 3], toward: [f32; 3], step: f32) -> [f32; 3] {
    let mut way: [f32; 3] = std::array::from_fn(|axis| toward[axis] - from[axis]);
    let length = (way.iter().map(|axis| axis * axis).sum::<f32>()).sqrt();
    if length != 0.0 {
        let inverse = 1.0 / length;
        way = way.map(|axis| axis * inverse);
    }
    std::array::from_fn(|axis| from[axis] + way[axis] * step)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_copies_trail_a_speed_scaled_step_behind_along_the_way_come() {
        let mut trail = SpeedTrail::default();
        let delta = [700.0, 0.0, 0.0];
        assert_eq!(trail.frame(true, [0.0; 3], delta, 10), [None, None]);
        // 700 u/s: 6 * 2.8 = 16.8, cut to 16.
        let images = trail.frame(true, [10.0, 0.0, 0.0], delta, 26);
        assert_eq!(images[0], Some(([-6.0, 0.0, 0.0], 100)));
        assert_eq!(images[1], None);
        let images = trail.frame(true, [20.0, 0.0, 0.0], delta, 42);
        assert_eq!(images[0], Some(([4.0, 0.0, 0.0], 100)));
        assert_eq!(images[1], Some(([-12.0, 0.0, 0.0], 50)));
        // A second look in the same frame sees the same images.
        assert_eq!(trail.frame(true, [99.0; 3], delta, 42), images);
        // Without the power the trail stops, and starts over a frame late.
        assert_eq!(
            trail.frame(false, [30.0, 0.0, 0.0], delta, 58),
            [None, None]
        );
        let images = trail.frame(true, [40.0, 0.0, 0.0], delta, 74);
        assert_eq!((images[0].is_some(), images[1].is_some()), (true, false));
    }
}
