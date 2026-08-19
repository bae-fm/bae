#![deny(unreachable_pub, dead_code)]

use axum::body::Body;
use axum::extract::State;
use axum::http::header::{AUTHORIZATION, WWW_AUTHENTICATE};
use axum::http::{Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
pub use bae_automation::Automation;

use bae_automation::{AutomationError, AutomationTool};
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
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::warn;

type McpTokenProvider = dyn Fn() -> Result<String, String> + Send + Sync;
#[derive(Debug, Clone)]
pub enum McpServerStatus {
    Disabled,
    Running { url: String },
    Error { error: McpServerError },
}

#[derive(Debug, Clone)]
pub enum McpServerError {
    InvalidConfig { detail: String },
    TokenUnavailable { detail: String },
    BindFailed { detail: String },
    ServerFailed { detail: String },
}

impl McpServerError {
    pub fn detail(&self) -> &str {
        match self {
            Self::InvalidConfig { detail }
            | Self::TokenUnavailable { detail }
            | Self::BindFailed { detail }
            | Self::ServerFailed { detail } => detail,
        }
    }
}

#[derive(Clone)]
pub struct McpServerController {
    automation: Automation,
    token_provider: Arc<McpTokenProvider>,
    inner: Arc<Mutex<McpServerControllerState>>,
}

enum McpServerControllerState {
    Disabled,
    Running {
        port: u16,
        url: String,
        cancellation: CancellationToken,
        task: JoinHandle<()>,
    },
    Error {
        error: McpServerError,
    },
}

impl McpServerControllerState {
    fn status(&self) -> McpServerStatus {
        match self {
            Self::Disabled => McpServerStatus::Disabled,
            Self::Running { url, .. } => McpServerStatus::Running { url: url.clone() },
            Self::Error { error } => McpServerStatus::Error {
                error: error.clone(),
            },
        }
    }
}

impl McpServerController {
    pub fn new(automation: Automation, token_provider: Arc<McpTokenProvider>) -> Self {
        Self {
            automation,
            token_provider,
            inner: Arc::new(Mutex::new(McpServerControllerState::Disabled)),
        }
    }

    pub async fn apply_config(&self, config: bae_core::config::McpConfig) -> McpServerStatus {
        if !config.enabled {
            self.shutdown().await;
            return McpServerStatus::Disabled;
        }
        if let Err(error) = config.validate() {
            return self
                .record_error(McpServerError::InvalidConfig {
                    detail: error.to_string(),
                })
                .await;
        }

        {
            let state = self.inner.lock().await;
            if let McpServerControllerState::Running { port, .. } = &*state {
                if *port == config.port {
                    return state.status();
                }
            }
        }

        self.shutdown().await;
        self.start(config.port).await
    }

    pub async fn status(&self) -> McpServerStatus {
        self.inner.lock().await.status()
    }

    pub async fn shutdown(&self) {
        let task = {
            let mut state = self.inner.lock().await;
            match std::mem::replace(&mut *state, McpServerControllerState::Disabled) {
                McpServerControllerState::Running {
                    cancellation, task, ..
                } => {
                    cancellation.cancel();
                    Some(task)
                }
                _ => None,
            }
        };
        if let Some(task) = task {
            if let Err(error) = task.await {
                warn!("MCP server task join failed: {error}");
            }
        }
    }

    async fn start(&self, port: u16) -> McpServerStatus {
        if let Err(e) = self.token_provider.as_ref()() {
            return self
                .record_error(McpServerError::TokenUnavailable { detail: e })
                .await;
        }

        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let listener = match TcpListener::bind(addr).await {
            Ok(listener) => listener,
            Err(e) => {
                return self
                    .record_error(McpServerError::BindFailed {
                        detail: format!("failed to bind 127.0.0.1:{port}: {e}"),
                    })
                    .await;
            }
        };

        let cancellation = CancellationToken::new();
        let service_cancellation = cancellation.child_token();
        let server_cancellation = cancellation.clone();
        let task_state = self.inner.clone();
        let automation = self.automation.clone();
        let token_provider = self.token_provider.clone();
        let service: StreamableHttpService<BaeMcpServer, LocalSessionManager> =
            StreamableHttpService::new(
                move || Ok(BaeMcpServer::new(automation.clone())),
                Default::default(),
                StreamableHttpServerConfig::default()
                    .with_sse_keep_alive(None)
                    .with_cancellation_token(service_cancellation),
            );
        let router = axum::Router::new().nest_service("/mcp", service).layer(
            middleware::from_fn_with_state(AuthState { token_provider }, bearer_auth),
        );
        let task = tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    server_cancellation.cancelled_owned().await;
                })
                .await
            {
                let error = McpServerError::ServerFailed {
                    detail: format!("MCP server stopped with error: {e}"),
                };
                warn!("{}", error.detail());
                let mut state = task_state.lock().await;
                if let McpServerControllerState::Running {
                    port: running_port, ..
                } = &*state
                {
                    if *running_port == port {
                        *state = McpServerControllerState::Error { error };
                    }
                }
            }
        });

        let url = format!("http://127.0.0.1:{port}/mcp");
        let status = McpServerStatus::Running { url: url.clone() };
        let mut state = self.inner.lock().await;
        *state = McpServerControllerState::Running {
            port,
            url,
            cancellation,
            task,
        };
        status
    }

    async fn record_error(&self, error: McpServerError) -> McpServerStatus {
        self.shutdown().await;
        let status = McpServerStatus::Error { error };
        let mut state = self.inner.lock().await;
        if let McpServerStatus::Error { error } = &status {
            *state = McpServerControllerState::Error {
                error: error.clone(),
            };
        }
        status
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
    let expected = match state.token_provider.as_ref()() {
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
            Err(error) => Ok(CallToolResult::structured_error(automation_error_value(
                error,
            )?)),
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
