use eyre::Result;
use hotpath::channels::ChannelLogs;
use hotpath::streams::{StreamLogs, StreamsJson};
use hotpath::threads::ThreadsJson;
use hotpath::{FunctionLogsJson, FunctionsJson, Route};

/// Fetches timing metrics from the hotpath HTTP server
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub(crate) fn fetch_functions_timing(agent: &ureq::Agent, port: u16) -> Result<FunctionsJson> {
    let url = Route::FunctionsTiming.to_url(port);
    let metrics: FunctionsJson = agent
        .get(&url)
        .call()
        .map_err(|e| eyre::eyre!("HTTP request failed: {}", e))?
        .body_mut()
        .read_json()
        .map_err(|e| eyre::eyre!("JSON deserialization failed: {}", e))?;
    Ok(metrics)
}

/// Fetches allocation metrics from the hotpath HTTP server
/// Returns None if hotpath-alloc feature is not enabled (404 response)
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub(crate) fn fetch_functions_alloc(
    agent: &ureq::Agent,
    port: u16,
) -> Result<Option<FunctionsJson>> {
    let url = Route::FunctionsAlloc.to_url(port);
    let response = agent.get(&url).call();

    match response {
        Ok(mut resp) => {
            let metrics: FunctionsJson = resp
                .body_mut()
                .read_json()
                .map_err(|e| eyre::eyre!("JSON deserialization failed: {}", e))?;
            Ok(Some(metrics))
        }
        Err(ureq::Error::StatusCode(404)) => {
            // Feature not enabled
            Ok(None)
        }
        Err(e) => Err(eyre::eyre!("HTTP request failed: {}", e)),
    }
}

/// Fetches channels from the hotpath HTTP server
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub(crate) fn fetch_channels(
    agent: &ureq::Agent,
    port: u16,
) -> Result<hotpath::channels::ChannelsJson> {
    let url = Route::Channels.to_url(port);
    let channels: hotpath::channels::ChannelsJson = agent
        .get(&url)
        .call()
        .map_err(|e| eyre::eyre!("HTTP request failed: {}", e))?
        .body_mut()
        .read_json()
        .map_err(|e| eyre::eyre!("JSON deserialization failed: {}", e))?;
    Ok(channels)
}

/// Fetches recent timing logs for a specific function
/// Returns None if function is not found (404 response)
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub(crate) fn fetch_function_logs_timing(
    agent: &ureq::Agent,
    port: u16,
    function_name: &str,
) -> Result<Option<FunctionLogsJson>> {
    let url = Route::FunctionTimingLogs {
        function_name: function_name.to_string(),
    }
    .to_url(port);
    let response = agent.get(&url).call();

    match response {
        Ok(mut resp) => {
            let function_logs: FunctionLogsJson = resp
                .body_mut()
                .read_json()
                .map_err(|e| eyre::eyre!("JSON deserialization failed: {}", e))?;
            Ok(Some(function_logs))
        }
        Err(ureq::Error::StatusCode(404)) => {
            // Function not found
            Ok(None)
        }
        Err(e) => Err(eyre::eyre!("HTTP request failed: {}", e)),
    }
}

/// Fetches recent allocation logs for a specific function
/// Returns None if hotpath-alloc feature is not enabled (404 response)
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub(crate) fn fetch_function_logs_alloc(
    agent: &ureq::Agent,
    port: u16,
    function_name: &str,
) -> Result<Option<FunctionLogsJson>> {
    let url = Route::FunctionAllocLogs {
        function_name: function_name.to_string(),
    }
    .to_url(port);
    let response = agent.get(&url).call();

    match response {
        Ok(mut resp) => {
            let function_logs: FunctionLogsJson = resp
                .body_mut()
                .read_json()
                .map_err(|e| eyre::eyre!("JSON deserialization failed: {}", e))?;
            Ok(Some(function_logs))
        }
        Err(ureq::Error::StatusCode(404)) => {
            // Feature not enabled
            Ok(None)
        }
        Err(e) => Err(eyre::eyre!("HTTP request failed: {}", e)),
    }
}

/// Fetches logs for a specific channel from the HTTP server
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub(crate) fn fetch_channel_logs(
    agent: &ureq::Agent,
    port: u16,
    channel_id: u64,
) -> Result<ChannelLogs> {
    let url = Route::ChannelLogs { channel_id }.to_url(port);
    let logs: ChannelLogs = agent
        .get(&url)
        .call()
        .map_err(|e| eyre::eyre!("HTTP request failed: {}", e))?
        .body_mut()
        .read_json()
        .map_err(|e| eyre::eyre!("JSON deserialization failed: {}", e))?;
    Ok(logs)
}

/// Fetches streams from the hotpath HTTP server
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub(crate) fn fetch_streams(agent: &ureq::Agent, port: u16) -> Result<StreamsJson> {
    let url = Route::Streams.to_url(port);
    let streams: StreamsJson = agent
        .get(&url)
        .call()
        .map_err(|e| eyre::eyre!("HTTP request failed: {}", e))?
        .body_mut()
        .read_json()
        .map_err(|e| eyre::eyre!("JSON deserialization failed: {}", e))?;
    Ok(streams)
}

/// Fetches logs for a specific stream from the HTTP server
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub(crate) fn fetch_stream_logs(
    agent: &ureq::Agent,
    port: u16,
    stream_id: u64,
) -> Result<StreamLogs> {
    let url = Route::StreamLogs { stream_id }.to_url(port);
    let logs: StreamLogs = agent
        .get(&url)
        .call()
        .map_err(|e| eyre::eyre!("HTTP request failed: {}", e))?
        .body_mut()
        .read_json()
        .map_err(|e| eyre::eyre!("JSON deserialization failed: {}", e))?;
    Ok(logs)
}

/// Fetches thread metrics from the hotpath HTTP server
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub(crate) fn fetch_threads(agent: &ureq::Agent, port: u16) -> Result<ThreadsJson> {
    let url = Route::Threads.to_url(port);
    let threads: ThreadsJson = agent
        .get(&url)
        .call()
        .map_err(|e| eyre::eyre!("HTTP request failed: {}", e))?
        .body_mut()
        .read_json()
        .map_err(|e| eyre::eyre!("JSON deserialization failed: {}", e))?;
    Ok(threads)
}
