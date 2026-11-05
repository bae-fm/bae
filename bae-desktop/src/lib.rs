// Laying out rmcp's HTTP service future over `BaeMcpServer` nests the tool
// dispatch's async state machines deeper than rustc's default query depth of
// 128; the test build of this crate is where that layout is first computed.
#![recursion_limit = "256"]

use std::sync::Arc;

use bae_app::AppServices;
use bae_diagnostics::Diagnostics;
use bae_mcp::{Automation, McpServerController};
use bae_preferences::{McpConfig, PreferencesError, SubsonicConfig};
use bae_subsonic::SubsonicServerController;
pub use bae_util::server::{ServerError, ServerStatus};
use bae_util::task_failure::caught;
use tokio::runtime::Handle;

pub struct DesktopServices {
    services: AppServices,
    mcp_controller: McpServerController,
    subsonic_controller: SubsonicServerController,
}

/// A rejected service-config change: either the config itself (validation, or
/// the write to disk) or the running server refusing to come up on it.
#[derive(Debug, thiserror::Error)]
pub enum DesktopConfigError {
    #[error("{0}")]
    Config(PreferencesError),
    #[error("{}", .0.detail())]
    Server(ServerError),
}

impl DesktopServices {
    /// Start the MCP and Subsonic servers the config enables, and keep them in
    /// step with later config changes. A panic in either server, a request or
    /// a config change is reported to `diagnostics`.
    pub async fn start(services: AppServices, runtime: Handle, diagnostics: Diagnostics) -> Self {
        let automation = Automation::new(services.clone());
        let token_manager = services.clone();
        let controller = McpServerController::new(
            automation,
            Arc::new(move || {
                let token_manager = token_manager.clone();
                Box::pin(async move {
                    token_manager.ensure_mcp_token().await.map_err(|e| e.to_string())
                })
            }),
            diagnostics.clone(),
        );

        // The Subsonic server's runtime credential is the config username plus
        // the keyring password; the provider supplies the password, and the
        // username rides on each applied `SubsonicConfig`.
        let password_manager = services.clone();
        let subsonic_controller = SubsonicServerController::new(
            services.clone(),
            Arc::new(move || {
                let password_manager = password_manager.clone();
                Box::pin(async move {
                    password_manager.get_subsonic_password().await.map_err(|e| e.to_string())
                })
            }),
            diagnostics.clone(),
        );
        // The servers' settings now, applied before this returns, then each
        // change to them as its write commits.
        let settings = services.follow_preferences(|prefs| (prefs.mcp, prefs.subsonic));
        match settings.next().await {
            Ok((mcp, subsonic)) => {
                controller.apply_config(mcp).await;
                subsonic_controller.apply_config(subsonic).await;
            }
            Err(error) => {
                tracing::warn!("the servers start off: their settings couldn't be read: {error}")
            }
        }

        let config_controller = controller.clone();
        let config_subsonic_controller = subsonic_controller.clone();
        let watcher_diagnostics = diagnostics.clone();
        // Each change is applied under its own catch, so a panic applying one
        // is reported and the next change is still applied.
        drop(diagnostics.spawn_reported(
            &runtime,
            "follow the servers' settings",
            async move {
                loop {
                    match settings.next().await {
                        Ok((mcp, subsonic)) => {
                            if let Err(failed) = caught("apply a changed server config", async {
                                config_controller.apply_config(mcp).await;
                                config_subsonic_controller.apply_config(subsonic).await;
                            })
                            .await
                            {
                                watcher_diagnostics.panicked(failed);
                            }
                        }
                        Err(bae_db::live_query::LiveQueryError::Query(error)) => {
                            tracing::warn!("the servers' settings couldn't be read: {error}");
                        }
                        Err(bae_db::live_query::LiveQueryError::Cancelled) => {
                            tracing::debug!("server settings follower stopped: the library closed");
                            break;
                        }
                    }
                }
            },
            // Each change is caught above; what is left to panic is the
            // follower itself, and with it the servers keep the config they
            // last applied.
            |_| {},
        ));

        Self { services, mcp_controller: controller, subsonic_controller }
    }

    pub async fn mcp_server_status(&self) -> ServerStatus {
        self.mcp_controller.status().await
    }

    pub async fn set_mcp_config(&self, config: McpConfig) -> Result<(), DesktopConfigError> {
        config.validate().map_err(DesktopConfigError::Config)?;
        apply_service_config(
            "MCP",
            config,
            self.stored_preferences().await?.mcp,
            |config| self.mcp_controller.apply_config(config),
            |config| self.services.set_mcp_config(config),
        )
        .await
    }

    pub async fn subsonic_server_status(&self) -> ServerStatus {
        self.subsonic_controller.status().await
    }

    pub async fn set_subsonic_config(
        &self,
        config: SubsonicConfig,
    ) -> Result<(), DesktopConfigError> {
        config.validate().map_err(DesktopConfigError::Config)?;
        apply_service_config(
            "Subsonic",
            config,
            self.stored_preferences().await?.subsonic,
            |config| self.subsonic_controller.apply_config(config),
            |config| self.services.set_subsonic_config(config),
        )
        .await
    }

    /// Store a new Subsonic server password in the keyring, then restart the
    /// running server so it authenticates against the new password. The password
    /// is not in config, so `apply_config` cannot see the change — the restart
    /// is how a running server picks it up.
    pub async fn set_subsonic_password(&self, password: &str) -> Result<(), DesktopConfigError> {
        self.services
            .set_subsonic_password(password.to_string())
            .await
            .map_err(|e| DesktopConfigError::Config(PreferencesError::Invalid(e.to_string())))?;
        let config = self.stored_preferences().await?.subsonic;
        if let ServerStatus::Error { error } = self.subsonic_controller.restart(config).await {
            return Err(DesktopConfigError::Server(error));
        }
        Ok(())
    }

    /// The preferences as stored now, whose server settings a change rolls
    /// back to.
    async fn stored_preferences(&self) -> Result<bae_preferences::Preferences, DesktopConfigError> {
        self.services
            .preferences()
            .await
            .map_err(|error| DesktopConfigError::Config(PreferencesError::Database(error)))
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
async fn apply_service_config<C, F, P>(
    label: &str,
    config: C,
    previous: C,
    apply: impl Fn(C) -> F,
    persist: impl FnOnce(C) -> P,
) -> Result<(), DesktopConfigError>
where
    C: Clone,
    F: std::future::Future<Output = ServerStatus>,
    P: std::future::Future<Output = Result<(), PreferencesError>>,
{
    if let ServerStatus::Error { error } = apply(config.clone()).await {
        apply(previous).await;
        return Err(DesktopConfigError::Server(error));
    }

    match persist(config).await {
        Ok(()) => Ok(()),
        Err(error) => {
            if let ServerStatus::Error { error: rollback_error } = apply(previous).await {
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
#[path = "lib_tests.rs"]
mod tests;
