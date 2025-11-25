use crate::channels::{get_channel_logs, get_channels_json};
use crate::output::FunctionsJson;
use crate::streams::{get_stream_logs, get_streams_json};
use crate::{FunctionLogsJson, QueryRequest, HOTPATH_STATE};
use crossbeam_channel::bounded;
use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::{LazyLock, OnceLock};
use std::thread;
use std::time::Duration;
use tiny_http::{Header, Request, Response, Server};

static RE_CHANNEL_LOGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/channels/(\d+)/logs$").unwrap());
static RE_STREAM_LOGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/streams/(\d+)/logs$").unwrap());
static RE_FUNCTION_LOGS_TIMING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/functions_timing/([^/]+)/logs$").unwrap());
static RE_FUNCTION_LOGS_ALLOC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/functions_alloc/([^/]+)/logs$").unwrap());

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
        }
    }

    /// Returns the full URL for this route with the given port.
    pub fn to_url(&self, port: u16) -> String {
        format!("http://localhost:{}{}", port, self.to_path())
    }

    /// Parses a URL path into a Route using regex patterns.
    /// Returns None if the path doesn't match any known route.
    pub fn from_path(path: &str) -> Option<Self> {
        let path = path.split('?').next().unwrap_or(path);

        match path {
            "/functions_timing" => return Some(Route::FunctionsTiming),
            "/functions_alloc" => return Some(Route::FunctionsAlloc),
            "/channels" => return Some(Route::Channels),
            "/streams" => return Some(Route::Streams),
            "/threads" => return Some(Route::Threads),
            _ => {}
        }

        if let Some(caps) = RE_FUNCTION_LOGS_TIMING.captures(path) {
            let function_name = base64_decode(&caps[1]).ok()?;
            return Some(Route::FunctionTimingLogs { function_name });
        }

        if let Some(caps) = RE_FUNCTION_LOGS_ALLOC.captures(path) {
            let function_name = base64_decode(&caps[1]).ok()?;
            return Some(Route::FunctionAllocLogs { function_name });
        }

        if let Some(caps) = RE_CHANNEL_LOGS.captures(path) {
            let channel_id = caps[1].parse().ok()?;
            return Some(Route::ChannelLogs { channel_id });
        }

        if let Some(caps) = RE_STREAM_LOGS.captures(path) {
            let stream_id = caps[1].parse().ok()?;
            return Some(Route::StreamLogs { stream_id });
        }

        None
    }
}

/// Tracks whether the HTTP server has been started to prevent duplicate instances
static HTTP_SERVER_STARTED: OnceLock<()> = OnceLock::new();

/// Starts the HTTP metrics server if it hasn't been started yet.
/// Uses OnceLock to ensure only one server instance is created.
pub fn start_metrics_server_once(port: u16) {
    HTTP_SERVER_STARTED.get_or_init(|| {
        start_metrics_server(port);
    });
}

fn start_metrics_server(port: u16) {
    crate::threads::init_threads_monitoring();

    thread::Builder::new()
        .name("hotpath-http-server".into())
        .spawn(move || {
            let addr = format!("0.0.0.0:{}", port);
            let server = match Server::http(&addr) {
                Ok(s) => s,
                Err(e) => {
                    panic!(
                        "Failed to bind metrics server to {}: {}. Customize the port using the HOTPATH_HTTP_PORT environment variable.",
                        addr, e
                    );
                }
            };

            eprintln!("[hotpath] Metrics server listening on http://{}", addr);

            for request in server.incoming_requests() {
                handle_request(request);
            }
        })
        .expect("Failed to spawn HTTP metrics server thread");
}

fn handle_request(request: Request) {
    let path = request.url();

    match Route::from_path(path) {
        Some(Route::FunctionsTiming) => {
            let metrics = get_functions_timing_json();
            respond_json(request, &metrics);
        }
        Some(Route::FunctionsAlloc) => match get_functions_alloc_json() {
            Some(metrics) => respond_json(request, &metrics),
            None => respond_error(
                request,
                404,
                "Memory profiling not available - enable hotpath-alloc feature",
            ),
        },
        Some(Route::Channels) => {
            let channels = get_channels_json();
            respond_json(request, &channels);
        }
        Some(Route::Streams) => {
            let streams = get_streams_json();
            respond_json(request, &streams);
        }
        Some(Route::FunctionTimingLogs { function_name }) => {
            match get_function_logs_timing(&function_name) {
                Some(logs) => respond_json(request, &logs),
                None => respond_error(
                    request,
                    404,
                    &format!("Function '{}' not found", function_name),
                ),
            }
        }
        Some(Route::FunctionAllocLogs { function_name }) => {
            match get_function_logs_alloc(&function_name) {
                Some(logs) => respond_json(request, &logs),
                None => respond_error(
                    request,
                    404,
                    "Memory profiling not available - enable hotpath-alloc feature",
                ),
            }
        }
        Some(Route::ChannelLogs { channel_id }) => {
            match get_channel_logs(&channel_id.to_string()) {
                Some(logs) => respond_json(request, &logs),
                None => respond_error(request, 404, "Channel not found"),
            }
        }
        Some(Route::StreamLogs { stream_id }) => match get_stream_logs(&stream_id.to_string()) {
            Some(logs) => respond_json(request, &logs),
            None => respond_error(request, 404, "Stream not found"),
        },
        #[cfg(feature = "threads")]
        Some(Route::Threads) => {
            let threads = crate::threads::get_threads_json();
            respond_json(request, &threads);
        }
        #[cfg(not(feature = "threads"))]
        Some(Route::Threads) => {
            respond_error(
                request,
                404,
                "Thread monitoring not available - enable threads feature",
            );
        }
        None => respond_error(request, 404, "Not found"),
    }
}

fn respond_json<T: Serialize>(request: Request, value: &T) {
    match serde_json::to_vec(value) {
        Ok(body) => {
            let mut response = Response::from_data(body);
            response.add_header(
                Header::from_bytes(b"Content-Type".as_slice(), b"application/json".as_slice())
                    .unwrap(),
            );
            let _ = request.respond(response);
        }
        Err(e) => respond_internal_error(request, e),
    }
}

fn respond_error(request: Request, code: u16, msg: &str) {
    let _ = request.respond(Response::from_string(msg).with_status_code(code));
}

fn respond_internal_error(request: Request, e: impl Display) {
    eprintln!("Internal server error: {}", e);
    let _ = request.respond(
        Response::from_string(format!("Internal server error: {}", e)).with_status_code(500),
    );
}

fn base64_decode(encoded: &str) -> Result<String, String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

fn get_function_logs_timing(function_name: &str) -> Option<FunctionLogsJson> {
    let arc_swap = HOTPATH_STATE.get()?;
    let state_option = arc_swap.load();
    let state_arc = (*state_option).as_ref()?.clone();

    let state_guard = state_arc.read().ok()?;

    let (response_tx, response_rx) = bounded::<Option<FunctionLogsJson>>(1);

    if let Some(query_tx) = &state_guard.query_tx {
        query_tx
            .send(QueryRequest::GetFunctionLogsTiming {
                function_name: function_name.to_string(),
                response_tx,
            })
            .ok()?;
        drop(state_guard);

        response_rx
            .recv_timeout(Duration::from_millis(250))
            .ok()
            .flatten()
    } else {
        None
    }
}

fn get_function_logs_alloc(function_name: &str) -> Option<FunctionLogsJson> {
    let arc_swap = HOTPATH_STATE.get()?;
    let state_option = arc_swap.load();
    let state_arc = (*state_option).as_ref()?.clone();

    let state_guard = state_arc.read().ok()?;

    let (response_tx, response_rx) = bounded::<Option<FunctionLogsJson>>(1);

    if let Some(query_tx) = &state_guard.query_tx {
        query_tx
            .send(QueryRequest::GetFunctionLogsAlloc {
                function_name: function_name.to_string(),
                response_tx,
            })
            .ok()?;
        drop(state_guard);

        response_rx
            .recv_timeout(Duration::from_millis(250))
            .ok()
            .flatten()
    } else {
        None
    }
}

fn get_functions_timing_json() -> FunctionsJson {
    if let Some(metrics) = try_get_functions_timing_from_worker() {
        return metrics;
    }

    // Fallback if query fails: return empty functions data
    FunctionsJson {
        hotpath_profiling_mode: crate::output::ProfilingMode::Timing,
        total_elapsed: 0,
        description: "No timing data available yet".to_string(),
        caller_name: "hotpath".to_string(),
        percentiles: vec![95],
        data: crate::output::FunctionsDataJson(HashMap::new()),
    }
}

fn get_functions_alloc_json() -> Option<FunctionsJson> {
    let arc_swap = HOTPATH_STATE.get()?;
    let state_option = arc_swap.load();
    let state_arc = (*state_option).as_ref()?.clone();

    let state_guard = state_arc.read().ok()?;

    let (response_tx, response_rx) = bounded::<Option<FunctionsJson>>(1);

    if let Some(query_tx) = &state_guard.query_tx {
        query_tx
            .send(QueryRequest::GetFunctions(response_tx))
            .ok()?;
        drop(state_guard);

        // Flatten the Option<Option<FunctionsJson>> to Option<FunctionsJson>
        response_rx
            .recv_timeout(Duration::from_millis(250))
            .ok()
            .flatten()
    } else {
        None
    }
}

fn try_get_functions_timing_from_worker() -> Option<FunctionsJson> {
    let arc_swap = HOTPATH_STATE.get()?;
    let state_option = arc_swap.load();
    let state_arc = (*state_option).as_ref()?.clone();

    let state_guard = state_arc.read().ok()?;

    let (response_tx, response_rx) = bounded::<FunctionsJson>(1);

    if let Some(query_tx) = &state_guard.query_tx {
        query_tx
            .send(QueryRequest::GetFunctionsTiming(response_tx))
            .ok()?;
        drop(state_guard);

        response_rx.recv_timeout(Duration::from_millis(250)).ok()
    } else {
        None
    }
}
