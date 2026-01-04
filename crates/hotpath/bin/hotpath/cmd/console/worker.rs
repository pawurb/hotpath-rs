//! Data worker thread with Tokio runtime for async HTTP fetching

use crossbeam_channel::{Receiver, Sender};
use hotpath::json::{
    ChannelLogs, ChannelsJson, FunctionLogsJson, FunctionsJson, FutureCalls, FuturesJson, Route,
    StreamLogs, StreamsJson, ThreadsJson,
};
use std::time::Duration;
use tokio::runtime::Runtime;

use super::events::{AppEvent, DataRequest, DataResponse};

const HTTP_TIMEOUT_MS: u64 = 2000;

pub(crate) fn spawn_data_worker(
    request_rx: Receiver<DataRequest>,
    event_tx: Sender<AppEvent>,
    metrics_port: u16,
) {
    std::thread::spawn(move || {
        let rt = Runtime::new().expect("Failed to create Tokio runtime");
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(HTTP_TIMEOUT_MS))
            .build()
            .expect("Failed to create HTTP client");

        let base_url = format!("http://127.0.0.1:{}", metrics_port);

        loop {
            match request_rx.recv() {
                Ok(request) => {
                    // Spawn without cancelling previous - allows parallel fetches
                    let client = client.clone();
                    let base_url = base_url.clone();
                    let event_tx = event_tx.clone();

                    rt.spawn(async move {
                        let response = handle_request(&client, &base_url, request).await;
                        let _ = event_tx.send(AppEvent::Data(response));
                    });
                }
                Err(_) => break,
            }
        }
    });
}

async fn handle_request(
    client: &reqwest::Client,
    base_url: &str,
    request: DataRequest,
) -> DataResponse {
    match request {
        DataRequest::RefreshTiming => fetch_functions_timing(client, base_url).await,
        DataRequest::RefreshMemory => fetch_functions_alloc(client, base_url).await,
        DataRequest::RefreshChannels => fetch_channels(client, base_url).await,
        DataRequest::RefreshStreams => fetch_streams(client, base_url).await,
        DataRequest::RefreshThreads => fetch_threads(client, base_url).await,
        DataRequest::RefreshFutures => fetch_futures(client, base_url).await,
        DataRequest::FetchFunctionLogsTiming(name) => {
            fetch_function_logs_timing(client, base_url, name).await
        }
        DataRequest::FetchFunctionLogsAlloc(name) => {
            fetch_function_logs_alloc(client, base_url, name).await
        }
        DataRequest::FetchChannelLogs(id) => fetch_channel_logs(client, base_url, id).await,
        DataRequest::FetchStreamLogs(id) => fetch_stream_logs(client, base_url, id).await,
        DataRequest::FetchFutureCalls(id) => fetch_future_calls(client, base_url, id).await,
    }
}

async fn fetch_functions_timing(client: &reqwest::Client, base_url: &str) -> DataResponse {
    let url = format!("{}{}", base_url, Route::FunctionsTiming.to_path());
    match client.get(&url).send().await {
        Ok(resp) => match resp.error_for_status() {
            Ok(resp) => match resp.json::<FunctionsJson>().await {
                Ok(data) => DataResponse::FunctionsTiming(data),
                Err(e) => DataResponse::Error(format!("JSON parse error: {}", e)),
            },
            Err(e) => DataResponse::Error(format!("HTTP error: {}", e)),
        },
        Err(e) => DataResponse::Error(format!("Request failed: {}", e)),
    }
}

async fn fetch_functions_alloc(client: &reqwest::Client, base_url: &str) -> DataResponse {
    let url = format!("{}{}", base_url, Route::FunctionsAlloc.to_path());
    match client.get(&url).send().await {
        Ok(resp) if resp.status() == 404 => DataResponse::FunctionsAllocUnavailable,
        Ok(resp) => match resp.error_for_status() {
            Ok(resp) => match resp.json::<FunctionsJson>().await {
                Ok(data) => DataResponse::FunctionsAlloc(data),
                Err(e) => DataResponse::Error(format!("JSON parse error: {}", e)),
            },
            Err(e) => DataResponse::Error(format!("HTTP error: {}", e)),
        },
        Err(e) => DataResponse::Error(format!("Request failed: {}", e)),
    }
}

async fn fetch_channels(client: &reqwest::Client, base_url: &str) -> DataResponse {
    let url = format!("{}{}", base_url, Route::Channels.to_path());
    match client.get(&url).send().await {
        Ok(resp) => match resp.error_for_status() {
            Ok(resp) => match resp.json::<ChannelsJson>().await {
                Ok(data) => DataResponse::Channels(data),
                Err(e) => DataResponse::Error(format!("JSON parse error: {}", e)),
            },
            Err(e) => DataResponse::Error(format!("HTTP error: {}", e)),
        },
        Err(e) => DataResponse::Error(format!("Request failed: {}", e)),
    }
}

async fn fetch_streams(client: &reqwest::Client, base_url: &str) -> DataResponse {
    let url = format!("{}{}", base_url, Route::Streams.to_path());
    match client.get(&url).send().await {
        Ok(resp) => match resp.error_for_status() {
            Ok(resp) => match resp.json::<StreamsJson>().await {
                Ok(data) => DataResponse::Streams(data),
                Err(e) => DataResponse::Error(format!("JSON parse error: {}", e)),
            },
            Err(e) => DataResponse::Error(format!("HTTP error: {}", e)),
        },
        Err(e) => DataResponse::Error(format!("Request failed: {}", e)),
    }
}

async fn fetch_threads(client: &reqwest::Client, base_url: &str) -> DataResponse {
    let url = format!("{}{}", base_url, Route::Threads.to_path());
    match client.get(&url).send().await {
        Ok(resp) => match resp.error_for_status() {
            Ok(resp) => match resp.json::<ThreadsJson>().await {
                Ok(data) => DataResponse::Threads(data),
                Err(e) => DataResponse::Error(format!("JSON parse error: {}", e)),
            },
            Err(e) => DataResponse::Error(format!("HTTP error: {}", e)),
        },
        Err(e) => DataResponse::Error(format!("Request failed: {}", e)),
    }
}

async fn fetch_futures(client: &reqwest::Client, base_url: &str) -> DataResponse {
    let url = format!("{}{}", base_url, Route::Futures.to_path());
    match client.get(&url).send().await {
        Ok(resp) => match resp.error_for_status() {
            Ok(resp) => match resp.json::<FuturesJson>().await {
                Ok(data) => DataResponse::Futures(data),
                Err(e) => DataResponse::Error(format!("JSON parse error: {}", e)),
            },
            Err(e) => DataResponse::Error(format!("HTTP error: {}", e)),
        },
        Err(e) => DataResponse::Error(format!("Request failed: {}", e)),
    }
}

async fn fetch_function_logs_timing(
    client: &reqwest::Client,
    base_url: &str,
    function_name: String,
) -> DataResponse {
    let route = Route::FunctionTimingLogs {
        function_name: function_name.clone(),
    };
    let url = format!("{}{}", base_url, route.to_path());
    match client.get(&url).send().await {
        Ok(resp) if resp.status() == 404 => DataResponse::FunctionLogsTimingNotFound(function_name),
        Ok(resp) => match resp.error_for_status() {
            Ok(resp) => match resp.json::<FunctionLogsJson>().await {
                Ok(logs) => DataResponse::FunctionLogsTiming {
                    function_name,
                    logs,
                },
                Err(e) => DataResponse::Error(format!("JSON parse error: {}", e)),
            },
            Err(e) => DataResponse::Error(format!("HTTP error: {}", e)),
        },
        Err(e) => DataResponse::Error(format!("Request failed: {}", e)),
    }
}

async fn fetch_function_logs_alloc(
    client: &reqwest::Client,
    base_url: &str,
    function_name: String,
) -> DataResponse {
    let route = Route::FunctionAllocLogs {
        function_name: function_name.clone(),
    };
    let url = format!("{}{}", base_url, route.to_path());
    match client.get(&url).send().await {
        Ok(resp) if resp.status() == 404 => DataResponse::FunctionLogsAllocNotFound(function_name),
        Ok(resp) => match resp.error_for_status() {
            Ok(resp) => match resp.json::<FunctionLogsJson>().await {
                Ok(logs) => DataResponse::FunctionLogsAlloc {
                    function_name,
                    logs,
                },
                Err(e) => DataResponse::Error(format!("JSON parse error: {}", e)),
            },
            Err(e) => DataResponse::Error(format!("HTTP error: {}", e)),
        },
        Err(e) => DataResponse::Error(format!("Request failed: {}", e)),
    }
}

async fn fetch_channel_logs(
    client: &reqwest::Client,
    base_url: &str,
    channel_id: u64,
) -> DataResponse {
    let route = Route::ChannelLogs { channel_id };
    let url = format!("{}{}", base_url, route.to_path());
    match client.get(&url).send().await {
        Ok(resp) => match resp.error_for_status() {
            Ok(resp) => match resp.json::<ChannelLogs>().await {
                Ok(logs) => DataResponse::ChannelLogs { channel_id, logs },
                Err(e) => DataResponse::Error(format!("JSON parse error: {}", e)),
            },
            Err(e) => DataResponse::Error(format!("HTTP error: {}", e)),
        },
        Err(e) => DataResponse::Error(format!("Request failed: {}", e)),
    }
}

async fn fetch_stream_logs(
    client: &reqwest::Client,
    base_url: &str,
    stream_id: u64,
) -> DataResponse {
    let route = Route::StreamLogs { stream_id };
    let url = format!("{}{}", base_url, route.to_path());
    match client.get(&url).send().await {
        Ok(resp) => match resp.error_for_status() {
            Ok(resp) => match resp.json::<StreamLogs>().await {
                Ok(logs) => DataResponse::StreamLogs { stream_id, logs },
                Err(e) => DataResponse::Error(format!("JSON parse error: {}", e)),
            },
            Err(e) => DataResponse::Error(format!("HTTP error: {}", e)),
        },
        Err(e) => DataResponse::Error(format!("Request failed: {}", e)),
    }
}

async fn fetch_future_calls(
    client: &reqwest::Client,
    base_url: &str,
    future_id: u64,
) -> DataResponse {
    let route = Route::FutureCalls { future_id };
    let url = format!("{}{}", base_url, route.to_path());
    match client.get(&url).send().await {
        Ok(resp) => match resp.error_for_status() {
            Ok(resp) => match resp.json::<FutureCalls>().await {
                Ok(calls) => DataResponse::FutureCalls { future_id, calls },
                Err(e) => DataResponse::Error(format!("JSON parse error: {}", e)),
            },
            Err(e) => DataResponse::Error(format!("HTTP error: {}", e)),
        },
        Err(e) => DataResponse::Error(format!("Request failed: {}", e)),
    }
}
