//! JSON serializable types for TUI and CLI consumers.
//!
//! This module contains all JSON types used by the HTTP server and TUI console.
//! It is gated behind `hotpath`, `tui`, or `ci` features.

mod formatted;
pub use formatted::*;

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::LazyLock;

pub use crate::output::FunctionLogsList;

/// State of a channel or stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelState {
    #[default]
    Active,
    Closed,
    Full,
    Notified,
}

impl ChannelState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChannelState::Active => "active",
            ChannelState::Closed => "closed",
            ChannelState::Full => "full",
            ChannelState::Notified => "notified",
        }
    }
}

impl std::fmt::Display for ChannelState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Type of a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelType {
    Bounded(usize),
    Unbounded,
    Oneshot,
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
pub struct DataFlowLogEntry {
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
pub struct ChannelLogs {
    pub id: String,
    pub sent_logs: Vec<DataFlowLogEntry>,
    pub received_logs: Vec<DataFlowLogEntry>,
}

/// Serializable log response containing yielded logs for streams.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamLogs {
    pub id: String,
    pub logs: Vec<DataFlowLogEntry>,
}

/// State of an instrumented future.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FutureState {
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
pub struct FutureLog {
    pub id: u64,
    pub future_id: u64,
    pub state: FutureState,
    pub poll_count: u64,
    pub result: Option<String>,
}

impl FutureLog {
    pub fn new(id: u64, future_id: u64) -> Self {
        Self {
            id,
            future_id,
            state: FutureState::default(),
            poll_count: 0,
            result: None,
        }
    }
}

/// Serializable response for future calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FutureLogsList {
    pub id: String,
    pub calls: Vec<FutureLog>,
}

/// Thread metrics collected from the OS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadMetrics {
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
    /// GET /threads - Returns thread metrics
    Threads,
    /// GET /functions_timing/{base64_name}/logs - Returns timing logs for a function
    FunctionTimingLogs { function_name: String },
    /// GET /functions_alloc/{base64_name}/logs - Returns allocation logs for a function
    FunctionAllocLogs { function_name: String },
    /// GET /debug - Returns all debug log statistics
    Debug,
    /// GET /debug/dbg/{id}/logs - Returns logs for a dbg! entry
    DebugDbgLogs { id: u64 },
    /// GET /debug/val/{id}/logs - Returns logs for a val! entry
    DebugValLogs { id: u64 },
    /// GET /debug/gauge/{id}/logs - Returns logs for a gauge! entry
    DebugGaugeLogs { id: u64 },
    /// GET /data_flow - Returns unified channels, streams, and futures statistics
    DataFlow,
    /// GET /data_flow/channel/{id}/logs - Returns logs for a specific channel
    DataFlowChannelLogs { channel_id: u64 },
    /// GET /data_flow/stream/{id}/logs - Returns logs for a specific stream
    DataFlowStreamLogs { stream_id: u64 },
    /// GET /data_flow/future/{id}/logs - Returns calls for a specific future
    DataFlowFutureLogs { future_id: u64 },
    /// GET /tokio_runtime - Returns Tokio runtime metrics snapshot
    TokioRuntime,
}

impl Route {
    /// Returns the path portion of the URL for this route.
    pub fn to_path(&self) -> String {
        use base64::Engine;
        match self {
            Route::FunctionsTiming => "/functions_timing".to_string(),
            Route::FunctionsAlloc => "/functions_alloc".to_string(),
            Route::Threads => "/threads".to_string(),
            Route::FunctionTimingLogs { function_name } => {
                let encoded =
                    base64::engine::general_purpose::STANDARD.encode(function_name.as_bytes());
                format!("/functions_timing/{}/logs", encoded)
            }
            Route::FunctionAllocLogs { function_name } => {
                let encoded =
                    base64::engine::general_purpose::STANDARD.encode(function_name.as_bytes());
                format!("/functions_alloc/{}/logs", encoded)
            }
            Route::Debug => "/debug".to_string(),
            Route::DebugDbgLogs { id } => format!("/debug/dbg/{}/logs", id),
            Route::DebugValLogs { id } => format!("/debug/val/{}/logs", id),
            Route::DebugGaugeLogs { id } => format!("/debug/gauge/{}/logs", id),
            Route::DataFlow => "/data_flow".to_string(),
            Route::DataFlowChannelLogs { channel_id } => {
                format!("/data_flow/channel/{}/logs", channel_id)
            }
            Route::DataFlowStreamLogs { stream_id } => {
                format!("/data_flow/stream/{}/logs", stream_id)
            }
            Route::DataFlowFutureLogs { future_id } => {
                format!("/data_flow/future/{}/logs", future_id)
            }
            Route::TokioRuntime => "/tokio_runtime".to_string(),
        }
    }

    /// Returns the full URL for this route with the given port.
    pub fn to_url(&self, port: u16) -> String {
        format!("http://localhost:{}{}", port, self.to_path())
    }
}

static RE_FUNCTION_LOGS_TIMING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/functions_timing/([^/]+)/logs$").unwrap());
static RE_FUNCTION_LOGS_ALLOC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/functions_alloc/([^/]+)/logs$").unwrap());
static RE_DEBUG_DBG_LOGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/debug/dbg/(\d+)/logs$").unwrap());
static RE_DEBUG_VAL_LOGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/debug/val/(\d+)/logs$").unwrap());
static RE_DEBUG_GAUGE_LOGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/debug/gauge/(\d+)/logs$").unwrap());
static RE_DATA_FLOW_CHANNEL_LOGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/data_flow/channel/(\d+)/logs$").unwrap());
static RE_DATA_FLOW_STREAM_LOGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/data_flow/stream/(\d+)/logs$").unwrap());
static RE_DATA_FLOW_FUTURE_LOGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/data_flow/future/(\d+)/logs$").unwrap());

fn base64_decode(encoded: &str) -> Result<String, String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

impl FromStr for Route {
    type Err = ();

    /// Parses a URL path into a Route using regex patterns.
    /// Returns Err(()) if the path doesn't match any known route.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let path = s.split('?').next().unwrap_or(s);

        match path {
            "/functions_timing" => return Ok(Route::FunctionsTiming),
            "/functions_alloc" => return Ok(Route::FunctionsAlloc),
            "/threads" => return Ok(Route::Threads),
            "/debug" => return Ok(Route::Debug),
            "/data_flow" => return Ok(Route::DataFlow),
            "/tokio_runtime" => return Ok(Route::TokioRuntime),
            _ => {}
        }

        if let Some(caps) = RE_FUNCTION_LOGS_TIMING.captures(path) {
            let function_name = base64_decode(&caps[1]).map_err(|_| ())?;
            return Ok(Route::FunctionTimingLogs { function_name });
        }

        if let Some(caps) = RE_FUNCTION_LOGS_ALLOC.captures(path) {
            let function_name = base64_decode(&caps[1]).map_err(|_| ())?;
            return Ok(Route::FunctionAllocLogs { function_name });
        }

        if let Some(caps) = RE_DEBUG_DBG_LOGS.captures(path) {
            let id = caps[1].parse().map_err(|_| ())?;
            return Ok(Route::DebugDbgLogs { id });
        }

        if let Some(caps) = RE_DEBUG_VAL_LOGS.captures(path) {
            let id = caps[1].parse().map_err(|_| ())?;
            return Ok(Route::DebugValLogs { id });
        }

        if let Some(caps) = RE_DEBUG_GAUGE_LOGS.captures(path) {
            let id = caps[1].parse().map_err(|_| ())?;
            return Ok(Route::DebugGaugeLogs { id });
        }

        if let Some(caps) = RE_DATA_FLOW_CHANNEL_LOGS.captures(path) {
            let channel_id = caps[1].parse().map_err(|_| ())?;
            return Ok(Route::DataFlowChannelLogs { channel_id });
        }

        if let Some(caps) = RE_DATA_FLOW_STREAM_LOGS.captures(path) {
            let stream_id = caps[1].parse().map_err(|_| ())?;
            return Ok(Route::DataFlowStreamLogs { stream_id });
        }

        if let Some(caps) = RE_DATA_FLOW_FUTURE_LOGS.captures(path) {
            let future_id = caps[1].parse().map_err(|_| ())?;
            return Ok(Route::DataFlowFutureLogs { future_id });
        }

        Err(())
    }
}
