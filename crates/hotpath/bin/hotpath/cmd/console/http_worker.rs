//! Data worker thread with Tokio runtime for async HTTP fetching

use crossbeam_channel::{Receiver, Sender};
use hotpath::json::{
    ChannelLogs, ChannelsJson, FunctionLogsJson, FunctionsJson, FutureCalls, FuturesJson, Route,
    StreamLogs, StreamsJson, ThreadsJson,
};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{runtime::Runtime, task::JoinHandle};
use tracing::{error, info, trace, warn};

use crate::cmd::console::events::{AppEvent, DataRequest, DataResponse};

const HTTP_TIMEOUT_MS: u64 = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RequestKey {
    Timing,
    Memory,
    Channels,
    Streams,
    Threads,
    Futures,
    FunctionLogsTiming,
    FunctionLogsAlloc,
    ChannelLogs,
    StreamLogs,
    FutureCalls,
}

impl DataRequest {
    fn key(&self) -> RequestKey {
        match self {
            DataRequest::RefreshTiming => RequestKey::Timing,
            DataRequest::RefreshMemory => RequestKey::Memory,
            DataRequest::RefreshChannels => RequestKey::Channels,
            DataRequest::RefreshStreams => RequestKey::Streams,
            DataRequest::RefreshThreads => RequestKey::Threads,
            DataRequest::RefreshFutures => RequestKey::Futures,
            DataRequest::FetchFunctionLogsTiming(_) => RequestKey::FunctionLogsTiming,
            DataRequest::FetchFunctionLogsAlloc(_) => RequestKey::FunctionLogsAlloc,
            DataRequest::FetchChannelLogs(_) => RequestKey::ChannelLogs,
            DataRequest::FetchStreamLogs(_) => RequestKey::StreamLogs,
            DataRequest::FetchFutureCalls(_) => RequestKey::FutureCalls,
        }
    }
}

pub(crate) fn spawn_http_worker(
    request_rx: Receiver<DataRequest>,
    event_tx: Sender<AppEvent>,
    base_url: String,
) {
    std::thread::spawn(move || {
        info!("HTTP worker started, connecting to {}", base_url);
        let rt = Runtime::new().expect("Failed to create Tokio runtime");
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(HTTP_TIMEOUT_MS))
            .build()
            .expect("Failed to create HTTP client");

        let base_url = Arc::new(base_url);
        let mut active_tasks: HashMap<RequestKey, JoinHandle<()>> = HashMap::new();

        while let Ok(request) = request_rx.recv() {
            let key = request.key();
            trace!("Received request: {:?}", key);

            if let Some(handle) = active_tasks.remove(&key) {
                if !handle.is_finished() {
                    trace!("Aborting in-flight request for {:?}", key);
                    handle.abort();
                }
            }

            let client = client.clone();
            let base_url = base_url.clone();
            let event_tx = event_tx.clone();

            let handle = rt.spawn(async move {
                let response = request.to_route().fetch(&client, &base_url).await;
                let _ = event_tx.send(AppEvent::Data(response));
            });

            active_tasks.insert(key, handle);
        }
        info!("HTTP worker shutting down");
    });
}

trait RouteExt {
    async fn fetch(&self, client: &reqwest::Client, base_url: &str) -> DataResponse;
    fn not_found_response(&self) -> Option<DataResponse>;
    fn parse_bytes(&self, bytes: &[u8]) -> DataResponse;
}

impl RouteExt for Route {
    async fn fetch(&self, client: &reqwest::Client, base_url: &str) -> DataResponse {
        let url = format!("{}{}", base_url, self.to_path());
        trace!("Fetching {}", url);

        let resp = match client.get(&url).send().await {
            Ok(resp) => resp,
            Err(e) => {
                warn!("Request failed for {}: {}", url, e);
                return DataResponse::Error(format!("Request failed: {}", e));
            }
        };

        let status = resp.status();
        trace!("Response status {} for {}", status, url);

        if status == StatusCode::NOT_FOUND {
            if let Some(not_found) = self.not_found_response() {
                trace!("Resource not found: {}", url);
                return not_found;
            }
        }

        let resp = match resp.error_for_status() {
            Ok(resp) => resp,
            Err(e) => {
                error!("HTTP error for {}: {}", url, e);
                return DataResponse::Error(format!("HTTP error: {}", e));
            }
        };

        let bytes = match resp.bytes().await {
            Ok(bytes) => bytes,
            Err(e) => {
                error!("Read error for {}: {}", url, e);
                return DataResponse::Error(format!("Read error: {}", e));
            }
        };

        trace!("Received {} bytes from {}", bytes.len(), url);
        self.parse_bytes(&bytes)
    }

    fn not_found_response(&self) -> Option<DataResponse> {
        match self {
            Route::FunctionsAlloc => Some(DataResponse::FunctionsAllocUnavailable),
            Route::FunctionTimingLogs { function_name } => Some(
                DataResponse::FunctionLogsTimingNotFound(function_name.clone()),
            ),
            Route::FunctionAllocLogs { function_name } => Some(
                DataResponse::FunctionLogsAllocNotFound(function_name.clone()),
            ),
            _ => None,
        }
    }

    fn parse_bytes(&self, bytes: &[u8]) -> DataResponse {
        match self {
            Route::FunctionsTiming => {
                parse_json::<FunctionsJson>(bytes).map(DataResponse::FunctionsTiming)
            }
            Route::FunctionsAlloc => {
                parse_json::<FunctionsJson>(bytes).map(DataResponse::FunctionsAlloc)
            }
            Route::Channels => parse_json::<ChannelsJson>(bytes).map(DataResponse::Channels),
            Route::Streams => parse_json::<StreamsJson>(bytes).map(DataResponse::Streams),
            Route::Threads => parse_json::<ThreadsJson>(bytes).map(DataResponse::Threads),
            Route::Futures => parse_json::<FuturesJson>(bytes).map(DataResponse::Futures),
            Route::FunctionTimingLogs { function_name } => parse_json::<FunctionLogsJson>(bytes)
                .map(|logs| DataResponse::FunctionLogsTiming {
                    function_name: function_name.clone(),
                    logs,
                }),
            Route::FunctionAllocLogs { function_name } => parse_json::<FunctionLogsJson>(bytes)
                .map(|logs| DataResponse::FunctionLogsAlloc {
                    function_name: function_name.clone(),
                    logs,
                }),
            Route::ChannelLogs { channel_id } => {
                parse_json::<ChannelLogs>(bytes).map(|logs| DataResponse::ChannelLogs {
                    channel_id: *channel_id,
                    logs,
                })
            }
            Route::StreamLogs { stream_id } => {
                parse_json::<StreamLogs>(bytes).map(|logs| DataResponse::StreamLogs {
                    stream_id: *stream_id,
                    logs,
                })
            }
            Route::FutureCalls { future_id } => {
                parse_json::<FutureCalls>(bytes).map(|calls| DataResponse::FutureCalls {
                    future_id: *future_id,
                    calls,
                })
            }
        }
        .unwrap_or_else(|e| DataResponse::Error(format!("JSON parse error: {}", e)))
    }
}

fn parse_json<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, serde_json::Error> {
    serde_json::from_slice(bytes)
}
