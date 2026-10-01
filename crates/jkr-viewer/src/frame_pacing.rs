//! Deadline pacing plus measured host-frame distributions.
use std::fmt::Write as _;
use std::time::{Duration, Instant};

#[path = "frame_budget.rs"]
pub(crate) mod budget;
#[path = "frame_completion.rs"]
pub(crate) mod completion;

/// Pacing deadlines do not stand in for measured frame durations.
pub(crate) struct FramePacer {
    next: Instant,
    frame_started: Instant,
    render_started: Instant,
    previous_start: Option<Instant>,
    interval: f64,
    sample_started: Instant,
    frames: u32,
    label: String,
    /// Retained measured work; also consumed by the headless budget harness.
    pub(crate) budget: budget::Window,
    /// Last completed frame, for allocation-free diagnostic sampling.
    pub(crate) last: budget::Sample,
    report: bool,
    report_context: bool,
    /// Frame encoder cuts finished on worker threads.
    pub(crate) split: crate::frame_split::Splitter,
}

impl FramePacer {
    /// Create retained statistics and formatting storage outside the frame loop.
    pub(crate) fn new() -> Self {
        let now = Instant::now();
        let report = std::env::var_os("JKR_FRAME_BUDGET").is_some();
        Self {
            next: now,
            frame_started: now,
            render_started: now,
            previous_start: None,
            interval: 0.0,
            sample_started: now,
            frames: 0,
            label: String::with_capacity(512),
            budget: budget::Window::new(),
            last: budget::Sample::default(),
            report,
            report_context: report,
            split: crate::frame_split::Splitter::new(),
        }
    }

    /// Preserve the existing redraw-request timestamp used only for pacing deadlines.
    pub(crate) fn begin_frame(&mut self, now: Instant) {
        self.frame_started = now;
    }

    /// Start the measured interval on actual render entry, independently of pacing.
    pub(crate) fn begin_render(&mut self, now: Instant) {
        self.interval = self.previous_start.map_or(0.0, |previous| {
            now.duration_since(previous).as_secs_f64() * 1000.0
        });
        self.render_started = now;
    }

    /// Record measured work before statistics formatting or diagnostic I/O.
    pub(crate) fn record(&mut self, mut sample: budget::Sample) {
        sample.interval = self.interval;
        self.previous_start = Some(self.render_started);
        self.last = sample;
        self.budget.push(sample);
    }

    /// Update pacing and the measured label. No reciprocal-FPS millisecond estimate.
    pub(crate) fn frame_rendered(&mut self, maximum_fps: u32) {
        let now = Instant::now();
        self.frames += 1;
        let elapsed = now.duration_since(self.sample_started);
        if elapsed >= Duration::from_millis(500) {
            let fps = self.frames as f64 / elapsed.as_secs_f64();
            let stats = self.budget.stats();
            let cadence = self.budget.cadence_stats();
            self.label.clear();
            let _ = write!(
                self.label,
                "^7{fps:.0} FPS  host {:.2} ms  p99 {:.2}  max {:.2}  1%low {:.0}",
                stats.mean, stats.p99, stats.worst.work, cadence.low
            );
            if self.report {
                eprintln!(
                    "frame-budget n={} mean={:.3} p99={:.3} max={:.3} work1%low={:.1} \
                    over3ms={} worst-interval={:.3} phases_ms={:?} mean_phases_ms={:?} order={:?}",
                    stats.count,
                    stats.mean,
                    stats.p99,
                    stats.worst.work,
                    stats.low,
                    stats.over_budget,
                    stats.worst.interval,
                    stats.worst.phases,
                    stats.mean_phases,
                    budget::NAMES
                );
                eprintln!(
                    "frame-cadence n={} mean={:.3} p99={:.3} max={:.3} 1%low={:.1}",
                    cadence.count, cadence.mean, cadence.p99, cadence.worst.interval, cadence.low
                );
                eprintln!(
                    "frame-encode mean_ms={:.6} worst_frame_ms={:.6}",
                    budget::encode_total(&stats.mean_phases),
                    budget::encode_total(&stats.worst.phases)
                );
                eprintln!(
                    "frame-effects mean_ms={:.6} worst_frame_ms={:.6}",
                    budget::effect_total(&stats.mean_phases),
                    budget::effect_total(&stats.worst.phases)
                );
            }
            self.frames = 0;
            self.sample_started = now;
        }
        self.schedule(maximum_fps);
    }

    /// Pace skipped attempts too, without counting them as completed frames.
    pub(crate) fn schedule(&mut self, maximum_fps: u32) {
        let now = Instant::now();
        self.next = if maximum_fps == 0 {
            now
        } else {
            (self.frame_started + Duration::from_secs_f64(1.0 / f64::from(maximum_fps))).max(now)
        };
    }

    /// Next allowed redraw time, preserving the existing cap semantics.
    pub(crate) fn deadline(&self) -> Instant {
        self.next
    }
    /// Cached label, formatted only at the statistics refresh interval.
    pub(crate) fn label(&self) -> &str {
        &self.label
    }
}
