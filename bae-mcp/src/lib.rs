use axum::body::Body;
use axum::extract::State;
use axum::http::header::{AUTHORIZATION, WWW_AUTHENTICATE};
use axum::http::{Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
pub use bae_automation::Automation;

use bae_automation::{AutomationError, AutomationTool};
use bae_diagnostics::Diagnostics;
use bae_util::server::{ServerController, ServerError, ServerStatus};
use bae_util::task_failure::caught;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use rmcp::{ErrorData, ServerHandler};
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::warn;

/// Reads the MCP bearer token from the keychain. Async, because a keychain read
/// can wait on the system and the server asks on every request.
type McpTokenProvider = dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send>>
    + Send
    + Sync;

#[derive(Clone)]
pub struct McpServerController {
    automation: Automation,
    token_provider: Arc<McpTokenProvider>,
    server: ServerController<u16>,
    /// Where a panic in the server or in one of its requests is reported.
    diagnostics: Diagnostics,
}

impl McpServerController {
    pub fn new(
        automation: Automation,
        token_provider: Arc<McpTokenProvider>,
        diagnostics: Diagnostics,
    ) -> Self {
        let reporting = diagnostics.clone();
        Self {
            automation,
            token_provider,
            server: ServerController::new("MCP", Arc::new(move |bug| reporting.panicked(bug))),
            diagnostics,
        }
    }

    pub async fn apply_config(&self, config: bae_preferences::McpConfig) -> ServerStatus {
        if !config.enabled {
            return self.server.disable().await;
        }
        if let Err(error) = config.validate() {
            return self
                .server
                .record_error(ServerError::InvalidConfig { detail: error.to_string() })
                .await;
        }
        self.server.apply(config.port, |port| self.start(port)).await
    }

    pub async fn status(&self) -> ServerStatus {
        self.server.status().await
    }

    pub async fn shutdown(&self) {
        self.server.shutdown().await;
    }

    async fn start(&self, port: u16) -> ServerStatus {
        if let Err(e) = self.token_provider.as_ref()().await {
            return self
                .server
                .record_error(ServerError::CredentialUnavailable { detail: e })
                .await;
        }

        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let listener = match TcpListener::bind(addr).await {
            Ok(listener) => listener,
            Err(e) => {
                return self
                    .server
                    .record_error(ServerError::BindFailed {
                        detail: format!("failed to bind 127.0.0.1:{port}: {e}"),
                    })
                    .await;
            }
        };

        let automation = self.automation.clone();
        let token_provider = self.token_provider.clone();
        let diagnostics = self.diagnostics.clone();
        self.server
            .start(port, format!("http://127.0.0.1:{port}/mcp"), |cancellation| {
                let service: StreamableHttpService<BaeMcpServer, LocalSessionManager> =
                    StreamableHttpService::new(
                        move || Ok(BaeMcpServer::new(automation.clone())),
                        Default::default(),
                        StreamableHttpServerConfig::default()
                            .with_sse_keep_alive(None)
                            .with_cancellation_token(cancellation.child_token()),
                    );
                let router = axum::Router::new()
                    .nest_service("/mcp", service)
                    .layer(middleware::from_fn_with_state(
                        AuthState { token_provider },
                        bearer_auth,
                    ))
                    .layer(middleware::from_fn_with_state(diagnostics, report_panics));
                let shutdown = cancellation.clone();
                axum::serve(listener, router)
                    .with_graceful_shutdown(async move { shutdown.cancelled_owned().await })
            })
            .await
    }
}

/// Answer a request whose handling panicked with an internal error, and
/// report the panic: the server goes on serving the next request.
async fn report_panics(
    State(diagnostics): State<Diagnostics>,
    request: Request<Body>,
    next: Next,
) -> Response {
    match caught("answer an MCP request", next.run(request)).await {
        Ok(response) => response,
        Err(bug) => (StatusCode::INTERNAL_SERVER_ERROR, diagnostics.panicked(bug).to_string())
            .into_response(),
    }
}

#[derive(Clone)]
struct AuthState {
    token_provider: Arc<McpTokenProvider>,
}

async fn bearer_auth(
    State(state): State<AuthState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let expected = match state.token_provider.as_ref()().await {
        Ok(token) => format!("Bearer {token}"),
        Err(e) => {
            return (StatusCode::SERVICE_UNAVAILABLE, e).into_response();
        }
    };
    let authorized = match request.headers().get(AUTHORIZATION) {
        Some(value) => match value.to_str() {
            Ok(value) => value == expected,
            Err(error) => {
                warn!("invalid MCP Authorization header: {error}");
                false
            }
        },
        None => false,
    };
    if !authorized {
        return (
            StatusCode::UNAUTHORIZED,
            [(WWW_AUTHENTICATE, "Bearer")],
            "missing or invalid bearer token",
        )
            .into_response();
    }
    next.run(request).await
}

#[derive(Clone)]
struct BaeMcpServer {
    automation: Automation,
}

impl BaeMcpServer {
    fn new(automation: Automation) -> Self {
        Self { automation }
    }
}

impl ServerHandler for BaeMcpServer {
    fn get_info(&self) -> ServerInfo {
        mcp_server_info()
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let Some(tool) = AutomationTool::from_name(&request.name) else {
            return Err(ErrorData::invalid_params("tool not found", None));
        };
        let args = match request.arguments {
            Some(arguments) => Value::Object(arguments),
            None if tool.accepts_missing_arguments() => Value::Null,
            None => {
                return Ok(CallToolResult::structured_error(automation_error_value(
                    AutomationError::validation(format!(
                        "tool '{}' requires arguments",
                        tool.name()
                    )),
                )?));
            }
        };
        match self.automation.call_tool(tool, args).await {
            Ok(value) => Ok(CallToolResult::structured(value)),
            Err(error) => Ok(CallToolResult::structured_error(automation_error_value(error)?)),
        }
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: AutomationTool::all().map(mcp_tool).collect(),
            meta: None,
            next_cursor: None,
        })
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        AutomationTool::from_name(name).map(mcp_tool)
    }
}

fn mcp_tool(tool: AutomationTool) -> Tool {
    Tool::new(tool.name(), tool.description(), tool.input_schema())
}

fn mcp_server_info() -> ServerInfo {
    ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
}

fn automation_error_value(error: AutomationError) -> Result<Value, ErrorData> {
    serde_json::to_value(error).map_err(|e| {
        ErrorData::internal_error(format!("failed to serialize tool error: {e}"), None)
    })
}
