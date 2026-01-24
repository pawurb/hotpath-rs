//! Event types for async TUI communication

use crossterm::event::KeyCode;
use hotpath::json::Route;
use hotpath::json::{
    FormattedChannelLogs, FormattedChannelsJson, FormattedDbgJson, FormattedDbgLogs,
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
    FetchDebugLogs { source: String, expression: String },
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
            DataRequest::FetchDebugLogs { source, expression } => Route::DebugLogs {
                source: source.clone(),
                expression: expression.clone(),
            },
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
    Debug(FormattedDbgJson),
    DebugLogs {
        source: String,
        expression: String,
        logs: FormattedDbgLogs,
    },
    DebugLogsNotFound {
        source: String,
        expression: String,
    },
    Error(String),
}

#[derive(Debug)]
pub(crate) enum AppEvent {
    Key(KeyCode),
    Data(DataResponse),
}
