//! This module provides real-time thread monitoring capabilities, collecting
//! CPU usage statistics for all threads in the current process.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
#[path = "threads/collector_macos.rs"]
mod collector;

#[cfg(target_os = "linux")]
#[path = "threads/collector_linux.rs"]
mod collector;

/// Thread metrics collected from the OS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadMetrics {
    /// Operating system thread ID (Mach port on macOS)
    pub os_tid: u64,
    /// Thread name (if available)
    pub name: String,
    /// CPU time spent in user mode (seconds)
    pub cpu_user: f64,
    /// CPU time spent in system/kernel mode (seconds)
    pub cpu_sys: f64,
    /// Total CPU time (user + system, seconds)
    pub cpu_total: f64,
    /// CPU usage percentage (based on delta from previous sample)
    /// None if this is the first sample
    pub cpu_percent: Option<f64>,
}

impl ThreadMetrics {
    pub fn new(os_tid: u64, name: String, cpu_user: f64, cpu_sys: f64) -> Self {
        Self {
            os_tid,
            name,
            cpu_user,
            cpu_sys,
            cpu_total: cpu_user + cpu_sys,
            cpu_percent: None,
        }
    }

    /// Calculate CPU percentage based on previous metrics and elapsed time
    pub fn with_percentage(mut self, prev: Option<&ThreadMetrics>, elapsed_secs: f64) -> Self {
        if let Some(prev_metrics) = prev {
            if prev_metrics.os_tid == self.os_tid && elapsed_secs > 0.0 {
                let cpu_delta = self.cpu_total - prev_metrics.cpu_total;
                // CPU % = (delta CPU time / elapsed wall time) * 100
                self.cpu_percent = Some((cpu_delta / elapsed_secs) * 100.0);
            }
        }
        self
    }
}

/// JSON response structure for /threads endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadsJson {
    /// Current elapsed time since program start in nanoseconds
    pub current_elapsed_ns: u64,
    /// Sample interval in milliseconds
    pub sample_interval_ms: u64,
    /// Thread metrics
    pub threads: Vec<ThreadMetrics>,
    /// Total number of threads
    pub thread_count: usize,
}

/// Internal state for thread monitoring
#[allow(dead_code)]
struct ThreadsState {
    /// Last sampled metrics for CPU percentage calculation
    previous_metrics: HashMap<u64, ThreadMetrics>,
    /// Current metrics snapshot
    current_metrics: Vec<ThreadMetrics>,
    /// Timestamp of last sample
    last_sample_time: Instant,
    /// Sample interval
    sample_interval: Duration,
    /// Start time for elapsed calculation
    start_time: Instant,
}

type ThreadsStateRef = Arc<RwLock<ThreadsState>>;

static THREADS_STATE: OnceLock<ThreadsStateRef> = OnceLock::new();

const DEFAULT_SAMPLE_INTERVAL_MS: u64 = 1000;

// Initialize thread monitoring worker
// Call it unless you use channel!, stream!, or #[hotpath::main] macro elsewhere in the code
pub fn init_threads_monitoring() {
    THREADS_STATE.get_or_init(|| {
        let sample_interval_ms = std::env::var("HOTPATH_THREADS_INTERVAL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_SAMPLE_INTERVAL_MS);

        let sample_interval = Duration::from_millis(sample_interval_ms);
        let start_time = Instant::now();

        let state = Arc::new(RwLock::new(ThreadsState {
            previous_metrics: HashMap::new(),
            current_metrics: Vec::new(),
            last_sample_time: start_time,
            sample_interval,
            start_time,
        }));

        let state_clone = Arc::clone(&state);

        std::thread::Builder::new()
            .name("hotpath-threads".into())
            .spawn(move || {
                collector_loop(state_clone, sample_interval);
            })
            .expect("Failed to spawn thread-metrics-collector thread");

        state
    });
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn collector_loop(state: ThreadsStateRef, interval: Duration) {
    loop {
        match collector::collect_thread_metrics() {
            Ok(raw_metrics) => {
                let mut state_guard = match state.write() {
                    Ok(guard) => guard,
                    Err(_) => continue,
                };
                let elapsed_secs = state_guard.last_sample_time.elapsed().as_secs_f64();

                // Calculate CPU percentages by comparing with previous sample
                let mut new_metrics = Vec::with_capacity(raw_metrics.len());
                for metric in raw_metrics {
                    let prev = state_guard.previous_metrics.get(&metric.os_tid);
                    let m_with_percent = metric.clone().with_percentage(prev, elapsed_secs);
                    new_metrics.push(m_with_percent);
                }

                state_guard.previous_metrics =
                    new_metrics.iter().map(|m| (m.os_tid, m.clone())).collect();
                state_guard.current_metrics = new_metrics;
                state_guard.last_sample_time = Instant::now();
            }
            Err(e) => {
                eprintln!("[hotpath] Failed to collect thread metrics: {}", e);
            }
        }

        std::thread::sleep(interval);
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn collector_loop(_state: ThreadsStateRef, _interval: Duration) {
    // No-op on unsupported platforms - sleep forever
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}

/// Get current thread metrics as JSON
pub fn get_threads_json() -> ThreadsJson {
    if let Some(state) = THREADS_STATE.get() {
        if let Ok(state_guard) = state.read() {
            let current_elapsed_ns = state_guard.start_time.elapsed().as_nanos() as u64;

            return ThreadsJson {
                current_elapsed_ns,
                sample_interval_ms: state_guard.sample_interval.as_millis() as u64,
                threads: state_guard.current_metrics.clone(),
                thread_count: state_guard.current_metrics.len(),
            };
        }
    }

    ThreadsJson {
        current_elapsed_ns: 0,
        sample_interval_ms: DEFAULT_SAMPLE_INTERVAL_MS,
        threads: Vec::new(),
        thread_count: 0,
    }
}
