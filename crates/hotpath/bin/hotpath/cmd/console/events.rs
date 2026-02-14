//! Event types for async TUI communication

use crossterm::event::KeyCode;
use hotpath::json::Route;
use hotpath::json::{
    JsonChannelLogsList, JsonDataFlowList, JsonDebugList, JsonDebugLog, JsonFunctionAllocLogsList,
    JsonFunctionTimingLogsList, JsonFunctionsList, JsonFutureLogsList, JsonProfilerStatus,
    JsonRuntimeSnapshot, JsonStreamLogsList, JsonThreadsList,
};

#[derive(Debug)]
pub(crate) enum DataRequest {
    RefreshTiming,
    RefreshMemory,
    RefreshDataFlow,
    RefreshThreads,
    RefreshDebug,
    RefreshTokioRuntime,
    FetchFunctionLogsTiming(String),
    FetchFunctionLogsAlloc(String),
    FetchDataFlowChannelLogs(u64),
    FetchDataFlowStreamLogs(u64),
    FetchDataFlowFutureLogs(u64),
    FetchDebugDbgLogs(u64),
    FetchDebugValLogs(u64),
    FetchDebugGaugeLogs(u64),
    FetchProfilerStatus,
}

impl DataRequest {
    pub(crate) fn to_route(&self) -> Route {
        match self {
            DataRequest::RefreshTiming => Route::FunctionsTiming,
            DataRequest::RefreshMemory => Route::FunctionsAlloc,
            DataRequest::RefreshDataFlow => Route::DataFlow,
            DataRequest::RefreshThreads => Route::Threads,
            DataRequest::RefreshDebug => Route::Debug,
            DataRequest::RefreshTokioRuntime => Route::TokioRuntime,
            DataRequest::FetchFunctionLogsTiming(name) => Route::FunctionTimingLogs {
                function_name: name.clone(),
            },
            DataRequest::FetchFunctionLogsAlloc(name) => Route::FunctionAllocLogs {
                function_name: name.clone(),
            },
            DataRequest::FetchDataFlowChannelLogs(id) => {
                Route::DataFlowChannelLogs { channel_id: *id }
            }
            DataRequest::FetchDataFlowStreamLogs(id) => {
                Route::DataFlowStreamLogs { stream_id: *id }
            }
            DataRequest::FetchDataFlowFutureLogs(id) => {
                Route::DataFlowFutureLogs { future_id: *id }
            }
            DataRequest::FetchDebugDbgLogs(id) => Route::DebugDbgLogs { id: *id },
            DataRequest::FetchDebugValLogs(id) => Route::DebugValLogs { id: *id },
            DataRequest::FetchDebugGaugeLogs(id) => Route::DebugGaugeLogs { id: *id },
            DataRequest::FetchProfilerStatus => Route::ProfilerStatus,
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum DataResponse {
    FunctionsTiming(JsonFunctionsList),
    FunctionsAlloc(JsonFunctionsList),
    FunctionsAllocUnavailable,
    FunctionLogsTiming {
        function_name: String,
        logs: JsonFunctionTimingLogsList,
    },
    FunctionLogsTimingNotFound(String),
    FunctionLogsAlloc {
        function_name: String,
        logs: JsonFunctionAllocLogsList,
    },
    FunctionLogsAllocNotFound(String),
    DataFlow(JsonDataFlowList),
    DataFlowChannelLogs {
        id: u64,
        logs: JsonChannelLogsList,
    },
    DataFlowStreamLogs {
        id: u64,
        logs: JsonStreamLogsList,
    },
    DataFlowFutureLogs {
        id: u64,
        calls: JsonFutureLogsList,
    },
    DataFlowLogsNotFound {
        id: u64,
    },
    Threads(JsonThreadsList),
    Debug(JsonDebugList),
    DebugDbgLogs {
        id: u64,
        logs: Vec<JsonDebugLog>,
    },
    DebugValLogs {
        id: u64,
        logs: Vec<JsonDebugLog>,
    },
    DebugGaugeLogs {
        id: u64,
        logs: Vec<JsonDebugLog>,
    },
    DebugLogsNotFound {
        id: u64,
    },
    TokioRuntime(JsonRuntimeSnapshot),
    ProfilerStatus(JsonProfilerStatus),
    Error(String),
}

#[derive(Debug)]
pub(crate) enum AppEvent {
    Key(KeyCode),
    Data(DataResponse),
}
