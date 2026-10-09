//! Fixed simulation ticks, independent of UDP wakeups and operating-system jitter.
//! The wall clock still drives transport timeouts. Game/snapshot time advances
//! only by complete simulation steps, as in codemp `SV_Frame`.

/// Accumulates elapsed wall time without passing fractional frames to gameplay.
#[derive(Debug, Default)]
pub struct FrameClock {
    wall_millis: i32,
    residual: i64,
    server_millis: i32,
    /// The fraction of a millisecond a `timescale` left over.
    scaled_fraction: f64,
}

impl FrameClock {
    /// Account for a monotonic wall-clock observation, the time passing `timescale` times
    /// as fast (`Com_ModifyMsec`: 0 is the normal speed; a negative scale stops it here).
    /// Frames keep their length; under a scale more or fewer of them run. Repeated
    /// observations add no time.
    pub fn observe(&mut self, wall_millis: i32, timescale: f32) {
        let elapsed = wall_millis.wrapping_sub(self.wall_millis).max(0);
        self.wall_millis = wall_millis;
        if timescale == 1.0 || timescale == 0.0 {
            self.residual += i64::from(elapsed);
            return;
        }
        self.scaled_fraction += f64::from(elapsed) * f64::from(timescale.max(0.0));
        let whole = self.scaled_fraction.floor();
        self.scaled_fraction -= whole;
        self.residual += whole as i64;
    }

    /// Time of the last complete simulation tick, also used between ticks.
    pub fn server_time(&self) -> i32 {
        self.server_millis
    }

    /// Whether a whole frame is ready. Frame lengths must be positive.
    pub fn ready(&self, frame_millis: i32) -> bool {
        self.residual >= i64::from(frame_millis.max(1))
    }

    /// Consume one complete tick. Call again to catch up after a delayed wakeup.
    pub fn advance(&mut self, frame_millis: i32) -> Option<i32> {
        let frame_millis = frame_millis.max(1);
        if !self.ready(frame_millis) {
            return None;
        }
        self.residual -= i64::from(frame_millis);
        self.server_millis = self.server_millis.wrapping_add(frame_millis);
        Some(self.server_millis)
    }

    /// A stopped server does not accumulate a gameplay backlog before the next map.
    pub fn discard_pending(&mut self) {
        self.residual = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Under `timescale` 2 a second of wall time runs two seconds of frames; at 0.25 a
    /// quarter, the fractions carried over; 0 is the normal speed.
    #[test]
    fn timescale_runs_time_faster_or_slower() {
        let frames = |timescale: f32| {
            let mut clock = FrameClock::default();
            for wall in 1..=1_000 {
                clock.observe(wall, timescale);
            }
            std::iter::from_fn(|| clock.advance(50)).count()
        };
        assert_eq!(frames(1.0), 20);
        assert_eq!(frames(2.0), 40);
        assert_eq!(frames(0.25), 5);
        assert_eq!(frames(0.0), 20);
        assert_eq!(frames(-1.0), 0);
    }
}
