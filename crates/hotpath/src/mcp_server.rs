use axum::Router;
use rmcp::{
    handler::server::tool::ToolRouter,
    model::*,
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    ErrorData as McpError, ServerHandler,
};
use std::sync::{Arc, LazyLock, OnceLock};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

static MCP_SERVER_PORT: LazyLock<u16> = LazyLock::new(|| {
    std::env::var("HOTPATH_MCP_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(6771)
});

#[derive(Clone)]
pub struct HotPathMcpServer {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl HotPathMcpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Get current time as Unix timestamp")]
    async fn hotpath_time(&self) -> Result<CallToolResult, McpError> {
        #[cfg(debug_assertions)]
        log_debug("Tool called: hotpath_time");

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Ok(CallToolResult::success(vec![Content::text(
            now.to_string(),
        )]))
    }
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
                "HotPath profiler metrics server. Provides tools to query profiling data.".into(),
            ),
        }
    }
}

static MCP_SERVER_STARTED: OnceLock<()> = OnceLock::new();

pub(crate) fn start_mcp_server_once() {
    MCP_SERVER_STARTED.get_or_init(|| {
        let port = *MCP_SERVER_PORT;

        #[cfg(debug_assertions)]
        log_debug(&format!("Starting MCP server on port {}", port));

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

                    let app = Router::new().nest_service("/mcp", service);

                    let addr = format!("localhost:{}", port);
                    let listener = match tokio::net::TcpListener::bind(&addr).await {
                        Ok(l) => l,
                        Err(e) => {
                            #[cfg(debug_assertions)]
                            log_debug(&format!("Failed to bind to {}: {}", addr, e));
                            return;
                        }
                    };

                    #[cfg(debug_assertions)]
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

#[cfg(debug_assertions)]
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
