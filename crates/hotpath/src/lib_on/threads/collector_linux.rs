//! Linux /proc filesystem thread metrics collection

use super::ThreadMetrics;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

static CLOCK_TICKS: OnceLock<u64> = OnceLock::new();

/// Get clock ticks per second for time conversion
fn clock_ticks_per_sec() -> u64 {
    *CLOCK_TICKS.get_or_init(|| {
        let v = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        if v <= 0 {
            100 // fallback to 100 Hz
        } else {
            v as u64
        }
    })
}

/// Collect per-thread CPU usage metrics for the current process on Linux
pub(crate) fn collect_thread_metrics() -> Result<Vec<ThreadMetrics>, String> {
    let ticks_per_sec = clock_ticks_per_sec() as f64;
    let task_dir = Path::new("/proc/self/task");

    let entries =
        fs::read_dir(task_dir).map_err(|e| format!("Failed to read /proc/self/task: {}", e))?;

    let mut metrics = Vec::new();

    for entry in entries.flatten() {
        let tid_str = entry.file_name();
        let tid_str = tid_str.to_string_lossy();

        if let Ok(tid) = tid_str.parse::<u64>() {
            match get_thread_info(tid, ticks_per_sec) {
                Ok(metric) => metrics.push(metric),
                Err(e) => {
                    eprintln!(
                        "[hotpath] Warning: Failed to get info for thread {}: {}",
                        tid, e
                    );
                }
            }
        }
    }

    Ok(metrics)
}

fn get_thread_info(tid: u64, ticks_per_sec: f64) -> Result<ThreadMetrics, String> {
    let stat_path = format!("/proc/self/task/{}/stat", tid);
    let comm_path = format!("/proc/self/task/{}/comm", tid);

    // Read thread name from comm (max 15 chars, no newline)
    let name = fs::read_to_string(&comm_path)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| format!("thread_{}", tid));

    // Read and parse stat file
    let stat_content = fs::read_to_string(&stat_path)
        .map_err(|e| format!("Failed to read {}: {}", stat_path, e))?;

    // Parse stat fields - format: "pid (comm) state field4 field5 ... field14(utime) field15(stime) ..."
    // Need to handle comm containing spaces/parens by finding last ')'
    let stat_after_comm = stat_content
        .rfind(')')
        .map(|i| &stat_content[i + 2..]) // Skip ") "
        .ok_or_else(|| "Invalid stat format".to_string())?;

    let fields: Vec<&str> = stat_after_comm.split_whitespace().collect();

    // Fields after comm: [0]=state, [1]=ppid, ... [11]=utime (index 13-1-1=11), [12]=stime
    // utime is field 14 in original (1-indexed), after removing pid and comm it's index 11
    // stime is field 15 in original, after removing pid and comm it's index 12
    if fields.len() < 13 {
        return Err(format!("stat file has too few fields: {}", fields.len()));
    }

    let utime_ticks: u64 = fields[11]
        .parse()
        .map_err(|_| "Failed to parse utime".to_string())?;
    let stime_ticks: u64 = fields[12]
        .parse()
        .map_err(|_| "Failed to parse stime".to_string())?;

    let cpu_user = utime_ticks as f64 / ticks_per_sec;
    let cpu_sys = stime_ticks as f64 / ticks_per_sec;

    Ok(ThreadMetrics::new(tid, name, cpu_user, cpu_sys))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::Duration;

    #[test]
    fn linux_thread_metrics_smoke_test() {
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
}
