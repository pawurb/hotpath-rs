//! Event types for async TUI communication

use crossterm::event::KeyCode;
use hotpath::json::{
    ChannelLogs, ChannelsJson, FunctionLogsJson, FunctionsJson, FutureCalls, FuturesJson,
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
