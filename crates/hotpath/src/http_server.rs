use crate::json::Route;
use std::sync::LazyLock;

pub(crate) static HTTP_SERVER_PORT: LazyLock<u16> = LazyLock::new(|| {
    std::env::var("HOTPATH_HTTP_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(6770)
});

use crate::channels::{get_channel_logs, get_channels_json};
use crate::futures::{get_future_calls, get_futures_json};
use crate::output::FunctionsJson;
use crate::streams::{get_stream_logs, get_streams_json};
use crate::{FunctionLogsJson, QueryRequest, HOTPATH_STATE};
use crossbeam_channel::bounded;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;
use tiny_http::{Header, Request, Response, Server};

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
    #[cfg(feature = "threads")]
    crate::threads::init_threads_monitoring();

    thread::Builder::new()
            .name("hp-server".into())
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

    match path.parse::<Route>() {
        Ok(Route::FunctionsTiming) => {
            let metrics = get_functions_timing_json();
            respond_json(request, &metrics);
        }
        Ok(Route::FunctionsAlloc) => match get_functions_alloc_json() {
            Some(metrics) => respond_json(request, &metrics),
            None => respond_error(
                request,
                404,
                "Memory profiling not available - enable hotpath-alloc feature",
            ),
        },
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
        Ok(Route::FunctionTimingLogs { function_name }) => {
            match get_function_logs_timing(&function_name) {
                Some(logs) => respond_json(request, &logs),
                None => respond_error(
                    request,
                    404,
                    &format!("Function '{}' not found", function_name),
                ),
            }
        }
        Ok(Route::FunctionAllocLogs { function_name }) => {
            match get_function_logs_alloc(&function_name) {
                Some(logs) => respond_json(request, &logs),
                None => respond_error(
                    request,
                    404,
                    "Memory profiling not available - enable hotpath-alloc feature",
                ),
            }
        }
        Ok(Route::ChannelLogs { channel_id }) => match get_channel_logs(&channel_id.to_string()) {
            Some(logs) => respond_json(request, &logs),
            None => respond_error(request, 404, "Channel not found"),
        },
        Ok(Route::StreamLogs { stream_id }) => match get_stream_logs(&stream_id.to_string()) {
            Some(logs) => respond_json(request, &logs),
            None => respond_error(request, 404, "Stream not found"),
        },
        Ok(Route::FutureCalls { future_id }) => match get_future_calls(future_id) {
            Some(calls) => respond_json(request, &calls),
            None => respond_error(request, 404, "Future not found"),
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
        Err(_) => respond_error(request, 404, "Not found"),
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
