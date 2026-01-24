//! Formatted JSON types for MCP server and TUI.
//!
//! These types provide human-readable formatting for profiling data,
//! suitable for both LLM-based tools (MCP) and terminal UI display.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{ChannelLogs, FutureCall, FutureCalls, LogEntry, StreamLogs, ThreadMetrics};

use crate::output::{
    format_bytes, format_duration, FunctionLogEntry, FunctionLogsJson, MetricType, MetricsProvider,
    ProfilingMode,
};

pub fn format_time_ago(nanos_ago: u64) -> String {
    if nanos_ago < 1_000_000_000 {
        "now".to_string()
    } else if nanos_ago < 60_000_000_000 {
        format!("{}s ago", nanos_ago / 1_000_000_000)
    } else if nanos_ago < 3_600_000_000_000 {
        format!("{}m ago", nanos_ago / 60_000_000_000)
    } else {
        format!("{}h ago", nanos_ago / 3_600_000_000_000)
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedFunctionData {
    pub name: String,
    pub calls: u64,
    pub avg: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avg_raw: Option<u64>,
    #[serde(flatten)]
    pub percentiles: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub percentiles_raw: HashMap<String, u64>,
    pub total: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_raw: Option<u64>,
    pub percent_total: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent_total_raw: Option<u64>,
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedFunctionsJson {
    pub hotpath_profiling_mode: ProfilingMode,
    pub time_elapsed: String,
    pub total_elapsed_ns: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub total_elapsed_raw: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_allocated: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_allocated_raw: Option<u64>,
    pub description: String,
    pub caller_name: String,
    pub percentiles: Vec<u8>,
    pub data: Vec<FormattedFunctionData>,
}

impl FormattedFunctionsJson {
    pub fn from_provider(provider: &dyn MetricsProvider<'_>, current_elapsed_ns: u64) -> Self {
        let hotpath_profiling_mode = provider.profiling_mode();
        let is_alloc = matches!(hotpath_profiling_mode, ProfilingMode::Alloc);
        let percentiles_config = provider.percentiles();
        let metric_data = provider.metric_data();
        let data = format_metric_data(&metric_data, &percentiles_config, false);
        let total_elapsed = provider.total_elapsed();

        let (time_elapsed, total_allocated) = if is_alloc {
            (
                format_duration(current_elapsed_ns),
                Some(format_bytes(total_elapsed)),
            )
        } else {
            (format_duration(total_elapsed), None)
        };

        FormattedFunctionsJson {
            hotpath_profiling_mode,
            time_elapsed,
            total_elapsed_ns: current_elapsed_ns,
            total_elapsed_raw: 0,
            total_allocated,
            total_allocated_raw: None,
            description: provider.description(),
            caller_name: provider.caller_name().to_string(),
            percentiles: percentiles_config,
            data,
        }
    }

    pub fn from_provider_with_raw(
        provider: &dyn MetricsProvider<'_>,
        current_elapsed_ns: u64,
    ) -> Self {
        let hotpath_profiling_mode = provider.profiling_mode();
        let is_alloc = matches!(hotpath_profiling_mode, ProfilingMode::Alloc);
        let percentiles_config = provider.percentiles();
        let metric_data = provider.metric_data();
        let data = format_metric_data(&metric_data, &percentiles_config, true);
        let total_elapsed = provider.total_elapsed();

        let (time_elapsed, total_allocated, total_allocated_raw) = if is_alloc {
            (
                format_duration(current_elapsed_ns),
                Some(format_bytes(total_elapsed)),
                Some(total_elapsed),
            )
        } else {
            (format_duration(total_elapsed), None, None)
        };

        FormattedFunctionsJson {
            hotpath_profiling_mode,
            time_elapsed,
            total_elapsed_ns: current_elapsed_ns,
            total_elapsed_raw: total_elapsed,
            total_allocated,
            total_allocated_raw,
            description: provider.description(),
            caller_name: provider.caller_name().to_string(),
            percentiles: percentiles_config,
            data,
        }
    }

    pub fn empty_fallback(current_elapsed_ns: u64) -> Self {
        FormattedFunctionsJson {
            hotpath_profiling_mode: ProfilingMode::Timing,
            time_elapsed: format_duration(0),
            total_elapsed_ns: current_elapsed_ns,
            total_elapsed_raw: 0,
            total_allocated: None,
            total_allocated_raw: None,
            description: "No timing data available yet".to_string(),
            caller_name: "hotpath".to_string(),
            percentiles: vec![95],
            data: Vec::new(),
        }
    }
}

fn extract_raw_value(metric: &MetricType) -> Option<u64> {
    match metric {
        MetricType::DurationNs(ns) => Some(*ns),
        MetricType::Alloc(bytes, _) => Some(*bytes),
        MetricType::Percentage(bp) => Some(*bp),
        MetricType::Unsupported => None,
        MetricType::CallsCount(_) => None,
    }
}

fn format_metric_data(
    data: &[(String, Vec<MetricType>)],
    percentiles_config: &[u8],
    include_raw: bool,
) -> Vec<FormattedFunctionData> {
    let format_value = |metric: &MetricType| -> String {
        match metric {
            MetricType::DurationNs(ns) => format_duration(*ns),
            MetricType::Alloc(bytes, _) => format_bytes(*bytes),
            MetricType::Unsupported => "N/A".to_string(),
            _ => metric.to_string(),
        }
    };

    data.iter()
        .map(|(name, metrics)| {
            let calls = match &metrics[0] {
                MetricType::CallsCount(c) => *c,
                _ => 0,
            };
            let avg = format_value(&metrics[1]);
            let avg_raw = if include_raw {
                extract_raw_value(&metrics[1])
            } else {
                None
            };

            let mut percentiles = HashMap::new();
            let mut percentiles_raw = HashMap::new();
            for (i, &p) in percentiles_config.iter().enumerate() {
                let metric_idx = 2 + i;
                if metric_idx < metrics.len() - 2 {
                    let key = format!("p{}", p);
                    percentiles.insert(key.clone(), format_value(&metrics[metric_idx]));
                    if include_raw {
                        if let Some(raw) = extract_raw_value(&metrics[metric_idx]) {
                            percentiles_raw.insert(key, raw);
                        }
                    }
                }
            }

            let total_idx = metrics.len() - 2;
            let percent_idx = metrics.len() - 1;

            let total = format_value(&metrics[total_idx]);
            let total_raw = if include_raw {
                extract_raw_value(&metrics[total_idx])
            } else {
                None
            };

            let percent_total = match &metrics[percent_idx] {
                MetricType::Percentage(bp) => format!("{:.2}%", *bp as f64 / 100.0),
                MetricType::Unsupported => "N/A".to_string(),
                _ => "0%".to_string(),
            };
            let percent_total_raw = if include_raw {
                extract_raw_value(&metrics[percent_idx])
            } else {
                None
            };

            FormattedFunctionData {
                name: name.clone(),
                calls,
                avg,
                avg_raw,
                percentiles,
                percentiles_raw,
                total,
                total_raw,
                percent_total,
                percent_total_raw,
            }
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedFunctionTimingLogEntry {
    pub invocation: u64,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedFunctionAllocLogEntry {
    pub invocation: u64,
    pub bytes: String,
    pub alloc_count: Option<u64>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedChannelsJson {
    pub current_elapsed_ns: u64,
    pub channels: Vec<FormattedChannelStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedSentLogEntry {
    pub index: u64,
    pub timestamp: String,
    pub ago: String,
    pub delay: Option<String>,
    pub message: Option<String>,
    pub thread_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedLogEntry {
    pub index: u64,
    pub timestamp: String,
    pub ago: String,
    pub message: Option<String>,
    pub thread_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedStreamsJson {
    pub current_elapsed_ns: u64,
    pub streams: Vec<FormattedStreamStats>,
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
                .map(|entry| format_log_entry(entry, current_elapsed_ns))
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedFuturesJson {
    pub current_elapsed_ns: u64,
    pub futures: Vec<FormattedFutureStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedThreadMetrics {
    pub os_tid: u64,
    pub name: String,
    pub status: String,
    pub status_code: String,
    pub cpu_user: String,
    pub cpu_sys: String,
    pub cpu_total: String,
    pub cpu_percent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub alloc_bytes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub dealloc_bytes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedThreadsJson {
    pub current_elapsed_ns: u64,
    pub sample_interval_ms: u64,
    pub threads: Vec<FormattedThreadMetrics>,
    pub thread_count: usize,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub rss_bytes: Option<String>,
}
