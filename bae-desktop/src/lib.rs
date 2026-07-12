#![deny(unreachable_pub, dead_code)]

use std::sync::Arc;

use bae_core::app::{bootstrap as bootstrap_core, BootstrapError, RunningApp};
use bae_core::config::{ConfigError, McpConfig};
use bae_core::library::AppServices;
use bae_core::ui::UiEventBus;
use bae_mcp::{Automation, McpServerController};
pub use bae_mcp::{McpServerError, McpServerStatus};
use tokio::runtime::Runtime;

pub struct DesktopApp {
    pub runtime: Runtime,
    pub services: AppServices,
    pub ui_event_bus: UiEventBus,
    mcp_controller: McpServerController,
}

#[derive(Debug)]
pub enum DesktopMcpConfigError {
    Config(ConfigError),
    Server(McpServerError),
}

impl std::fmt::Display for DesktopMcpConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(error) => write!(f, "{error}"),
            Self::Server(error) => write!(f, "{}", error.detail()),
        }
    }
}

impl std::error::Error for DesktopMcpConfigError {}

pub fn bootstrap(
    library_id: String,
    position_update_interval_ms: u32,
    restore_playback: bool,
    cloudkit_ops: Option<bae_core::CloudKitOpsRef>,
) -> Result<DesktopApp, BootstrapError> {
    bootstrap_core(
        library_id,
        position_update_interval_ms,
        restore_playback,
        cloudkit_ops,
    )
    .map(DesktopApp::from_running_app)
}

impl DesktopApp {
    pub fn from_running_app(app: RunningApp) -> Self {
        let RunningApp {
            runtime,
            services,
            ui_event_bus,
        } = app;

        let automation = Automation::new(services.clone(), runtime.handle().clone());
        automation.start_event_indexing();
        let token_manager = services.library_manager().clone();
        let controller = McpServerController::new(
            automation,
            Arc::new(move || token_manager.ensure_mcp_token().map_err(|e| e.to_string())),
        );
        let initial = services.library_manager().get_config().mcp;
        runtime.block_on(controller.apply_config(initial));

        let config_controller = controller.clone();
        let mut config_rx = services.library_manager().subscribe_config_changes();
        runtime.spawn(async move {
            loop {
                match config_rx.changed().await {
                    Ok(()) => {
                        let mcp = config_rx.borrow().mcp;
                        config_controller.apply_config(mcp).await;
                    }
                    Err(error) => {
                        tracing::debug!("MCP config watcher stopped: {error}");
                        break;
                    }
                }
            }
        });

        Self {
            runtime,
            services,
            ui_event_bus,
            mcp_controller: controller,
        }
    }

    pub fn mcp_server_status(&self) -> McpServerStatus {
        self.runtime.block_on(self.mcp_controller.status())
    }

    pub fn set_mcp_config(&self, config: McpConfig) -> Result<(), DesktopMcpConfigError> {
        config.validate().map_err(DesktopMcpConfigError::Config)?;
        let previous = self.services.library_manager().get_config().mcp;
        let status = self
            .runtime
            .block_on(self.mcp_controller.apply_config(config));
        if let McpServerStatus::Error { error } = status {
            self.runtime
                .block_on(self.mcp_controller.apply_config(previous));
            return Err(DesktopMcpConfigError::Server(error));
        }

        match self.services.library_manager().set_mcp_config(config) {
            Ok(()) => Ok(()),
            Err(error) => {
                if let McpServerStatus::Error {
                    error: rollback_error,
                } = self
                    .runtime
                    .block_on(self.mcp_controller.apply_config(previous))
                {
                    tracing::warn!(
                        "MCP runtime rollback failed after config save error: {}",
                        rollback_error.detail()
                    );
                }
                Err(DesktopMcpConfigError::Config(error))
            }
        }
    }

    pub fn shutdown_mcp(&self) {
        self.runtime.block_on(self.mcp_controller.shutdown());
    }
}
