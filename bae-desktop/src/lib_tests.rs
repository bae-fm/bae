use super::*;

use bae_subsonic::{NotRunning, SubsonicStatus};

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
        assert_eq!(desktop.subscribe_subsonic().next().await.unwrap().status, SubsonicStatus::Off);
        desktop.shutdown().await;
    });
}

/// The Subsonic switch is what the person wants: it stays on while the server
/// can't run, the view says why, and the server starts once nothing stands in
/// the way, with the switch never having moved.
#[test]
fn the_subsonic_switch_stays_on_while_the_server_cannot_run() {
    let (runtime, services, _tmp) = bae_app::test_services::runtime_with_services();
    runtime.block_on(async {
        let desktop = DesktopServices::start(
            services.clone(),
            tokio::runtime::Handle::current(),
            Diagnostics::noop(),
        )
        .await;
        let subsonic = desktop.subscribe_subsonic();
        let next = || async { subsonic.next().await.expect("the view arrives") };
        assert_eq!(next().await.status, SubsonicStatus::Off);

        desktop.set_subsonic_enabled(true).await.unwrap();
        assert_eq!(
            next().await.status,
            SubsonicStatus::NotRunning {
                causes: vec![NotRunning::MissingUsername, NotRunning::MissingPassword]
            }
        );
        assert!(services.preferences().await.unwrap().subsonic.enabled);

        let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = occupied.local_addr().unwrap().port();
        desktop.set_subsonic_port(u32::from(port)).await.unwrap();
        desktop.set_subsonic_username("listener".to_string()).await.unwrap();
        desktop.set_subsonic_password("s3cret").await.unwrap();
        let view = loop {
            let view = next().await;
            if view.status == (SubsonicStatus::NotRunning { causes: vec![NotRunning::PortTaken] }) {
                break view;
            }
        };
        assert!(view.password_saved);
        assert!(services.preferences().await.unwrap().subsonic.enabled);

        drop(occupied);
        desktop.set_subsonic_port(u32::from(port) + 1).await.unwrap();
        desktop.set_subsonic_port(u32::from(port)).await.unwrap();
        loop {
            if matches!(next().await.status, SubsonicStatus::Running { .. }) {
                break;
            }
        }
        desktop.shutdown().await;
    });
}
