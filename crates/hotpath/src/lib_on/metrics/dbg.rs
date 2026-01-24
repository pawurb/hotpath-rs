//! Debug logging - like std::dbg! but tracked in profiler.

use std::fmt::Debug;

#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

use crate::channels::START_TIME;
use crate::json::{
    format_time_ago, FormattedDebugJson, FormattedDebugLogEntry, FormattedDebugLogs,
    FormattedDebugStats,
};
use crate::metrics::{
    get_all_debug_stats, get_sorted_debug_stats, init_metrics_state, send_metric_event,
    DebugStats, MetricEvent,
};
use crate::output::{format_duration, truncate_result};

fn get_thread_id() -> Option<u64> {
    Some(crate::tid::current_tid())
}

#[doc(hidden)]
#[inline]
pub fn log_debug<T: Debug>(source: &'static str, expression: &'static str, value: &T) {
    init_metrics_state();

    let value_str = truncate_result(format!("{:?}", value));
    let timestamp = Instant::now();
    let tid = get_thread_id();

    send_metric_event(MetricEvent::DebugLog {
        source,
        expression,
        value: value_str,
        timestamp,
        tid,
    });
}

#[doc(hidden)]
#[inline]
pub fn log_debug_location(file: &'static str, line: u32, column: u32) {
    init_metrics_state();

    let source: &'static str = Box::leak(format!("{}:{}:{}", file, line, column).into_boxed_str());
    let timestamp = Instant::now();
    let tid = get_thread_id();

    send_metric_event(MetricEvent::DebugLocation {
        source,
        timestamp,
        tid,
    });
}

pub fn get_debug_stats_json() -> FormattedDebugJson {
    let stats = get_sorted_debug_stats();
    let formatted: Vec<FormattedDebugStats> = stats.iter().map(FormattedDebugStats::from).collect();

    let current_elapsed_ns = START_TIME
        .get()
        .map(|t| t.elapsed().as_nanos() as u64)
        .unwrap_or(0);

    FormattedDebugJson {
        current_elapsed_ns,
        debug_logs: formatted,
    }
}

pub fn get_debug_logs(source: &str) -> Option<FormattedDebugLogs> {
    let current_elapsed_ns = START_TIME
        .get()
        .map(|t| t.elapsed().as_nanos() as u64)
        .unwrap_or(0);

    let stats = get_all_debug_stats();
    stats
        .iter()
        .find(|(k, _)| **k == source)
        .map(|(_, s)| FormattedDebugLogs::from_stats(s, current_elapsed_ns))
}

impl From<&DebugStats> for FormattedDebugStats {
    fn from(stats: &DebugStats) -> Self {
        FormattedDebugStats {
            source: stats.source.to_string(),
            expression: stats.expression.to_string(),
            log_count: stats.log_count,
        }
    }
}

impl FormattedDebugLogs {
    pub fn from_stats(stats: &DebugStats, current_elapsed_ns: u64) -> Self {
        FormattedDebugLogs {
            source: stats.source.to_string(),
            expression: stats.expression.to_string(),
            total_logs: stats.log_count,
            logs: stats
                .logs
                .iter()
                .map(|e| FormattedDebugLogEntry {
                    index: e.index,
                    timestamp: format_duration(e.timestamp_ns),
                    ago: format_time_ago(current_elapsed_ns.saturating_sub(e.timestamp_ns)),
                    value: e.value.clone(),
                    thread_id: e.tid,
                })
                .collect(),
        }
    }
}
