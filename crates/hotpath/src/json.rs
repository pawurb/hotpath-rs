//! JSON serializable types for TUI and CLI consumers.
//!
//! This module contains all JSON types used by the HTTP server and TUI console.
//! It is gated behind `hotpath`, `tui`, or `ci` features.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::LazyLock;

pub use crate::output::{FunctionLogsJson, FunctionsJson};

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
pub struct LogEntry {
    pub index: u64,
    pub timestamp: u64,
    pub message: Option<String>,
    pub tid: Option<u64>,
}

impl LogEntry {
    pub fn new(index: u64, timestamp: u64, message: Option<String>, tid: Option<u64>) -> Self {
        Self {
            index,
            timestamp,
            message,
            tid,
        }
    }
}

/// Wrapper for channels-only JSON response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelsJson {
    /// Current elapsed time since program start in nanoseconds
    pub current_elapsed_ns: u64,
    /// Channel statistics
    pub channels: Vec<SerializableChannelStats>,
}

/// Serializable version of channel statistics for JSON responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableChannelStats {
    pub id: u64,
    pub source: String,
    pub label: String,
    pub has_custom_label: bool,
    pub channel_type: ChannelType,
    pub state: ChannelState,
    pub sent_count: u64,
    pub received_count: u64,
    pub queued: u64,
    pub type_name: String,
    pub type_size: usize,
    pub queued_bytes: u64,
    pub iter: u32,
}

/// Serializable log response containing sent and received logs for channels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelLogs {
    pub id: String,
    pub sent_logs: Vec<LogEntry>,
    pub received_logs: Vec<LogEntry>,
}

/// Wrapper for streams-only JSON response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamsJson {
    /// Current elapsed time since program start in nanoseconds
    pub current_elapsed_ns: u64,
    /// Stream statistics
    pub streams: Vec<SerializableStreamStats>,
}

/// Serializable version of stream statistics for JSON responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableStreamStats {
    pub id: u64,
    pub source: String,
    pub label: String,
    pub has_custom_label: bool,
    pub state: ChannelState,
    pub items_yielded: u64,
    pub type_name: String,
    pub type_size: usize,
    pub iter: u32,
}

/// Serializable log response containing yielded logs for streams.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamLogs {
    pub id: String,
    pub logs: Vec<LogEntry>,
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
pub struct FutureCall {
    pub id: u64,
    pub future_id: u64,
    pub state: FutureState,
    pub poll_count: u64,
    pub result: Option<String>,
}

impl FutureCall {
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

/// Wrapper for futures-only JSON response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturesJson {
    pub current_elapsed_ns: u64,
    pub futures: Vec<SerializableFutureStats>,
}

/// Serializable version of future statistics for JSON responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableFutureStats {
    pub id: u64,
    pub source: String,
    pub label: String,
    pub has_custom_label: bool,
    pub call_count: u64,
    pub total_polls: u64,
}

/// Serializable response for future calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FutureCalls {
    pub id: String,
    pub calls: Vec<FutureCall>,
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

/// JSON response structure for /threads endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadsJson {
    /// Current elapsed time since program start in nanoseconds
    pub current_elapsed_ns: u64,
    /// Sample interval in milliseconds
    pub sample_interval_ms: u64,
    /// Thread metrics
    pub threads: Vec<ThreadMetrics>,
    /// Total number of threads
    pub thread_count: usize,
    /// Process RSS (Resident Set Size) in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rss_bytes: Option<u64>,
}

/// HTTP routes for the hotpath metrics server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// GET /functions_timing - Returns timing metrics for all functions
    FunctionsTiming,
    /// GET /functions_alloc - Returns allocation metrics for all functions
    FunctionsAlloc,
    /// GET /channels - Returns all channel statistics
    Channels,
    /// GET /streams - Returns all stream statistics
    Streams,
    /// GET /futures - Returns all future statistics
    Futures,
    /// GET /threads - Returns thread metrics
    Threads,
    /// GET /functions_timing/{base64_name}/logs - Returns timing logs for a function
    FunctionTimingLogs { function_name: String },
    /// GET /functions_alloc/{base64_name}/logs - Returns allocation logs for a function
    FunctionAllocLogs { function_name: String },
    /// GET /channels/{id}/logs - Returns logs for a specific channel
    ChannelLogs { channel_id: u64 },
    /// GET /streams/{id}/logs - Returns logs for a specific stream
    StreamLogs { stream_id: u64 },
    /// GET /futures/{id}/calls - Returns calls for a specific future
    FutureCalls { future_id: u64 },
}

impl Route {
    /// Returns the path portion of the URL for this route.
    pub fn to_path(&self) -> String {
        use base64::Engine;
        match self {
            Route::FunctionsTiming => "/functions_timing".to_string(),
            Route::FunctionsAlloc => "/functions_alloc".to_string(),
            Route::Channels => "/channels".to_string(),
            Route::Streams => "/streams".to_string(),
            Route::Futures => "/futures".to_string(),
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
            Route::ChannelLogs { channel_id } => format!("/channels/{}/logs", channel_id),
            Route::StreamLogs { stream_id } => format!("/streams/{}/logs", stream_id),
            Route::FutureCalls { future_id } => format!("/futures/{}/calls", future_id),
        }
    }

    /// Returns the full URL for this route with the given port.
    pub fn to_url(&self, port: u16) -> String {
        format!("http://localhost:{}{}", port, self.to_path())
    }
}

static RE_CHANNEL_LOGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/channels/(\d+)/logs$").unwrap());
static RE_STREAM_LOGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/streams/(\d+)/logs$").unwrap());
static RE_FUTURE_CALLS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/futures/(\d+)/calls$").unwrap());
static RE_FUNCTION_LOGS_TIMING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/functions_timing/([^/]+)/logs$").unwrap());
static RE_FUNCTION_LOGS_ALLOC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/functions_alloc/([^/]+)/logs$").unwrap());

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
            "/channels" => return Ok(Route::Channels),
            "/streams" => return Ok(Route::Streams),
            "/futures" => return Ok(Route::Futures),
            "/threads" => return Ok(Route::Threads),
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

        if let Some(caps) = RE_CHANNEL_LOGS.captures(path) {
            let channel_id = caps[1].parse().map_err(|_| ())?;
            return Ok(Route::ChannelLogs { channel_id });
        }

        if let Some(caps) = RE_STREAM_LOGS.captures(path) {
            let stream_id = caps[1].parse().map_err(|_| ())?;
            return Ok(Route::StreamLogs { stream_id });
        }

        if let Some(caps) = RE_FUTURE_CALLS.captures(path) {
            let future_id = caps[1].parse().map_err(|_| ())?;
            return Ok(Route::FutureCalls { future_id });
        }

        Err(())
    }
}
