//! Event types for async TUI communication

use crossterm::event::KeyCode;
use hotpath::json::Route;
use hotpath::json::{
    FormattedChannelLogs, FormattedChannelsJson, FormattedDebugJson, FormattedDebugLogEntry,
    FormattedFunctionAllocLogsJson, FormattedFunctionTimingLogsJson, FormattedFunctionsJson,
    FormattedFutureCalls, FormattedFuturesJson, FormattedStreamLogs, FormattedStreamsJson,
    FormattedThreadsJson,
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
            DataRequest::RefreshDebug => Route::DebugStats,
            DataRequest::FetchFunctionLogsTiming(name) => Route::FunctionTimingLogs {
                function_name: name.clone(),
            },
            DataRequest::FetchFunctionLogsAlloc(name) => Route::FunctionAllocLogs {
                function_name: name.clone(),
            },
            DataRequest::FetchChannelLogs(id) => Route::ChannelLogs { channel_id: *id },
            DataRequest::FetchStreamLogs(id) => Route::StreamLogs { stream_id: *id },
            DataRequest::FetchFutureCalls(id) => Route::FutureCalls { future_id: *id },
            DataRequest::FetchDebugDbgLogs(id) => Route::DebugDbgLogs { id: *id },
            DataRequest::FetchDebugValLogs(id) => Route::DebugValLogs { id: *id },
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum DataResponse {
    FunctionsTiming(FormattedFunctionsJson),
    FunctionsAlloc(FormattedFunctionsJson),
    FunctionsAllocUnavailable,
    FunctionLogsTiming {
        function_name: String,
        logs: FormattedFunctionTimingLogsJson,
    },
    FunctionLogsTimingNotFound(String),
    FunctionLogsAlloc {
        function_name: String,
        logs: FormattedFunctionAllocLogsJson,
    },
    FunctionLogsAllocNotFound(String),
    Channels(FormattedChannelsJson),
    ChannelLogs {
        channel_id: u64,
        logs: FormattedChannelLogs,
    },
    Streams(FormattedStreamsJson),
    StreamLogs {
        stream_id: u64,
        logs: FormattedStreamLogs,
    },
    Threads(FormattedThreadsJson),
    Futures(FormattedFuturesJson),
    FutureCalls {
        future_id: u64,
        calls: FormattedFutureCalls,
    },
    Debug(FormattedDebugJson),
    DebugDbgLogs {
        id: u64,
        logs: Vec<FormattedDebugLogEntry>,
    },
    DebugValLogs {
        id: u64,
        logs: Vec<FormattedDebugLogEntry>,
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
