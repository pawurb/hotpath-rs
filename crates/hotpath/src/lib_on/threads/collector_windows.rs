//! Windows thread metrics collection using Win32 APIs

use crate::json::ThreadMetrics;
use std::mem;

// Windows API types
type DWORD = u32;
type HANDLE = *mut std::ffi::c_void;
type BOOL = i32;
type HRESULT = i32;
type PWSTR = *mut u16;

const TH32CS_SNAPTHREAD: DWORD = 0x00000004;
const THREAD_QUERY_LIMITED_INFORMATION: DWORD = 0x0800;

#[repr(C)]
struct THREADENTRY32 {
    dw_size: DWORD,
    c_usage: DWORD,
    th32_thread_id: DWORD,
    th32_owner_process_id: DWORD,
    tp_base_pri: i32,
    tp_delta_pri: i32,
    dw_flags: DWORD,
}

#[repr(C)]
struct FILETIME {
    dw_low_date_time: DWORD,
    dw_high_date_time: DWORD,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateToolhelp32Snapshot(dw_flags: DWORD, th32_process_id: DWORD) -> HANDLE;
    fn Thread32First(h_snapshot: HANDLE, lp_te: *mut THREADENTRY32) -> BOOL;
    fn Thread32Next(h_snapshot: HANDLE, lp_te: *mut THREADENTRY32) -> BOOL;
    fn OpenThread(dw_desired_access: DWORD, b_inherit_handle: BOOL, dw_thread_id: DWORD) -> HANDLE;
    fn GetThreadTimes(
        h_thread: HANDLE,
        lp_creation_time: *mut FILETIME,
        lp_exit_time: *mut FILETIME,
        lp_kernel_time: *mut FILETIME,
        lp_user_time: *mut FILETIME,
    ) -> BOOL;
    fn CloseHandle(h_object: HANDLE) -> BOOL;
    fn GetCurrentProcessId() -> DWORD;
    fn GetProcessIdOfThread(thread: HANDLE) -> DWORD;
    fn GetLastError() -> DWORD;
    fn GetThreadDescription(h_thread: HANDLE, ppsz_thread_description: *mut PWSTR) -> HRESULT;
    fn LocalFree(h_mem: HANDLE) -> HANDLE;
}

const INVALID_HANDLE_VALUE: HANDLE = !0 as HANDLE;
const ERROR_NO_MORE_FILES: DWORD = 18;
const ERROR_BAD_LENGTH: DWORD = 24;

/// RAII wrapper for Windows HANDLE to ensure cleanup on drop/panic
struct AutoHandle(HANDLE);

impl Drop for AutoHandle {
    fn drop(&mut self) {
        if self.0 != INVALID_HANDLE_VALUE && !self.0.is_null() {
            unsafe { CloseHandle(self.0) };
        }
    }
}

fn filetime_to_seconds(ft: &FILETIME) -> f64 {
    let ticks = ((ft.dw_high_date_time as u64) << 32) | (ft.dw_low_date_time as u64);
    // FILETIME is in 100-nanosecond intervals
    ticks as f64 / 10_000_000.0
}

/// Collect per-thread CPU usage metrics for the current process on Windows
pub(crate) fn collect_thread_metrics() -> Result<Vec<ThreadMetrics>, String> {
    unsafe {
        let current_pid = GetCurrentProcessId();

        // Retry snapshot creation if ERROR_BAD_LENGTH occurs (rapid process/thread churn)
        let mut snapshot = INVALID_HANDLE_VALUE;
        let mut retries = 0;

        while snapshot == INVALID_HANDLE_VALUE && retries < 5 {
            snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                let error = GetLastError();
                if error != ERROR_BAD_LENGTH || retries >= 4 {
                    return Err(format!("Failed to create thread snapshot: error {}", error));
                }
                retries += 1;
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }

        let _snapshot_guard = AutoHandle(snapshot);

        let mut thread_entry = THREADENTRY32 {
            dw_size: mem::size_of::<THREADENTRY32>() as DWORD,
            c_usage: 0,
            th32_thread_id: 0,
            th32_owner_process_id: 0,
            tp_base_pri: 0,
            tp_delta_pri: 0,
            dw_flags: 0,
        };

        let mut metrics = Vec::new();

        if Thread32First(snapshot, &mut thread_entry) == 0 {
            let error = GetLastError();
            return Err(format!("Thread32First failed: error {}", error));
        }

        loop {
            // Only process threads from our own process
            if thread_entry.th32_owner_process_id == current_pid {
                match get_thread_info(thread_entry.th32_thread_id, current_pid) {
                    Ok(metric) => metrics.push(metric),
                    Err(_) => {
                        // Thread may have exited - this is normal
                    }
                }
            }

            if Thread32Next(snapshot, &mut thread_entry) == 0 {
                let error = GetLastError();
                if error != ERROR_NO_MORE_FILES {
                    return Err(format!("Thread32Next failed: error {}", error));
                }
                break;
            }
        }

        Ok(metrics)
    }
}

fn read_thread_description(h_thread: HANDLE) -> Option<String> {
    let mut desc_ptr: PWSTR = std::ptr::null_mut();
    let hr = unsafe { GetThreadDescription(h_thread, &mut desc_ptr) };
    if hr < 0 || desc_ptr.is_null() {
        return None;
    }
    let mut len = 0usize;
    while unsafe { *desc_ptr.add(len) } != 0 {
        len += 1;
    }
    let slice = unsafe { std::slice::from_raw_parts(desc_ptr, len) };
    let name = String::from_utf16_lossy(slice);
    unsafe { LocalFree(desc_ptr as HANDLE) };
    Some(name)
}

fn get_thread_info(thread_id: DWORD, current_pid: DWORD) -> Result<ThreadMetrics, String> {
    let h_thread = unsafe { OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, thread_id) };

    if h_thread.is_null() {
        return Err(format!(
            "Failed to open thread {}: {}",
            thread_id,
            std::io::Error::last_os_error()
        ));
    }

    let _thread_guard = AutoHandle(h_thread);

    // Verify the thread still belongs to our process (guard against TID reuse)
    let process_id = unsafe { GetProcessIdOfThread(h_thread) };
    if process_id == 0 {
        return Err(format!(
            "GetProcessIdOfThread failed for thread {}: {}",
            thread_id,
            std::io::Error::last_os_error()
        ));
    }
    if process_id != current_pid {
        return Err("Thread ID was reassigned to another process".to_string());
    }

    let mut creation_time = FILETIME {
        dw_low_date_time: 0,
        dw_high_date_time: 0,
    };
    let mut exit_time = FILETIME {
        dw_low_date_time: 0,
        dw_high_date_time: 0,
    };
    let mut kernel_time = FILETIME {
        dw_low_date_time: 0,
        dw_high_date_time: 0,
    };
    let mut user_time = FILETIME {
        dw_low_date_time: 0,
        dw_high_date_time: 0,
    };

    let result = unsafe {
        GetThreadTimes(
            h_thread,
            &mut creation_time,
            &mut exit_time,
            &mut kernel_time,
            &mut user_time,
        )
    };

    if result == 0 {
        return Err(format!(
            "GetThreadTimes failed for thread {}: {}",
            thread_id,
            std::io::Error::last_os_error()
        ));
    }

    let cpu_user = filetime_to_seconds(&user_time);
    let cpu_sys = filetime_to_seconds(&kernel_time);

    let name = read_thread_description(h_thread)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("thread_{}", thread_id));
    let status = "Unknown".to_string();
    let status_code = "".to_string();

    Ok(ThreadMetrics::new(
        thread_id as u64,
        name,
        status,
        status_code,
        cpu_user,
        cpu_sys,
    ))
}

/// Get the RSS (Resident Set Size) of the current process in bytes
pub(crate) fn get_rss_bytes() -> Option<u64> {
    use std::mem::MaybeUninit;

    #[repr(C)]
    struct PROCESS_MEMORY_COUNTERS_EX {
        cb: DWORD,
        page_fault_count: DWORD,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
        private_usage: usize,
    }

    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(
            h_process: HANDLE,
            ppsm_memcounters: *mut PROCESS_MEMORY_COUNTERS_EX,
            cb: DWORD,
        ) -> BOOL;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> HANDLE;
    }

    unsafe {
        let mut counters: MaybeUninit<PROCESS_MEMORY_COUNTERS_EX> = MaybeUninit::zeroed();
        let counters_ptr = counters.as_mut_ptr();
        (*counters_ptr).cb = mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as DWORD;

        let result = GetProcessMemoryInfo(
            GetCurrentProcess(),
            counters_ptr,
            mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as DWORD,
        );

        if result != 0 {
            let counters = counters.assume_init();
            Some(counters.working_set_size as u64)
        } else {
            None
        }
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::Duration;

    #[test]
    fn windows_thread_metrics_smoke_test() {
        let metrics = collect_thread_metrics().expect("collect_thread_metrics should succeed");
        assert!(!metrics.is_empty());

        for m in &metrics {
            assert_ne!(m.os_tid, 0, "os_tid should not be zero");

            assert!(
                m.cpu_user >= 0.0,
                "cpu_user should be non-negative, got {}",
                m.cpu_user
            );
            assert!(
                m.cpu_sys >= 0.0,
                "cpu_sys should be non-negative, got {}",
                m.cpu_sys
            );
            assert!(
                m.cpu_total >= 0.0,
                "cpu_total should be non-negative, got {}",
                m.cpu_total
            );
        }

        std::thread::sleep(Duration::from_millis(10));

        let metrics2 =
            collect_thread_metrics().expect("second collect_thread_metrics should succeed");

        if !metrics.is_empty() && !metrics2.is_empty() {
            let mut first_map = HashMap::new();
            for m in &metrics {
                first_map.insert(m.os_tid, m.cpu_total);
            }

            for m in &metrics2 {
                if let Some(first_total) = first_map.get(&m.os_tid) {
                    let delta = m.cpu_total - first_total;
                    assert!(
                        delta > -0.1,
                        "cpu_total went backwards too much for tid {}: {} -> {} (Δ={})",
                        m.os_tid,
                        first_total,
                        m.cpu_total,
                        delta
                    );
                }
            }
        }
    }

    #[test]
    fn windows_rss_test() {
        let rss = get_rss_bytes();
        assert!(rss.is_some(), "RSS should be available on Windows");
        let rss_bytes = rss.unwrap();
        assert!(rss_bytes > 0, "RSS should be greater than zero");
    }
}
