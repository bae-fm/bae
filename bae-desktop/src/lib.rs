#![deny(unreachable_pub, dead_code)]

mod cast;

use std::sync::Arc;

use bae_core::app::{bootstrap as bootstrap_core, BootstrapError, RunningApp};
use bae_core::config::{ConfigError, McpConfig, SubsonicConfig};
use bae_core::diagnostics::Diagnostics;
use bae_core::library::{AppServices, LibraryManager};
use bae_core::renderer::RendererDevice;
use bae_core::ui::UiEventBus;
use bae_mcp::{Automation, McpServerController};
pub use bae_mcp::{McpServerError, McpServerStatus};
use bae_subsonic::SubsonicServerController;
pub use bae_subsonic::{SubsonicServerError, SubsonicServerStatus};
use tokio::runtime::Runtime;

pub use cast::{CastController, CastError, CastStatus};

/// Field order is drop order: the tokio runtime is declared **last** so
/// everything that runs on it — `AppServices`' background tasks above all — is
/// torn down while the runtime is still alive to run their shutdown. Declared
/// first, as it was, the runtime is destroyed before `AppServicesInner::drop`
/// gets to stop anything, and every task it would have cancelled is already
/// gone.
pub struct DesktopApp {
    pub services: AppServices,
    pub ui_event_bus: UiEventBus,
    mcp_controller: McpServerController,
    subsonic_controller: SubsonicServerController,
    cast_controller: CastController,
    pub runtime: Runtime,
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

pub fn bootstrap(
    library_id: String,
    position_update_interval_ms: u32,
    restore_playback: bool,
    diagnostics: Diagnostics,
    cloudkit_ops: Option<bae_core::CloudKitOpsRef>,
) -> Result<DesktopApp, BootstrapError> {
    bootstrap_core(
        library_id,
        position_update_interval_ms,
        restore_playback,
        diagnostics,
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

        // The Subsonic server's runtime credential is the config username plus
        // the keyring password; the provider supplies the password, and the
        // username rides on each applied `SubsonicConfig`.
        let password_manager = services.library_manager().clone();
        let subsonic_controller = SubsonicServerController::new(
            services.library_manager().clone(),
            Arc::new(move || {
                password_manager
                    .get_subsonic_password()
                    .map_err(|e| e.to_string())
            }),
        );
        let initial_subsonic = services.library_manager().get_config().subsonic;
        runtime.block_on(subsonic_controller.apply_config(initial_subsonic));

        let cast_controller =
            CastController::new(&services, ui_event_bus.clone(), runtime.handle().clone());

        let config_controller = controller.clone();
        let config_subsonic_controller = subsonic_controller.clone();
        let mut config_rx = services.library_manager().subscribe_config_changes();
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
            runtime,
            services,
            ui_event_bus,
            mcp_controller: controller,
            subsonic_controller,
            cast_controller,
        }
    }

    /// The current merged list of discovered remote-renderer devices (Cast and
    /// UPnP). Requery on a `CastDevices` invalidation.
    pub fn cast_devices(&self) -> Vec<RendererDevice> {
        self.cast_controller.devices()
    }

    /// Start browsing for Cast devices (the device picker opened).
    pub fn start_cast_discovery(&self) {
        self.cast_controller.start_discovery();
    }

    /// Stop browsing for Cast devices (the device picker closed).
    pub fn stop_cast_discovery(&self) {
        self.cast_controller.stop_discovery();
    }

    /// Cast playback to the device with `device_id`.
    pub fn cast_to(&self, device_id: &str) -> Result<(), CastError> {
        self.cast_controller.cast_to(device_id)
    }

    /// Stop casting and return playback to local output.
    pub fn stop_casting(&self) {
        self.cast_controller.stop_casting();
    }

    /// The current cast status (whether casting, and to which device).
    pub fn cast_status(&self) -> CastStatus {
        self.cast_controller.status()
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

    pub fn subsonic_server_status(&self) -> SubsonicServerStatus {
        self.runtime.block_on(self.subsonic_controller.status())
    }

    /// Apply a new Subsonic server config: validate, apply to the running server
    /// with rollback if it errors, then persist. Mirrors [`Self::set_mcp_config`].
    pub fn set_subsonic_config(
        &self,
        config: SubsonicConfig,
    ) -> Result<(), DesktopSubsonicConfigError> {
        apply_subsonic_config(
            &self.runtime,
            &self.subsonic_controller,
            self.services.library_manager(),
            config,
        )
    }

    /// Store a new Subsonic server password in the keyring, then restart the
    /// running server so it authenticates against the new password. The password
    /// is not in config, so `apply_config` cannot see the change — the restart
    /// is how a running server picks it up.
    pub fn set_subsonic_password(&self, password: &str) -> Result<(), DesktopSubsonicConfigError> {
        self.services
            .library_manager()
            .set_subsonic_password(password.to_string())
            .map_err(|e| DesktopSubsonicConfigError::Config(ConfigError::Config(e.to_string())))?;
        let config = self.services.library_manager().get_config().subsonic;
        if let SubsonicServerStatus::Error { error } = self
            .runtime
            .block_on(self.subsonic_controller.restart(config))
        {
            return Err(DesktopSubsonicConfigError::Server(error));
        }
        Ok(())
    }

    pub fn shutdown_subsonic(&self) {
        self.runtime.block_on(self.subsonic_controller.shutdown());
    }
}

/// Validate, runtime-apply (with rollback on error), then persist a Subsonic
/// config. Extracted from the [`DesktopApp`] method so the rollback contract is
/// testable against a bare controller + manager, without a full app bootstrap.
fn apply_subsonic_config(
    runtime: &Runtime,
    controller: &SubsonicServerController,
    manager: &LibraryManager,
    config: SubsonicConfig,
) -> Result<(), DesktopSubsonicConfigError> {
    config
        .validate()
        .map_err(DesktopSubsonicConfigError::Config)?;
    let previous = manager.get_config().subsonic;
    let status = runtime.block_on(controller.apply_config(config.clone()));
    if let SubsonicServerStatus::Error { error } = status {
        runtime.block_on(controller.apply_config(previous));
        return Err(DesktopSubsonicConfigError::Server(error));
    }

    match manager.set_subsonic_config(config) {
        Ok(()) => Ok(()),
        Err(error) => {
            if let SubsonicServerStatus::Error {
                error: rollback_error,
            } = runtime.block_on(controller.apply_config(previous))
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
        manager
            .set_subsonic_password("s3cret".to_string())
            .expect("seed keyring password");

        let provider_manager = manager.clone();
        let controller = SubsonicServerController::new(
            manager.clone(),
            Arc::new(move || {
                provider_manager
                    .get_subsonic_password()
                    .map_err(|e| e.to_string())
            }),
        );

        // Occupy the port so the runtime bind fails.
        let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = occupied.local_addr().unwrap().port();

        let before = manager.get_config().subsonic;
        let result = apply_subsonic_config(
            &runtime,
            &controller,
            &manager,
            SubsonicConfig {
                enabled: true,
                port,
                username: "listener".to_string(),
                bind_address: "127.0.0.1".to_string(),
            },
        );

        assert!(
            matches!(result, Err(DesktopSubsonicConfigError::Server(_))),
            "a failed bind must surface as a server error, got {result:?}"
        );
        assert_eq!(
            manager.get_config().subsonic,
            before,
            "a runtime apply failure must not persist the new config"
        );
    }
}
