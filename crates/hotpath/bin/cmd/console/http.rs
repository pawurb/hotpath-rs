use eyre::Result;
use hotpath::{MetricsJson, SamplesJson};

/// Fetches metrics from the hotpath HTTP server
pub(crate) fn fetch_metrics(agent: &ureq::Agent, port: u16) -> Result<MetricsJson> {
    let url = format!("http://localhost:{}/metrics", port);
    let metrics: MetricsJson = agent
        .get(&url)
        .call()
        .map_err(|e| eyre::eyre!("HTTP request failed: {}", e))?
        .body_mut()
        .read_json()
        .map_err(|e| eyre::eyre!("JSON deserialization failed: {}", e))?;
    Ok(metrics)
}

/// Fetches channels from the hotpath HTTP server
pub(crate) fn fetch_channels(
    agent: &ureq::Agent,
    port: u16,
) -> Result<hotpath::channels::ChannelsJson> {
    let url = format!("http://localhost:{}/channels", port);
    let channels: hotpath::channels::ChannelsJson = agent
        .get(&url)
        .call()
        .map_err(|e| eyre::eyre!("HTTP request failed: {}", e))?
        .body_mut()
        .read_json()
        .map_err(|e| eyre::eyre!("JSON deserialization failed: {}", e))?;
    Ok(channels)
}

/// Fetches recent samples for a specific function
pub(crate) fn fetch_samples(
    agent: &ureq::Agent,
    port: u16,
    function_name: &str,
) -> Result<SamplesJson> {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(function_name.as_bytes());

    let url = format!("http://localhost:{}/samples/{}", port, encoded);
    let samples: SamplesJson = agent
        .get(&url)
        .call()
        .map_err(|e| eyre::eyre!("HTTP request failed: {}", e))?
        .body_mut()
        .read_json()
        .map_err(|e| eyre::eyre!("JSON deserialization failed: {}", e))?;
    Ok(samples)
}
