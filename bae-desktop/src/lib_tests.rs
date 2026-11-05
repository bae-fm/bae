use super::*;

use bae_preferences::SubsonicConfig;

#[test]
fn desktop_initialization_does_not_reenter_the_owned_runtime() {
    let (runtime, services, _tmp) = bae_app::test_services::runtime_with_services();

    runtime.block_on(async {
        let desktop = DesktopServices::start(
            services,
            tokio::runtime::Handle::current(),
            Diagnostics::noop(),
        )
        .await;
        desktop.shutdown().await;
    });
}

#[test]
fn desktop_controller_calls_do_not_reenter_the_owned_runtime() {
    let (runtime, services, _tmp) = bae_app::test_services::runtime_with_services();
    let desktop = runtime.block_on(DesktopServices::start(
        services,
        runtime.handle().clone(),
        Diagnostics::noop(),
    ));

    runtime.block_on(async {
        assert!(matches!(desktop.mcp_server_status().await, ServerStatus::Disabled));
        assert!(matches!(desktop.subsonic_server_status().await, ServerStatus::Disabled));
        desktop.shutdown().await;
    });
}

/// When the runtime apply fails (here: the configured port is already bound,
/// so the bind fails), `set_subsonic_config` must surface the error and leave
/// the persisted config untouched — no half-applied enable.
#[test]
fn set_subsonic_config_rolls_back_persisted_config_on_runtime_error() {
    let (runtime, services, _tmp) = bae_app::test_services::runtime_with_services();
    runtime
        .block_on(services.set_subsonic_password("s3cret".to_string()))
        .expect("seed keyring password");

    let password_services = services.clone();
    let controller = SubsonicServerController::new(
        services.clone(),
        Arc::new(move || {
            let password_services = password_services.clone();
            Box::pin(async move {
                password_services.get_subsonic_password().await.map_err(|e| e.to_string())
            })
        }),
        Diagnostics::noop(),
    );

    // Occupy the port so the runtime bind fails.
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = occupied.local_addr().unwrap().port();

    let before = runtime.block_on(services.preferences()).unwrap().subsonic;
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
        runtime.block_on(services.preferences()).unwrap().subsonic,
        before,
        "a runtime apply failure must not persist the new config"
    );
}
