//! Event types for async TUI communication

use crossterm::event::KeyCode;
use hotpath::json::Route;
use hotpath::json::{
    JsonChannelLogsList, JsonChannelsList, JsonDebugList, JsonDebugLog, JsonFunctionAllocLogsList,
    JsonFunctionTimingLogsList, JsonFunctionsCpuEnvelope, JsonFunctionsList, JsonFutureLogsList,
    JsonFuturesList, JsonMutexesList, JsonProfilerStatus, JsonRuntimeSnapshot, JsonRwLocksList,
    JsonSqlList, JsonStreamLogsList, JsonStreamsList, JsonThreadsList,
};

#[derive(Debug)]
pub(crate) enum DataRequest {
    RefreshTiming,
    RefreshMemory,
    RefreshCpu,
    TriggerCpuSnapshot,
    RefreshChannels,
    RefreshStreams,
    RefreshFutures,
    RefreshRwLocks,
    RefreshMutexes,
    RefreshSql,
    RefreshThreads,
    RefreshDebug,
    RefreshTokioRuntime,
    FetchFunctionLogsTiming(u32),
    FetchFunctionLogsAlloc(u32),
    FetchChannelLogs(u32),
    FetchStreamLogs(u32),
    FetchFutureLogs(u32),
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
            DataRequest::RefreshCpu => Route::FunctionsCpu,
            DataRequest::TriggerCpuSnapshot => Route::FunctionsCpuSnapshot,
            DataRequest::RefreshChannels => Route::Channels,
            DataRequest::RefreshStreams => Route::Streams,
            DataRequest::RefreshFutures => Route::Futures,
            DataRequest::RefreshRwLocks => Route::RwLocks,
            DataRequest::RefreshMutexes => Route::Mutexes,
            DataRequest::RefreshSql => Route::Sql,
            DataRequest::RefreshThreads => Route::Threads,
            DataRequest::RefreshDebug => Route::Debug,
            DataRequest::RefreshTokioRuntime => Route::TokioRuntime,
            DataRequest::FetchFunctionLogsTiming(id) => {
                Route::FunctionTimingLogs { function_id: *id }
            }
            DataRequest::FetchFunctionLogsAlloc(id) => {
                Route::FunctionAllocLogs { function_id: *id }
            }
            DataRequest::FetchChannelLogs(id) => Route::ChannelLogs { channel_id: *id },
            DataRequest::FetchStreamLogs(id) => Route::StreamLogs { stream_id: *id },
            DataRequest::FetchFutureLogs(id) => Route::FutureLogs { future_id: *id },
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
    FunctionsAllocUnavailable(String),
    FunctionsCpu(JsonFunctionsCpuEnvelope),
    FunctionsCpuUnavailable(String),
    CpuSnapshotTriggered,
    CpuSnapshotBusy,
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
    Channels(JsonChannelsList),
    Streams(JsonStreamsList),
    Futures(JsonFuturesList),
    RwLocks(JsonRwLocksList),
    Mutexes(JsonMutexesList),
    Sql(JsonSqlList),
    ChannelLogs {
        id: u32,
        logs: JsonChannelLogsList,
    },
    StreamLogs {
        id: u32,
        logs: JsonStreamLogsList,
    },
    FutureLogs {
        id: u32,
        calls: JsonFutureLogsList,
    },
    ChannelLogsNotFound {
        id: u32,
    },
    StreamLogsNotFound {
        id: u32,
    },
    FutureLogsNotFound {
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
    Data(Box<DataResponse>),
}
