//! Windows-specific thread metrics collection implementation.
//!
//! This module provides the Windows backend for thread monitoring, leveraging
//! the Toolhelp32 API for thread enumeration and the `GetThreadTimes` API for
//! CPU usage tracking. It implements robust resource management via RAII guards
//! and adheres to idiomatic Rust patterns for Win32 interop.

use super::ThreadMetrics;
use std::mem;
use windows::core::{Error, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::System::Diagnostics::ToolHelp::*;
use windows::Win32::System::Memory::LocalFree;
use windows::Win32::System::ProcessStatus::*;
use windows::Win32::System::Threading::*;

/// RAII wrapper for Win32 handles to ensure they are always closed.
struct ScopedHandle(HANDLE);

impl ScopedHandle {
    /// Wraps a raw Win32 handle.
    fn new(handle: HANDLE) -> Self {
        Self(handle)
    }

    /// Returns true if the underlying handle is invalid or null.
    fn is_invalid(&self) -> bool {
        self.0.is_invalid() || self.0 .0 == 0
    }

    /// Returns the raw handle for use with Win32 APIs.
    fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for ScopedHandle {
    fn drop(&mut self) {
        if !self.is_invalid() {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

/// Collects CPU usage and state metrics for all threads in the current process.
///
/// This function captures a snapshot of all system threads and filters for those
/// belonging to the current process. For each identified thread, it queries
/// detailed timing and metadata.
pub(crate) fn collect_thread_metrics() -> Result<Vec<ThreadMetrics>, String> {
    let mut metrics = Vec::new();
    let current_pid = unsafe { GetCurrentProcessId() };

    // Capture a snapshot of all threads in the system.
    let snapshot = unsafe {
        CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0)
            .map(ScopedHandle::new)
            .map_err(|e| format!("Failed to capture thread snapshot: {}", e))?
    };

    if snapshot.is_invalid() {
        return Err("Thread snapshot handle is invalid".to_string());
    }

    let mut entry = THREADENTRY32 {
        dwSize: mem::size_of::<THREADENTRY32>() as u32,
        ..unsafe { mem::zeroed() }
    };

    unsafe {
        // Iterate through the thread list in the snapshot.
        if Thread32First(snapshot.raw(), &mut entry).is_ok() {
            loop {
                // Filter for threads belonging to the current process.
                if entry.th32OwnerProcessID == current_pid {
                    if let Ok(metric) = query_thread_metrics(entry.th32ThreadID) {
                        metrics.push(metric);
                    }
                }

                if Thread32Next(snapshot.raw(), &mut entry).is_err() {
                    break;
                }
            }
        }
    }

    Ok(metrics)
}

/// Queries detailed metrics for a specific thread by its ID.
fn query_thread_metrics(tid: u32) -> Result<ThreadMetrics, Error> {
    unsafe {
        // Open the thread with minimal required permissions for metrics collection.
        let handle =
            OpenThread(THREAD_QUERY_LIMITED_INFORMATION, FALSE, tid).map(ScopedHandle::new)?;

        let mut creation_time = FILETIME::default();
        let mut exit_time = FILETIME::default();
        let mut kernel_time = FILETIME::default();
        let mut user_time = FILETIME::default();

        // Retrieve thread-level CPU usage (user and kernel mode times).
        GetThreadTimes(
            handle.raw(),
            &mut creation_time,
            &mut exit_time,
            &mut kernel_time,
            &mut user_time,
        )?;

        let cpu_user = filetime_to_seconds(user_time);
        let cpu_sys = filetime_to_seconds(kernel_time);

        // Attempt to retrieve the thread's descriptive name (Windows 10 1607+).
        let name = fetch_thread_name(handle.raw()).unwrap_or_else(|| format!("thread_{}", tid));

        // Note: Detailed thread state (Running/Waiting/Suspended) requires NtQueryInformationThread
        // which involves undocumented APIs. We report 'Running' for all active threads.
        let status = "Running ".to_string();
        let status_code = "R".to_string();

        Ok(ThreadMetrics::new(
            tid as u64,
            name,
            status,
            status_code,
            cpu_user,
            cpu_sys,
        ))
    }
}

/// Converts a Windows `FILETIME` (100-nanosecond intervals) to a `f64` representing seconds.
#[inline]
fn filetime_to_seconds(ft: FILETIME) -> f64 {
    let intervals = ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64);
    intervals as f64 / 10_000_000.0
}

/// Helper to retrieve the thread name using `GetThreadDescription`.
unsafe fn fetch_thread_name(handle: HANDLE) -> Option<String> {
    let mut name_ptr = PWSTR::null();
    if GetThreadDescription(handle, &mut name_ptr).is_ok() {
        if name_ptr.is_null() {
            return None;
        }
        let name = name_ptr.to_string().ok();
        // The memory allocated by GetThreadDescription must be freed with LocalFree.
        let _ = LocalFree(HLOCAL(name_ptr.as_ptr() as *mut _));
        name
    } else {
        None
    }
}

/// Retrieves the Resident Set Size (RSS) of the current process in bytes.
pub(crate) fn get_rss_bytes() -> Option<u64> {
    unsafe {
        let mut pmc = PROCESS_MEMORY_COUNTERS::default();
        if GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut pmc,
            mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
        .is_ok()
        {
            Some(pmc.WorkingSetSize as u64)
        } else {
            None
        }
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_windows_collector_functionality() {
        let metrics = collect_thread_metrics().expect("Should succeed in capturing metrics");
        assert!(
            !metrics.is_empty(),
            "Captured thread list should not be empty"
        );

        for m in &metrics {
            assert!(m.os_tid > 0, "TID should be positive");
            assert!(m.cpu_total >= 0.0, "CPU time should be non-negative");
        }

        let rss = get_rss_bytes();
        assert!(rss.is_some(), "RSS should be retrievable");
    }

    #[test]
    fn test_cpu_time_progression() {
        let initial = collect_thread_metrics().unwrap();
        let initial_sum: f64 = initial.iter().map(|m| m.cpu_total).sum();

        // Busy loop to consume some CPU time.
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_millis(50) {
            std::hint::spin_loop();
        }

        let subsequent = collect_thread_metrics().unwrap();
        let subsequent_sum: f64 = subsequent.iter().map(|m| m.cpu_total).sum();

        assert!(
            subsequent_sum >= initial_sum,
            "CPU time should not decrease"
        );
    }
}
