use crate::functions::{
    get_function_logs_alloc, get_function_logs_timing, get_functions_alloc_json,
    get_functions_timing_json,
};
use crate::json::Route;
use std::sync::LazyLock;

pub(crate) static METRICS_SERVER_PORT: LazyLock<u16> = LazyLock::new(|| {
    std::env::var("HOTPATH_METRICS_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(6770)
});

pub(crate) static METRICS_SERVER_DISABLED: LazyLock<bool> = LazyLock::new(|| {
    std::env::var("HOTPATH_METRICS_SERVER_OFF")
        .ok()
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
});

pub(crate) static RECV_TIMEOUT_MS: u64 = 250;

use crate::channels::{get_channel_logs, get_channels_json};
use crate::futures::{get_future_calls, get_futures_json};
use crate::streams::{get_stream_logs, get_streams_json};
use serde::Serialize;
use std::fmt::Display;
use std::sync::OnceLock;
use std::thread;
use tiny_http::{Header, Request, Response, Server};

static HTTP_SERVER_STARTED: OnceLock<()> = OnceLock::new();

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
            .name("hp-server".into())
            .spawn(move || {
                let addr = format!("127.0.0.1:{}", port);
                let server = match Server::http(&addr) {
                    Ok(s) => s,
                    Err(e) => {
                        panic!(
                            "Failed to bind metrics server to {}: {}. Customize the port using the HOTPATH_METRICS_PORT environment variable.",
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
