//! Formatted JSON types for MCP server and TUI.
//!
//! These types provide human-readable formatting for profiling data,
//! suitable for both LLM-based tools (MCP) and terminal UI display.

use serde::Serialize;
use std::collections::HashMap;

use crate::json::{
    ChannelLogs, ChannelsJson, FutureCall, FutureCalls, FuturesJson, LogEntry,
    SerializableChannelStats, SerializableFutureStats, SerializableStreamStats, StreamLogs,
    StreamsJson, ThreadMetrics, ThreadsJson,
};
use crate::output::{
    format_bytes, format_duration, FunctionLogEntry, FunctionLogsJson, FunctionsJson, MetricType,
    ProfilingMode,
};

pub fn format_time_ago(nanos_ago: u64) -> String {
    if nanos_ago < 1_000_000_000 {
        format!("{}ms ago", nanos_ago / 1_000_000)
    } else if nanos_ago < 60_000_000_000 {
        format!("{:.1}s ago", nanos_ago as f64 / 1_000_000_000.0)
    } else if nanos_ago < 3_600_000_000_000 {
        format!("{:.1}m ago", nanos_ago as f64 / 60_000_000_000.0)
    } else {
        format!("{:.1}h ago", nanos_ago as f64 / 3_600_000_000_000.0)
    }
}

pub fn format_delay(nanos: u64) -> String {
    if nanos < 1_000 {
        format!("{} ns", nanos)
    } else if nanos < 1_000_000 {
        format!("{:.1} µs", nanos as f64 / 1_000.0)
    } else if nanos < 1_000_000_000 {
        format!("{:.1} ms", nanos as f64 / 1_000_000.0)
    } else {
        format!("{:.2} s", nanos as f64 / 1_000_000_000.0)
    }
}

pub fn format_queue_status(queued: u64, capacity: Option<usize>) -> String {
    match capacity {
        Some(cap) => format!("{}/{}", queued, cap),
        None => format!("{}/∞", queued),
    }
}

pub fn format_bytes_signed(bytes: i64) -> String {
    let sign = if bytes < 0 { "-" } else { "" };
    let abs_bytes = bytes.unsigned_abs();
    format!("{}{}", sign, format_bytes(abs_bytes))
}

#[derive(Debug, Clone, Serialize)]
pub struct FormattedFunctionData {
    pub name: String,
    pub calls: u64,
    pub avg: String,
    #[serde(flatten)]
    pub percentiles: HashMap<String, String>,
    pub total: String,
    pub percent_total: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FormattedFunctionsJson {
    pub profiling_mode: String,
    pub time_elapsed: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_allocated: Option<String>,
    pub description: String,
    pub caller_name: String,
    pub percentiles: Vec<u8>,
    pub data: Vec<FormattedFunctionData>,
}

impl FormattedFunctionsJson {
    pub fn new(json: &FunctionsJson, current_elapsed_ns: u64) -> Self {
        let is_alloc = matches!(json.hotpath_profiling_mode, ProfilingMode::Alloc);

        let format_value = |metric: &MetricType| -> String {
            match metric {
                MetricType::DurationNs(ns) => format_duration(*ns),
                MetricType::Alloc(bytes, _) => format_bytes(*bytes),
                MetricType::Unsupported => "N/A".to_string(),
                _ => metric.to_string(),
            }
        };

        let data = json
            .data
            .iter()
            .map(|(name, metrics)| {
                let calls = match &metrics[0] {
                    MetricType::CallsCount(c) => *c,
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
                    MetricType::Percentage(bp) => format!("{:.2}%", *bp as f64 / 100.0),
                    MetricType::Unsupported => "N/A".to_string(),
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

        let (time_elapsed, total_allocated) = if is_alloc {
            (
                format_duration(current_elapsed_ns),
                Some(format_bytes(json.total_elapsed)),
            )
        } else {
            (format_duration(json.total_elapsed), None)
        };

        FormattedFunctionsJson {
            profiling_mode: json.hotpath_profiling_mode.to_string(),
            time_elapsed,
            total_allocated,
            description: json.description.clone(),
            caller_name: json.caller_name.clone(),
            percentiles: json.percentiles.clone(),
            data,
        }
    }
}

impl From<&FunctionsJson> for FormattedFunctionsJson {
    fn from(json: &FunctionsJson) -> Self {
        Self::new(json, json.total_elapsed)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FormattedFunctionTimingLogEntry {
    pub invocation: u64,
    pub duration: String,
    pub timestamp: String,
    pub ago: String,
    pub thread_id: Option<u64>,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FormattedFunctionTimingLogsJson {
    pub function_name: String,
    pub total_invocations: usize,
    pub logs: Vec<FormattedFunctionTimingLogEntry>,
}

impl FormattedFunctionTimingLogsJson {
    pub fn from_logs(json: &FunctionLogsJson, current_elapsed_ns: u64) -> Self {
        let total = json.count;
        let logs_len = json.logs.len();

        let logs = json
            .logs
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let invocation = (total - logs_len + i + 1) as u64;
                format_timing_log_entry(entry, current_elapsed_ns, invocation)
            })
            .collect();

        FormattedFunctionTimingLogsJson {
            function_name: json.function_name.clone(),
            total_invocations: total,
            logs,
        }
    }
}

fn format_timing_log_entry(
    entry: &FunctionLogEntry,
    current_elapsed_ns: u64,
    invocation: u64,
) -> FormattedFunctionTimingLogEntry {
    let duration = entry
        .value
        .map(format_duration)
        .unwrap_or_else(|| "N/A".to_string());

    let timestamp = format_duration(entry.elapsed_nanos);
    let ago = format_time_ago(current_elapsed_ns.saturating_sub(entry.elapsed_nanos));

    FormattedFunctionTimingLogEntry {
        invocation,
        duration,
        timestamp,
        ago,
        thread_id: entry.tid,
        result: entry.result.clone(),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FormattedFunctionAllocLogEntry {
    pub invocation: u64,
    pub bytes: String,
    pub alloc_count: Option<u64>,
    pub timestamp: String,
    pub ago: String,
    pub thread_id: Option<u64>,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FormattedFunctionAllocLogsJson {
    pub function_name: String,
    pub total_invocations: usize,
    pub logs: Vec<FormattedFunctionAllocLogEntry>,
}

impl FormattedFunctionAllocLogsJson {
    pub fn from_logs(json: &FunctionLogsJson, current_elapsed_ns: u64) -> Self {
        let total = json.count;
        let logs_len = json.logs.len();

        let logs = json
            .logs
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let invocation = (total - logs_len + i + 1) as u64;
                format_alloc_log_entry(entry, current_elapsed_ns, invocation)
            })
            .collect();

        FormattedFunctionAllocLogsJson {
            function_name: json.function_name.clone(),
            total_invocations: total,
            logs,
        }
    }
}

fn format_alloc_log_entry(
    entry: &FunctionLogEntry,
    current_elapsed_ns: u64,
    invocation: u64,
) -> FormattedFunctionAllocLogEntry {
    let bytes = entry
        .value
        .map(format_bytes)
        .unwrap_or_else(|| "N/A".to_string());

    let timestamp = format_duration(entry.elapsed_nanos);
    let ago = format_time_ago(current_elapsed_ns.saturating_sub(entry.elapsed_nanos));

    FormattedFunctionAllocLogEntry {
        invocation,
        bytes,
        alloc_count: entry.alloc_count,
        timestamp,
        ago,
        thread_id: entry.tid,
        result: entry.result.clone(),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FormattedChannelStats {
    pub id: u64,
    pub source: String,
    pub label: String,
    pub has_custom_label: bool,
    pub channel_type: String,
    pub state: String,
    pub sent_count: u64,
    pub received_count: u64,
    pub queued: u64,
    pub queue_status: String,
    pub type_name: String,
    pub type_size: usize,
    pub queued_bytes: String,
    pub iter: u32,
}

impl From<&SerializableChannelStats> for FormattedChannelStats {
    fn from(stats: &SerializableChannelStats) -> Self {
        let capacity = match &stats.channel_type {
            crate::json::ChannelType::Bounded(cap) => Some(*cap),
            _ => None,
        };

        FormattedChannelStats {
            id: stats.id,
            source: stats.source.clone(),
            label: stats.label.clone(),
            has_custom_label: stats.has_custom_label,
            channel_type: stats.channel_type.to_string(),
            state: stats.state.as_str().to_string(),
            sent_count: stats.sent_count,
            received_count: stats.received_count,
            queued: stats.queued,
            queue_status: format_queue_status(stats.queued, capacity),
            type_name: stats.type_name.clone(),
            type_size: stats.type_size,
            queued_bytes: format_bytes(stats.queued_bytes),
            iter: stats.iter,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FormattedChannelsJson {
    pub channels: Vec<FormattedChannelStats>,
}

impl From<&ChannelsJson> for FormattedChannelsJson {
    fn from(json: &ChannelsJson) -> Self {
        FormattedChannelsJson {
            channels: json
                .channels
                .iter()
                .map(FormattedChannelStats::from)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FormattedSentLogEntry {
    pub index: u64,
    pub timestamp: String,
    pub ago: String,
    pub delay: Option<String>,
    pub message: Option<String>,
    pub thread_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FormattedLogEntry {
    pub index: u64,
    pub timestamp: String,
    pub ago: String,
    pub message: Option<String>,
    pub thread_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FormattedChannelLogs {
    pub id: String,
    pub sent_logs: Vec<FormattedSentLogEntry>,
    pub received_logs: Vec<FormattedLogEntry>,
}

impl FormattedChannelLogs {
    pub fn from_logs(logs: &ChannelLogs, current_elapsed_ns: u64) -> Self {
        let sent_logs = logs
            .sent_logs
            .iter()
            .map(|entry| format_sent_log_entry(entry, current_elapsed_ns, &logs.received_logs))
            .collect();

        let received_logs = logs
            .received_logs
            .iter()
            .map(|entry| format_log_entry(entry, current_elapsed_ns))
            .collect();

        FormattedChannelLogs {
            id: logs.id.clone(),
            sent_logs,
            received_logs,
        }
    }
}

fn format_sent_log_entry(
    entry: &LogEntry,
    current_elapsed_ns: u64,
    received_logs: &[LogEntry],
) -> FormattedSentLogEntry {
    let delay = received_logs
        .iter()
        .find(|recv| recv.index == entry.index)
        .map(|recv| format_delay(recv.timestamp.saturating_sub(entry.timestamp)));

    FormattedSentLogEntry {
        index: entry.index,
        timestamp: format_duration(entry.timestamp),
        ago: format_time_ago(current_elapsed_ns.saturating_sub(entry.timestamp)),
        delay,
        message: entry.message.clone(),
        thread_id: entry.tid,
    }
}

fn format_log_entry(entry: &LogEntry, current_elapsed_ns: u64) -> FormattedLogEntry {
    FormattedLogEntry {
        index: entry.index,
        timestamp: format_duration(entry.timestamp),
        ago: format_time_ago(current_elapsed_ns.saturating_sub(entry.timestamp)),
        message: entry.message.clone(),
        thread_id: entry.tid,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FormattedStreamStats {
    pub id: u64,
    pub source: String,
    pub label: String,
    pub has_custom_label: bool,
    pub state: String,
    pub items_yielded: u64,
    pub type_name: String,
    pub type_size: usize,
    pub iter: u32,
}

impl From<&SerializableStreamStats> for FormattedStreamStats {
    fn from(stats: &SerializableStreamStats) -> Self {
        FormattedStreamStats {
            id: stats.id,
            source: stats.source.clone(),
            label: stats.label.clone(),
            has_custom_label: stats.has_custom_label,
            state: stats.state.as_str().to_string(),
            items_yielded: stats.items_yielded,
            type_name: stats.type_name.clone(),
            type_size: stats.type_size,
            iter: stats.iter,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FormattedStreamsJson {
    pub streams: Vec<FormattedStreamStats>,
}

impl From<&StreamsJson> for FormattedStreamsJson {
    fn from(json: &StreamsJson) -> Self {
        FormattedStreamsJson {
            streams: json
                .streams
                .iter()
                .map(FormattedStreamStats::from)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
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
                .map(|entry| format_log_entry(entry, current_elapsed_ns))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FormattedFutureStats {
    pub id: u64,
    pub source: String,
    pub label: String,
    pub has_custom_label: bool,
    pub call_count: u64,
    pub total_polls: u64,
}

impl From<&SerializableFutureStats> for FormattedFutureStats {
    fn from(stats: &SerializableFutureStats) -> Self {
        FormattedFutureStats {
            id: stats.id,
            source: stats.source.clone(),
            label: stats.label.clone(),
            has_custom_label: stats.has_custom_label,
            call_count: stats.call_count,
            total_polls: stats.total_polls,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FormattedFuturesJson {
    pub futures: Vec<FormattedFutureStats>,
}

impl From<&FuturesJson> for FormattedFuturesJson {
    fn from(json: &FuturesJson) -> Self {
        FormattedFuturesJson {
            futures: json
                .futures
                .iter()
                .map(FormattedFutureStats::from)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FormattedFutureCall {
    pub id: u64,
    pub future_id: u64,
    pub state: String,
    pub poll_count: u64,
    pub result: Option<String>,
}

impl From<&FutureCall> for FormattedFutureCall {
    fn from(call: &FutureCall) -> Self {
        FormattedFutureCall {
            id: call.id,
            future_id: call.future_id,
            state: call.state.as_str().to_string(),
            poll_count: call.poll_count,
            result: call.result.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FormattedFutureCalls {
    pub id: String,
    pub calls: Vec<FormattedFutureCall>,
}

impl From<&FutureCalls> for FormattedFutureCalls {
    fn from(calls: &FutureCalls) -> Self {
        FormattedFutureCalls {
            id: calls.id.clone(),
            calls: calls.calls.iter().map(FormattedFutureCall::from).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FormattedThreadMetrics {
    pub os_tid: u64,
    pub name: String,
    pub status: String,
    pub status_code: String,
    pub cpu_user: String,
    pub cpu_sys: String,
    pub cpu_total: String,
    pub cpu_percent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alloc_bytes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dealloc_bytes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_diff: Option<String>,
}

impl From<&ThreadMetrics> for FormattedThreadMetrics {
    fn from(metrics: &ThreadMetrics) -> Self {
        FormattedThreadMetrics {
            os_tid: metrics.os_tid,
            name: metrics.name.clone(),
            status: metrics.status.clone(),
            status_code: metrics.status_code.clone(),
            cpu_user: format!("{:.3}s", metrics.cpu_user),
            cpu_sys: format!("{:.3}s", metrics.cpu_sys),
            cpu_total: format!("{:.3}s", metrics.cpu_total),
            cpu_percent: metrics.cpu_percent.map(|p| format!("{:.1}%", p)),
            alloc_bytes: metrics.alloc_bytes.map(format_bytes),
            dealloc_bytes: metrics.dealloc_bytes.map(format_bytes),
            mem_diff: metrics.mem_diff.map(format_bytes_signed),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FormattedThreadsJson {
    pub sample_interval_ms: u64,
    pub threads: Vec<FormattedThreadMetrics>,
    pub thread_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rss_bytes: Option<String>,
}

impl From<&ThreadsJson> for FormattedThreadsJson {
    fn from(json: &ThreadsJson) -> Self {
        FormattedThreadsJson {
            sample_interval_ms: json.sample_interval_ms,
            threads: json
                .threads
                .iter()
                .map(FormattedThreadMetrics::from)
                .collect(),
            thread_count: json.thread_count,
            rss_bytes: json.rss_bytes.map(format_bytes),
        }
    }
}
