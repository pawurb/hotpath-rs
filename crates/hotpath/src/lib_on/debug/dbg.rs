//! Debug logging - like std::dbg! but tracked in profiler.

use std::fmt::Debug;

#[cfg(target_os = "linux")]
use quanta::Instant;

#[cfg(not(target_os = "linux"))]
use std::time::Instant;

use crate::channels::{extract_filename, START_TIME};
use crate::debug::{
    get_sorted_debug_stats, get_sorted_value_stats, init_debug_state, send_debug_event, DbgEntry,
    DebugEvent,
};
use crate::json::{
    format_time_ago, FormattedDebugDbgLogs, FormattedDebugEntries, FormattedDebugJson,
    FormattedDebugLog,
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

    send_debug_event(DebugEvent::Dbg {
        source,
        expression,
        value: value_str,
        timestamp,
        tid,
    });
}

pub fn get_debug_entries_json() -> FormattedDebugJson {
    let dbg_stats = get_sorted_debug_stats();
    let val_stats = get_sorted_value_stats();

    let mut formatted: Vec<FormattedDebugEntries> =
        dbg_stats.iter().map(FormattedDebugEntries::from).collect();
    formatted.extend(val_stats.iter().map(FormattedDebugEntries::from));

    formatted.sort_by(|a, b| {
        a.entry_type
            .as_str()
            .cmp(b.entry_type.as_str())
            .then(a.source.cmp(&b.source))
            .then(a.expression.cmp(&b.expression))
    });

    let current_elapsed_ns = START_TIME
        .get()
        .map(|t| t.elapsed().as_nanos() as u64)
        .unwrap_or(0);

    FormattedDebugJson {
        current_elapsed_ns,
        entries: formatted,
    }
}

pub fn get_dbg_logs(id: u64) -> Option<FormattedDebugDbgLogs> {
    let current_elapsed_ns = START_TIME
        .get()
        .map(|t| t.elapsed().as_nanos() as u64)
        .unwrap_or(0);

    crate::debug::get_debug_stats_by_id(id)
        .map(|s| FormattedDebugDbgLogs::from_stats(&s, current_elapsed_ns))
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

impl From<&DbgEntry> for FormattedDebugEntries {
    fn from(stats: &DbgEntry) -> Self {
        let last_value = stats.logs.back().map(|e| e.value.clone());
        FormattedDebugEntries {
            id: stats.id,
            entry_type: crate::json::DebugEntryType::Dbg,
            source: stats.source.to_string(),
            source_display: truncate_source_path(stats.source),
            expression: stats.expression.to_string(),
            log_count: stats.log_count,
            last_value,
        }
    }
}

impl FormattedDebugDbgLogs {
    pub fn from_stats(stats: &DbgEntry, current_elapsed_ns: u64) -> Self {
        FormattedDebugDbgLogs {
            source: truncate_source_path(stats.source),
            expression: stats.expression.to_string(),
            total_logs: stats.log_count,
            logs: stats
                .logs
                .iter()
                .map(|e| FormattedDebugLog {
                    index: e.index,
                    timestamp: format_duration(e.timestamp_ns),
                    ago: format_time_ago(current_elapsed_ns.saturating_sub(e.timestamp_ns)),
                    value: e.value.clone(),
                    thread_id: e.tid,
                    source: None,
                })
                .collect(),
        }
    }
}
