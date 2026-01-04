//! Data worker thread with Tokio runtime for async HTTP fetching

use crossbeam_channel::{Receiver, Sender};
use hotpath::json::{
    ChannelLogs, ChannelsJson, FunctionLogsJson, FunctionsJson, FutureCalls, FuturesJson, Route,
    StreamLogs, StreamsJson, ThreadsJson,
};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use std::{sync::Arc, time::Duration};
use tokio::runtime::Runtime;

use super::events::{AppEvent, DataRequest, DataResponse};

const HTTP_TIMEOUT_MS: u64 = 2000;

pub(crate) fn spawn_http_worker(
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

        let base_url = Arc::new(format!("http://127.0.0.1:{}", metrics_port));

        while let Ok(request) = request_rx.recv() {
            let client = client.clone();
            let base_url = base_url.clone();
            let event_tx = event_tx.clone();

            rt.spawn(async move {
                let response = request.to_route().fetch(&client, &base_url).await;
                let _ = event_tx.send(AppEvent::Data(response));
            });
        }
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

        let resp = match client.get(&url).send().await {
            Ok(resp) => resp,
            Err(e) => return DataResponse::Error(format!("Request failed: {}", e)),
        };

        let status = resp.status();

        if status == StatusCode::NOT_FOUND {
            if let Some(not_found) = self.not_found_response() {
                return not_found;
            }
        }

        let resp = match resp.error_for_status() {
            Ok(resp) => resp,
            Err(e) => return DataResponse::Error(format!("HTTP error: {}", e)),
        };

        let bytes = match resp.bytes().await {
            Ok(bytes) => bytes,
            Err(e) => return DataResponse::Error(format!("Read error: {}", e)),
        };

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
