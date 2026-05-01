use crate::debug::dbg::{get_dbg_logs, get_debug_entries_json};
use crate::debug::gauge::get_debug_gauge_logs;
use crate::debug::val::get_val_logs;
use crate::functions::{
    get_function_logs_alloc, get_function_logs_timing, get_functions_alloc_json,
    get_functions_timing_json,
};
use crate::json::Route;
use crate::json::{
    JsonChannelLogsList, JsonFunctionAllocLogsList, JsonFunctionTimingLogsList, JsonFutureLogsList,
    JsonProfilerStatus, JsonStreamLogsList,
};
use crate::lib_on::START_TIME;
use crate::output::format_duration;
use std::sync::LazyLock;

pub(crate) static METRICS_SERVER_PORT: LazyLock<u16> = LazyLock::new(|| {
    std::env::var("HOTPATH_META_METRICS_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(6780)
});

pub(crate) static METRICS_SERVER_DISABLED: LazyLock<bool> = LazyLock::new(|| {
    std::env::var("HOTPATH_META_METRICS_SERVER_OFF")
        .ok()
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
});

pub(crate) static RECV_TIMEOUT_MS: u64 = 250;

const TOKIO_RUNTIME_HINT: &str =
    "Tokio runtime metrics not available - use hotpath_meta::tokio_runtime!() to start collection";

use crate::channels::{get_channel_logs, get_channels_json};
use crate::futures::{get_future_logs_list, get_futures_json};
use crate::streams::{get_stream_logs, get_streams_json};
use serde::Serialize;
use std::fmt::Display;
use std::sync::OnceLock;
use std::thread;
use tiny_http::{Header, Request, Response, Server};

static HTTP_SERVER_STARTED: OnceLock<()> = OnceLock::new();
static METRICS_SERVER_ERROR: OnceLock<String> = OnceLock::new();

pub(crate) fn get_metrics_server_error() -> Option<&'static str> {
    METRICS_SERVER_ERROR.get().map(|s| s.as_str())
}

pub(crate) fn start_metrics_server_once(port: u16) {
    if *METRICS_SERVER_DISABLED {
        return;
    }
    HTTP_SERVER_STARTED.get_or_init(|| {
        start_metrics_server(port);
    });
}

fn start_metrics_server(port: u16) {
    #[cfg(feature = "threads")]
    crate::threads::init_threads_monitoring();

    thread::Builder::new()
        .name("hp-meta-server".into())
        .spawn(move || {
            let _suspend = crate::lib_on::SuspendAllocTracking::new();
            let addr = format!("127.0.0.1:{}", port);
            let server = match Server::http(&addr) {
                Ok(s) => s,
                Err(e) => {
                    let msg = format!(
                        "{} busy ({}), skipping metrics server start. Use HOTPATH_META_METRICS_PORT to change the port.",
                        addr, e
                    );
                    eprintln!("[hotpath-meta - error] {}", msg);
                    let _ = METRICS_SERVER_ERROR.set(msg);
                    return;
                }
            };

            for request in server.incoming_requests() {
                handle_request(request);
            }
        })
        .expect("Failed to spawn HTTP metrics server thread");
}

fn handle_request(request: Request) {
    let path = request.url();

    match path.parse::<Route>() {
        Ok(Route::FunctionsTiming) => {
            let formatted = get_functions_timing_json();
            respond_json(request, &formatted);
        }
        Ok(Route::FunctionsAlloc) => match get_functions_alloc_json() {
            Some(formatted) => respond_json(request, &formatted),
            None => respond_error(
                request,
                404,
                "Memory profiling not available - enable hotpath-alloc-meta feature",
            ),
        },
        Ok(Route::FunctionTimingLogs { function_id }) => {
            match get_function_logs_timing(function_id) {
                Some(logs) => {
                    let formatted =
                        JsonFunctionTimingLogsList::from_logs(&logs, get_current_elapsed_ns());
                    respond_json(request, &formatted);
                }
                None => respond_error(
                    request,
                    404,
                    &format!("Function with id {} not found", function_id),
                ),
            }
        }
        Ok(Route::FunctionAllocLogs { function_id }) => {
            match get_function_logs_alloc(function_id) {
                Some(logs) => {
                    let formatted =
                        JsonFunctionAllocLogsList::from_logs(&logs, get_current_elapsed_ns());
                    respond_json(request, &formatted);
                }
                None => respond_error(
                    request,
                    404,
                    "Memory profiling not available - enable hotpath-alloc-meta feature",
                ),
            }
        }
        Ok(Route::Debug) => {
            let debug_stats = get_debug_entries_json();
            respond_json(request, &debug_stats);
        }
        Ok(Route::Channels) => {
            let channels = get_channels_json();
            respond_json(request, &channels);
        }
        Ok(Route::Streams) => {
            let streams = get_streams_json();
            respond_json(request, &streams);
        }
        Ok(Route::Futures) => {
            let futures = get_futures_json();
            respond_json(request, &futures);
        }
        Ok(Route::ChannelLogs { channel_id }) => match get_channel_logs(channel_id) {
            Some(logs) => {
                let formatted = JsonChannelLogsList::from_logs(&logs, get_current_elapsed_ns());
                respond_json(request, &formatted);
            }
            None => respond_error(request, 404, "Channel not found"),
        },
        Ok(Route::StreamLogs { stream_id }) => match get_stream_logs(stream_id) {
            Some(logs) => {
                let formatted = JsonStreamLogsList::from_logs(&logs, get_current_elapsed_ns());
                respond_json(request, &formatted);
            }
            None => respond_error(request, 404, "Stream not found"),
        },
        Ok(Route::FutureLogs { future_id }) => match get_future_logs_list(future_id) {
            Some(calls) => {
                let formatted = JsonFutureLogsList::from(&calls);
                respond_json(request, &formatted);
            }
            None => respond_error(request, 404, "Future not found"),
        },
        Ok(Route::DebugDbgLogs { id }) => match get_dbg_logs(id) {
            Some(formatted) => respond_json(request, &formatted),
            None => respond_error(request, 404, "Debug entry not found"),
        },
        Ok(Route::DebugValLogs { id }) => match get_val_logs(id) {
            Some(formatted) => respond_json(request, &formatted),
            None => respond_error(request, 404, "Value entry not found"),
        },
        Ok(Route::DebugGaugeLogs { id }) => match get_debug_gauge_logs(id) {
            Some(logs) => respond_json(request, &logs),
            None => respond_error(request, 404, "Gauge entry not found"),
        },
        #[cfg(feature = "threads")]
        Ok(Route::Threads) => {
            let threads = crate::threads::get_threads_json();
            respond_json(request, &threads);
        }
        #[cfg(not(feature = "threads"))]
        Ok(Route::Threads) => {
            respond_error(
                request,
                404,
                "Thread monitoring not available - enable threads feature",
            );
        }
        Ok(Route::TokioRuntime) => {
            #[cfg(feature = "tokio")]
            match crate::tokio_runtime::get_runtime_json() {
                Some(snapshot) => respond_json(request, &snapshot),
                None => respond_error(request, 404, TOKIO_RUNTIME_HINT),
            }
            #[cfg(not(feature = "tokio"))]
            respond_error(request, 404, TOKIO_RUNTIME_HINT);
        }
        Ok(Route::ProfilerStatus) => {
            let status = JsonProfilerStatus {
                uptime: format_duration(get_current_elapsed_ns()),
                pid: std::process::id(),
            };
            respond_json(request, &status);
        }
        Err(_) => respond_error(request, 404, "Not found"),
    }
}

fn get_current_elapsed_ns() -> u64 {
    START_TIME
        .get()
        .map(|start| start.elapsed().as_nanos() as u64)
        .unwrap_or(0)
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
    let body = format!(r#"{{"error":"{}"}}"#, msg);
    let mut response = Response::from_string(body).with_status_code(code);
    response.add_header(
        Header::from_bytes(b"Content-Type".as_slice(), b"application/json".as_slice()).unwrap(),
    );
    let _ = request.respond(response);
}

fn respond_internal_error(request: Request, e: impl Display) {
    eprintln!("Internal server error: {}", e);
    let _ = request.respond(
        Response::from_string(format!("Internal server error: {}", e)).with_status_code(500),
    );
}
