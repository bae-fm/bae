//! Owns the Subsonic server's lifecycle, mirroring `bae-mcp`'s
//! `McpServerController`: it starts, stops, and restarts the server as the
//! configuration changes, and reports the current status.
//!
//! The runtime credential the server checks is assembled here from two sources:
//! the `username` carried in [`SubsonicConfig`] and the password read from the
//! keyring through the caller-supplied provider. The password is not in config,
//! so it never appears in a `SubsonicConfig` — a password change is applied by
//! [`SubsonicServerController::restart`], which the host calls after writing the
//! new password to the keyring.

use std::net::SocketAddr;
use std::sync::Arc;

use bae_core::config::{SubsonicConfig, SubsonicCredential};
use bae_core::library::AppServices;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::warn;

/// Reads the Subsonic password from the keyring. `Ok(None)` means no password is
/// stored (an unconfigured server); `Err` is a real keyring failure.
type SubsonicPasswordProvider = dyn Fn() -> Result<Option<String>, String> + Send + Sync;

#[derive(Debug, Clone)]
pub enum SubsonicServerStatus {
    Disabled,
    Running { url: String },
    Error { error: SubsonicServerError },
}

#[derive(Debug, Clone)]
pub enum SubsonicServerError {
    InvalidConfig { detail: String },
    CredentialUnavailable { detail: String },
    BindFailed { detail: String },
    ServerFailed { detail: String },
}

impl SubsonicServerError {
    pub fn detail(&self) -> &str {
        match self {
            Self::InvalidConfig { detail }
            | Self::CredentialUnavailable { detail }
            | Self::BindFailed { detail }
            | Self::ServerFailed { detail } => detail,
        }
    }
}

#[derive(Clone)]
pub struct SubsonicServerController {
    services: AppServices,
    password_provider: Arc<SubsonicPasswordProvider>,
    inner: Arc<Mutex<SubsonicServerControllerState>>,
}

enum SubsonicServerControllerState {
    Disabled,
    Running {
        /// The address and port the running server is bound to, and the username
        /// it authenticates against. A config that changes any of these must
        /// restart the server, so all three form the identity `apply_config`
        /// compares against the running server.
        bind_address: String,
        port: u16,
        username: String,
        url: String,
        cancellation: CancellationToken,
        task: JoinHandle<()>,
    },
    Error {
        error: SubsonicServerError,
    },
}

impl SubsonicServerControllerState {
    fn status(&self) -> SubsonicServerStatus {
        match self {
            Self::Disabled => SubsonicServerStatus::Disabled,
            Self::Running { url, .. } => SubsonicServerStatus::Running { url: url.clone() },
            Self::Error { error } => SubsonicServerStatus::Error {
                error: error.clone(),
            },
        }
    }
}

impl SubsonicServerController {
    pub fn new(services: AppServices, password_provider: Arc<SubsonicPasswordProvider>) -> Self {
        Self {
            services,
            password_provider,
            inner: Arc::new(Mutex::new(SubsonicServerControllerState::Disabled)),
        }
    }

    /// Bring the running server in line with `config`. Disabled config stops the
    /// server; an enabled config (re)starts it whenever the bind address, port,
    /// or username differs from what is already running. A password change is not
    /// visible here — the password is keyring-only — so the host drives it
    /// through [`Self::restart`].
    pub async fn apply_config(&self, config: SubsonicConfig) -> SubsonicServerStatus {
        if !config.enabled {
            self.shutdown().await;
            return SubsonicServerStatus::Disabled;
        }
        if let Err(error) = config.validate() {
            return self
                .record_error(SubsonicServerError::InvalidConfig {
                    detail: error.to_string(),
                })
                .await;
        }

        {
            let state = self.inner.lock().await;
            if let SubsonicServerControllerState::Running {
                bind_address,
                port,
                username,
                ..
            } = &*state
            {
                if *bind_address == config.bind_address
                    && *port == config.port
                    && *username == config.username
                {
                    return state.status();
                }
            }
        }

        self.shutdown().await;
        self.start(config.bind_address, config.port, config.username)
            .await
    }

    /// Stop and re-apply `config`, so a running server picks up a credential
    /// change (a new keyring password) that `apply_config` alone would treat as
    /// no change and skip. A no-op restart of a disabled config just reports
    /// `Disabled`.
    pub async fn restart(&self, config: SubsonicConfig) -> SubsonicServerStatus {
        self.shutdown().await;
        self.apply_config(config).await
    }

    pub async fn status(&self) -> SubsonicServerStatus {
        self.inner.lock().await.status()
    }

    pub async fn shutdown(&self) {
        let task = {
            let mut state = self.inner.lock().await;
            match std::mem::replace(&mut *state, SubsonicServerControllerState::Disabled) {
                SubsonicServerControllerState::Running {
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
                warn!("Subsonic server task join failed: {error}");
            }
        }
    }

    async fn start(
        &self,
        bind_address: String,
        port: u16,
        username: String,
    ) -> SubsonicServerStatus {
        let password = match self.password_provider.as_ref()() {
            Ok(password) => password.unwrap_or_default(),
            Err(detail) => {
                return self
                    .record_error(SubsonicServerError::CredentialUnavailable { detail })
                    .await;
            }
        };
        // The config layer refuses a misconfigured start: an enabled server with
        // no username (already rejected by `validate`) or no stored password can
        // authenticate no client, so it is an error rather than a server bound to
        // reject every request. The request-time guard in `auth.rs` is a separate
        // layer that keeps `serve` correct in isolation.
        if username.is_empty() || password.is_empty() {
            return self
                .record_error(SubsonicServerError::InvalidConfig {
                    detail: "Subsonic server enabled but no username/password is configured"
                        .to_string(),
                })
                .await;
        }
        let credential = SubsonicCredential {
            username: username.clone(),
            password,
        };

        // `validate` already guaranteed the address parses; surface a parse
        // failure as a config error rather than unwrapping.
        let ip = match bind_address.parse::<std::net::IpAddr>() {
            Ok(ip) => ip,
            Err(e) => {
                return self
                    .record_error(SubsonicServerError::InvalidConfig {
                        detail: format!("invalid bind address {bind_address:?}: {e}"),
                    })
                    .await;
            }
        };
        let addr = SocketAddr::new(ip, port);
        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => listener,
            Err(e) => {
                return self
                    .record_error(SubsonicServerError::BindFailed {
                        detail: format!("failed to bind {bind_address}:{port}: {e}"),
                    })
                    .await;
            }
        };

        let cancellation = CancellationToken::new();
        let server_cancellation = cancellation.clone();
        let task_state = self.inner.clone();
        let router = crate::router(self.services.clone(), credential);
        let task = tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    server_cancellation.cancelled_owned().await;
                })
                .await
            {
                let error = SubsonicServerError::ServerFailed {
                    detail: format!("Subsonic server stopped with error: {e}"),
                };
                warn!("{}", error.detail());
                let mut state = task_state.lock().await;
                if let SubsonicServerControllerState::Running {
                    port: running_port, ..
                } = &*state
                {
                    if *running_port == port {
                        *state = SubsonicServerControllerState::Error { error };
                    }
                }
            }
        });

        let url = format!("http://{bind_address}:{port}/rest");
        let status = SubsonicServerStatus::Running { url: url.clone() };
        let mut state = self.inner.lock().await;
        *state = SubsonicServerControllerState::Running {
            bind_address,
            port,
            username,
            url,
            cancellation,
            task,
        };
        status
    }

    async fn record_error(&self, error: SubsonicServerError) -> SubsonicServerStatus {
        self.shutdown().await;
        let status = SubsonicServerStatus::Error {
            error: error.clone(),
        };
        let mut state = self.inner.lock().await;
        *state = SubsonicServerControllerState::Error { error };
        status
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use bae_core::db::Database;
    use bae_core::library::{AppServices, LibraryManager};
    use bae_test_support as support;
    use coven::StoreDir;
    use tempfile::TempDir;

    async fn test_manager() -> (AppServices, TempDir) {
        support::tracing_init();
        let temp = TempDir::new().unwrap();
        let db_dir = temp.path().join("db");
        std::fs::create_dir_all(&db_dir).unwrap();
        let database = Database::new_test(
            db_dir.join("test.db").to_str().unwrap(),
            Arc::new(coven::SystemClock),
            Arc::new(coven::UuidProvider),
        )
        .await
        .expect("database");
        let library_dir = StoreDir::new(db_dir);
        let config_handle = support::test_config(&library_dir);
        let manager = LibraryManager::new(
            database,
            config_handle,
            Arc::new(coven::SystemClock),
            Arc::new(coven::UuidProvider),
            bae_core::diagnostics::Diagnostics::noop(),
            tokio::runtime::Handle::current(),
            bae_core::import::cover_art::RemoteImageCache::for_test(),
        );
        let services = AppServices::for_test(manager).await.expect("app services");
        (services, temp)
    }

    /// A free port on loopback: bind an ephemeral listener, read the port, and
    /// drop it. The controller then binds the same number. A different process
    /// could grab it in the gap, but for a single-threaded test loopback this is
    /// stable enough and keeps the test off a fixed, possibly-busy port.
    fn free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    fn password_provider(password: Option<&str>) -> Arc<SubsonicPasswordProvider> {
        let password = password.map(str::to_string);
        Arc::new(move || Ok(password.clone()))
    }

    fn controller(services: &AppServices, password: Option<&str>) -> SubsonicServerController {
        SubsonicServerController::new(services.clone(), password_provider(password))
    }

    /// An enabled config on loopback with the given port and username.
    fn enabled_config(port: u16, username: &str) -> SubsonicConfig {
        SubsonicConfig {
            enabled: true,
            port,
            username: username.to_string(),
            bind_address: "127.0.0.1".to_string(),
        }
    }

    #[tokio::test]
    async fn disabled_config_reports_disabled() {
        let (services, _temp) = test_manager().await;
        let controller = controller(&services, Some("s3cret"));
        let status = controller
            .apply_config(SubsonicConfig::disabled_default())
            .await;
        assert!(matches!(status, SubsonicServerStatus::Disabled));
    }

    #[tokio::test]
    async fn enabled_with_valid_credential_runs() {
        let (services, _temp) = test_manager().await;
        let controller = controller(&services, Some("s3cret"));
        let config = enabled_config(free_port(), "listener");
        let status = controller.apply_config(config).await;
        assert!(
            matches!(status, SubsonicServerStatus::Running { .. }),
            "a valid enabled config must start the server, got {status:?}"
        );
        controller.shutdown().await;
    }

    #[tokio::test]
    async fn enabled_without_stored_password_is_invalid_and_does_not_bind() {
        let (services, _temp) = test_manager().await;
        let controller = controller(&services, None);
        let port = free_port();
        let config = enabled_config(port, "listener");
        let status = controller.apply_config(config).await;
        assert!(
            matches!(
                status,
                SubsonicServerStatus::Error {
                    error: SubsonicServerError::InvalidConfig { .. }
                }
            ),
            "a missing password is a config error, got {status:?}"
        );
        // The port was never bound, so it is still free to take.
        assert!(
            std::net::TcpListener::bind(("127.0.0.1", port)).is_ok(),
            "the server must not have bound the port"
        );
    }

    #[tokio::test]
    async fn keyring_read_failure_is_credential_unavailable() {
        let (services, _temp) = test_manager().await;
        let controller = SubsonicServerController::new(
            services.clone(),
            Arc::new(|| Err("keyring is locked".to_string())),
        );
        let config = enabled_config(free_port(), "listener");
        let status = controller.apply_config(config).await;
        assert!(
            matches!(
                status,
                SubsonicServerStatus::Error {
                    error: SubsonicServerError::CredentialUnavailable { .. }
                }
            ),
            "a keyring read error is CredentialUnavailable, got {status:?}"
        );
    }

    #[tokio::test]
    async fn transitions_disabled_enabled_disabled() {
        let (services, _temp) = test_manager().await;
        let controller = controller(&services, Some("s3cret"));
        let enabled = enabled_config(free_port(), "listener");

        assert!(matches!(
            controller
                .apply_config(SubsonicConfig::disabled_default())
                .await,
            SubsonicServerStatus::Disabled
        ));
        assert!(matches!(
            controller.apply_config(enabled).await,
            SubsonicServerStatus::Running { .. }
        ));
        assert!(matches!(
            controller
                .apply_config(SubsonicConfig::disabled_default())
                .await,
            SubsonicServerStatus::Disabled
        ));
        assert!(matches!(
            controller.status().await,
            SubsonicServerStatus::Disabled
        ));
    }

    #[tokio::test]
    async fn bind_then_shutdown_frees_the_port() {
        let (services, _temp) = test_manager().await;
        let controller = controller(&services, Some("s3cret"));
        let port = free_port();
        let config = enabled_config(port, "listener");
        assert!(matches!(
            controller.apply_config(config).await,
            SubsonicServerStatus::Running { .. }
        ));
        controller.shutdown().await;
        assert!(matches!(
            controller.status().await,
            SubsonicServerStatus::Disabled
        ));
        // Once shut down, the port is released and can be bound again.
        assert!(
            std::net::TcpListener::bind(("127.0.0.1", port)).is_ok(),
            "shutdown must release the bound port"
        );
    }

    /// Changing only the bind address (same port, same username) restarts the
    /// server: the URL reflects the new address, and the old loopback binding is
    /// released. `apply_config` must not treat a bind-address change as no-op.
    #[tokio::test]
    async fn changing_bind_address_restarts_the_server() {
        let (services, _temp) = test_manager().await;
        let controller = controller(&services, Some("s3cret"));
        let port = free_port();

        let loopback = enabled_config(port, "listener");
        let status = controller.apply_config(loopback).await;
        assert!(
            matches!(&status, SubsonicServerStatus::Running { url } if url.contains("127.0.0.1")),
            "loopback config runs on 127.0.0.1, got {status:?}"
        );

        let lan = SubsonicConfig {
            bind_address: "0.0.0.0".to_string(),
            ..enabled_config(port, "listener")
        };
        let status = controller.apply_config(lan).await;
        assert!(
            matches!(&status, SubsonicServerStatus::Running { url } if url.contains("0.0.0.0")),
            "a bind-address change must restart on the new address, got {status:?}"
        );

        controller.shutdown().await;
        assert!(
            std::net::TcpListener::bind(("127.0.0.1", port)).is_ok(),
            "shutdown must release the bound port"
        );
    }
}
