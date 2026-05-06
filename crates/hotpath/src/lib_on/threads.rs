//! This module provides real-time thread monitoring capabilities, collecting
//! CPU usage statistics for all threads in the current process.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, OnceLock, RwLock};
use std::time::Duration;

use crate::instant::Instant;

#[cfg(target_os = "macos")]
#[path = "threads/collector_macos.rs"]
mod collector;

#[cfg(target_os = "linux")]
#[path = "threads/collector_linux.rs"]
mod collector;

pub(crate) use crate::json::ThreadMetrics;
use crate::json::{format_bytes_signed, JsonThreadEntry, JsonThreadsList};
use crate::output::format_bytes;

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn thread_metrics_with_percentage(
    mut metrics: ThreadMetrics,
    prev: Option<&ThreadMetrics>,
    elapsed_secs: f64,
) -> ThreadMetrics {
    if let Some(prev_metrics) = prev {
        if prev_metrics.os_tid == metrics.os_tid && elapsed_secs > 0.0 {
            let cpu_delta = metrics.cpu_total - prev_metrics.cpu_total;
            metrics.cpu_percent = Some((cpu_delta / elapsed_secs) * 100.0);
        }
    }
    metrics
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
    /// Peak CPU percentage per thread (keyed by os_tid)
    max_cpu_percent: HashMap<u64, f64>,
    /// Per-thread baseline (cpu_total, timestamp) at first observation, so
    /// cpu_percent_avg excludes CPU accumulated before profiler started.
    baseline_cpu: HashMap<u64, (f64, Instant)>,
}

type ThreadsStateRef = Arc<RwLock<ThreadsState>>;

static THREADS_STATE: OnceLock<ThreadsStateRef> = OnceLock::new();

static THREADS_INTERVAL_MS: LazyLock<u64> = LazyLock::new(|| {
    std::env::var("HOTPATH_THREADS_INTERVAL_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(250)
});

// Initialize thread monitoring worker
// Call it unless you use channel!, stream!, or #[hotpath::main] macro elsewhere in the code
#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn init_threads_monitoring() {
    THREADS_STATE.get_or_init(|| {
        let sample_interval_ms = *THREADS_INTERVAL_MS;

        let sample_interval = Duration::from_millis(sample_interval_ms);
        let start_time = Instant::now();

        let state = Arc::new(RwLock::new(ThreadsState {
            previous_metrics: HashMap::new(),
            current_metrics: Vec::new(),
            last_sample_time: start_time,
            sample_interval,
            start_time,
            max_cpu_percent: HashMap::new(),
            baseline_cpu: HashMap::new(),
        }));

        let state_clone = Arc::clone(&state);

        std::thread::Builder::new()
            .name("hp-threads".into())
            .spawn(move || {
                let _suspend = crate::lib_on::SuspendAllocTracking::new();
                collector_loop(state_clone, sample_interval);
            })
            .expect("Failed to spawn thread-metrics-collector thread");

        state
    });
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
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
                    #[allow(unused_mut)]
                    let mut m_with_percent =
                        thread_metrics_with_percentage(metric, prev, elapsed_secs);

                    // Merge per-thread allocation stats
                    #[cfg(feature = "hotpath-alloc")]
                    if let Some((alloc, dealloc)) =
                        crate::lib_on::functions::alloc::core::get_thread_alloc_stats(
                            m_with_percent.os_tid,
                        )
                    {
                        m_with_percent.alloc_bytes = Some(alloc);
                        m_with_percent.dealloc_bytes = Some(dealloc);
                        m_with_percent.mem_diff = Some(alloc as i64 - dealloc as i64);
                    }

                    let now = Instant::now();
                    let (baseline_cpu, baseline_at) = *state_guard
                        .baseline_cpu
                        .entry(m_with_percent.os_tid)
                        .or_insert((m_with_percent.cpu_total, now));
                    let observed_secs = now.duration_since(baseline_at).as_secs_f64();
                    if observed_secs > 0.0 {
                        let cpu_since_baseline = m_with_percent.cpu_total - baseline_cpu;
                        m_with_percent.cpu_percent_avg =
                            Some((cpu_since_baseline / observed_secs) * 100.0);
                    }

                    if let Some(pct) = m_with_percent.cpu_percent {
                        let max = state_guard
                            .max_cpu_percent
                            .entry(m_with_percent.os_tid)
                            .or_insert(0.0);
                        if pct > *max {
                            *max = pct;
                        }
                        m_with_percent.cpu_percent_max = Some(*max);
                    } else if let Some(&max) =
                        state_guard.max_cpu_percent.get(&m_with_percent.os_tid)
                    {
                        m_with_percent.cpu_percent_max = Some(max);
                    }

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

/// Get RSS from collector (platform-specific)
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn get_rss_bytes() -> Option<u64> {
    collector::get_rss_bytes()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn get_rss_bytes() -> Option<u64> {
    None
}

/// Get current thread metrics as JSON
#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn get_threads_json() -> JsonThreadsList {
    let rss_bytes = get_rss_bytes();

    if let Some(state) = THREADS_STATE.get() {
        if let Ok(state_guard) = state.read() {
            let current_elapsed_ns = state_guard.start_time.elapsed().as_nanos() as u64;

            let (total_alloc, total_dealloc) =
                state_guard
                    .current_metrics
                    .iter()
                    .fold((0u64, 0u64), |(alloc, dealloc), m| {
                        (
                            alloc + m.alloc_bytes.unwrap_or(0),
                            dealloc + m.dealloc_bytes.unwrap_or(0),
                        )
                    });

            let has_alloc_data = state_guard
                .current_metrics
                .iter()
                .any(|m| m.alloc_bytes.is_some());

            let (total_alloc_bytes, total_dealloc_bytes, alloc_dealloc_diff) = if has_alloc_data {
                let diff = total_alloc as i64 - total_dealloc as i64;
                (
                    Some(format_bytes(total_alloc)),
                    Some(format_bytes(total_dealloc)),
                    Some(format_bytes_signed(diff)),
                )
            } else {
                (None, None, None)
            };

            let mut sorted_metrics: Vec<&ThreadMetrics> =
                state_guard.current_metrics.iter().collect();

            #[cfg(feature = "hotpath-alloc")]
            sorted_metrics.sort_by_key(|m| std::cmp::Reverse(m.alloc_bytes.unwrap_or(0)));

            #[cfg(not(feature = "hotpath-alloc"))]
            sorted_metrics.sort_by(|a, b| {
                b.cpu_percent_max
                    .unwrap_or(0.0)
                    .total_cmp(&a.cpu_percent_max.unwrap_or(0.0))
            });

            return JsonThreadsList {
                current_elapsed_ns,
                sample_interval_ms: state_guard.sample_interval.as_millis() as u64,
                data: sorted_metrics
                    .iter()
                    .map(|m| JsonThreadEntry::from(*m))
                    .collect(),
                thread_count: state_guard.current_metrics.len(),
                rss_bytes: rss_bytes.map(format_bytes),
                total_alloc_bytes,
                total_dealloc_bytes,
                alloc_dealloc_diff,
            };
        }
    }

    JsonThreadsList {
        current_elapsed_ns: 0,
        sample_interval_ms: *THREADS_INTERVAL_MS,
        data: Vec::new(),
        thread_count: 0,
        rss_bytes: rss_bytes.map(format_bytes),
        total_alloc_bytes: None,
        total_dealloc_bytes: None,
        alloc_dealloc_diff: None,
    }
}
