//! Finish a frame's command encoders on worker threads.
//!
//! wgpu-core defers validation and backend encoding of every pass to
//! `CommandEncoder::finish`, so a single frame encoder serializes all of that work on
//! the render thread. A [`Splitter`] cuts the frame into several encoders at pass
//! boundaries: workers finish earlier cuts while the render thread keeps recording.
//! Command buffers are submitted in recording order in one `Queue::submit`, so the GPU
//! executes exactly the command stream a single encoder would have produced.
//!
//! `JKR_FRAME_SPLIT=0` keeps the single encoder; any other number sets the worker count.
//! Hand-off uses bounded channels allocated once: a cut neither allocates nor takes a lock
//! on the render thread. Idle workers share one queue, so the next cut always goes to a
//! free worker instead of waiting behind a long one.
use std::cell::{Cell, RefCell};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex, OnceLock};

/// Cuts beyond this many per frame stay in the current encoder. It is below every
/// channel's capacity, so neither a cut nor a worker's reply can block on a full queue.
const MAX_CUTS: usize = 28;
const CAPACITY: usize = 32;
const DEFAULT_WORKERS: usize = 6;

type Done = (usize, Option<wgpu::CommandBuffer>);
struct Job {
    order: usize,
    encoder: wgpu::CommandEncoder,
    reply: SyncSender<Done>,
}

/// Process-wide encode workers, started on first use. `None`: single-encoder frames.
fn workers() -> Option<&'static SyncSender<Job>> {
    static WORKERS: OnceLock<Option<SyncSender<Job>>> = OnceLock::new();
    WORKERS
        .get_or_init(|| {
            let wanted = match std::env::var("JKR_FRAME_SPLIT") {
                Ok(value) => value.parse::<usize>().ok()?,
                Err(_) => DEFAULT_WORKERS.min(
                    std::thread::available_parallelism()
                        .map_or(1, std::num::NonZero::get)
                        .saturating_sub(1),
                ),
            };
            if wanted == 0 {
                return None;
            }
            let (jobs, take) = sync_channel::<Job>(CAPACITY);
            let take = Arc::new(Mutex::new(take));
            for index in 0..wanted {
                let take = take.clone();
                std::thread::Builder::new()
                    .name(format!("jkr-encode-{index}"))
                    .spawn(move || work(&take))
                    .ok()?;
            }
            Some(jobs)
        })
        .as_ref()
}

fn work(jobs: &Mutex<Receiver<Job>>) {
    // Only idle workers contend for the queue: the lock is released before `finish`.
    loop {
        let job = jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .recv();
        let Ok(Job {
            order,
            encoder,
            reply,
        }) = job
        else {
            return;
        };

        // A validation panic must reach the render thread instead of stranding its wait.
        let command =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| encoder.finish())).ok();

        if reply.send((order, command)).is_err() {
            return;
        }
    }
}

/// One renderer's frame cuts; owned by its frame pacer.
pub(crate) struct Splitter {
    workers: Option<&'static SyncSender<Job>>,
    reply: SyncSender<Done>,
    done: Receiver<Done>,
    sent: Cell<usize>,
    ordered: RefCell<Vec<Option<wgpu::CommandBuffer>>>,
}

impl Splitter {
    /// Reply storage is allocated here, outside the frame loop.
    pub(crate) fn new() -> Self {
        let (reply, done) = sync_channel(CAPACITY);
        let mut ordered = Vec::with_capacity(MAX_CUTS + 1);
        ordered.resize_with(MAX_CUTS + 1, || None);
        Self {
            workers: workers(),
            reply,
            done,
            sent: Cell::new(0),
            ordered: RefCell::new(ordered),
        }
    }

    /// End the current encoder here, between passes, and keep recording into a fresh one.
    pub(crate) fn cut(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder) {
        let Some(workers) = self.workers else {
            return;
        };
        let order = self.sent.get();
        if order >= MAX_CUTS {
            return;
        }
        let next = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("JKR frame encoder"),
        });
        let job = Job {
            order,
            encoder: std::mem::replace(encoder, next),
            reply: self.reply.clone(),
        };
        workers.send(job).expect("frame encode worker stopped");
        self.sent.set(order + 1);
    }

    /// Finish the last cut here, gather earlier cuts in recording order, submit once.
    pub(crate) fn submit(
        &self,
        queue: &wgpu::Queue,
        encoder: wgpu::CommandEncoder,
        timing: &mut crate::frame_pacing::budget::Timer,
    ) {
        let last = encoder.finish();
        let sent = self.sent.replace(0);

        let mut ordered = self.ordered.borrow_mut();
        for _ in 0..sent {
            let (order, command) = self.done.recv().expect("frame encode worker stopped");
            ordered[order] = Some(command.expect("frame encoder cut failed on its worker"));
        }
        timing.mark(crate::frame_pacing::budget::Phase::QueueSubmit);
        queue.submit(
            ordered[..sent]
                .iter_mut()
                .filter_map(Option::take)
                .chain(Some(last)),
        );
    }
}
