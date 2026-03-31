//! Windows system API thread metrics collection
//!
//! Implementation plan:
//! 1. Use Toolhelp32 snapshot to iterate all threads in the current process.
//! 2. Open each thread with THREAD_QUERY_LIMITED_INFORMATION access.
//! 3. Use GetThreadTimes to retrieve user and kernel CPU times.
//! 4. Use GetThreadDescription (Windows 10+) to retrieve the thread name.
//! 5. Use GetProcessMemoryInfo for process-level RSS (Resident Set Size).

use super::ThreadMetrics;
use std::mem;
use windows::core::PWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::System::Diagnostics::ToolHelp::*;
use windows::Win32::System::Memory::LocalFree;
use windows::Win32::System::ProcessStatus::*;
use windows::Win32::System::Threading::*;

/// Collect per-thread CPU usage metrics for the current process on Windows
pub(crate) fn collect_thread_metrics() -> Result<Vec<ThreadMetrics>, String> {
    let mut metrics = Vec::new();
    let pid = unsafe { GetCurrentProcessId() };

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) }
        .map_err(|e| format!("Failed to create toolhelp snapshot: {}", e))?;

    let mut te = THREADENTRY32 {
        dwSize: mem::size_of::<THREADENTRY32>() as u32,
        ..unsafe { mem::zeroed() }
    };

    unsafe {
        if Thread32First(snapshot, &mut te).is_ok() {
            loop {
                if te.th32OwnerProcessID == pid {
                    match get_thread_info(te.th32ThreadID) {
                        Ok(metric) => metrics.push(metric),
                        Err(_) => {
                            // Thread may have exited between listing and querying - this is normal
                        }
                    }
                }
                if Thread32Next(snapshot, &mut te).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }

    Ok(metrics)
}

fn get_thread_info(tid: u32) -> Result<ThreadMetrics, String> {
    unsafe {
        let handle = OpenThread(THREAD_QUERY_LIMITED_INFORMATION, FALSE, tid)
            .map_err(|e| format!("Failed to open thread {}: {}", tid, e))?;

        let mut creation_time = FILETIME::default();
        let mut exit_time = FILETIME::default();
        let mut kernel_time = FILETIME::default();
        let mut user_time = FILETIME::default();

        let res = GetThreadTimes(
            handle,
            &mut creation_time,
            &mut exit_time,
            &mut kernel_time,
            &mut user_time,
        );

        if res.is_err() {
            let _ = CloseHandle(handle);
            return Err(format!("GetThreadTimes failed for thread {}", tid));
        }

        let cpu_user = filetime_to_seconds(user_time);
        let cpu_sys = filetime_to_seconds(kernel_time);

        let name = get_thread_name(handle).unwrap_or_else(|| format!("thread_{}", tid));

        // Windows doesn't expose a simple thread status like Linux /proc.
        // We'll report "Running" as the default status for active threads.
        let status = "Running ".to_string();
        let status_code = "R".to_string();

        let _ = CloseHandle(handle);

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

/// Convert Windows FILETIME (100-nanosecond intervals) to seconds
fn filetime_to_seconds(ft: FILETIME) -> f64 {
    let intervals = ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64);
    intervals as f64 / 10_000_000.0
}

/// Retrieve thread name via GetThreadDescription (Windows 10, version 1607+)
unsafe fn get_thread_name(handle: HANDLE) -> Option<String> {
    let mut name_ptr = PWSTR::null();
    if GetThreadDescription(handle, &mut name_ptr).is_ok() && !name_ptr.is_null() {
        let name = name_ptr.to_string().ok();
        let _ = LocalFree(HLOCAL(name_ptr.as_ptr() as *mut _));
        name
    } else {
        None
    }
}

/// Get the RSS (Working Set Size) of the current process in bytes
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
    fn windows_thread_metrics_smoke_test() {
        let metrics = collect_thread_metrics().expect("collect_thread_metrics should succeed");
        assert!(!metrics.is_empty());

        for m in &metrics {
            assert_ne!(m.os_tid, 0, "os_tid should not be zero");
            assert!(m.cpu_user >= 0.0);
            assert!(m.cpu_sys >= 0.0);
            assert!(m.cpu_total >= 0.0);
        }

        let rss = get_rss_bytes();
        assert!(rss.is_some());
        assert!(rss.unwrap() > 0);

        // Verify metrics change over time
        let start_total: f64 = metrics.iter().map(|m| m.cpu_total).sum();
        
        // Do some work
        let mut _v = Vec::new();
        for i in 0..10000 {
            _v.push(i);
        }
        std::thread::sleep(Duration::from_millis(50));
        
        let metrics2 = collect_thread_metrics().expect("second collection should succeed");
        let end_total: f64 = metrics2.iter().map(|m| m.cpu_total).sum();
        
        assert!(end_total >= start_total);
    }
}
