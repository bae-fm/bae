#![deny(unreachable_pub, dead_code)]
// Laying out rmcp's HTTP service future over `BaeMcpServer` nests the tool
// dispatch's async state machines deeper than rustc's default query depth of
// 128; the test build of this crate is where that layout is first computed.
#![recursion_limit = "256"]

use std::sync::Arc;

use bae_core::config::{ConfigError, McpConfig, SubsonicConfig};
use bae_core::library::AppServices;
use bae_core::server::{ServerError, ServerStatus};
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

/// A rejected service-config change: either the config itself (validation, or
/// the write to disk) or the running server refusing to come up on it.
#[derive(Debug, thiserror::Error)]
pub enum DesktopConfigError<E: ServerError> {
    #[error("{0}")]
    Config(ConfigError),
    #[error("{}", .0.detail())]
    Server(E),
}

impl DesktopServices {
    pub async fn start(services: AppServices, runtime: Handle) -> Self {
        let automation = Automation::new(services.clone(), &runtime);
        let token_manager = services.clone();
        let controller = McpServerController::new(
            automation,
            Arc::new(move || token_manager.ensure_mcp_token().map_err(|e| e.to_string())),
        );
        let initial = services.get_config().prefs.mcp;
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
        let initial_subsonic = services.get_config().prefs.subsonic;
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
                            (config.prefs.mcp, config.prefs.subsonic.clone())
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

    pub async fn set_mcp_config(
        &self,
        config: McpConfig,
    ) -> Result<(), DesktopConfigError<McpServerError>> {
        config.validate().map_err(DesktopConfigError::Config)?;
        apply_service_config(
            "MCP",
            config,
            self.services.get_config().prefs.mcp,
            |config| self.mcp_controller.apply_config(config),
            |config| self.services.set_mcp_config(config),
        )
        .await
    }

    pub async fn subsonic_server_status(&self) -> SubsonicServerStatus {
        self.subsonic_controller.status().await
    }

    pub async fn set_subsonic_config(
        &self,
        config: SubsonicConfig,
    ) -> Result<(), DesktopConfigError<SubsonicServerError>> {
        config.validate().map_err(DesktopConfigError::Config)?;
        apply_service_config(
            "Subsonic",
            config,
            self.services.get_config().prefs.subsonic,
            |config| self.subsonic_controller.apply_config(config),
            |config| self.services.set_subsonic_config(config),
        )
        .await
    }

    /// Store a new Subsonic server password in the keyring, then restart the
    /// running server so it authenticates against the new password. The password
    /// is not in config, so `apply_config` cannot see the change — the restart
    /// is how a running server picks it up.
    pub async fn set_subsonic_password(
        &self,
        password: &str,
    ) -> Result<(), DesktopConfigError<SubsonicServerError>> {
        self.services
            .set_subsonic_password(password.to_string())
            .map_err(|e| DesktopConfigError::Config(ConfigError::Config(e.to_string())))?;
        let config = self.services.get_config().prefs.subsonic;
        if let SubsonicServerStatus::Error { error } =
            self.subsonic_controller.restart(config).await
        {
            return Err(DesktopConfigError::Server(error));
        }
        Ok(())
    }

    /// Stop both hosted servers. They start and stop together with the app, and
    /// no caller has ever wanted one without the other.
    pub async fn shutdown(&self) {
        self.mcp_controller.shutdown().await;
        self.subsonic_controller.shutdown().await;
    }
}

/// Apply an already-validated service config to the running server, then
/// persist it. A server that refuses the new config, or a failed write, leaves
/// both the server and the stored config on `previous` — no half-applied
/// change. A free function so the rollback contract is testable against a bare
/// controller + manager, without a full app bootstrap.
async fn apply_service_config<C, E, F>(
    label: &str,
    config: C,
    previous: C,
    apply: impl Fn(C) -> F,
    persist: impl FnOnce(C) -> Result<(), ConfigError>,
) -> Result<(), DesktopConfigError<E>>
where
    C: Clone,
    E: ServerError,
    F: std::future::Future<Output = ServerStatus<E>>,
{
    if let ServerStatus::Error { error } = apply(config.clone()).await {
        apply(previous).await;
        return Err(DesktopConfigError::Server(error));
    }

    match persist(config) {
        Ok(()) => Ok(()),
        Err(error) => {
            if let ServerStatus::Error {
                error: rollback_error,
            } = apply(previous).await
            {
                tracing::warn!(
                    "{label} runtime rollback failed after config save error: {}",
                    rollback_error.detail()
                );
            }
            Err(DesktopConfigError::Config(error))
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
        let (runtime, services, _tmp) = support::runtime_with_services();

        runtime.block_on(async {
            let desktop = DesktopServices::start(services, tokio::runtime::Handle::current()).await;
            desktop.shutdown().await;
        });
    }

    #[test]
    fn desktop_controller_calls_do_not_reenter_the_owned_runtime() {
        let (runtime, services, _tmp) = support::runtime_with_services();
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
            desktop.shutdown().await;
        });
    }

    /// When the runtime apply fails (here: the configured port is already bound,
    /// so the bind fails), `set_subsonic_config` must surface the error and leave
    /// the persisted config untouched — no half-applied enable.
    #[test]
    fn set_subsonic_config_rolls_back_persisted_config_on_runtime_error() {
        let (runtime, services, _tmp) = support::runtime_with_services();
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

        let before = services.get_config().prefs.subsonic;
        let result = runtime.block_on(apply_service_config(
            "Subsonic",
            SubsonicConfig {
                enabled: true,
                port,
                username: "listener".to_string(),
                bind_address: "127.0.0.1".to_string(),
            },
            before.clone(),
            |config| controller.apply_config(config),
            |config| services.set_subsonic_config(config),
        ));

        assert!(
            matches!(result, Err(DesktopConfigError::Server(_))),
            "a failed bind must surface as a server error, got {result:?}"
        );
        assert_eq!(
            services.get_config().prefs.subsonic,
            before,
            "a runtime apply failure must not persist the new config"
        );
    }
}
