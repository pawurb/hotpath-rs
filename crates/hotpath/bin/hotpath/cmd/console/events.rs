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
    FetchFunctionLogsTiming(u32),
    FetchFunctionLogsAlloc(u32),
    FetchDataFlowChannelLogs(u32),
    FetchDataFlowStreamLogs(u32),
    FetchDataFlowFutureLogs(u32),
    FetchDebugDbgLogs(u32),
    FetchDebugValLogs(u32),
    FetchDebugGaugeLogs(u32),
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
            DataRequest::FetchFunctionLogsTiming(id) => {
                Route::FunctionTimingLogs { function_id: *id }
            }
            DataRequest::FetchFunctionLogsAlloc(id) => {
                Route::FunctionAllocLogs { function_id: *id }
            }
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
        function_id: u32,
        logs: JsonFunctionTimingLogsList,
    },
    FunctionLogsTimingNotFound(u32),
    FunctionLogsAlloc {
        function_id: u32,
        logs: JsonFunctionAllocLogsList,
    },
    FunctionLogsAllocNotFound(u32),
    DataFlow(JsonDataFlowList),
    DataFlowChannelLogs {
        id: u32,
        logs: JsonChannelLogsList,
    },
    DataFlowStreamLogs {
        id: u32,
        logs: JsonStreamLogsList,
    },
    DataFlowFutureLogs {
        id: u32,
        calls: JsonFutureLogsList,
    },
    DataFlowLogsNotFound {
        id: u32,
    },
    Threads(JsonThreadsList),
    Debug(JsonDebugList),
    DebugDbgLogs {
        id: u32,
        logs: Vec<JsonDebugLog>,
    },
    DebugValLogs {
        id: u32,
        logs: Vec<JsonDebugLog>,
    },
    DebugGaugeLogs {
        id: u32,
        logs: Vec<JsonDebugLog>,
    },
    DebugLogsNotFound {
        id: u32,
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
