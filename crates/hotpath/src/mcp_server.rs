use axum::Router;
use rmcp::{
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    ErrorData as McpError, ServerHandler,
};
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::{Arc, LazyLock, OnceLock};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use crate::channels::{get_channel_logs, get_channels_json};
use crate::debug::dbg::{get_dbg_logs, get_debug_dbg_entries_json};
use crate::debug::gauge::{get_debug_gauge_entries_json, get_debug_gauge_logs};
use crate::debug::val::{get_debug_val_entries_json, get_val_logs};
use crate::functions::{
    get_function_logs_alloc, get_function_logs_timing, get_functions_alloc_json,
    get_functions_timing_json,
};
use crate::futures::{get_future_logs_list, get_futures_json};
use crate::json::{
    JsonChannelLogsList, JsonFunctionAllocLogsList, JsonFunctionTimingLogsList, JsonFutureLogsList,
    JsonProfilerStatus, JsonStreamLogsList,
};
use crate::output::format_duration;
use crate::streams::{get_stream_logs, get_streams_json};
use crate::threads::get_threads_json;

#[derive(Debug, Deserialize, JsonSchema)]
struct FunctionIdParam {
    #[schemars(description = "Function ID from the functions_timing or functions_alloc response")]
    function_id: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ChannelIdParam {
    #[schemars(description = "Channel identifier from the channels list")]
    channel_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct StreamIdParam {
    #[schemars(description = "Stream identifier from the streams list")]
    stream_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct FutureIdParam {
    #[schemars(description = "Future identifier from the futures list")]
    future_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GaugeIdParam {
    #[schemars(description = "Gauge identifier from the gauges list")]
    gauge_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DebugIdParam {
    #[schemars(description = "Debug entry ID from the debug tool response")]
    debug_id: u32,
}

static MCP_SERVER_PORT: LazyLock<u16> = LazyLock::new(|| {
    std::env::var("HOTPATH_MCP_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(6771)
});

#[derive(Clone)]
pub(crate) struct HotPathMcpServer {
    tool_router: ToolRouter<Self>,
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure_all)]
#[tool_router]
impl HotPathMcpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = r#"Get execution timing metrics for all profiled functions.

Returns JSON array of functions sorted by total time. Each entry contains:
- name: fully qualified function name (e.g. "my_app::db::query")
- call_count: number of invocations
- total_ns: cumulative execution time in nanoseconds
- mean_ns, p50_ns, p95_ns, p99_ns: latency percentiles

Use this first to identify performance hotspots. Look for high p95/p99 values indicating tail latency issues."#
    )]
    async fn functions_timing(&self) -> Result<CallToolResult, McpError> {
        log_debug("Tool called: functions_timing");

        let formatted = get_functions_timing_json();
        Ok(CallToolResult::success(vec![Content::text(to_json(
            &formatted,
        )?)]))
    }

    #[tool(
        description = r#"Get memory allocation metrics per function (requires hotpath-alloc feature).

Returns JSON array with:
- name: function name
- alloc_bytes: total bytes allocated
- alloc_count: number of allocations

Returns error if hotpath-alloc feature is not enabled. Cross-reference with functions_timing to find functions that are both slow and allocation-heavy."#
    )]
    async fn functions_alloc(&self) -> Result<CallToolResult, McpError> {
        log_debug("Tool called: functions_alloc");

        match get_functions_alloc_json() {
            Some(formatted) => Ok(CallToolResult::success(vec![Content::text(to_json(
                &formatted,
            )?)])),
            None => Ok(CallToolResult::error(vec![Content::text(
                "Memory profiling not available - enable hotpath-alloc feature",
            )])),
        }
    }

    #[tool(
        description = r#"Get metrics for all monitored async channels (tokio, crossbeam, std, futures-channel).

Returns JSON array with:
- id: channel identifier
- label: optional custom label
- channel_type: "bounded", "unbounded", or "oneshot"
- sent/received: message counts
- state: "active", "closed"

Use to track channel throughput and identify stalled or closed channels."#
    )]
    async fn channels(&self) -> Result<CallToolResult, McpError> {
        log_debug("Tool called: channels");

        let channels = get_channels_json();
        Ok(CallToolResult::success(vec![Content::text(to_json(
            &channels,
        )?)]))
    }

    #[tool(description = r#"Get metrics for all monitored async streams.

Returns JSON array with:
- id: stream identifier
- label: optional custom label
- items_yielded: count of items produced
- state: "active" or "closed"

Use to track stream throughput and identify stalled streams."#)]
    async fn streams(&self) -> Result<CallToolResult, McpError> {
        log_debug("Tool called: streams");

        let streams = get_streams_json();
        Ok(CallToolResult::success(vec![Content::text(to_json(
            &streams,
        )?)]))
    }

    #[tool(description = r#"Get lifecycle metrics for all monitored futures.

Returns JSON array with:
- id: future identifier
- label: optional custom label
- call_count: number of future invocations observed
- total_polls: cumulative number of poll calls across invocations
- total_poll_duration_ns: cumulative poll time in nanoseconds

High poll counts can indicate futures that wake frequently without making progress."#)]
    async fn futures(&self) -> Result<CallToolResult, McpError> {
        log_debug("Tool called: futures");

        let futures = get_futures_json();
        Ok(CallToolResult::success(vec![Content::text(to_json(
            &futures,
        )?)]))
    }

    #[tool(description = r#"Get CPU usage metrics for all monitored threads.

Returns JSON array with:
- name: thread name (e.g. "tokio-runtime-worker")
- cpu_percent: CPU utilization (0-100 per core)

Sampled at configurable interval (HOTPATH_THREADS_INTERVAL_MS env var, default 250ms). Useful for identifying CPU-bound threads."#)]
    async fn threads(&self) -> Result<CallToolResult, McpError> {
        log_debug("Tool called: threads");

        let threads = get_threads_json();
        Ok(CallToolResult::success(vec![Content::text(to_json(
            &threads,
        )?)]))
    }

    #[tool(description = r#"Get detailed timing logs for a specific function.

Returns JSON array of recent execution logs with timestamps and duration. Use functions_timing first to get function IDs, then use this tool to get detailed logs."#)]
    async fn function_timing_logs(
        &self,
        params: Parameters<FunctionIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let function_id = params.0.function_id;
        log_debug(&format!(
            "Tool called: function_timing_logs({})",
            function_id
        ));

        match get_function_logs_timing(function_id) {
            Some(logs) => {
                let current_elapsed_ns = get_current_elapsed_ns();
                let formatted = JsonFunctionTimingLogsList::from_logs(&logs, current_elapsed_ns);
                Ok(CallToolResult::success(vec![Content::text(to_json(
                    &formatted,
                )?)]))
            }
            None => Ok(CallToolResult::error(vec![Content::text(format!(
                "Function with id {} not found",
                function_id
            ))])),
        }
    }

    #[tool(
        description = r#"Get detailed allocation logs for a specific function (requires hotpath-alloc feature).

Returns JSON array of recent allocation logs. Use functions_alloc first to get function IDs, then use this tool to get detailed logs."#
    )]
    async fn function_alloc_logs(
        &self,
        params: Parameters<FunctionIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let function_id = params.0.function_id;
        log_debug(&format!(
            "Tool called: function_alloc_logs({})",
            function_id
        ));

        match get_function_logs_alloc(function_id) {
            Some(logs) => {
                let current_elapsed_ns = get_current_elapsed_ns();
                let formatted = JsonFunctionAllocLogsList::from_logs(&logs, current_elapsed_ns);
                Ok(CallToolResult::success(vec![Content::text(to_json(
                    &formatted,
                )?)]))
            }
            None => Ok(CallToolResult::error(vec![Content::text(
                "Memory profiling not available - enable hotpath-alloc feature",
            )])),
        }
    }

    #[tool(description = r#"Get detailed message logs for a specific channel.

Returns JSON array of recent send/receive events with timestamps. Use channels first to get channel IDs, then use this tool to get detailed logs."#)]
    async fn channel_logs(
        &self,
        params: Parameters<ChannelIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let channel_id = &params.0.channel_id;
        log_debug(&format!("Tool called: channel_logs({})", channel_id));

        let id: u32 = channel_id.parse().map_err(|_| {
            McpError::invalid_params(format!("Invalid channel_id: {}", channel_id), None)
        })?;

        match get_channel_logs(id) {
            Some(logs) => {
                let current_elapsed_ns = get_current_elapsed_ns();
                let formatted = JsonChannelLogsList::from_logs(&logs, current_elapsed_ns);
                Ok(CallToolResult::success(vec![Content::text(to_json(
                    &formatted,
                )?)]))
            }
            None => Ok(CallToolResult::error(vec![Content::text(
                "Channel not found",
            )])),
        }
    }

    #[tool(description = r#"Get detailed item logs for a specific stream.

Returns JSON array of recent yield events with timestamps. Use streams first to get stream IDs, then use this tool to get detailed logs."#)]
    async fn stream_logs(
        &self,
        params: Parameters<StreamIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let stream_id = &params.0.stream_id;
        log_debug(&format!("Tool called: stream_logs({})", stream_id));

        let id: u32 = stream_id.parse().map_err(|_| {
            McpError::invalid_params(format!("Invalid stream_id: {}", stream_id), None)
        })?;

        match get_stream_logs(id) {
            Some(logs) => {
                let current_elapsed_ns = get_current_elapsed_ns();
                let formatted = JsonStreamLogsList::from_logs(&logs, current_elapsed_ns);
                Ok(CallToolResult::success(vec![Content::text(to_json(
                    &formatted,
                )?)]))
            }
            None => Ok(CallToolResult::error(vec![Content::text(
                "Stream not found",
            )])),
        }
    }

    #[tool(description = r#"Get detailed call/poll logs for a specific future.

Returns JSON array of poll events and completion status. Use futures first to get future IDs, then use this tool to get detailed logs."#)]
    async fn future_logs(
        &self,
        params: Parameters<FutureIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let future_id = &params.0.future_id;
        log_debug(&format!("Tool called: future_logs({})", future_id));

        let id: u32 = future_id.parse().map_err(|_| {
            McpError::invalid_params(format!("Invalid future_id: {}", future_id), None)
        })?;

        match get_future_logs_list(id) {
            Some(calls) => {
                let formatted = JsonFutureLogsList::from(&calls);
                Ok(CallToolResult::success(vec![Content::text(to_json(
                    &formatted,
                )?)]))
            }
            None => Ok(CallToolResult::error(vec![Content::text(
                "Future not found",
            )])),
        }
    }

    #[tool(description = r#"Get metrics for all tracked gauges.

Returns JSON array with:
- id: gauge identifier
- key: gauge name/label
- current_value: current numeric value
- min_value: minimum value seen
- max_value: maximum value seen
- update_count: number of set/inc/dec operations

Use gauges to track numeric values that change over time like queue sizes, connection counts, or custom metrics."#)]
    async fn gauges(&self) -> Result<CallToolResult, McpError> {
        log_debug("Tool called: gauges");

        let gauges = get_debug_gauge_entries_json();
        Ok(CallToolResult::success(vec![Content::text(to_json(
            &gauges,
        )?)]))
    }

    #[tool(description = r#"Get detailed logs for a specific gauge.

Returns JSON array of recent value updates with timestamps. Use gauges first to get gauge IDs, then use this tool to get detailed logs."#)]
    async fn gauge_logs(
        &self,
        params: Parameters<GaugeIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let gauge_id = &params.0.gauge_id;
        log_debug(&format!("Tool called: gauge_logs({})", gauge_id));

        let id: u32 = gauge_id.parse().map_err(|_| {
            McpError::invalid_params(format!("Invalid gauge_id: {}", gauge_id), None)
        })?;

        match get_debug_gauge_logs(id) {
            Some(logs) => Ok(CallToolResult::success(vec![Content::text(to_json(
                &logs,
            )?)])),
            None => Ok(CallToolResult::error(vec![Content::text(
                "Gauge not found",
            )])),
        }
    }

    #[tool(description = r#"Get all dbg! debug entries.

Returns JSON array of debug entries with IDs, source locations, expressions, and current values. Use the returned IDs with dbg_logs to get detailed history."#)]
    async fn dbg_entries(&self) -> Result<CallToolResult, McpError> {
        log_debug("Tool called: dbg_entries");

        let entries = get_debug_dbg_entries_json();
        Ok(CallToolResult::success(vec![Content::text(to_json(
            &entries,
        )?)]))
    }

    #[tool(description = r#"Get all val! value tracking entries.

Returns JSON array of value entries with IDs, keys, source locations, and current values. Use the returned IDs with val_logs to get detailed history."#)]
    async fn val_entries(&self) -> Result<CallToolResult, McpError> {
        log_debug("Tool called: val_entries");

        let entries = get_debug_val_entries_json();
        Ok(CallToolResult::success(vec![Content::text(to_json(
            &entries,
        )?)]))
    }

    #[tool(description = r#"Get detailed logs for a specific dbg! debug entry.

Returns JSON array of recent debug values with timestamps. Use dbg_entries first to get entry IDs, then use this tool to get detailed logs."#)]
    async fn dbg_logs(&self, params: Parameters<DebugIdParam>) -> Result<CallToolResult, McpError> {
        let debug_id = params.0.debug_id;
        log_debug(&format!("Tool called: dbg_logs({})", debug_id));

        match get_dbg_logs(debug_id) {
            Some(logs) => Ok(CallToolResult::success(vec![Content::text(to_json(
                &logs,
            )?)])),
            None => Ok(CallToolResult::error(vec![Content::text(
                "Debug entry not found",
            )])),
        }
    }

    #[tool(description = r#"Get detailed logs for a specific val! debug entry.

Returns JSON array of recent value updates with timestamps. Use val_entries first to get entry IDs, then use this tool to get detailed logs."#)]
    async fn val_logs(&self, params: Parameters<DebugIdParam>) -> Result<CallToolResult, McpError> {
        let debug_id = params.0.debug_id;
        log_debug(&format!("Tool called: val_logs({})", debug_id));

        match get_val_logs(debug_id) {
            Some(logs) => Ok(CallToolResult::success(vec![Content::text(to_json(
                &logs,
            )?)])),
            None => Ok(CallToolResult::error(vec![Content::text(
                "Value entry not found",
            )])),
        }
    }

    #[tool(
        description = r#"Get Tokio runtime metrics snapshot (requires tokio feature).

Returns JSON with per-worker stats (park count, busy duration, poll count, steal count) and global stats (alive tasks, queue depths, blocking threads, IO driver metrics). Requires calling hotpath::tokio_runtime!() in the profiled application."#
    )]
    async fn tokio_runtime(&self) -> Result<CallToolResult, McpError> {
        log_debug("Tool called: tokio_runtime");

        #[cfg(feature = "tokio")]
        match crate::tokio_runtime::get_runtime_json() {
            Some(snapshot) => {
                return Ok(CallToolResult::success(vec![Content::text(to_json(
                    &snapshot,
                )?)]));
            }
            None => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "Tokio runtime metrics not available - use hotpath::tokio_runtime!() to start collection",
                )]));
            }
        }

        #[cfg(not(feature = "tokio"))]
        Ok(CallToolResult::error(vec![Content::text(
            "Tokio runtime metrics not available - enable tokio feature",
        )]))
    }

    #[tool(description = r#"Get profiler status including uptime.

Returns JSON with:
- uptime: human-readable duration since profiler started (e.g. "1m 23s", "2h 5m 30s")

Use to check if the profiler is running and how long it has been active."#)]
    async fn profiler_status(&self) -> Result<CallToolResult, McpError> {
        log_debug("Tool called: profiler_status");

        let status = JsonProfilerStatus {
            uptime: format_duration(get_current_elapsed_ns()),
        };
        Ok(CallToolResult::success(vec![Content::text(to_json(
            &status,
        )?)]))
    }
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn get_current_elapsed_ns() -> u64 {
    crate::lib_on::current_elapsed_ns()
}

#[tool_handler]
impl ServerHandler for HotPathMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "hotpath".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                title: None,
                website_url: None,
                icons: None,
            },
            instructions: Some(
                "hothath profiler metrics MCP server. Provides tools to query profiling data."
                    .into(),
            ),
        }
    }
}

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
fn to_json<T: serde::Serialize>(value: &T) -> Result<String, McpError> {
    serde_json::to_string(value)
        .map_err(|e| McpError::internal_error(format!("Failed to serialize metrics: {}", e), None))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

fn check_auth(expected: Option<&str>, provided: Option<&str>) -> bool {
    match expected {
        None => true,
        Some(expected) => provided
            .map(|p| constant_time_eq(p.as_bytes(), expected.as_bytes()))
            .unwrap_or(false),
    }
}

async fn auth_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, axum::http::StatusCode> {
    let expected = std::env::var("HOTPATH_MCP_AUTH_TOKEN")
        .ok()
        .filter(|s| !s.is_empty());
    let provided = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    if check_auth(expected.as_deref(), provided) {
        Ok(next.run(request).await)
    } else {
        Err(axum::http::StatusCode::UNAUTHORIZED)
    }
}

static MCP_SERVER_STARTED: OnceLock<()> = OnceLock::new();

#[cfg_attr(feature = "hotpath-meta", hotpath_meta::measure(log = true))]
pub(crate) fn start_mcp_server_once() {
    MCP_SERVER_STARTED.get_or_init(|| {
        let port = *MCP_SERVER_PORT;

        let auth_enabled = std::env::var("HOTPATH_MCP_AUTH_TOKEN")
            .ok()
            .filter(|s| !s.is_empty())
            .is_some();
        log_debug(&format!(
            "Starting MCP server on port {} (auth: {})",
            port,
            if auth_enabled { "enabled" } else { "disabled" }
        ));

        std::thread::Builder::new()
            .name("hp-mcp".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create MCP runtime");

                rt.block_on(async move {
                    let cancellation_token = CancellationToken::new();

                    let config = StreamableHttpServerConfig {
                        sse_keep_alive: Some(Duration::from_secs(15)),
                        sse_retry: None,
                        stateful_mode: true,
                        cancellation_token: cancellation_token.clone(),
                    };

                    let service = StreamableHttpService::new(
                        || Ok(HotPathMcpServer::new()),
                        Arc::new(LocalSessionManager::default()),
                        config,
                    );

                    let app = Router::new()
                        .nest_service("/mcp", service)
                        .layer(axum::middleware::from_fn(auth_middleware));

                    let addr = format!("localhost:{}", port);
                    let listener = match tokio::net::TcpListener::bind(&addr).await {
                        Ok(l) => l,
                        Err(e) => {
                            log_debug(&format!("Failed to bind to {}: {}", addr, e));
                            return;
                        }
                    };

                    log_debug(&format!("Listening on http://{}/mcp", addr));

                    let _ = axum::serve(listener, app)
                        .with_graceful_shutdown(async move {
                            cancellation_token.cancelled().await;
                        })
                        .await;
                });
            })
            .expect("Failed to spawn MCP server thread");
    });
}

#[cfg(feature = "dev")]
fn log_debug(msg: &str) {
    use std::io::Write;
    let _ = std::fs::create_dir_all("log");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("log/development.log")
    {
        let now = chrono::Local::now();
        let _ = writeln!(
            file,
            "{} DEBUG [hotpath-mcp] {}",
            now.format("%Y-%m-%dT%H:%M:%S"),
            msg
        );
    }
}

#[cfg(not(feature = "dev"))]
fn log_debug(_msg: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_disabled_allows_all() {
        assert!(check_auth(None, None));
        assert!(check_auth(None, Some("anything")));
    }

    #[test]
    fn auth_enabled_rejects_missing() {
        assert!(!check_auth(Some("secret"), None));
    }

    #[test]
    fn auth_enabled_rejects_wrong() {
        assert!(!check_auth(Some("secret"), Some("wrong")));
        assert!(!check_auth(Some("secret"), Some("Secret")));
        assert!(!check_auth(Some("secret"), Some("")));
    }

    #[test]
    fn auth_enabled_accepts_correct() {
        assert!(check_auth(Some("secret"), Some("secret")));
        assert!(check_auth(Some("Bearer token"), Some("Bearer token")));
    }
}
