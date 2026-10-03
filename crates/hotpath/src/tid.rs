//! OS-level thread ID (TID) of the current thread.

use std::cell::Cell;

thread_local! {
    static CACHED_TID: Cell<u64> = const { Cell::new(0) };
}

/// OS thread ID, cached per thread. Kernel TID on Linux, Mach thread ID on
/// macOS, Win32 thread ID on Windows, 0 elsewhere.
#[inline]
pub(crate) fn current_tid() -> u64 {
    CACHED_TID.with(|cached| {
        let tid = cached.get();
        if tid != 0 {
            return tid;
        }
        let tid = current_tid_uncached();
        cached.set(tid);
        tid
    })
}

#[inline]
fn current_tid_uncached() -> u64 {
    #[cfg(target_os = "linux")]
    {
        current_tid_linux()
    }

    #[cfg(target_os = "macos")]
    {
        current_tid_macos()
    }

    #[cfg(target_os = "windows")]
    {
        current_tid_windows()
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        0
    }
}

#[cfg(target_os = "windows")]
extern "system" {
    fn GetCurrentThreadId() -> u32;
}

#[cfg(target_os = "windows")]
#[inline]
fn current_tid_windows() -> u64 {
    // SAFETY: GetCurrentThreadId takes no arguments, has no preconditions and
    // cannot fail; it returns the calling thread's Win32 thread ID.
    unsafe { GetCurrentThreadId() as u64 }
}

#[cfg(target_os = "linux")]
#[inline]
fn current_tid_linux() -> u64 {
    // SAFETY: gettid takes no arguments, has no preconditions and cannot fail;
    // this is a plain FFI syscall returning the calling thread's kernel TID.
    unsafe { libc::syscall(libc::SYS_gettid) as u64 }
}

#[cfg(target_os = "macos")]
#[inline]
fn current_tid_macos() -> u64 {
    // SAFETY: pthread_self always returns a valid handle for the calling
    // thread, and pthread_mach_thread_np only reads it.
    unsafe {
        let pthread = libc::pthread_self();
        libc::pthread_mach_thread_np(pthread) as u64
    }
}
