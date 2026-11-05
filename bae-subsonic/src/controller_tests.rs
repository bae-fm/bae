use super::*;
use std::sync::Arc;

use bae_app::AppServices;
use tempfile::TempDir;

async fn test_manager() -> (AppServices, TempDir) {
    let (library, temp) = bae_library::test_libraries::setup_test_library().await;
    let services = bae_app::test_services::start_services(&library)
        .await
        .expect("the test library's services start");
    (services, temp)
}

/// A free port on loopback: bind an ephemeral listener, read the port, and
/// drop it. The controller then binds the same number. A different process
/// could grab it in the gap, but for a single-threaded test loopback this is
/// stable enough and keeps the test off a fixed, possibly-busy port.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn password_provider(password: Option<&str>) -> Arc<SubsonicPasswordProvider> {
    let password = password.map(str::to_string);
    Arc::new(move || {
        let password = password.clone();
        Box::pin(async move { Ok(password) })
    })
}

fn controller(services: &AppServices, password: Option<&str>) -> SubsonicServerController {
    SubsonicServerController::new(
        services.clone(),
        password_provider(password),
        Diagnostics::noop(),
    )
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
    let status = controller.apply_config(SubsonicConfig::disabled_default()).await;
    assert!(matches!(status, ServerStatus::Disabled));
}

#[tokio::test]
async fn enabled_with_valid_credential_runs() {
    let (services, _temp) = test_manager().await;
    let controller = controller(&services, Some("s3cret"));
    let config = enabled_config(free_port(), "listener");
    let status = controller.apply_config(config).await;
    assert!(
        matches!(status, ServerStatus::Running { .. }),
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
        matches!(status, ServerStatus::Error { error: ServerError::InvalidConfig { .. } }),
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
        Arc::new(|| Box::pin(async { Err("keyring is locked".to_string()) })),
        Diagnostics::noop(),
    );
    let config = enabled_config(free_port(), "listener");
    let status = controller.apply_config(config).await;
    assert!(
        matches!(status, ServerStatus::Error { error: ServerError::CredentialUnavailable { .. } }),
        "a keyring read error is CredentialUnavailable, got {status:?}"
    );
}

#[tokio::test]
async fn transitions_disabled_enabled_disabled() {
    let (services, _temp) = test_manager().await;
    let controller = controller(&services, Some("s3cret"));
    let enabled = enabled_config(free_port(), "listener");

    assert!(matches!(
        controller.apply_config(SubsonicConfig::disabled_default()).await,
        ServerStatus::Disabled
    ));
    assert!(matches!(controller.apply_config(enabled).await, ServerStatus::Running { .. }));
    assert!(matches!(
        controller.apply_config(SubsonicConfig::disabled_default()).await,
        ServerStatus::Disabled
    ));
    assert!(matches!(controller.status().await, ServerStatus::Disabled));
}

#[tokio::test]
async fn bind_then_shutdown_frees_the_port() {
    let (services, _temp) = test_manager().await;
    let controller = controller(&services, Some("s3cret"));
    let port = free_port();
    let config = enabled_config(port, "listener");
    assert!(matches!(controller.apply_config(config).await, ServerStatus::Running { .. }));
    controller.shutdown().await;
    assert!(matches!(controller.status().await, ServerStatus::Disabled));
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
        matches!(&status, ServerStatus::Running { url } if url.contains("127.0.0.1")),
        "loopback config runs on 127.0.0.1, got {status:?}"
    );

    let lan =
        SubsonicConfig { bind_address: "0.0.0.0".to_string(), ..enabled_config(port, "listener") };
    let status = controller.apply_config(lan).await;
    assert!(
        matches!(&status, ServerStatus::Running { url } if url.contains("0.0.0.0")),
        "a bind-address change must restart on the new address, got {status:?}"
    );

    controller.shutdown().await;
    assert!(
        std::net::TcpListener::bind(("127.0.0.1", port)).is_ok(),
        "shutdown must release the bound port"
    );
}
