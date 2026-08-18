//! Diagnostic global allocator wrapper.
//!
//! Logs any allocation larger than 1 GiB along with a backtrace so we can
//! identify the code path that requests the impossible allocation seen when
//! the Python REPL starts with certain documents.

use std::alloc::{GlobalAlloc, Layout, System};
use std::backtrace::Backtrace;
use std::cell::Cell;

const THRESHOLD: usize = 1024 * 1024 * 1024; // 1 GiB
const LOG_NAME: &str = "ocs_repl_alloc.log";

/// Write a marker line to the allocation log without triggering an allocation.
///
/// This is used to confirm the allocator logging path works from a given
/// process (especially the Python child process that loads this cdylib).
pub fn init_log() {
    log_line("alloc log initialized");
}

/// Probe whether this process is actually routing allocations through our
/// global allocator. Allocates a 1 GiB + 1 byte buffer inside a catch-unwind
/// so the probe itself does not abort the process.
pub fn probe_allocator() {
    struct ResetGuard;
    impl Drop for ResetGuard {
        fn drop(&mut self) {
            INSIDE.with(|b| b.set(false));
        }
    }
    let _guard = ResetGuard;
    let result = std::panic::catch_unwind(|| {
        let size = THRESHOLD + 1;
        let _ = vec![0u8; size];
    });
    match result {
        Ok(()) => log_line("allocator probe: large allocation succeeded"),
        Err(_) => log_line("allocator probe: large allocation failed (expected)"),
    }
}

fn log_line(msg: &str) {
    use std::io::Write;
    let path = std::env::temp_dir().join(LOG_NAME);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(
            f,
            "{} {msg}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis().to_string())
                .unwrap_or_else(|_| "?".to_string())
        );
        let _ = f.flush();
    }
}

pub struct LoggingAllocator;

thread_local! {
    static INSIDE: Cell<bool> = const { Cell::new(false) };
}

fn log_large_alloc(size: usize) {
    // Prevent recursion: Backtrace::capture allocates.
    if INSIDE.with(|b| b.get()) {
        return;
    }
    INSIDE.with(|b| b.set(true));

    use std::io::Write;
    let path = std::env::temp_dir().join(LOG_NAME);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(
            f,
            "{} large alloc: {size} bytes\n{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis().to_string())
                .unwrap_or_else(|_| "?".to_string()),
            Backtrace::capture()
        );
        let _ = f.flush();
    }

    INSIDE.with(|b| b.set(false));
}

unsafe impl GlobalAlloc for LoggingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() > THRESHOLD {
            log_large_alloc(layout.size());
        }
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if new_size > THRESHOLD {
            log_large_alloc(new_size);
        }
        System.realloc(ptr, layout, new_size)
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if layout.size() > THRESHOLD {
            log_large_alloc(layout.size());
        }
        System.alloc_zeroed(layout)
    }
}

#[global_allocator]
static GLOBAL: LoggingAllocator = LoggingAllocator;
