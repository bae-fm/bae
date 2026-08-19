#![deny(unreachable_pub, dead_code)]

use std::sync::Arc;

use bae_core::config::{ConfigError, McpConfig, SubsonicConfig};
use bae_core::library::AppServices;
use bae_mcp::{Automation, McpServerController};
pub use bae_mcp::{McpServerError, McpServerStatus};
use bae_subsonic::SubsonicServerController;
pub use bae_subsonic::{SubsonicServerError, SubsonicServerStatus};
use tokio::runtime::Handle;

pub struct DesktopServices {
    services: AppServices,
    mcp_controller: McpServerController,
    subsonic_controller: SubsonicServerController,
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

#[derive(Debug)]
pub enum DesktopSubsonicConfigError {
    Config(ConfigError),
    Server(SubsonicServerError),
}

impl std::fmt::Display for DesktopSubsonicConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(error) => write!(f, "{error}"),
            Self::Server(error) => write!(f, "{}", error.detail()),
        }
    }
}

impl std::error::Error for DesktopSubsonicConfigError {}

impl DesktopServices {
    pub async fn start(services: AppServices, runtime: Handle) -> Self {
        let automation = Automation::new(services.clone());
        let token_manager = services.clone();
        let controller = McpServerController::new(
            automation,
            Arc::new(move || token_manager.ensure_mcp_token().map_err(|e| e.to_string())),
        );
        let initial = services.get_config().mcp;
        controller.apply_config(initial).await;

        // The Subsonic server's runtime credential is the config username plus
        // the keyring password; the provider supplies the password, and the
        // username rides on each applied `SubsonicConfig`.
        let password_manager = services.clone();
        let subsonic_controller = SubsonicServerController::new(
            services.clone(),
            Arc::new(move || {
                password_manager
                    .get_subsonic_password()
                    .map_err(|e| e.to_string())
            }),
        );
        let initial_subsonic = services.get_config().subsonic;
        subsonic_controller.apply_config(initial_subsonic).await;

        let config_controller = controller.clone();
        let config_subsonic_controller = subsonic_controller.clone();
        let mut config_rx = services.subscribe_config_changes();
        runtime.spawn(async move {
            loop {
                match config_rx.changed().await {
                    Ok(()) => {
                        let (mcp, subsonic) = {
                            let config = config_rx.borrow();
                            (config.mcp, config.subsonic.clone())
                        };
                        config_controller.apply_config(mcp).await;
                        config_subsonic_controller.apply_config(subsonic).await;
                    }
                    Err(error) => {
                        tracing::debug!("config watcher stopped: {error}");
                        break;
                    }
                }
            }
        });

        Self {
            services,
            mcp_controller: controller,
            subsonic_controller,
        }
    }

    pub async fn mcp_server_status(&self) -> McpServerStatus {
        self.mcp_controller.status().await
    }

    pub async fn set_mcp_config(&self, config: McpConfig) -> Result<(), DesktopMcpConfigError> {
        config.validate().map_err(DesktopMcpConfigError::Config)?;
        let previous = self.services.get_config().mcp;
        let status = self.mcp_controller.apply_config(config).await;
        if let McpServerStatus::Error { error } = status {
            self.mcp_controller.apply_config(previous).await;
            return Err(DesktopMcpConfigError::Server(error));
        }

        match self.services.set_mcp_config(config) {
            Ok(()) => Ok(()),
            Err(error) => {
                if let McpServerStatus::Error {
                    error: rollback_error,
                } = self.mcp_controller.apply_config(previous).await
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

    pub async fn shutdown_mcp(&self) {
        self.mcp_controller.shutdown().await;
    }

    pub async fn subsonic_server_status(&self) -> SubsonicServerStatus {
        self.subsonic_controller.status().await
    }

    /// Apply a new Subsonic server config: validate, apply to the running server
    /// with rollback if it errors, then persist. Mirrors [`Self::set_mcp_config`].
    pub async fn set_subsonic_config(
        &self,
        config: SubsonicConfig,
    ) -> Result<(), DesktopSubsonicConfigError> {
        apply_subsonic_config(&self.subsonic_controller, &self.services, config).await
    }

    /// Store a new Subsonic server password in the keyring, then restart the
    /// running server so it authenticates against the new password. The password
    /// is not in config, so `apply_config` cannot see the change — the restart
    /// is how a running server picks it up.
    pub async fn set_subsonic_password(
        &self,
        password: &str,
    ) -> Result<(), DesktopSubsonicConfigError> {
        self.services
            .set_subsonic_password(password.to_string())
            .map_err(|e| DesktopSubsonicConfigError::Config(ConfigError::Config(e.to_string())))?;
        let config = self.services.get_config().subsonic;
        if let SubsonicServerStatus::Error { error } =
            self.subsonic_controller.restart(config).await
        {
            return Err(DesktopSubsonicConfigError::Server(error));
        }
        Ok(())
    }

    pub async fn shutdown_subsonic(&self) {
        self.subsonic_controller.shutdown().await;
    }
}

/// Validate, runtime-apply (with rollback on error), then persist a Subsonic
/// config. Extracted from the [`DesktopServices`] method so the rollback contract is
/// testable against a bare controller + manager, without a full app bootstrap.
async fn apply_subsonic_config(
    controller: &SubsonicServerController,
    services: &AppServices,
    config: SubsonicConfig,
) -> Result<(), DesktopSubsonicConfigError> {
    config
        .validate()
        .map_err(DesktopSubsonicConfigError::Config)?;
    let previous = services.get_config().subsonic;
    let status = controller.apply_config(config.clone()).await;
    if let SubsonicServerStatus::Error { error } = status {
        controller.apply_config(previous).await;
        return Err(DesktopSubsonicConfigError::Server(error));
    }

    match services.set_subsonic_config(config) {
        Ok(()) => Ok(()),
        Err(error) => {
            if let SubsonicServerStatus::Error {
                error: rollback_error,
            } = controller.apply_config(previous).await
            {
                tracing::warn!(
                    "Subsonic runtime rollback failed after config save error: {}",
                    rollback_error.detail()
                );
            }
            Err(DesktopSubsonicConfigError::Config(error))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use bae_core::config::SubsonicConfig;
    use bae_test_support as support;

    #[test]
    fn desktop_initialization_does_not_reenter_the_owned_runtime() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let (manager, _tmp) = support::setup_fresh_library(&runtime);
        let services = runtime
            .block_on(AppServices::for_test(manager))
            .expect("app services");

        runtime.block_on(async {
            let desktop = DesktopServices::start(services, tokio::runtime::Handle::current()).await;
            desktop.shutdown_mcp().await;
            desktop.shutdown_subsonic().await;
        });
    }

    #[test]
    fn desktop_controller_calls_do_not_reenter_the_owned_runtime() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let (manager, _tmp) = support::setup_fresh_library(&runtime);
        let services = runtime
            .block_on(AppServices::for_test(manager))
            .expect("app services");
        let desktop = runtime.block_on(DesktopServices::start(services, runtime.handle().clone()));

        runtime.block_on(async {
            assert!(matches!(
                desktop.mcp_server_status().await,
                McpServerStatus::Disabled
            ));
            assert!(matches!(
                desktop.subsonic_server_status().await,
                SubsonicServerStatus::Disabled
            ));
            desktop.shutdown_mcp().await;
            desktop.shutdown_subsonic().await;
        });
    }

    /// When the runtime apply fails (here: the configured port is already bound,
    /// so the bind fails), `set_subsonic_config` must surface the error and leave
    /// the persisted config untouched — no half-applied enable.
    #[test]
    fn set_subsonic_config_rolls_back_persisted_config_on_runtime_error() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let (manager, _tmp) = support::setup_fresh_library(&runtime);
        let services = runtime
            .block_on(AppServices::for_test(manager))
            .expect("app services");
        services
            .set_subsonic_password("s3cret".to_string())
            .expect("seed keyring password");

        let password_services = services.clone();
        let controller = SubsonicServerController::new(
            services.clone(),
            Arc::new(move || {
                password_services
                    .get_subsonic_password()
                    .map_err(|e| e.to_string())
            }),
        );

        // Occupy the port so the runtime bind fails.
        let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = occupied.local_addr().unwrap().port();

        let before = services.get_config().subsonic;
        let result = runtime.block_on(apply_subsonic_config(
            &controller,
            &services,
            SubsonicConfig {
                enabled: true,
                port,
                username: "listener".to_string(),
                bind_address: "127.0.0.1".to_string(),
            },
        ));

        assert!(
            matches!(result, Err(DesktopSubsonicConfigError::Server(_))),
            "a failed bind must surface as a server error, got {result:?}"
        );
        assert_eq!(
            services.get_config().subsonic,
            before,
            "a runtime apply failure must not persist the new config"
        );
    }
}
