//! Formatted output types for HTTP server, MCP server, and TUI.
//!
//! This module provides pre-formatted, human-readable versions of all profiling data.
//! The server formats all values (durations, bytes, timestamps) so consumers can
//! display them directly without additional processing.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::json::{
    ChannelLogs, ChannelState, ChannelType, ChannelsJson, FutureCall, FutureCalls, FutureState,
    FuturesJson, LogEntry, SerializableChannelStats, SerializableFutureStats,
    SerializableStreamStats, StreamLogs, StreamsJson, ThreadMetrics, ThreadsJson,
};
use crate::output::{
    format_bytes, format_duration, FunctionLogsJson, FunctionsJson, ProfilingMode,
};

fn format_time_ago(nanos_ago: u64) -> String {
    const NANOS_PER_SEC: u64 = 1_000_000_000;
    const NANOS_PER_MIN: u64 = 60 * NANOS_PER_SEC;
    const NANOS_PER_HOUR: u64 = 60 * NANOS_PER_MIN;

    if nanos_ago < NANOS_PER_SEC {
        "now".to_string()
    } else if nanos_ago < NANOS_PER_MIN {
        let secs = nanos_ago / NANOS_PER_SEC;
        if secs == 1 {
            "1s ago".to_string()
        } else {
            format!("{}s ago", secs)
        }
    } else if nanos_ago < NANOS_PER_HOUR {
        let mins = nanos_ago / NANOS_PER_MIN;
        if mins == 1 {
            "1m ago".to_string()
        } else {
            format!("{}m ago", mins)
        }
    } else {
        let hours = nanos_ago / NANOS_PER_HOUR;
        if hours == 1 {
            "1h ago".to_string()
        } else {
            format!("{}h ago", hours)
        }
    }
}

fn format_delay(delay_ns: u64) -> String {
    if delay_ns < 1_000 {
        format!("{}ns", delay_ns)
    } else if delay_ns < 1_000_000 {
        format!("{:.1}μs", delay_ns as f64 / 1_000.0)
    } else if delay_ns < 1_000_000_000 {
        format!("{:.2}ms", delay_ns as f64 / 1_000_000.0)
    } else {
        format!("{:.3}s", delay_ns as f64 / 1_000_000_000.0)
    }
}

fn state_level(state: &ChannelState) -> &'static str {
    match state {
        ChannelState::Active => "green",
        ChannelState::Closed => "yellow",
        ChannelState::Full => "red",
        ChannelState::Notified => "blue",
    }
}

fn future_state_level(state: &FutureState) -> &'static str {
    match state {
        FutureState::Pending => "gray",
        FutureState::Running => "green",
        FutureState::Suspended => "yellow",
        FutureState::Ready => "cyan",
        FutureState::Cancelled => "red",
    }
}

fn queue_level(queued: u64, channel_type: &ChannelType) -> &'static str {
    let capacity = match channel_type {
        ChannelType::Bounded(cap) => Some(*cap),
        ChannelType::Oneshot => Some(1),
        ChannelType::Unbounded => None,
    };

    match capacity {
        Some(cap) if cap > 0 => {
            let percentage = (queued as f64 / cap as f64 * 100.0).min(100.0);
            if percentage >= 100.0 {
                "red"
            } else if percentage >= 50.0 {
                "yellow"
            } else {
                "green"
            }
        }
        _ => "gray",
    }
}

fn format_queue_status(queued: u64, channel_type: &ChannelType) -> String {
    match channel_type {
        ChannelType::Bounded(cap) => format!("[{}/{}]", queued, cap),
        ChannelType::Oneshot => format!("[{}/1]", queued),
        ChannelType::Unbounded => "N/A".to_string(),
    }
}

fn format_bytes_signed(bytes: i64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let prefix = if bytes < 0 { "-" } else { "+" };
    let abs_bytes = bytes.unsigned_abs();
    format!("{}{}", prefix, format_bytes(abs_bytes))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedFunctionData {
    pub name: String,
    pub calls: u64,
    pub avg: String,
    #[serde(flatten)]
    pub percentiles: HashMap<String, String>,
    pub total: String,
    pub percent_total: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedFunctionsJson {
    pub profiling_mode: String,
    pub total_elapsed: String,
    pub description: String,
    pub caller_name: String,
    pub percentiles: Vec<u8>,
    pub data: Vec<FormattedFunctionData>,
}

impl From<&FunctionsJson> for FormattedFunctionsJson {
    fn from(json: &FunctionsJson) -> Self {
        let is_alloc = matches!(json.hotpath_profiling_mode, ProfilingMode::Alloc);

        let format_value = |metric: &crate::output::MetricType| -> String {
            match metric {
                crate::output::MetricType::DurationNs(ns) => format_duration(*ns),
                crate::output::MetricType::Alloc(bytes, _) => format_bytes(*bytes),
                crate::output::MetricType::Unsupported => "N/A".to_string(),
                _ => metric.to_string(),
            }
        };

        let data = json
            .data
            .iter()
            .map(|(name, metrics)| {
                let calls = match &metrics[0] {
                    crate::output::MetricType::CallsCount(c) => *c,
                    _ => 0,
                };
                let avg = format_value(&metrics[1]);

                let mut percentiles = HashMap::new();
                for (i, &p) in json.percentiles.iter().enumerate() {
                    let metric_idx = 2 + i;
                    if metric_idx < metrics.len() - 2 {
                        percentiles.insert(format!("p{}", p), format_value(&metrics[metric_idx]));
                    }
                }

                let total_idx = metrics.len() - 2;
                let percent_idx = metrics.len() - 1;

                let total = format_value(&metrics[total_idx]);
                let percent_total = match &metrics[percent_idx] {
                    crate::output::MetricType::Percentage(bp) => {
                        format!("{:.2}%", *bp as f64 / 100.0)
                    }
                    crate::output::MetricType::Unsupported => "N/A".to_string(),
                    _ => "0%".to_string(),
                };

                FormattedFunctionData {
                    name: name.clone(),
                    calls,
                    avg,
                    percentiles,
                    total,
                    percent_total,
                }
            })
            .collect();

        FormattedFunctionsJson {
            profiling_mode: json.hotpath_profiling_mode.to_string(),
            total_elapsed: if is_alloc {
                format_bytes(json.total_elapsed)
            } else {
                format_duration(json.total_elapsed)
            },
            description: json.description.clone(),
            caller_name: json.caller_name.clone(),
            percentiles: json.percentiles.clone(),
            data,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedFunctionTimingLogEntry {
    pub invocation: usize,
    pub duration: String,
    pub timestamp: String,
    pub ago: String,
    pub thread_id: Option<u64>,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedFunctionTimingLogsJson {
    pub function_name: String,
    pub total_invocations: usize,
    pub logs: Vec<FormattedFunctionTimingLogEntry>,
}

impl FormattedFunctionTimingLogsJson {
    pub fn from_logs(json: &FunctionLogsJson, current_elapsed_ns: u64) -> Self {
        let total_invocations = json.count;
        let logs = json
            .logs
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                let duration = entry
                    .value
                    .map(format_duration)
                    .unwrap_or_else(|| "N/A".to_string());

                let timestamp = format_duration(entry.elapsed_nanos);
                let ago = format_time_ago(current_elapsed_ns.saturating_sub(entry.elapsed_nanos));
                let invocation = total_invocations - idx;

                FormattedFunctionTimingLogEntry {
                    invocation,
                    duration,
                    timestamp,
                    ago,
                    thread_id: entry.tid,
                    result: entry.result.clone(),
                }
            })
            .collect();

        FormattedFunctionTimingLogsJson {
            function_name: json.function_name.clone(),
            total_invocations,
            logs,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedFunctionAllocLogEntry {
    pub invocation: usize,
    pub bytes: String,
    pub timestamp: String,
    pub ago: String,
    pub thread_id: Option<u64>,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedFunctionAllocLogsJson {
    pub function_name: String,
    pub total_invocations: usize,
    pub logs: Vec<FormattedFunctionAllocLogEntry>,
}

impl FormattedFunctionAllocLogsJson {
    pub fn from_logs(json: &FunctionLogsJson, current_elapsed_ns: u64) -> Self {
        let total_invocations = json.count;
        let logs = json
            .logs
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                let bytes = entry
                    .value
                    .map(format_bytes)
                    .unwrap_or_else(|| "N/A".to_string());

                let timestamp = format_duration(entry.elapsed_nanos);
                let ago = format_time_ago(current_elapsed_ns.saturating_sub(entry.elapsed_nanos));
                let invocation = total_invocations - idx;

                FormattedFunctionAllocLogEntry {
                    invocation,
                    bytes,
                    timestamp,
                    ago,
                    thread_id: entry.tid,
                    result: entry.result.clone(),
                }
            })
            .collect();

        FormattedFunctionAllocLogsJson {
            function_name: json.function_name.clone(),
            total_invocations,
            logs,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedChannelStats {
    pub id: u64,
    pub source: String,
    pub label: String,
    pub has_custom_label: bool,
    pub channel_type: String,
    pub state: String,
    pub state_level: String,
    pub sent_count: u64,
    pub received_count: u64,
    pub queue_status: String,
    pub queue_level: String,
    pub queued: u64,
    pub type_name: String,
    pub type_size: usize,
    pub queued_bytes: String,
    pub iter: u32,
}

impl FormattedChannelStats {
    fn from_stats(stat: &SerializableChannelStats) -> Self {
        let queue_status = format_queue_status(stat.queued, &stat.channel_type);
        let q_level = queue_level(stat.queued, &stat.channel_type);
        let s_level = state_level(&stat.state);
        let queued_bytes = match &stat.channel_type {
            ChannelType::Unbounded => "N/A".to_string(),
            _ => format_bytes(stat.queued_bytes),
        };

        FormattedChannelStats {
            id: stat.id,
            source: stat.source.clone(),
            label: stat.label.clone(),
            has_custom_label: stat.has_custom_label,
            channel_type: stat.channel_type.to_string(),
            state: stat.state.to_string(),
            state_level: s_level.to_string(),
            sent_count: stat.sent_count,
            received_count: stat.received_count,
            queue_status,
            queue_level: q_level.to_string(),
            queued: stat.queued,
            type_name: stat.type_name.clone(),
            type_size: stat.type_size,
            queued_bytes,
            iter: stat.iter,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedChannelsJson {
    pub current_elapsed: String,
    pub current_elapsed_ns: u64,
    pub channels: Vec<FormattedChannelStats>,
}

impl From<&ChannelsJson> for FormattedChannelsJson {
    fn from(json: &ChannelsJson) -> Self {
        FormattedChannelsJson {
            current_elapsed: format_duration(json.current_elapsed_ns),
            current_elapsed_ns: json.current_elapsed_ns,
            channels: json
                .channels
                .iter()
                .map(FormattedChannelStats::from_stats)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedLogEntry {
    pub index: u64,
    pub timestamp: String,
    pub ago: String,
    pub message: Option<String>,
    pub tid: Option<u64>,
}

impl FormattedLogEntry {
    fn from_entry(entry: &LogEntry, current_elapsed_ns: u64) -> Self {
        let ago = format_time_ago(current_elapsed_ns.saturating_sub(entry.timestamp));
        FormattedLogEntry {
            index: entry.index,
            timestamp: format_duration(entry.timestamp),
            ago,
            message: entry.message.clone(),
            tid: entry.tid,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedSentLogEntry {
    pub index: u64,
    pub timestamp: String,
    pub ago: String,
    pub message: Option<String>,
    pub tid: Option<u64>,
    pub delay: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedChannelLogs {
    pub id: String,
    pub sent_logs: Vec<FormattedSentLogEntry>,
    pub received_logs: Vec<FormattedLogEntry>,
}

impl FormattedChannelLogs {
    pub fn from_logs(logs: &ChannelLogs, current_elapsed_ns: u64) -> Self {
        let received_map: HashMap<u64, &LogEntry> = logs
            .received_logs
            .iter()
            .map(|entry| (entry.index, entry))
            .collect();

        let sent_logs = logs
            .sent_logs
            .iter()
            .map(|entry| {
                let ago = format_time_ago(current_elapsed_ns.saturating_sub(entry.timestamp));
                let delay = if let Some(received_entry) = received_map.get(&entry.index) {
                    if received_entry.timestamp >= entry.timestamp {
                        format_delay(received_entry.timestamp - entry.timestamp)
                    } else {
                        "⚠".to_string()
                    }
                } else {
                    "queued".to_string()
                };

                FormattedSentLogEntry {
                    index: entry.index,
                    timestamp: format_duration(entry.timestamp),
                    ago,
                    message: entry.message.clone(),
                    tid: entry.tid,
                    delay,
                }
            })
            .collect();

        let received_logs = logs
            .received_logs
            .iter()
            .map(|e| FormattedLogEntry::from_entry(e, current_elapsed_ns))
            .collect();

        FormattedChannelLogs {
            id: logs.id.clone(),
            sent_logs,
            received_logs,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedStreamStats {
    pub id: u64,
    pub source: String,
    pub label: String,
    pub has_custom_label: bool,
    pub state: String,
    pub state_level: String,
    pub items_yielded: u64,
    pub type_name: String,
    pub type_size: usize,
    pub iter: u32,
}

impl FormattedStreamStats {
    fn from_stats(stat: &SerializableStreamStats) -> Self {
        let s_level = state_level(&stat.state);
        FormattedStreamStats {
            id: stat.id,
            source: stat.source.clone(),
            label: stat.label.clone(),
            has_custom_label: stat.has_custom_label,
            state: stat.state.to_string(),
            state_level: s_level.to_string(),
            items_yielded: stat.items_yielded,
            type_name: stat.type_name.clone(),
            type_size: stat.type_size,
            iter: stat.iter,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedStreamsJson {
    pub current_elapsed: String,
    pub current_elapsed_ns: u64,
    pub streams: Vec<FormattedStreamStats>,
}

impl From<&StreamsJson> for FormattedStreamsJson {
    fn from(json: &StreamsJson) -> Self {
        FormattedStreamsJson {
            current_elapsed: format_duration(json.current_elapsed_ns),
            current_elapsed_ns: json.current_elapsed_ns,
            streams: json
                .streams
                .iter()
                .map(FormattedStreamStats::from_stats)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedStreamLogs {
    pub id: String,
    pub logs: Vec<FormattedLogEntry>,
}

impl FormattedStreamLogs {
    pub fn from_logs(logs: &StreamLogs, current_elapsed_ns: u64) -> Self {
        FormattedStreamLogs {
            id: logs.id.clone(),
            logs: logs
                .logs
                .iter()
                .map(|e| FormattedLogEntry::from_entry(e, current_elapsed_ns))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedFutureStats {
    pub id: u64,
    pub source: String,
    pub label: String,
    pub has_custom_label: bool,
    pub call_count: u64,
    pub total_polls: u64,
}

impl FormattedFutureStats {
    fn from_stats(stat: &SerializableFutureStats) -> Self {
        FormattedFutureStats {
            id: stat.id,
            source: stat.source.clone(),
            label: stat.label.clone(),
            has_custom_label: stat.has_custom_label,
            call_count: stat.call_count,
            total_polls: stat.total_polls,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedFuturesJson {
    pub current_elapsed: String,
    pub current_elapsed_ns: u64,
    pub futures: Vec<FormattedFutureStats>,
}

impl From<&FuturesJson> for FormattedFuturesJson {
    fn from(json: &FuturesJson) -> Self {
        FormattedFuturesJson {
            current_elapsed: format_duration(json.current_elapsed_ns),
            current_elapsed_ns: json.current_elapsed_ns,
            futures: json
                .futures
                .iter()
                .map(FormattedFutureStats::from_stats)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedFutureCall {
    pub id: u64,
    pub future_id: u64,
    pub state: String,
    pub state_level: String,
    pub poll_count: u64,
    pub result: Option<String>,
}

impl FormattedFutureCall {
    fn from_call(call: &FutureCall) -> Self {
        let s_level = future_state_level(&call.state);
        FormattedFutureCall {
            id: call.id,
            future_id: call.future_id,
            state: call.state.to_string(),
            state_level: s_level.to_string(),
            poll_count: call.poll_count,
            result: call.result.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedFutureCalls {
    pub id: String,
    pub calls: Vec<FormattedFutureCall>,
}

impl FormattedFutureCalls {
    pub fn from_calls(calls: &FutureCalls) -> Self {
        FormattedFutureCalls {
            id: calls.id.clone(),
            calls: calls
                .calls
                .iter()
                .map(FormattedFutureCall::from_call)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedThreadMetrics {
    pub os_tid: u64,
    pub name: String,
    pub status: String,
    pub status_code: String,
    pub cpu_user: String,
    pub cpu_sys: String,
    pub cpu_total: String,
    pub cpu_percent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alloc_bytes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dealloc_bytes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_diff: Option<String>,
}

impl FormattedThreadMetrics {
    fn from_metrics(thread: &ThreadMetrics) -> Self {
        let cpu_percent = thread
            .cpu_percent
            .map(|p| format!("{:.1}%", p))
            .unwrap_or_else(|| "-".to_string());

        let alloc_bytes = thread.alloc_bytes.map(format_bytes);
        let dealloc_bytes = thread.dealloc_bytes.map(format_bytes);
        let mem_diff = thread.mem_diff.map(format_bytes_signed);

        FormattedThreadMetrics {
            os_tid: thread.os_tid,
            name: thread.name.clone(),
            status: thread.status.clone(),
            status_code: thread.status_code.clone(),
            cpu_user: format!("{:.2}s", thread.cpu_user),
            cpu_sys: format!("{:.2}s", thread.cpu_sys),
            cpu_total: format!("{:.2}s", thread.cpu_total),
            cpu_percent,
            alloc_bytes,
            dealloc_bytes,
            mem_diff,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedThreadsJson {
    pub current_elapsed: String,
    pub current_elapsed_ns: u64,
    pub sample_interval_ms: u64,
    pub threads: Vec<FormattedThreadMetrics>,
    pub thread_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rss_bytes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rss_bytes_raw: Option<u64>,
}

impl From<&ThreadsJson> for FormattedThreadsJson {
    fn from(json: &ThreadsJson) -> Self {
        FormattedThreadsJson {
            current_elapsed: format_duration(json.current_elapsed_ns),
            current_elapsed_ns: json.current_elapsed_ns,
            sample_interval_ms: json.sample_interval_ms,
            threads: json
                .threads
                .iter()
                .map(FormattedThreadMetrics::from_metrics)
                .collect(),
            thread_count: json.thread_count,
            rss_bytes: json.rss_bytes.map(format_bytes),
            rss_bytes_raw: json.rss_bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_time_ago() {
        assert_eq!(format_time_ago(0), "now");
        assert_eq!(format_time_ago(500_000_000), "now");
        assert_eq!(format_time_ago(1_000_000_000), "1s ago");
        assert_eq!(format_time_ago(5_000_000_000), "5s ago");
        assert_eq!(format_time_ago(60_000_000_000), "1m ago");
        assert_eq!(format_time_ago(120_000_000_000), "2m ago");
        assert_eq!(format_time_ago(3_600_000_000_000), "1h ago");
        assert_eq!(format_time_ago(7_200_000_000_000), "2h ago");
    }

    #[test]
    fn test_format_delay() {
        assert_eq!(format_delay(500), "500ns");
        assert_eq!(format_delay(1_500), "1.5μs");
        assert_eq!(format_delay(1_500_000), "1.50ms");
        assert_eq!(format_delay(1_500_000_000), "1.500s");
    }

    #[test]
    fn test_queue_level() {
        assert_eq!(queue_level(0, &ChannelType::Bounded(100)), "green");
        assert_eq!(queue_level(49, &ChannelType::Bounded(100)), "green");
        assert_eq!(queue_level(50, &ChannelType::Bounded(100)), "yellow");
        assert_eq!(queue_level(99, &ChannelType::Bounded(100)), "yellow");
        assert_eq!(queue_level(100, &ChannelType::Bounded(100)), "red");
        assert_eq!(queue_level(10, &ChannelType::Unbounded), "gray");
    }

    #[test]
    fn test_format_queue_status() {
        assert_eq!(
            format_queue_status(5, &ChannelType::Bounded(100)),
            "[5/100]"
        );
        assert_eq!(format_queue_status(0, &ChannelType::Oneshot), "[0/1]");
        assert_eq!(format_queue_status(50, &ChannelType::Unbounded), "N/A");
    }
}
