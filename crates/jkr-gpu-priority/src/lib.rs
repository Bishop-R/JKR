//! The process's GPU scheduling priority on Windows.
//!
//! An uncapped client keeps the GPU busy, so a recorder or streamer sharing it
//! (OBS, Discord) waits behind the game's work and skips frames; OBS logs this as
//! "skipped frames due to encoding lag". Raising the recorder's priority needs
//! administrator rights, but a process may always lower its own. With
//! [`GpuPriority::BelowNormal`] the Windows GPU scheduler serves other processes'
//! work first when they compete; with no competition the game is unaffected.
//!
//! This crate is the workspace's only use of `unsafe`: two calls into `gdi32`,
//! wrapped so the rest of the workspace keeps `unsafe_code = "forbid"`. Other
//! platforms report [`Error::Unsupported`].

use std::fmt;

/// Scheduling classes from `D3DKMT_SCHEDULINGPRIORITYCLASS`; only the two a
/// process can always select for itself are offered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuPriority {
    Normal,
    BelowNormal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Not Windows.
    Unsupported,
    /// The kernel thunk returned this failing `NTSTATUS`.
    Status(i32),
    /// The current class is one this crate does not name (idle, above normal,
    /// high or realtime), set by something else.
    OtherClass(i32),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => formatter.write_str("GPU priority is only available on Windows"),
            Self::Status(status) => write!(formatter, "NTSTATUS {:#010x}", *status as u32),
            Self::OtherClass(class) => write!(formatter, "unnamed GPU priority class {class}"),
        }
    }
}

impl std::error::Error for Error {}

/// Set the GPU scheduling priority of the current process.
pub fn set(priority: GpuPriority) -> Result<(), Error> {
    platform::set(priority)
}

/// The GPU scheduling priority of the current process.
pub fn get() -> Result<GpuPriority, Error> {
    platform::get()
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod platform {
    use super::{Error, GpuPriority};
    use std::ffi::c_void;

    // D3DKMT_SCHEDULINGPRIORITYCLASS (d3dkmthk.h).
    const BELOW_NORMAL: i32 = 1;
    const NORMAL: i32 = 2;

    /// `GetCurrentProcess()`: a pseudo-handle that is always valid and needs no
    /// closing.
    fn current_process() -> *mut c_void {
        -1_isize as *mut c_void
    }

    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn D3DKMTSetProcessSchedulingPriorityClass(process: *mut c_void, priority: i32) -> i32;
        fn D3DKMTGetProcessSchedulingPriorityClass(process: *mut c_void, priority: *mut i32)
        -> i32;
    }

    pub(super) fn set(priority: GpuPriority) -> Result<(), Error> {
        let class = match priority {
            GpuPriority::Normal => NORMAL,
            GpuPriority::BelowNormal => BELOW_NORMAL,
        };
        // SAFETY: the pseudo-handle names this process and stays valid for its
        // lifetime; the class is a valid enum value; no memory is passed.
        let status = unsafe { D3DKMTSetProcessSchedulingPriorityClass(current_process(), class) };
        if status >= 0 {
            Ok(())
        } else {
            Err(Error::Status(status))
        }
    }

    pub(super) fn get() -> Result<GpuPriority, Error> {
        let mut class = 0_i32;
        // SAFETY: the pseudo-handle names this process; `class` is a live,
        // aligned i32 the call writes once before returning.
        let status =
            unsafe { D3DKMTGetProcessSchedulingPriorityClass(current_process(), &mut class) };
        if status < 0 {
            return Err(Error::Status(status));
        }
        match class {
            NORMAL => Ok(GpuPriority::Normal),
            BELOW_NORMAL => Ok(GpuPriority::BelowNormal),
            other => Err(Error::OtherClass(other)),
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{Error, GpuPriority};

    pub(super) fn set(_priority: GpuPriority) -> Result<(), Error> {
        Err(Error::Unsupported)
    }

    pub(super) fn get() -> Result<GpuPriority, Error> {
        Err(Error::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn lowering_and_restoring_round_trips() {
        set(GpuPriority::BelowNormal).unwrap();
        assert_eq!(get().unwrap(), GpuPriority::BelowNormal);
        set(GpuPriority::Normal).unwrap();
        assert_eq!(get().unwrap(), GpuPriority::Normal);
    }

    #[cfg(not(windows))]
    #[test]
    fn other_platforms_report_unsupported() {
        assert_eq!(set(GpuPriority::BelowNormal), Err(Error::Unsupported));
        assert_eq!(get(), Err(Error::Unsupported));
    }
}
