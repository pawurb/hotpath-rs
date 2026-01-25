//! Event types for async TUI communication

use crossterm::event::KeyCode;
use hotpath::json::Route;
use hotpath::json::{
    JsonChannelLogsList, JsonChannelsList, JsonDebugList, JsonDebugLog, JsonFunctionAllocLogsList,
    JsonFunctionTimingLogsList, JsonFunctionsList, JsonFutureLogsList, JsonFuturesList,
    JsonStreamLogsList, JsonStreamsList, JsonThreadsList,
};

#[derive(Debug)]
pub(crate) enum DataRequest {
    RefreshTiming,
    RefreshMemory,
    RefreshChannels,
    RefreshStreams,
    RefreshThreads,
    RefreshFutures,
    RefreshDebug,
    FetchFunctionLogsTiming(String),
    FetchFunctionLogsAlloc(String),
    FetchChannelLogs(u64),
    FetchStreamLogs(u64),
    FetchFutureCalls(u64),
    FetchDebugDbgLogs(u64),
    FetchDebugValLogs(u64),
}

impl DataRequest {
    pub(crate) fn to_route(&self) -> Route {
        match self {
            DataRequest::RefreshTiming => Route::FunctionsTiming,
            DataRequest::RefreshMemory => Route::FunctionsAlloc,
            DataRequest::RefreshChannels => Route::Channels,
            DataRequest::RefreshStreams => Route::Streams,
            DataRequest::RefreshThreads => Route::Threads,
            DataRequest::RefreshFutures => Route::Futures,
            DataRequest::RefreshDebug => Route::Debug,
            DataRequest::FetchFunctionLogsTiming(name) => Route::FunctionTimingLogs {
                function_name: name.clone(),
            },
            DataRequest::FetchFunctionLogsAlloc(name) => Route::FunctionAllocLogs {
                function_name: name.clone(),
            },
            DataRequest::FetchChannelLogs(id) => Route::ChannelLogs { channel_id: *id },
            DataRequest::FetchStreamLogs(id) => Route::StreamLogs { stream_id: *id },
            DataRequest::FetchFutureCalls(id) => Route::FutureLogs { future_id: *id },
            DataRequest::FetchDebugDbgLogs(id) => Route::DebugDbgLogs { id: *id },
            DataRequest::FetchDebugValLogs(id) => Route::DebugValLogs { id: *id },
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
    Channels(JsonChannelsList),
    ChannelLogs {
        channel_id: u64,
        logs: JsonChannelLogsList,
    },
    Streams(JsonStreamsList),
    StreamLogs {
        stream_id: u64,
        logs: JsonStreamLogsList,
    },
    Threads(JsonThreadsList),
    Futures(JsonFuturesList),
    FutureLogs {
        future_id: u64,
        calls: JsonFutureLogsList,
    },
    Debug(JsonDebugList),
    DebugDbgLogs {
        id: u64,
        logs: Vec<JsonDebugLog>,
    },
    DebugValLogs {
        id: u64,
        logs: Vec<JsonDebugLog>,
    },
    DebugLogsNotFound {
        id: u64,
    },
    Error(String),
}

#[derive(Debug)]
pub(crate) enum AppEvent {
    Key(KeyCode),
    Data(DataResponse),
}
