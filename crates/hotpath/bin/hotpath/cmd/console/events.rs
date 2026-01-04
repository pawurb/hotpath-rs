//! Event types for async TUI communication

use crossterm::event::KeyCode;
use hotpath::json::{
    ChannelLogs, ChannelsJson, FunctionLogsJson, FunctionsJson, FutureCalls, FuturesJson, Route,
    StreamLogs, StreamsJson, ThreadsJson,
};

#[derive(Debug)]
pub(crate) enum DataRequest {
    RefreshTiming,
    RefreshMemory,
    RefreshChannels,
    RefreshStreams,
    RefreshThreads,
    RefreshFutures,
    FetchFunctionLogsTiming(String),
    FetchFunctionLogsAlloc(String),
    FetchChannelLogs(u64),
    FetchStreamLogs(u64),
    FetchFutureCalls(u64),
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
            DataRequest::FetchFunctionLogsTiming(name) => Route::FunctionTimingLogs {
                function_name: name.clone(),
            },
            DataRequest::FetchFunctionLogsAlloc(name) => Route::FunctionAllocLogs {
                function_name: name.clone(),
            },
            DataRequest::FetchChannelLogs(id) => Route::ChannelLogs { channel_id: *id },
            DataRequest::FetchStreamLogs(id) => Route::StreamLogs { stream_id: *id },
            DataRequest::FetchFutureCalls(id) => Route::FutureCalls { future_id: *id },
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum DataResponse {
    FunctionsTiming(FunctionsJson),
    FunctionsAlloc(FunctionsJson),
    FunctionsAllocUnavailable,
    FunctionLogsTiming {
        function_name: String,
        logs: FunctionLogsJson,
    },
    FunctionLogsTimingNotFound(String),
    FunctionLogsAlloc {
        function_name: String,
        logs: FunctionLogsJson,
    },
    FunctionLogsAllocNotFound(String),
    Channels(ChannelsJson),
    ChannelLogs {
        channel_id: u64,
        logs: ChannelLogs,
    },
    Streams(StreamsJson),
    StreamLogs {
        stream_id: u64,
        logs: StreamLogs,
    },
    Threads(ThreadsJson),
    Futures(FuturesJson),
    FutureCalls {
        future_id: u64,
        calls: FutureCalls,
    },
    Error(String),
}

#[derive(Debug)]
pub(crate) enum AppEvent {
    Key(KeyCode),
    Data(DataResponse),
}
