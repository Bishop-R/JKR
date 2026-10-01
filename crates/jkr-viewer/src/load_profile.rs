//! One-shot world-install profiling with monotonic sub-phase timestamps.

use std::time::Instant;

/// Prints individual and cumulative load durations without affecting frame hot paths.
pub(crate) struct LoadProfile {
    started: Instant,
    previous: Instant,
}

impl LoadProfile {
    pub(crate) fn start() -> Self {
        let now = Instant::now();
        Self {
            started: now,
            previous: now,
        }
    }

    pub(crate) fn mark(&mut self, phase: &str) {
        let now = Instant::now();
        crate::log::progress(format_args!(
            "world load phase {phase}: phase={:.1}ms cumulative={:.1}ms",
            now.duration_since(self.previous).as_secs_f64() * 1_000.0,
            now.duration_since(self.started).as_secs_f64() * 1_000.0,
        ));
        self.previous = now;
    }
}
