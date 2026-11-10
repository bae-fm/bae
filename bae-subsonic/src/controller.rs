//! Wires the Subsonic server onto [`bae_util::server::ServerController`], which
//! runs the shared lifecycle: start, stop, and restart as the configuration
//! changes, and report the current status. What is Subsonic's own is here — the
//! bind/credential preflight, the error vocabulary, and the identity a config
//! change is compared against.
//!
//! The runtime credential the server checks is assembled here from two sources:
//! the `username` carried in [`SubsonicConfig`] and the password read from the
//! keyring through the caller-supplied provider. The password is not in config,
//! so it never appears in a `SubsonicConfig` — a password change is applied by
//! [`SubsonicServerController::restart`], which the host calls after writing the
//! new password to the keyring.

use std::net::SocketAddr;
use std::sync::Arc;

use bae_app::AppServices;
use bae_diagnostics::Diagnostics;
use bae_preferences::{SubsonicConfig, SubsonicCredential};
use bae_util::server::{ServerController, ServerError, ServerStatus};

/// Reads the Subsonic password from the keyring. `Ok(None)` means no password is
/// stored (an unconfigured server); `Err` is a real keyring failure.
/// Async, because a keychain read can wait on the system.
type SubsonicPasswordProvider = dyn Fn() -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<String>, String>> + Send>,
    > + Send
    + Sync;

/// The address and port a running server is bound to, and the username it
/// authenticates against. A config that changes any of these must restart the
/// server, so all three form the identity `apply_config` compares against the
/// running server.
#[derive(Clone, PartialEq)]
struct Binding {
    bind_address: String,
    port: u16,
    username: String,
}

#[derive(Clone)]
pub struct SubsonicServerController {
    services: AppServices,
    password_provider: Arc<SubsonicPasswordProvider>,
    server: ServerController<Binding>,
    /// Where a panic in the server, a request or a stream is reported.
    diagnostics: Diagnostics,
}

impl SubsonicServerController {
    pub fn new(
        services: AppServices,
        password_provider: Arc<SubsonicPasswordProvider>,
        diagnostics: Diagnostics,
    ) -> Self {
        Self {
            services,
            password_provider,
            server: ServerController::new("Subsonic", diagnostics.bug_sink()),
            diagnostics,
        }
    }

    /// Bring the running server in line with `config`. Disabled config stops the
    /// server; an enabled config (re)starts it whenever the bind address, port,
    /// or username differs from what is already running. A password change is not
    /// visible here — the password is keyring-only — so the host drives it
    /// through [`Self::restart`].
    pub async fn apply_config(&self, config: SubsonicConfig) -> ServerStatus {
        if !config.enabled {
            return self.server.disable().await;
        }
        if let Err(error) = config.validate() {
            return self
                .server
                .record_error(ServerError::InvalidConfig { detail: error.to_string() })
                .await;
        }

        let binding = Binding {
            bind_address: config.bind_address,
            port: config.port,
            username: config.username,
        };
        self.server.apply(binding, |binding| self.start(binding)).await
    }

    /// Stop and re-apply `config`, so a running server picks up a credential
    /// change (a new keyring password) that `apply_config` alone would treat as
    /// no change and skip. A no-op restart of a disabled config just reports
    /// `Disabled`.
    pub async fn restart(&self, config: SubsonicConfig) -> ServerStatus {
        self.server.shutdown().await;
        self.apply_config(config).await
    }

    pub async fn status(&self) -> ServerStatus {
        self.server.status().await
    }

    pub async fn shutdown(&self) {
        self.server.shutdown().await;
    }

    async fn start(&self, binding: Binding) -> ServerStatus {
        let password = match self.password_provider.as_ref()().await {
            Ok(password) => password.unwrap_or_default(),
            Err(detail) => {
                return self
                    .server
                    .record_error(ServerError::CredentialUnavailable { detail })
                    .await;
            }
        };
        // The config layer refuses a misconfigured start: an enabled server with
        // no username (already rejected by `validate`) or no stored password can
        // authenticate no client, so it is an error rather than a server bound to
        // reject every request. The request-time guard in `auth.rs` is a separate
        // layer that keeps `serve` correct in isolation.
        if binding.username.is_empty() || password.is_empty() {
            return self
                .server
                .record_error(ServerError::InvalidConfig {
                    detail: "Subsonic server enabled but no username/password is configured"
                        .to_string(),
                })
                .await;
        }
        let credential = SubsonicCredential { username: binding.username.clone(), password };

        // `validate` already guaranteed the address parses; surface a parse
        // failure as a config error rather than unwrapping.
        let ip = match binding.bind_address.parse::<std::net::IpAddr>() {
            Ok(ip) => ip,
            Err(e) => {
                return self
                    .server
                    .record_error(ServerError::InvalidConfig {
                        detail: format!("invalid bind address {:?}: {e}", binding.bind_address),
                    })
                    .await;
            }
        };
        let addr = SocketAddr::new(ip, binding.port);
        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => listener,
            Err(e) => {
                return self
                    .server
                    .record_error(ServerError::BindFailed {
                        detail: format!(
                            "failed to bind {}:{}: {e}",
                            binding.bind_address, binding.port
                        ),
                    })
                    .await;
            }
        };

        let url = format!("http://{}:{}/rest", binding.bind_address, binding.port);
        let router = crate::router(self.services.clone(), credential, self.diagnostics.clone());
        self.server
            .start(binding, url, |cancellation| {
                let shutdown = cancellation.clone();
                axum::serve(listener, router)
                    .with_graceful_shutdown(async move { shutdown.cancelled_owned().await })
            })
            .await
    }
}

#[cfg(test)]
#[path = "controller_tests.rs"]
mod tests;
