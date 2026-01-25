//! Debug logging - like std::dbg! but tracked in profiler.

use std::fmt::Debug;

#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

use crate::channels::{extract_filename, START_TIME};
use crate::debug::{
    get_all_debug_stats, get_sorted_debug_stats, init_debug_state, send_debug_event, DebugEvent,
    DebugStats,
};
use crate::json::{
    format_time_ago, FormattedDbgJson, FormattedDbgLogEntry, FormattedDbgLogs, FormattedDbgStats,
};
use crate::output::{format_duration, truncate_result};

fn get_thread_id() -> Option<u64> {
    Some(crate::tid::current_tid())
}

#[doc(hidden)]
#[inline]
pub fn log_dbg<T: Debug>(source: &'static str, expression: &'static str, value: &T) {
    init_debug_state();

    let value_str = truncate_result(format!("{:?}", value));
    let timestamp = Instant::now();
    let tid = get_thread_id();

    send_debug_event(DebugEvent::DbgLog {
        source,
        expression,
        value: value_str,
        timestamp,
        tid,
    });
}

pub fn get_dbg_stats_json() -> FormattedDbgJson {
    let stats = get_sorted_debug_stats();
    let formatted: Vec<FormattedDbgStats> = stats.iter().map(FormattedDbgStats::from).collect();

    let current_elapsed_ns = START_TIME
        .get()
        .map(|t| t.elapsed().as_nanos() as u64)
        .unwrap_or(0);

    FormattedDbgJson {
        current_elapsed_ns,
        debug_logs: formatted,
    }
}

pub fn get_dbg_logs(source: &str, expression: &str) -> Option<FormattedDbgLogs> {
    let current_elapsed_ns = START_TIME
        .get()
        .map(|t| t.elapsed().as_nanos() as u64)
        .unwrap_or(0);

    let stats = get_all_debug_stats();
    stats
        .iter()
        .find(|((s, e), _)| *s == source && *e == expression)
        .map(|(_, s)| FormattedDbgLogs::from_stats(s, current_elapsed_ns))
}

fn truncate_source_path(source: &str) -> String {
    if let Some(colon_pos) = source.find(':') {
        let path_part = &source[..colon_pos];
        let suffix = &source[colon_pos..];
        format!("{}{}", extract_filename(path_part), suffix)
    } else {
        extract_filename(source)
    }
}

impl From<&DebugStats> for FormattedDbgStats {
    fn from(stats: &DebugStats) -> Self {
        let id = format!("{}\0{}", stats.source, stats.expression);
        let last_value = stats.logs.back().map(|e| e.value.clone());
        FormattedDbgStats {
            source: stats.source.to_string(),
            source_display: truncate_source_path(stats.source),
            expression: stats.expression.to_string(),
            log_count: stats.log_count,
            id,
            last_value,
        }
    }
}

impl FormattedDbgLogs {
    pub fn from_stats(stats: &DebugStats, current_elapsed_ns: u64) -> Self {
        FormattedDbgLogs {
            source: truncate_source_path(stats.source),
            expression: stats.expression.to_string(),
            total_logs: stats.log_count,
            logs: stats
                .logs
                .iter()
                .map(|e| FormattedDbgLogEntry {
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
