//! Formatted JSON types for MCP server and TUI.
//!
//! These types provide human-readable formatting for profiling data,
//! suitable for both LLM-based tools (MCP) and terminal UI display.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::json::{
    ChannelLogs, DataFlowLogEntry, FutureLog, FutureLogsList, StreamLogs, ThreadMetrics,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonFunctionEntry {
    pub id: u32,
    pub name: String,
    pub calls: u64,
    pub avg: String,
    #[serde(flatten)]
    pub percentiles: HashMap<String, String>,
    pub total: String,
    pub percent_total: String,
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

impl JsonFunctionsList {
    pub fn empty_fallback(current_elapsed_ns: u64) -> Self {
        JsonFunctionsList {
            profiling_mode: ProfilingMode::Timing,
            time_elapsed: format_duration(0),
            total_elapsed_ns: current_elapsed_ns,
            total_allocated: None,
            description: "No timing data available yet".to_string(),
            caller_name: "hotpath-meta".to_string(),
            percentiles: vec![95.0],
            data: Vec::new(),
            displayed_count: 0,
            total_count: 0,
        }
    }
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
    #[serde(default)]
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
    pub state: String,
    pub sent_count: u64,
    pub received_count: u64,
    pub type_name: String,
    pub type_size: usize,
    #[serde(default)]
    pub wrap: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_size: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_queue_size: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proc_avg: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub proc_percentiles: HashMap<String, String>,
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
    pub read_wait_avg: String,
    pub write_wait_avg: String,
    pub read_acquire_avg: String,
    pub write_acquire_avg: String,
    pub read_wait_percentiles: HashMap<String, String>,
    pub write_wait_percentiles: HashMap<String, String>,
    pub read_acquire_percentiles: HashMap<String, String>,
    pub write_acquire_percentiles: HashMap<String, String>,
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
    pub wait_avg: String,
    pub acquire_avg: String,
    pub wait_percentiles: HashMap<String, String>,
    pub acquire_percentiles: HashMap<String, String>,
    pub iter: u32,
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
    // Pair by message identity (wrap mode only). Proxy channels have no `msg_id` and
    // their forwarder-stamped timestamps aren't true latency, so they get no delay.
    let delay = entry.msg_id.and_then(|sent_id| {
        received_logs
            .iter()
            .find(|recv| recv.msg_id == Some(sent_id))
            .map(|recv| format_delay(recv.timestamp.saturating_sub(entry.timestamp)))
    });

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
    pub state: String,
    pub items_yielded: u64,
    pub type_name: String,
    pub type_size: usize,
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
    pub total_poll_duration_ns: u64,
    pub total_poll_alloc_bytes: Option<u64>,
    pub total_poll_alloc_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonFutureLog {
    pub id: u32,
    pub future_id: u32,
    pub state: String,
    pub poll_count: u64,
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
    #[serde(default)]
    pub entry_type: DebugEntryType,
    pub source: String,
    pub source_display: String,
    pub expression: String,
    pub log_count: u64,
    pub last_value: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonReport {
    #[serde(default = "default_report_type")]
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
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
            label: None,
            functions_timing: None,
            functions_alloc: None,
            functions_cpu: None,
            channels: None,
            streams: None,
            futures: None,
            rw_locks: None,
            mutexes: None,
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

    /// A send must pair with its exact receive by `msg_id`, not by arrival
    /// position. Receives here are in reverse msg-id order, so index pairing
    /// would mismatch both.
    #[test]
    fn delay_pairs_by_msg_id_not_arrival_index() {
        let logs = ChannelLogs {
            id: 1,
            // (index, timestamp, message, tid, msg_id)
            sent_logs: vec![
                DataFlowLogEntry::new(1, 10, None, None, Some(100)),
                DataFlowLogEntry::new(2, 15, None, None, Some(200)),
            ],
            received_logs: vec![
                DataFlowLogEntry::new(1, 18, None, None, Some(200)),
                DataFlowLogEntry::new(2, 30, None, None, Some(100)),
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

    /// Proxy channels (no `msg_id`) carry no log delay: their events are stamped
    /// inside the forwarder thread, so the interval would be a misleading
    /// forwarder-hop time rather than true send->receive latency.
    #[test]
    fn delay_is_none_for_proxy_channels_without_msg_id() {
        let logs = ChannelLogs {
            id: 1,
            sent_logs: vec![DataFlowLogEntry::new(1, 10, None, None, None)],
            received_logs: vec![DataFlowLogEntry::new(1, 25, None, None, None)],
        };

        let out = JsonChannelLogsList::from_logs(&logs, 1_000);
        assert_eq!(out.sent_logs[0].delay, None);
    }
}
