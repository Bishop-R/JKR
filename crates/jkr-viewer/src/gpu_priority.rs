//! `jkr_yieldGpu`: lower the client's GPU scheduling priority so recorders and
//! streamers sharing the GPU (OBS, Discord) get their frames encoded while the game
//! runs uncapped. See `jkr-gpu-priority` for the mechanism.

use std::sync::atomic::{AtomicU8, Ordering};

use jkr_gpu_priority::GpuPriority;

const UNAPPLIED: u8 = 0;
const NORMAL: u8 = 1;
const BELOW_NORMAL: u8 = 2;

/// Last class applied; process-wide, like the priority itself.
static APPLIED: AtomicU8 = AtomicU8::new(UNAPPLIED);

/// Apply `jkr_yieldGpu` when it changes. A failure is logged once per change and
/// leaves the priority as it was.
pub(crate) fn sync(console: &crate::console::ViewerConsole) {
    let wanted = if console.bool_cvar("jkr_yieldGpu").unwrap_or(true) {
        BELOW_NORMAL
    } else {
        NORMAL
    };
    if APPLIED.swap(wanted, Ordering::Relaxed) == wanted {
        return;
    }
    let priority = if wanted == BELOW_NORMAL {
        GpuPriority::BelowNormal
    } else {
        GpuPriority::Normal
    };
    match jkr_gpu_priority::set(priority) {
        Ok(()) => crate::log::progress(format_args!("GPU scheduling priority: {priority:?}")),
        Err(jkr_gpu_priority::Error::Unsupported) => {}
        Err(error) => crate::log::progress(format_args!(
            "GPU scheduling priority {priority:?} not applied: {error}"
        )),
    }
}
