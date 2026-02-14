//! Data worker thread with Tokio runtime for async HTTP fetching

use crossbeam_channel::{Receiver, Sender};
use hotpath::json::Route;
use hotpath::json::{
    JsonChannelLogsList, JsonDataFlowList, JsonDebugDbgLogs, JsonDebugGaugeLogs, JsonDebugList,
    JsonDebugValLogs, JsonFunctionAllocLogsList, JsonFunctionTimingLogsList, JsonFunctionsList,
    JsonFutureLogsList, JsonProfilerStatus, JsonRuntimeSnapshot, JsonStreamLogsList,
    JsonThreadsList,
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
    DataFlow,
    Threads,
    Debug,
    TokioRuntime,
    FunctionLogsTiming,
    FunctionLogsAlloc,
    DataFlowChannelLogs,
    DataFlowStreamLogs,
    DataFlowFutureLogs,
    DebugDbgLogs,
    DebugValLogs,
    DebugGaugeLogs,
    ProfilerStatus,
}

impl DataRequest {
    fn key(&self) -> RequestKey {
        match self {
            DataRequest::RefreshTiming => RequestKey::Timing,
            DataRequest::RefreshMemory => RequestKey::Memory,
            DataRequest::RefreshDataFlow => RequestKey::DataFlow,
            DataRequest::RefreshThreads => RequestKey::Threads,
            DataRequest::RefreshDebug => RequestKey::Debug,
            DataRequest::RefreshTokioRuntime => RequestKey::TokioRuntime,
            DataRequest::FetchFunctionLogsTiming(_) => RequestKey::FunctionLogsTiming,
            DataRequest::FetchFunctionLogsAlloc(_) => RequestKey::FunctionLogsAlloc,
            DataRequest::FetchDataFlowChannelLogs(_) => RequestKey::DataFlowChannelLogs,
            DataRequest::FetchDataFlowStreamLogs(_) => RequestKey::DataFlowStreamLogs,
            DataRequest::FetchDataFlowFutureLogs(_) => RequestKey::DataFlowFutureLogs,
            DataRequest::FetchDebugDbgLogs(_) => RequestKey::DebugDbgLogs,
            DataRequest::FetchDebugValLogs(_) => RequestKey::DebugValLogs,
            DataRequest::FetchDebugGaugeLogs(_) => RequestKey::DebugGaugeLogs,
            DataRequest::FetchProfilerStatus => RequestKey::ProfilerStatus,
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
        hotpath::tokio_runtime!(rt.handle());
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
            let body = resp.text().await.unwrap_or_default();
            let msg = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v.get("error")?.as_str().map(String::from))
                .unwrap_or(body);
            return DataResponse::Error(msg);
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
            Route::DataFlowChannelLogs { channel_id } => {
                Some(DataResponse::DataFlowLogsNotFound { id: *channel_id })
            }
            Route::DataFlowStreamLogs { stream_id } => {
                Some(DataResponse::DataFlowLogsNotFound { id: *stream_id })
            }
            Route::DataFlowFutureLogs { future_id } => {
                Some(DataResponse::DataFlowLogsNotFound { id: *future_id })
            }
            Route::DebugDbgLogs { id }
            | Route::DebugValLogs { id }
            | Route::DebugGaugeLogs { id } => Some(DataResponse::DebugLogsNotFound { id: *id }),
            Route::ProfilerStatus => Some(DataResponse::ProfilerStatus(JsonProfilerStatus {
                uptime: String::new(),
            })),
            _ => None,
        }
    }

    fn parse_bytes(&self, bytes: &[u8]) -> DataResponse {
        match self {
            Route::FunctionsTiming => {
                parse_json::<JsonFunctionsList>(bytes).map(DataResponse::FunctionsTiming)
            }
            Route::FunctionsAlloc => {
                parse_json::<JsonFunctionsList>(bytes).map(DataResponse::FunctionsAlloc)
            }
            Route::DataFlow => parse_json::<JsonDataFlowList>(bytes).map(DataResponse::DataFlow),
            Route::Threads => parse_json::<JsonThreadsList>(bytes).map(DataResponse::Threads),
            Route::FunctionTimingLogs { function_name } => {
                parse_json::<JsonFunctionTimingLogsList>(bytes).map(|logs| {
                    DataResponse::FunctionLogsTiming {
                        function_name: function_name.clone(),
                        logs,
                    }
                })
            }
            Route::FunctionAllocLogs { function_name } => {
                parse_json::<JsonFunctionAllocLogsList>(bytes).map(|logs| {
                    DataResponse::FunctionLogsAlloc {
                        function_name: function_name.clone(),
                        logs,
                    }
                })
            }
            Route::DataFlowChannelLogs { channel_id } => parse_json::<JsonChannelLogsList>(bytes)
                .map(|logs| DataResponse::DataFlowChannelLogs {
                    id: *channel_id,
                    logs,
                }),
            Route::DataFlowStreamLogs { stream_id } => {
                parse_json::<JsonStreamLogsList>(bytes).map(|logs| {
                    DataResponse::DataFlowStreamLogs {
                        id: *stream_id,
                        logs,
                    }
                })
            }
            Route::DataFlowFutureLogs { future_id } => {
                parse_json::<JsonFutureLogsList>(bytes).map(|calls| {
                    DataResponse::DataFlowFutureLogs {
                        id: *future_id,
                        calls,
                    }
                })
            }
            Route::Debug => parse_json::<JsonDebugList>(bytes).map(DataResponse::Debug),
            Route::DebugDbgLogs { id } => {
                parse_json::<JsonDebugDbgLogs>(bytes).map(|logs| DataResponse::DebugDbgLogs {
                    id: *id,
                    logs: logs.logs,
                })
            }
            Route::DebugValLogs { id } => {
                parse_json::<JsonDebugValLogs>(bytes).map(|logs| DataResponse::DebugValLogs {
                    id: *id,
                    logs: logs.logs,
                })
            }
            Route::DebugGaugeLogs { id } => {
                parse_json::<JsonDebugGaugeLogs>(bytes).map(|logs| DataResponse::DebugGaugeLogs {
                    id: *id,
                    logs: logs.logs,
                })
            }
            Route::TokioRuntime => {
                parse_json::<JsonRuntimeSnapshot>(bytes).map(DataResponse::TokioRuntime)
            }
            Route::ProfilerStatus => {
                parse_json::<JsonProfilerStatus>(bytes).map(DataResponse::ProfilerStatus)
            }
        }
        .unwrap_or_else(|e| DataResponse::Error(format!("JSON parse error: {}", e)))
    }
}

fn parse_json<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, serde_json::Error> {
    serde_json::from_slice(bytes)
}
