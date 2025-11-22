//! Thread ID utilities for capturing OS-level thread identifiers.
//!
//! This module provides cross-platform functions to retrieve the current thread's
//! OS-level thread ID (TID), which is useful for debugging and profiling.

/// Return the OS thread ID (TID) as u64.
///
/// # Platform Support
///
/// - **Linux**: Uses `syscall(SYS_gettid)` to get the kernel thread ID
/// - **macOS**: Uses `pthread_self()` + `pthread_mach_thread_np()` to get the Mach thread ID
///
/// # Panics
///
/// This function will fail to compile on unsupported platforms.
#[inline]
pub fn current_tid() -> u64 {
    #[cfg(target_os = "linux")]
    {
        current_tid_linux()
    }

    #[cfg(target_os = "macos")]
    {
        current_tid_macos()
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        compile_error!("current_tid() is only implemented for Linux and macOS");
    }
}

#[cfg(target_os = "linux")]
#[inline]
fn current_tid_linux() -> u64 {
    unsafe { libc::syscall(libc::SYS_gettid) as u64 }
}

#[cfg(target_os = "macos")]
#[inline]
fn current_tid_macos() -> u64 {
    unsafe {
        let pthread = libc::pthread_self();
        libc::pthread_mach_thread_np(pthread) as u64
    }
}
