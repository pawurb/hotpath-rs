//! JSON serializable types for TUI and CLI consumers.
//!
//! This module contains all JSON types used by the HTTP server and TUI console.
//! It is gated behind `hotpath`, `utils`, or `tui` features.

mod formatted;
pub use formatted::*;

use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::channels::ChannelType;

/// State of a channel or stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ChannelState {
    #[default]
    Active,
    Closed,
    Notified,
}

impl ChannelState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChannelState::Active => "active",
            ChannelState::Closed => "closed",
            ChannelState::Notified => "notified",
        }
    }
}

impl std::fmt::Display for ChannelState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::fmt::Display for ChannelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelType::Bounded(size) => write!(f, "bounded[{}]", size),
            ChannelType::Unbounded => write!(f, "unbounded"),
            ChannelType::Oneshot => write!(f, "oneshot"),
        }
    }
}

impl Serialize for ChannelType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ChannelType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

        match s.as_str() {
            "unbounded" => Ok(ChannelType::Unbounded),
            "oneshot" => Ok(ChannelType::Oneshot),
            _ => {
                if let Some(inner) = s.strip_prefix("bounded[").and_then(|x| x.strip_suffix(']')) {
                    let size = inner
                        .parse()
                        .map_err(|_| serde::de::Error::custom("invalid bounded size"))?;
                    Ok(ChannelType::Bounded(size))
                } else {
                    Err(serde::de::Error::custom("invalid channel type"))
                }
            }
        }
    }
}

/// A single log entry for a message sent/received or item yielded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DataFlowLogEntry {
    pub index: u64,
    pub timestamp: u64,
    pub message: Option<String>,
    pub tid: Option<u64>,
}

impl DataFlowLogEntry {
    pub fn new(index: u64, timestamp: u64, message: Option<String>, tid: Option<u64>) -> Self {
        Self {
            index,
            timestamp,
            message,
            tid,
        }
    }
}

/// Serializable log response containing sent and received logs for channels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChannelLogs {
    pub id: u32,
    pub sent_logs: Vec<DataFlowLogEntry>,
    pub received_logs: Vec<DataFlowLogEntry>,
}

/// Serializable log response containing yielded logs for streams.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StreamLogs {
    pub id: u32,
    pub logs: Vec<DataFlowLogEntry>,
}

/// State of an instrumented future.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FutureState {
    #[default]
    Pending,
    Running,
    Suspended,
    Ready,
    Cancelled,
}

impl FutureState {
    pub fn as_str(&self) -> &'static str {
        match self {
            FutureState::Pending => "pending",
            FutureState::Running => "running",
            FutureState::Suspended => "suspended",
            FutureState::Ready => "ready",
            FutureState::Cancelled => "cancelled",
        }
    }
}

impl std::fmt::Display for FutureState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A single invocation/call of a future.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FutureLog {
    pub id: u32,
    pub future_id: u32,
    pub state: FutureState,
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

impl FutureLog {
    pub fn new(id: u32, future_id: u32) -> Self {
        Self {
            id,
            future_id,
            state: FutureState::default(),
            poll_count: 0,
            total_poll_duration_ns: 0,
            max_poll_duration_ns: 0,
            last_poll_duration_ns: 0,
            total_poll_alloc_bytes: None,
            total_poll_alloc_count: None,
            max_poll_alloc_bytes: None,
            last_poll_alloc_bytes: None,
            result: None,
        }
    }
}

/// Serializable response for future calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FutureLogsList {
    pub id: String,
    pub call_count: u64,
    pub total_polls: u64,
    pub total_poll_duration_ns: u64,
    pub total_poll_alloc_bytes: Option<u64>,
    pub total_poll_alloc_count: Option<u64>,
    pub calls: Vec<FutureLog>,
}

/// Thread metrics collected from the OS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ThreadMetrics {
    /// Operating system thread ID (Mach port on macOS)
    pub os_tid: u64,
    /// Thread name (if available)
    pub name: String,
    /// Thread run state as unified name (Running, Sleeping, Blocked, Stopped, Zombie)
    pub status: String,
    /// Native OS state code (e.g., "R", "S", "D" on Linux; "1", "2", "3" on macOS)
    pub status_code: String,
    /// CPU time spent in user mode (seconds)
    pub cpu_user: f64,
    /// CPU time spent in system/kernel mode (seconds)
    pub cpu_sys: f64,
    /// Total CPU time (user + system, seconds)
    pub cpu_total: f64,
    /// CPU usage percentage (based on delta from previous sample)
    /// None if this is the first sample
    pub cpu_percent: Option<f64>,
    /// Peak CPU usage percentage ever observed for this thread
    pub cpu_percent_max: Option<f64>,
    /// Lifetime average CPU utilization: (cpu_total / profiler_elapsed) * 100
    pub cpu_percent_avg: Option<f64>,
    /// Total bytes allocated by this thread (only with hotpath-alloc)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alloc_bytes: Option<u64>,
    /// Total bytes deallocated by this thread (only with hotpath-alloc)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dealloc_bytes: Option<u64>,
    /// Current memory held (alloc - dealloc)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_diff: Option<i64>,
}

impl ThreadMetrics {
    pub fn new(
        os_tid: u64,
        name: String,
        status: String,
        status_code: String,
        cpu_user: f64,
        cpu_sys: f64,
    ) -> Self {
        Self {
            os_tid,
            name,
            status,
            status_code,
            cpu_user,
            cpu_sys,
            cpu_total: cpu_user + cpu_sys,
            cpu_percent: None,
            cpu_percent_max: None,
            cpu_percent_avg: None,
            alloc_bytes: None,
            dealloc_bytes: None,
            mem_diff: None,
        }
    }
}

/// HTTP routes for the hotpath metrics server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// GET /functions_timing - Returns timing metrics for all functions
    FunctionsTiming,
    /// GET /functions_alloc - Returns allocation metrics for all functions
    FunctionsAlloc,
    /// GET /functions_cpu - Returns CPU snapshot envelope (status + last report)
    FunctionsCpu,
    /// POST /functions_cpu/snapshot - Triggers a new CPU snapshot capture
    FunctionsCpuSnapshot,
    /// GET /threads - Returns thread metrics
    Threads,
    /// GET /functions_timing/{id}/logs - Returns timing logs for a function
    FunctionTimingLogs { function_id: u32 },
    /// GET /functions_alloc/{id}/logs - Returns allocation logs for a function
    FunctionAllocLogs { function_id: u32 },
    /// GET /debug - Returns all debug log statistics
    Debug,
    /// GET /debug/dbg/{id}/logs - Returns logs for a dbg! entry
    DebugDbgLogs { id: u32 },
    /// GET /debug/val/{id}/logs - Returns logs for a val! entry
    DebugValLogs { id: u32 },
    /// GET /debug/gauge/{id}/logs - Returns logs for a gauge! entry
    DebugGaugeLogs { id: u32 },
    /// GET /channels - Returns channel statistics
    Channels,
    /// GET /channels/{id}/logs - Returns logs for a specific channel
    ChannelLogs { channel_id: u32 },
    /// GET /streams - Returns stream statistics
    Streams,
    /// GET /streams/{id}/logs - Returns logs for a specific stream
    StreamLogs { stream_id: u32 },
    /// GET /futures - Returns future statistics
    Futures,
    /// GET /futures/{id}/logs - Returns calls for a specific future
    FutureLogs { future_id: u32 },
    /// GET /tokio_runtime - Returns Tokio runtime metrics snapshot
    TokioRuntime,
    /// GET /profiler_status - Returns profiler uptime
    ProfilerStatus,
}

impl Route {
    /// Returns the path portion of the URL for this route.
    pub fn to_path(&self) -> String {
        match self {
            Route::FunctionsTiming => "/functions_timing".to_string(),
            Route::FunctionsAlloc => "/functions_alloc".to_string(),
            Route::FunctionsCpu => "/functions_cpu".to_string(),
            Route::FunctionsCpuSnapshot => "/functions_cpu/snapshot".to_string(),
            Route::Threads => "/threads".to_string(),
            Route::FunctionTimingLogs { function_id } => {
                format!("/functions_timing/{}/logs", function_id)
            }
            Route::FunctionAllocLogs { function_id } => {
                format!("/functions_alloc/{}/logs", function_id)
            }
            Route::Debug => "/debug".to_string(),
            Route::DebugDbgLogs { id } => format!("/debug/dbg/{}/logs", id),
            Route::DebugValLogs { id } => format!("/debug/val/{}/logs", id),
            Route::DebugGaugeLogs { id } => format!("/debug/gauge/{}/logs", id),
            Route::Channels => "/channels".to_string(),
            Route::ChannelLogs { channel_id } => format!("/channels/{}/logs", channel_id),
            Route::Streams => "/streams".to_string(),
            Route::StreamLogs { stream_id } => format!("/streams/{}/logs", stream_id),
            Route::Futures => "/futures".to_string(),
            Route::FutureLogs { future_id } => format!("/futures/{}/logs", future_id),
            Route::TokioRuntime => "/tokio_runtime".to_string(),
            Route::ProfilerStatus => "/profiler_status".to_string(),
        }
    }

    /// Returns the full URL for this route with the given port.
    pub fn to_url(&self, port: u16) -> String {
        format!("http://localhost:{}{}", port, self.to_path())
    }
}

fn parse_id_from_path(path: &str, prefix: &str) -> Option<u32> {
    let rest = path.strip_prefix(prefix)?;
    let id_str = rest.strip_suffix("/logs")?;
    id_str.parse().ok()
}

impl FromStr for Route {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let path = s.split('?').next().unwrap_or(s);

        match path {
            "/functions_timing" => return Ok(Route::FunctionsTiming),
            "/functions_alloc" => return Ok(Route::FunctionsAlloc),
            "/functions_cpu" => return Ok(Route::FunctionsCpu),
            "/functions_cpu/snapshot" => return Ok(Route::FunctionsCpuSnapshot),
            "/threads" => return Ok(Route::Threads),
            "/debug" => return Ok(Route::Debug),
            "/channels" => return Ok(Route::Channels),
            "/streams" => return Ok(Route::Streams),
            "/futures" => return Ok(Route::Futures),
            "/tokio_runtime" => return Ok(Route::TokioRuntime),
            "/profiler_status" => return Ok(Route::ProfilerStatus),
            _ => {}
        }

        if let Some(function_id) = parse_id_from_path(path, "/functions_timing/") {
            return Ok(Route::FunctionTimingLogs { function_id });
        }

        if let Some(function_id) = parse_id_from_path(path, "/functions_alloc/") {
            return Ok(Route::FunctionAllocLogs { function_id });
        }

        if let Some(id) = parse_id_from_path(path, "/debug/dbg/") {
            return Ok(Route::DebugDbgLogs { id });
        }

        if let Some(id) = parse_id_from_path(path, "/debug/val/") {
            return Ok(Route::DebugValLogs { id });
        }

        if let Some(id) = parse_id_from_path(path, "/debug/gauge/") {
            return Ok(Route::DebugGaugeLogs { id });
        }

        if let Some(channel_id) = parse_id_from_path(path, "/channels/") {
            return Ok(Route::ChannelLogs { channel_id });
        }

        if let Some(stream_id) = parse_id_from_path(path, "/streams/") {
            return Ok(Route::StreamLogs { stream_id });
        }

        if let Some(future_id) = parse_id_from_path(path, "/futures/") {
            return Ok(Route::FutureLogs { future_id });
        }

        Err(())
    }
}
