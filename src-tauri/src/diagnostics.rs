//! What the app is costing, for the readout in the title bar.
//!
//! Its own process, not the machine: the numbers are there to answer "is this
//! app being expensive", and a machine-wide figure answers a different
//! question. Sampled on demand rather than kept up to date in the background —
//! nothing reads it unless the readout is on screen.

use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// A reading of this process.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    /// Percent of one core, as the OS accounts it. Over 100 on several cores.
    pub cpu: f32,
    /// Resident memory in mebibytes.
    pub memory_mb: f64,
    /// Threads in the process, or `None` where the platform will not say.
    pub threads: Option<u32>,
    /// Open file descriptors, or `None` where the platform will not say.
    pub open_files: Option<u32>,
    /// Percent of the GPU.
    ///
    /// Always `None` on macOS: per-process GPU accounting is behind
    /// `powermetrics`, which needs root, and nothing else exposes it. Reported
    /// rather than dropped so the readout can say it is unavailable instead of
    /// implying the app uses no GPU.
    pub gpu: Option<f32>,
}

/// The sampler, kept between calls.
///
/// CPU is the difference between two readings, so a fresh `System` each time
/// would report zero for ever.
static SAMPLER: std::sync::Mutex<Option<System>> = std::sync::Mutex::new(None);

/// Samples this process. Cheap enough to call once a second.
pub fn sample_shared() -> Diagnostics {
    let Ok(mut held) = SAMPLER.lock() else {
        // A poisoned lock means a previous sample panicked. The readout is not
        // worth propagating that into the window.
        return Diagnostics { cpu: 0.0, memory_mb: 0.0, threads: None, open_files: None, gpu: None };
    };
    sample(held.get_or_insert_with(System::new))
}

/// Samples this process into a caller's sampler, which the tests use.
pub fn sample(system: &mut System) -> Diagnostics {
    let pid = Pid::from_u32(std::process::id());
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::new().with_cpu().with_memory(),
    );
    let process = system.process(pid);
    Diagnostics {
        cpu: process.map_or(0.0, sysinfo::Process::cpu_usage),
        // A process big enough to lose precision here would be sixteen
        // petabytes of resident memory.
        #[allow(clippy::cast_precision_loss, reason = "RSS is nowhere near 2^53 bytes")]
        memory_mb: process.map_or(0.0, |p| p.memory() as f64 / 1024.0 / 1024.0),
        threads: thread_count(),
        open_files: open_files(),
        gpu: None,
    }
}

/// Descriptors this process holds.
///
/// `/dev/fd` is this process's own descriptor table on macOS and the BSDs.
/// Reading it opens one itself, which is not counted.
fn open_files() -> Option<u32> {
    let entries = std::fs::read_dir("/dev/fd").ok()?.count();
    u32::try_from(entries.saturating_sub(1)).ok()
}

/// Threads in this process.
///
/// macOS has no `/proc`, and `sysinfo` counts tasks on Linux only, so this is
/// the mach call the OS itself uses. The port array it hands back is owned by
/// the caller and has to be given back, which is the whole reason for the
/// second call.
#[cfg(target_os = "macos")]
#[allow(
    unsafe_code,
    reason = "mach's task_threads is the only way to count threads on macOS without shelling out"
)]
fn thread_count() -> Option<u32> {
    use std::ffi::c_uint;

    unsafe extern "C" {
        fn mach_task_self() -> c_uint;
        fn task_threads(task: c_uint, threads: *mut *mut c_uint, count: *mut c_uint) -> i32;
        fn vm_deallocate(target: c_uint, address: usize, size: usize) -> i32;
    }

    let mut threads: *mut c_uint = std::ptr::null_mut();
    let mut count: c_uint = 0;
    // SAFETY: `task_threads` writes an array it allocates and its length, or
    // returns non-zero and writes neither. Both out-pointers are valid for the
    // call, and the array is handed straight back to the kernel.
    let ok = unsafe {
        let task = mach_task_self();
        let result = task_threads(task, &raw mut threads, &raw mut count);
        if result == 0 && !threads.is_null() {
            let size = count as usize * std::mem::size_of::<c_uint>();
            vm_deallocate(task, threads as usize, size);
        }
        result
    };
    if ok == 0 { Some(count) } else { None }
}

#[cfg(not(target_os = "macos"))]
fn thread_count() -> Option<u32> {
    // Windows would be `Thread32First` over a snapshot; not written until
    // there is a Windows machine to check it on.
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn a_sample_describes_this_process() {
        let mut system = System::new();
        let first = sample(&mut system);
        assert!(first.memory_mb > 0.0, "a running process has resident memory");
        assert!(first.gpu.is_none(), "macOS will not account GPU per process");
    }

    #[test]
    fn it_counts_the_descriptors_a_process_holds() {
        let before = open_files().expect("descriptors are countable here");
        let held = std::fs::File::open("/dev/null").expect("open /dev/null");
        let after = open_files().expect("descriptors are countable here");
        assert!(after > before, "{before} then {after}");
        drop(held);
    }

    #[test]
    fn it_counts_at_least_the_thread_running_the_test() {
        if let Some(threads) = thread_count() {
            assert!(threads >= 1);
        }
    }
}
