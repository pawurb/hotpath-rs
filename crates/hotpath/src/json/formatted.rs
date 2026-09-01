//! Formatted JSON types for MCP server and TUI.
//!
//! These types provide human-readable formatting for profiling data,
//! suitable for both LLM-based tools (MCP) and terminal UI display.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::json::{
    ChannelLogs, DataFlowLogEntry, FutureLog, FutureLogsList, HttpLogs, SqlLogs, StreamLogs,
    ThreadMetrics,
};

use crate::output::{format_bytes, format_duration, FunctionLog, FunctionLogsList, ProfilingMode};

pub(crate) fn format_time_ago(nanos_ago: u64) -> String {
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

/// Parses a human-readable delay string back to nanoseconds.
/// Inverse of [`format_delay`].
#[cfg(test)]
pub(crate) fn parse_delay(s: &str) -> Option<u64> {
    crate::output::parse_duration(s)
}

pub fn format_bytes_signed(bytes: i64) -> String {
    let sign = if bytes < 0 { "-" } else { "" };
    let abs_bytes = bytes.unsigned_abs();
    format!("{}{}", sign, format_bytes(abs_bytes))
}

/// Parses a human-readable signed byte string back to a byte count.
/// Inverse of [`format_bytes_signed`].
pub fn parse_bytes_signed(s: &str) -> Option<i64> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('-') {
        crate::output::parse_bytes(rest).map(|v| -(v as i64))
    } else {
        crate::output::parse_bytes(s).map(|v| v as i64)
    }
}

/// Structured source location of an instrumented item, joined from the
/// call-site registry at report-build time (`lib_on/locations.rs`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonLocation {
    /// As captured by `file!()`: workspace-root-relative for workspace
    /// members, `<external>/<crate>-<version>/...` for registry dependencies.
    pub file: String,
    pub line: u32,
    pub column: u32,
}

/// Joins an identity string (function name, resource id, caller name) against
/// the location registry; `None` in builds without the profiling runtime.
fn lookup_location(name: &str) -> Option<JsonLocation> {
    #[cfg(feature = "hotpath")]
    {
        crate::lib_on::locations::lookup_location(name)
    }
    #[cfg(not(feature = "hotpath"))]
    {
        let _ = name;
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonFunctionEntry {
    pub id: u32,
    pub name: String,
    pub calls: u64,
    pub sampled_calls: u64,
    pub avg: String,
    #[serde(flatten)]
    pub percentiles: HashMap<String, String>,
    pub total: String,
    pub percent_total: String,
    /// Base64 HdrHistogram V2 (deflate) of sampled call durations in ns.
    /// Present only in static reports with `hotpath-cloud` upload enabled;
    /// same convention for the `*histogram` fields on the other entry types
    /// (see AGENTS.md).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub histogram: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<JsonLocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonFunctionsList {
    pub profiling_mode: ProfilingMode,
    pub time_elapsed: String,
    pub total_elapsed_ns: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_allocated: Option<String>,
    pub description: String,
    pub caller_name: String,
    pub percentiles: Vec<f64>,
    pub data: Vec<JsonFunctionEntry>,
    #[serde(skip)]
    pub displayed_count: usize,
    #[serde(skip)]
    pub total_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonFunctionCpuEntry {
    pub id: u32,
    pub name: String,
    pub samples: u64,
    pub percent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonFunctionsCpuList {
    pub time_elapsed: String,
    pub total_elapsed_ns: u64,
    pub total_samples: u64,
    pub attributed_samples: u64,
    pub description: String,
    pub caller_name: String,
    pub data: Vec<JsonFunctionCpuEntry>,
    pub profile_path: String,
    #[serde(skip)]
    pub displayed_count: usize,
    #[serde(skip)]
    pub total_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonFunctionsCpu {
    Ok(JsonFunctionsCpuList),
    Error { message: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CpuSnapshotStatus {
    Idle,
    Capturing,
    Ready,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonFunctionsCpuEnvelope {
    pub status: CpuSnapshotStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub captured_at_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<JsonFunctionsCpuList>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_session_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonFunctionTimingLog {
    pub invocation: u64,
    pub duration: String,
    pub timestamp: String,
    pub ago: String,
    pub thread_id: Option<u64>,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonFunctionTimingLogsList {
    pub function_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<JsonLocation>,
    pub total_invocations: usize,
    pub logs: Vec<JsonFunctionTimingLog>,
}

impl JsonFunctionTimingLogsList {
    pub(crate) fn from_logs(json: &FunctionLogsList, current_elapsed_ns: u64) -> Self {
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

        JsonFunctionTimingLogsList {
            location: lookup_location(&json.function_name),
            function_name: json.function_name.clone(),
            total_invocations: total,
            logs,
        }
    }
}

fn format_timing_log_entry(
    entry: &FunctionLog,
    current_elapsed_ns: u64,
    invocation: u64,
) -> JsonFunctionTimingLog {
    let duration = entry
        .value
        .map(format_duration)
        .unwrap_or_else(|| "N/A".to_string());

    let timestamp = format_duration(entry.elapsed_nanos);
    let ago = format_time_ago(current_elapsed_ns.saturating_sub(entry.elapsed_nanos));

    JsonFunctionTimingLog {
        invocation,
        duration,
        timestamp,
        ago,
        thread_id: entry.tid,
        result: entry.result.clone(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonFunctionAllocLog {
    pub invocation: u64,
    pub bytes: String,
    pub alloc_count: Option<u64>,
    pub timestamp: String,
    pub ago: String,
    pub thread_id: Option<u64>,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonFunctionAllocLogsList {
    pub function_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<JsonLocation>,
    pub total_invocations: usize,
    pub logs: Vec<JsonFunctionAllocLog>,
}

impl JsonFunctionAllocLogsList {
    pub(crate) fn from_logs(json: &FunctionLogsList, current_elapsed_ns: u64) -> Self {
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

        JsonFunctionAllocLogsList {
            location: lookup_location(&json.function_name),
            function_name: json.function_name.clone(),
            total_invocations: total,
            logs,
        }
    }
}

fn format_alloc_log_entry(
    entry: &FunctionLog,
    current_elapsed_ns: u64,
    invocation: u64,
) -> JsonFunctionAllocLog {
    let bytes = entry
        .value
        .map(format_bytes)
        .unwrap_or_else(|| "N/A".to_string());

    let timestamp = format_duration(entry.elapsed_nanos);
    let ago = format_time_ago(current_elapsed_ns.saturating_sub(entry.elapsed_nanos));

    JsonFunctionAllocLog {
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
pub struct JsonChannelsList {
    pub current_elapsed_ns: u64,
    pub percentiles: Vec<f64>,
    pub data: Vec<JsonChannelEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonChannelEntry {
    pub id: u32,
    pub source: String,
    pub label: String,
    pub has_custom_label: bool,
    pub channel_type: String,
    /// `None` for aggregated entries (`instances > 1`): their instances open
    /// and close independently, so no single state applies. Rendered as `-`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// Number of channel instances aggregated into this call-site entry.
    pub instances: u64,
    /// Number of aggregated instances that have closed.
    pub closed_instances: u64,
    pub sent_count: u64,
    pub received_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent_per_sec: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub received_per_sec: Option<f64>,
    pub type_name: String,
    pub type_size: usize,
    pub wrap: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_size: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_queue_size: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proc_avg: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub proc_percentiles: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proc_sampled_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proc_histogram: Option<String>,
    pub location: JsonLocation,
    pub iter: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRwLocksList {
    pub current_elapsed_ns: u64,
    pub percentiles: Vec<f64>,
    pub data: Vec<JsonRwLockEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRwLockEntry {
    pub id: u32,
    pub source: String,
    pub label: String,
    pub has_custom_label: bool,
    pub type_name: String,
    pub read_count: u64,
    pub write_count: u64,
    pub read_sampled_count: u64,
    pub write_sampled_count: u64,
    pub read_wait_avg: String,
    pub write_wait_avg: String,
    pub read_acquire_avg: String,
    pub write_acquire_avg: String,
    pub read_wait_percentiles: HashMap<String, String>,
    pub write_wait_percentiles: HashMap<String, String>,
    pub read_acquire_percentiles: HashMap<String, String>,
    pub write_acquire_percentiles: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_wait_histogram: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_wait_histogram: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_acquire_histogram: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_acquire_histogram: Option<String>,
    pub location: JsonLocation,
    pub iter: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonMutexesList {
    pub current_elapsed_ns: u64,
    pub percentiles: Vec<f64>,
    pub data: Vec<JsonMutexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonMutexEntry {
    pub id: u32,
    pub source: String,
    pub label: String,
    pub has_custom_label: bool,
    pub type_name: String,
    pub count: u64,
    pub sampled_count: u64,
    pub wait_avg: String,
    pub acquire_avg: String,
    pub wait_percentiles: HashMap<String, String>,
    pub acquire_percentiles: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_histogram: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acquire_histogram: Option<String>,
    pub location: JsonLocation,
    pub iter: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSqlList {
    pub current_elapsed_ns: u64,
    pub total_ns: u64,
    /// Total number of detected calls across all entries, including ones
    /// truncated from `data` by the display limit.
    pub total_calls: u64,
    pub percentiles: Vec<f64>,
    pub data: Vec<JsonSqlEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSqlEntry {
    pub id: u32,
    pub query: String,
    /// Instrumented function the query was executed from, `None` when it ran
    /// outside any measured scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// axum route template (`GET /users/{id}`) whose handler issued it, `None`
    /// outside the server middleware or with route scoping disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    pub count: u64,
    pub avg: String,
    pub total: String,
    pub percent_total: String,
    pub percentiles: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub histogram: Option<String>,
    /// Location of the instrumented caller named in `source`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<JsonLocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSqlLog {
    pub index: u64,
    pub timestamp: String,
    pub ago: String,
    pub duration: String,
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSqlLogsList {
    pub id: String,
    pub logs: Vec<JsonSqlLog>,
}

impl JsonSqlLogsList {
    pub(crate) fn from_logs(logs: &SqlLogs, current_elapsed_ns: u64) -> Self {
        JsonSqlLogsList {
            id: logs.id.to_string(),
            logs: logs
                .logs
                .iter()
                .map(|entry| JsonSqlLog {
                    index: entry.index,
                    timestamp: format_duration(entry.timestamp),
                    ago: format_time_ago(current_elapsed_ns.saturating_sub(entry.timestamp)),
                    duration: format_duration(entry.duration_nanos),
                    query: entry.query.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonHttpList {
    pub current_elapsed_ns: u64,
    pub total_ns: u64,
    /// Total number of detected calls across all entries, including ones
    /// truncated from `data` by the display limit.
    pub total_calls: u64,
    pub percentiles: Vec<f64>,
    pub data: Vec<JsonHttpEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonHttpEntry {
    pub id: u32,
    pub endpoint: String,
    /// Instrumented function the request was issued from, `None` when it was
    /// sent outside any measured scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// axum route template (`GET /users/{id}`) whose handler issued it, `None`
    /// outside the server middleware or with route scoping disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    pub count: u64,
    pub errors: u64,
    pub avg: String,
    pub total: String,
    pub percent_total: String,
    pub percentiles: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub histogram: Option<String>,
    /// Location of the instrumented caller named in `source`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<JsonLocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonHttpLog {
    pub index: u64,
    pub timestamp: String,
    pub ago: String,
    pub duration: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonHttpLogsList {
    pub id: String,
    pub logs: Vec<JsonHttpLog>,
}

impl JsonHttpLogsList {
    pub(crate) fn from_logs(logs: &HttpLogs, current_elapsed_ns: u64) -> Self {
        JsonHttpLogsList {
            id: logs.id.to_string(),
            logs: logs
                .logs
                .iter()
                .map(|entry| JsonHttpLog {
                    index: entry.index,
                    timestamp: format_duration(entry.timestamp),
                    ago: format_time_ago(current_elapsed_ns.saturating_sub(entry.timestamp)),
                    duration: format_duration(entry.duration_nanos),
                    status: entry
                        .status
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "error".to_string()),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonServerList {
    pub current_elapsed_ns: u64,
    pub total_ns: u64,
    /// Total number of served requests across all entries, including ones
    /// truncated from `data` by the display limit.
    pub total_calls: u64,
    pub percentiles: Vec<f64>,
    pub data: Vec<JsonServerEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonServerEntry {
    pub id: u32,
    /// `METHOD template` of the matched route, e.g. `GET /users/{id}`.
    pub route: String,
    pub count: u64,
    pub status_4xx: u64,
    pub status_5xx: u64,
    /// Average SQL queries issued per completed request of this route,
    /// counted under the request's route scope. `None` when SQL profiling is
    /// inactive or no completed request carried a route scope (unmatched
    /// routes, route scoping disabled, route interner cap hit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql_per_request: Option<f64>,
    /// Average outbound HTTP requests per request of this route; same
    /// semantics as `sql_per_request`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_per_request: Option<f64>,
    pub avg: String,
    pub total: String,
    pub percent_total: String,
    pub percentiles: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub histogram: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonIoList {
    pub current_elapsed_ns: u64,
    pub percentiles: Vec<f64>,
    pub data: Vec<JsonIoEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonIoEntry {
    pub id: u32,
    pub source: String,
    pub label: String,
    pub has_custom_label: bool,
    pub type_name: String,
    pub read: JsonIoOpStats,
    pub write: JsonIoOpStats,
    pub flush: JsonIoOpStats,
    pub shutdown: JsonIoOpStats,
    /// Number of wrapper instances aggregated into this call-site entry.
    pub instances: u32,
    pub location: JsonLocation,
    pub iter: u32,
}

/// Per-operation-kind statistics for one instrumented I/O value. `total_ns`,
/// `bytes`, and `sampled_bytes` are raw values; `avg`, `throughput`, and
/// `percentiles` are formatted. `throughput` is the transfer rate over timed
/// operations (`sampled_bytes / total_ns`), `None` when nothing was timed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonIoOpStats {
    pub count: u64,
    pub sampled_count: u64,
    pub bytes: u64,
    pub sampled_bytes: u64,
    pub errors: u64,
    pub avg: String,
    #[serde(default)]
    pub throughput: Option<String>,
    pub total_ns: u64,
    pub percentiles: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub histogram: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonChannelSentLog {
    pub index: u64,
    pub timestamp: String,
    pub ago: String,
    pub delay: Option<String>,
    pub message: Option<String>,
    pub thread_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonDataFlowLog {
    pub index: u64,
    pub timestamp: String,
    pub ago: String,
    pub message: Option<String>,
    pub thread_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonChannelLogsList {
    pub id: String,
    pub sent_logs: Vec<JsonChannelSentLog>,
    pub received_logs: Vec<JsonDataFlowLog>,
}

impl JsonChannelLogsList {
    pub(crate) fn from_logs(logs: &ChannelLogs, current_elapsed_ns: u64) -> Self {
        let sent_logs = logs
            .sent_logs
            .iter()
            .map(|entry| format_sent_log_entry(entry, current_elapsed_ns, &logs.received_logs))
            .collect();

        let received_logs = logs
            .received_logs
            .iter()
            .map(|entry| format_received_log_entry(entry, current_elapsed_ns))
            .collect();

        JsonChannelLogsList {
            id: logs.id.to_string(),
            sent_logs,
            received_logs,
        }
    }
}

fn format_sent_log_entry(
    entry: &DataFlowLogEntry,
    current_elapsed_ns: u64,
    received_logs: &[DataFlowLogEntry],
) -> JsonChannelSentLog {
    // Pair by message identity (wrap mode only). Proxy channels have no `msg_id`
    // and their forwarder-stamped timestamps aren't true latency, so their delay
    // is always "N/A". A received message without `delay_nanos` was skipped by
    // time sampling.
    let delay = match entry.msg_id {
        Some(sent_id) => received_logs
            .iter()
            .find(|recv| recv.msg_id == Some(sent_id))
            .map(|recv| {
                recv.delay_nanos
                    .map(format_delay)
                    .unwrap_or_else(|| "N/A".to_string())
            }),
        None => Some("N/A".to_string()),
    };

    JsonChannelSentLog {
        index: entry.index,
        timestamp: format_duration(entry.timestamp),
        ago: format_time_ago(current_elapsed_ns.saturating_sub(entry.timestamp)),
        delay,
        message: entry.message.clone(),
        thread_id: entry.tid,
    }
}

fn format_received_log_entry(entry: &DataFlowLogEntry, current_elapsed_ns: u64) -> JsonDataFlowLog {
    JsonDataFlowLog {
        index: entry.index,
        timestamp: format_duration(entry.timestamp),
        ago: format_time_ago(current_elapsed_ns.saturating_sub(entry.timestamp)),
        message: entry.message.clone(),
        thread_id: entry.tid,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonStreamsList {
    pub current_elapsed_ns: u64,
    pub data: Vec<JsonStreamEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonStreamEntry {
    pub id: u32,
    pub source: String,
    pub label: String,
    pub has_custom_label: bool,
    /// `None` for aggregated entries (`instances > 1`): their instances
    /// complete independently, so no single state applies. Rendered as `-`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// Number of stream instances aggregated into this call-site entry.
    pub instances: u64,
    /// Number of aggregated instances that have completed.
    pub closed_instances: u64,
    pub items_yielded: u64,
    pub type_name: String,
    pub type_size: usize,
    pub location: JsonLocation,
    pub iter: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonStreamLogsList {
    pub id: String,
    pub logs: Vec<JsonDataFlowLog>,
}

impl JsonStreamLogsList {
    pub(crate) fn from_logs(logs: &StreamLogs, current_elapsed_ns: u64) -> Self {
        JsonStreamLogsList {
            id: logs.id.to_string(),
            logs: logs
                .logs
                .iter()
                .map(|entry| format_received_log_entry(entry, current_elapsed_ns))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonFuturesList {
    pub current_elapsed_ns: u64,
    pub data: Vec<JsonFutureEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonFutureEntry {
    pub id: u32,
    pub source: String,
    pub label: String,
    pub has_custom_label: bool,
    pub call_count: u64,
    pub total_polls: u64,
    pub sampled_polls: u64,
    pub total_poll_duration_ns: u64,
    pub total_poll_alloc_bytes: Option<u64>,
    pub total_poll_alloc_count: Option<u64>,
    pub location: JsonLocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonFutureLog {
    pub id: u32,
    pub future_id: u32,
    pub state: String,
    pub poll_count: u64,
    pub sampled_polls: u64,
    pub total_poll_duration_ns: u64,
    pub max_poll_duration_ns: u64,
    pub last_poll_duration_ns: u64,
    pub total_poll_alloc_bytes: Option<u64>,
    pub total_poll_alloc_count: Option<u64>,
    pub max_poll_alloc_bytes: Option<u64>,
    pub last_poll_alloc_bytes: Option<u64>,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonFutureLogsList {
    pub id: String,
    pub call_count: u64,
    pub total_polls: u64,
    pub total_poll_duration_ns: u64,
    pub total_poll_alloc_bytes: Option<u64>,
    pub total_poll_alloc_count: Option<u64>,
    pub calls: Vec<JsonFutureLog>,
}

impl From<&FutureLog> for JsonFutureLog {
    fn from(log: &FutureLog) -> Self {
        JsonFutureLog {
            id: log.id,
            future_id: log.future_id,
            state: log.state.as_str().to_string(),
            poll_count: log.poll_count,
            sampled_polls: log.sampled_polls,
            // Sampling is decided per call, so an unsampled call's raw total is
            // simply 0 and a sampled call's is exact - no extrapolation needed.
            total_poll_duration_ns: log.total_poll_duration_ns,
            max_poll_duration_ns: log.max_poll_duration_ns,
            last_poll_duration_ns: log.last_poll_duration_ns,
            total_poll_alloc_bytes: log.total_poll_alloc_bytes,
            total_poll_alloc_count: log.total_poll_alloc_count,
            max_poll_alloc_bytes: log.max_poll_alloc_bytes,
            last_poll_alloc_bytes: log.last_poll_alloc_bytes,
            result: log.result.clone(),
        }
    }
}

impl From<&FutureLogsList> for JsonFutureLogsList {
    fn from(calls: &FutureLogsList) -> Self {
        JsonFutureLogsList {
            id: calls.id.clone(),
            call_count: calls.call_count,
            total_polls: calls.total_polls,
            total_poll_duration_ns: calls.total_poll_duration_ns,
            total_poll_alloc_bytes: calls.total_poll_alloc_bytes,
            total_poll_alloc_count: calls.total_poll_alloc_count,
            calls: calls.calls.iter().map(JsonFutureLog::from).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonThreadEntry {
    pub os_tid: u64,
    pub name: String,
    pub status: String,
    pub status_code: String,
    pub cpu_percent: Option<String>,
    pub cpu_percent_max: Option<String>,
    pub cpu_percent_avg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub alloc_bytes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub dealloc_bytes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub mem_diff: Option<String>,
}

impl From<&ThreadMetrics> for JsonThreadEntry {
    fn from(metrics: &ThreadMetrics) -> Self {
        JsonThreadEntry {
            os_tid: metrics.os_tid,
            name: metrics.name.clone(),
            status: metrics.status.clone(),
            status_code: metrics.status_code.clone(),
            cpu_percent: metrics.cpu_percent.map(|p| format!("{:.1}%", p)),
            cpu_percent_max: metrics.cpu_percent_max.map(|p| format!("{:.1}%", p)),
            cpu_percent_avg: metrics.cpu_percent_avg.map(|p| format!("{:.1}%", p)),
            alloc_bytes: metrics.alloc_bytes.map(format_bytes),
            dealloc_bytes: metrics.dealloc_bytes.map(format_bytes),
            mem_diff: metrics.mem_diff.map(format_bytes_signed),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonThreadsList {
    pub current_elapsed_ns: u64,
    pub sample_interval_ms: u64,
    pub thread_count: usize,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub rss_bytes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub total_alloc_bytes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub total_dealloc_bytes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub alloc_dealloc_diff: Option<String>,
    pub data: Vec<JsonThreadEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DebugEntryType {
    #[default]
    Dbg,
    Val,
    Gauge,
}

impl DebugEntryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DebugEntryType::Dbg => "dbg!",
            DebugEntryType::Val => "val!",
            DebugEntryType::Gauge => "gauge!",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonDebugList {
    pub current_elapsed_ns: u64,
    pub entries: Vec<JsonDebugEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonDebugEntry {
    pub id: u32,
    pub entry_type: DebugEntryType,
    pub source: String,
    pub source_display: String,
    pub expression: String,
    pub log_count: u64,
    pub last_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<JsonLocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonDebugDbgLogs {
    pub source: String,
    pub expression: String,
    pub total_logs: u64,
    pub logs: Vec<JsonDebugLog>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonDebugValLogs {
    pub key: String,
    pub total_logs: u64,
    pub logs: Vec<JsonDebugLog>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonDebugGaugeLogs {
    pub key: String,
    pub total_logs: u64,
    pub logs: Vec<JsonDebugLog>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonDebugLog {
    pub index: u64,
    pub timestamp: String,
    pub ago: String,
    pub value: String,
    pub thread_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRuntimeWorker {
    pub index: usize,
    pub park_count: u64,
    pub busy_duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steal_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steal_operations: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overflow_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_queue_depth: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mean_poll_time_us: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonProfilerStatus {
    pub uptime: String,
    pub pid: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRuntimeSnapshot {
    pub num_workers: usize,
    pub num_alive_tasks: usize,
    pub global_queue_depth: usize,
    pub workers: Vec<JsonRuntimeWorker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_blocking_threads: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_idle_blocking_threads: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking_queue_depth: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spawned_tasks_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_schedule_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub io_driver_fd_registered_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub io_driver_fd_deregistered_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub io_driver_ready_count: Option<u64>,
}

fn default_report_type() -> String {
    "hotpath_report".to_string()
}

/// Build/runtime environment of a static report, plus the git and source-root
/// data the server needs to render clickable source links from `location`
/// fields.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JsonMeta {
    /// Compiler version, e.g. `1.89.0`; empty when `rustc --version` failed
    /// at build time.
    pub rustc: String,
    /// `<os>-<arch>`, e.g. `macos-aarch64`.
    pub os: String,
    /// RFC 3339 UTC timestamp of report generation.
    pub created_at: String,
    /// Working directory relative to the enclosing git root: the prefix to
    /// prepend to relative `location.file` values ("" when the process ran
    /// from the repo root). `HOTPATH_SOURCE_ROOT` overrides; omitted when no
    /// git root was found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_root: Option<String>,
    /// Only present with the `hotpath-cloud` feature; read straight from the
    /// `.git` directory, no git binary involved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<JsonGitInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonGitInfo {
    pub sha: String,
    /// Full ref name (`refs/heads/main`); `None` on a detached HEAD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonReport {
    pub r#type: String,
    /// hotpath crate version that produced the report.
    pub version: String,
    pub meta: JsonMeta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_sampling: Option<HashMap<String, f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub functions_timing: Option<JsonFunctionsList>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub functions_alloc: Option<JsonFunctionsList>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub functions_cpu: Option<JsonFunctionsCpu>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<JsonChannelsList>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streams: Option<JsonStreamsList>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub futures: Option<JsonFuturesList>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rw_locks: Option<JsonRwLocksList>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mutexes: Option<JsonMutexesList>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql: Option<JsonSqlList>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http: Option<JsonHttpList>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<JsonServerList>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub io: Option<JsonIoList>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threads: Option<JsonThreadsList>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<JsonDebugList>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_baseline: Option<JsonCpuBaseline>,
}

impl Default for JsonReport {
    fn default() -> Self {
        Self {
            r#type: default_report_type(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            meta: JsonMeta::default(),
            label: None,
            time_sampling: None,
            functions_timing: None,
            functions_alloc: None,
            functions_cpu: None,
            channels: None,
            streams: None,
            futures: None,
            rw_locks: None,
            mutexes: None,
            sql: None,
            http: None,
            server: None,
            io: None,
            threads: None,
            debug: None,
            cpu_baseline: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonCpuBaseline {
    pub avg: String,
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn test_parse_delay_units() {
        assert_eq!(parse_delay("123 ns"), Some(123));
        assert_eq!(parse_delay("0 ns"), Some(0));
        assert_eq!(parse_delay("1.5 µs"), Some(1500));
        assert_eq!(parse_delay("1.5 ms"), Some(1500000));
        assert_eq!(parse_delay("1.50 s"), Some(1500000000));
    }

    #[test]
    fn test_parse_delay_invalid() {
        assert_eq!(parse_delay(""), None);
        assert_eq!(parse_delay("invalid"), None);
    }

    #[test]
    fn test_parse_delay_roundtrip() {
        for val in [0, 500, 1500, 1_500_000, 1_500_000_000] {
            let formatted = format_delay(val);
            let parsed = parse_delay(&formatted);
            assert_eq!(
                parsed,
                Some(val),
                "round-trip failed for {val}: formatted as '{formatted}'"
            );
        }
    }

    #[test]
    fn test_parse_bytes_signed_units() {
        assert_eq!(parse_bytes_signed("0 B"), Some(0));
        assert_eq!(parse_bytes_signed("123 B"), Some(123));
        assert_eq!(parse_bytes_signed("-1.5 KB"), Some(-1536));
        assert_eq!(parse_bytes_signed("2.0 MB"), Some(2097152));
    }

    #[test]
    fn test_parse_bytes_signed_invalid() {
        assert_eq!(parse_bytes_signed(""), None);
        assert_eq!(parse_bytes_signed("invalid"), None);
    }

    #[test]
    fn test_parse_bytes_signed_roundtrip() {
        for val in [0i64, 100, 1536, -1024, -1536, 1048576, -1048576] {
            let formatted = format_bytes_signed(val);
            let parsed = parse_bytes_signed(&formatted);
            assert_eq!(
                parsed,
                Some(val),
                "round-trip failed for {val}: formatted as '{formatted}'"
            );
        }
    }

    #[test]
    fn json_functions_cpu_accepts_result_list_shape() {
        let result = r#"{
            "time_elapsed":"1s","total_elapsed_ns":1,
            "total_samples":10,"attributed_samples":5,
            "description":"d","caller_name":"main","data":[],
            "profile_path":"/tmp/hp.json.gz"
        }"#;
        match serde_json::from_str::<JsonFunctionsCpu>(result).unwrap() {
            JsonFunctionsCpu::Ok(list) => assert_eq!(list.caller_name, "main"),
            _ => panic!("expected Ok variant"),
        }
    }

    #[test]
    fn json_functions_cpu_accepts_error_shape() {
        let body = r#"{"message":"samply worker not started"}"#;
        match serde_json::from_str::<JsonFunctionsCpu>(body).unwrap() {
            JsonFunctionsCpu::Error { message } => {
                assert_eq!(message, "samply worker not started")
            }
            _ => panic!("expected Error variant"),
        }
    }

    /// A send must pair with its exact receive by `msg_id`, not by arrival
    /// position. Receives here are in reverse msg-id order, so index pairing
    /// would mismatch both.
    #[test]
    fn delay_pairs_by_msg_id_not_arrival_index() {
        let logs = ChannelLogs {
            id: 1,
            // (index, timestamp, message, tid, msg_id, delay_nanos)
            sent_logs: vec![
                DataFlowLogEntry::new(1, 10, None, None, Some(100), None),
                DataFlowLogEntry::new(2, 15, None, None, Some(200), None),
            ],
            received_logs: vec![
                DataFlowLogEntry::new(1, 18, None, None, Some(200), Some(3)),
                DataFlowLogEntry::new(2, 30, None, None, Some(100), Some(20)),
            ],
        };

        let out = JsonChannelLogsList::from_logs(&logs, 1_000);

        let by_index: HashMap<u64, Option<String>> = out
            .sent_logs
            .iter()
            .map(|s| (s.index, s.delay.clone()))
            .collect();

        // msg 100: recv@30 - send@10 = 20ns; msg 200: recv@18 - send@15 = 3ns.
        assert_eq!(by_index[&1], Some("20 ns".to_string()));
        assert_eq!(by_index[&2], Some("3 ns".to_string()));
    }

    /// Proxy channels (no `msg_id`) always show "N/A": their events are stamped
    /// inside the forwarder thread, so the interval would be a misleading
    /// forwarder-hop time rather than true send->receive latency.
    #[test]
    fn delay_is_na_for_proxy_channels_without_msg_id() {
        let logs = ChannelLogs {
            id: 1,
            sent_logs: vec![DataFlowLogEntry::new(1, 10, None, None, None, None)],
            received_logs: vec![DataFlowLogEntry::new(1, 25, None, None, None, None)],
        };

        let out = JsonChannelLogsList::from_logs(&logs, 1_000);
        assert_eq!(out.sent_logs[0].delay, Some("N/A".to_string()));
    }

    /// Unsampled wrap messages carry no `delay_nanos`, so the delay must read
    /// "N/A", not a bogus near-zero derived from drain-time stamps.
    #[test]
    fn delay_is_na_for_unsampled_wrap_messages() {
        let logs = ChannelLogs {
            id: 1,
            sent_logs: vec![
                DataFlowLogEntry::new(1, 10, None, None, Some(100), None),
                DataFlowLogEntry::new(2, 15, None, None, Some(101), None),
            ],
            received_logs: vec![
                DataFlowLogEntry::new(1, 30, None, None, Some(100), Some(20)),
                DataFlowLogEntry::new(2, 15, None, None, Some(101), None),
            ],
        };

        let out = JsonChannelLogsList::from_logs(&logs, 1_000);

        let by_index: HashMap<u64, Option<String>> = out
            .sent_logs
            .iter()
            .map(|s| (s.index, s.delay.clone()))
            .collect();

        assert_eq!(by_index[&1], Some("20 ns".to_string()));
        assert_eq!(by_index[&2], Some("N/A".to_string()));
    }
}
