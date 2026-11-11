use super::*;
use std::sync::{Arc, Mutex};

use crate::{ServedAddress, SubsonicStatus};
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

/// The machine's networks as a test sets them, which it changes as networks
/// come and go.
#[derive(Clone, Default)]
struct FakeNetworks(Arc<Mutex<Vec<NetworkAddress>>>);

impl FakeNetworks {
    fn set(&self, networks: &[(&str, [u8; 4])]) {
        *self.0.lock().unwrap() = networks
            .iter()
            .map(|(interface, octets)| NetworkAddress {
                interface: interface.to_string(),
                address: Ipv4Addr::from(*octets),
            })
            .collect();
    }

    fn lister(&self) -> Arc<NetworkLister> {
        let networks = self.0.clone();
        Arc::new(move || Ok(networks.lock().unwrap().clone()))
    }
}

fn controller_on(
    services: &AppServices,
    password: Option<&str>,
    networks: &FakeNetworks,
) -> SubsonicServerController {
    SubsonicServerController::new(
        services.clone(),
        password_provider(password),
        networks.lister(),
        Diagnostics::noop(),
    )
}

fn controller(services: &AppServices, password: Option<&str>) -> SubsonicServerController {
    controller_on(services, password, &FakeNetworks::default())
}

fn config(port: u16, username: &str, network: SubsonicNetwork) -> SubsonicConfig {
    SubsonicConfig { enabled: true, port: u32::from(port), username: username.to_string(), network }
}

/// An enabled config on this machine with the given port and username.
fn enabled_config(port: u16, username: &str) -> SubsonicConfig {
    config(port, username, SubsonicNetwork::ThisMachine)
}

async fn status(controller: &SubsonicServerController) -> SubsonicStatus {
    controller.view().await.status
}

fn not_running(causes: &[NotRunning]) -> SubsonicStatus {
    SubsonicStatus::NotRunning { causes: causes.to_vec() }
}

fn port_is_free(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// Whether a sign-in with these credentials is accepted by the server on
/// `port`, asked over a real connection.
async fn signs_in(port: u16, username: &str, password: &str) -> bool {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let token = crate::token(password, "abcdef");
    let mut connection = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let request = format!(
        "GET /rest/ping?u={username}&s=abcdef&t={token}&v=1.16.1&c=test&f=json HTTP/1.1\r\n\
         Host: localhost\r\nConnection: close\r\n\r\n"
    );
    connection.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    connection.read_to_string(&mut response).await.unwrap();
    response.contains("\"status\":\"ok\"")
}

#[tokio::test]
async fn a_server_switched_off_is_off() {
    let (services, _temp) = test_manager().await;
    let controller = controller(&services, Some("s3cret"));
    controller.apply_config(SubsonicConfig::disabled_default()).await;
    assert_eq!(status(&controller).await, SubsonicStatus::Off);
}

#[tokio::test]
async fn a_server_with_a_username_and_password_runs_on_this_machine() {
    let (services, _temp) = test_manager().await;
    let controller = controller(&services, Some("s3cret"));
    let port = free_port();
    controller.apply_config(enabled_config(port, "listener")).await;
    assert_eq!(
        status(&controller).await,
        SubsonicStatus::Running {
            addresses: vec![ServedAddress {
                url: format!("http://localhost:{port}"),
                interface: None
            }],
        }
    );
    assert!(signs_in(port, "listener", "s3cret").await);
    controller.shutdown().await;
}

#[tokio::test]
async fn no_username_is_why_it_does_not_run() {
    let (services, _temp) = test_manager().await;
    let controller = controller(&services, Some("s3cret"));
    let port = free_port();
    controller.apply_config(enabled_config(port, "")).await;
    assert_eq!(status(&controller).await, not_running(&[NotRunning::MissingUsername]));
    assert!(port_is_free(port), "the server must not have bound the port");
}

#[tokio::test]
async fn no_password_is_why_it_does_not_run() {
    let (services, _temp) = test_manager().await;
    let controller = controller(&services, None);
    let port = free_port();
    controller.apply_config(enabled_config(port, "listener")).await;
    assert_eq!(status(&controller).await, not_running(&[NotRunning::MissingPassword]));
    assert!(port_is_free(port), "the server must not have bound the port");
}

#[tokio::test]
async fn an_empty_password_is_no_password() {
    let (services, _temp) = test_manager().await;
    let controller = controller(&services, Some(""));
    controller.apply_config(enabled_config(free_port(), "listener")).await;
    let view = controller.view().await;
    assert_eq!(view.status, not_running(&[NotRunning::MissingPassword]));
    assert!(!view.password_saved);
}

#[tokio::test]
async fn both_missing_are_both_reported() {
    let (services, _temp) = test_manager().await;
    let controller = controller(&services, None);
    controller.apply_config(enabled_config(free_port(), "")).await;
    assert_eq!(
        status(&controller).await,
        not_running(&[NotRunning::MissingUsername, NotRunning::MissingPassword])
    );
}

#[tokio::test]
async fn a_port_out_of_range_is_why_it_does_not_run() {
    let (services, _temp) = test_manager().await;
    let controller = controller(&services, Some("s3cret"));
    for port in [0, 65536, 70000] {
        controller.apply_config(SubsonicConfig { port, ..enabled_config(1, "listener") }).await;
        assert_eq!(
            status(&controller).await,
            not_running(&[NotRunning::PortOutOfRange]),
            "port {port}"
        );
    }
}

#[tokio::test]
async fn a_port_another_program_holds_is_why_it_does_not_run() {
    let (services, _temp) = test_manager().await;
    let controller = controller(&services, Some("s3cret"));
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = held.local_addr().unwrap().port();
    controller.apply_config(enabled_config(port, "listener")).await;
    assert_eq!(status(&controller).await, not_running(&[NotRunning::PortTaken]));

    // Once the other program lets go, the next change starts the server.
    drop(held);
    controller.password_changed().await;
    assert!(matches!(status(&controller).await, SubsonicStatus::Running { .. }));
    controller.shutdown().await;
}

#[tokio::test]
async fn a_keyring_that_cannot_be_read_is_a_failure_with_its_detail() {
    let (services, _temp) = test_manager().await;
    let controller = SubsonicServerController::new(
        services.clone(),
        Arc::new(|| Box::pin(async { Err("keyring is locked".to_string()) })),
        FakeNetworks::default().lister(),
        Diagnostics::noop(),
    );
    controller.apply_config(enabled_config(free_port(), "listener")).await;
    assert_eq!(
        status(&controller).await,
        not_running(&[NotRunning::Failed { detail: "keyring is locked".to_string() }])
    );
}

#[tokio::test]
async fn a_network_that_cannot_be_listed_is_a_failure_with_its_detail() {
    let (services, _temp) = test_manager().await;
    let controller = SubsonicServerController::new(
        services.clone(),
        password_provider(Some("s3cret")),
        Arc::new(|| Err("the system refused".to_string())),
        Diagnostics::noop(),
    );
    controller.apply_config(config(free_port(), "listener", SubsonicNetwork::Every)).await;
    assert_eq!(
        status(&controller).await,
        not_running(&[NotRunning::Failed { detail: "the system refused".to_string() }])
    );
}

#[tokio::test]
async fn a_network_with_no_address_is_why_it_does_not_run_and_stays_in_the_menu() {
    let (services, _temp) = test_manager().await;
    let networks = FakeNetworks::default();
    networks.set(&[("en5", [10, 0, 0, 5])]);
    let controller = controller_on(&services, Some("s3cret"), &networks);
    let port = free_port();
    controller
        .apply_config(config(port, "listener", SubsonicNetwork::Interface("en0".to_string())))
        .await;

    let view = controller.view().await;
    assert_eq!(
        view.status,
        not_running(&[NotRunning::NetworkGone { interface: "en0".to_string() }])
    );
    assert_eq!(
        view.networks,
        vec![
            NetworkChoice { interface: "en0".to_string(), address: None },
            NetworkChoice {
                interface: "en5".to_string(),
                address: Some(Ipv4Addr::new(10, 0, 0, 5))
            },
        ]
    );
    assert!(port_is_free(port));
}

/// The network is kept as its interface: the server stops when the interface
/// has no address and starts when it has one again, with no change to the
/// settings. (The interface here is reached at a loopback address, which is
/// all a test may bind.)
#[tokio::test]
async fn the_server_follows_its_network_as_the_interface_loses_and_gains_an_address() {
    let (services, _temp) = test_manager().await;
    let networks = FakeNetworks::default();
    networks.set(&[("en0", [127, 0, 0, 1])]);
    let controller = controller_on(&services, Some("s3cret"), &networks);
    let port = free_port();
    controller
        .apply_config(config(port, "listener", SubsonicNetwork::Interface("en0".to_string())))
        .await;
    assert_eq!(
        status(&controller).await,
        SubsonicStatus::Running {
            addresses: vec![ServedAddress {
                url: format!("http://127.0.0.1:{port}"),
                interface: Some("en0".to_string()),
            }],
        }
    );

    networks.set(&[]);
    controller.networks_changed().await;
    assert_eq!(
        status(&controller).await,
        not_running(&[NotRunning::NetworkGone { interface: "en0".to_string() }])
    );
    assert!(port_is_free(port), "the server must have let go of the port");

    networks.set(&[("en0", [127, 0, 0, 1])]);
    controller.networks_changed().await;
    assert!(matches!(status(&controller).await, SubsonicStatus::Running { .. }));
    assert!(signs_in(port, "listener", "s3cret").await);
    controller.shutdown().await;
}

/// On every network the server listens everywhere and tells each address an
/// app can reach it at, by its network, as networks come and go.
#[tokio::test]
async fn on_every_network_each_address_is_listed_as_networks_come_and_go() {
    let (services, _temp) = test_manager().await;
    let networks = FakeNetworks::default();
    networks.set(&[("en0", [192, 168, 1, 20]), ("utun4", [100, 64, 0, 3])]);
    let controller = controller_on(&services, Some("s3cret"), &networks);
    let port = free_port();
    controller.apply_config(config(port, "listener", SubsonicNetwork::Every)).await;
    let address = |host: &str, interface: &str| ServedAddress {
        url: format!("http://{host}:{port}"),
        interface: Some(interface.to_string()),
    };
    assert_eq!(
        status(&controller).await,
        SubsonicStatus::Running {
            addresses: vec![address("192.168.1.20", "en0"), address("100.64.0.3", "utun4")],
        }
    );

    networks.set(&[("en0", [192, 168, 1, 21])]);
    controller.networks_changed().await;
    assert_eq!(
        status(&controller).await,
        SubsonicStatus::Running { addresses: vec![address("192.168.1.21", "en0")] }
    );
    assert!(signs_in(port, "listener", "s3cret").await, "the server was not restarted away");

    networks.set(&[]);
    controller.networks_changed().await;
    assert_eq!(
        status(&controller).await,
        SubsonicStatus::Running {
            addresses: vec![ServedAddress {
                url: format!("http://localhost:{port}"),
                interface: None
            }],
        },
        "on no network it is still reached at this machine"
    );
    controller.shutdown().await;
}

#[tokio::test]
async fn a_changed_port_moves_the_server_and_frees_the_old_one() {
    let (services, _temp) = test_manager().await;
    let controller = controller(&services, Some("s3cret"));
    let (first, second) = (free_port(), free_port());
    controller.apply_config(enabled_config(first, "listener")).await;
    controller.apply_config(enabled_config(second, "listener")).await;
    assert!(matches!(status(&controller).await, SubsonicStatus::Running { .. }));
    assert!(signs_in(second, "listener", "s3cret").await);
    assert!(port_is_free(first));
    controller.shutdown().await;
}

/// A username or password change applies to the next sign-in, on the server
/// as it runs.
#[tokio::test]
async fn a_changed_username_or_password_applies_to_the_next_sign_in_without_a_restart() {
    let (services, _temp) = test_manager().await;
    let password = Arc::new(Mutex::new(Some("first".to_string())));
    let provider_password = password.clone();
    let controller = SubsonicServerController::new(
        services.clone(),
        Arc::new(move || {
            let password = provider_password.lock().unwrap().clone();
            Box::pin(async move { Ok(password) })
        }),
        FakeNetworks::default().lister(),
        Diagnostics::noop(),
    );
    let port = free_port();
    controller.apply_config(enabled_config(port, "listener")).await;
    let mut watch = controller.watch();
    watch.next().await.unwrap();
    assert!(signs_in(port, "listener", "first").await);

    *password.lock().unwrap() = Some("second".to_string());
    controller.password_changed().await;
    assert!(!signs_in(port, "listener", "first").await);
    assert!(signs_in(port, "listener", "second").await);

    controller.apply_config(enabled_config(port, "renamed")).await;
    assert!(!signs_in(port, "listener", "second").await);
    assert!(signs_in(port, "renamed", "second").await);
    controller.shutdown().await;
}

/// The password arriving is what starts a server that was waiting for it, and
/// a view says a password is saved.
#[tokio::test]
async fn a_saved_password_starts_a_server_that_was_waiting_for_it() {
    let (services, _temp) = test_manager().await;
    let password = Arc::new(Mutex::new(None));
    let provider_password = password.clone();
    let controller = SubsonicServerController::new(
        services.clone(),
        Arc::new(move || {
            let password = provider_password.lock().unwrap().clone();
            Box::pin(async move { Ok(password) })
        }),
        FakeNetworks::default().lister(),
        Diagnostics::noop(),
    );
    controller.apply_config(enabled_config(free_port(), "listener")).await;
    assert!(!controller.view().await.password_saved);

    *password.lock().unwrap() = Some("s3cret".to_string());
    controller.password_changed().await;
    let view = controller.view().await;
    assert!(view.password_saved);
    assert!(matches!(view.status, SubsonicStatus::Running { .. }));
    controller.shutdown().await;
}

#[tokio::test]
async fn switching_off_stops_the_server_and_frees_the_port() {
    let (services, _temp) = test_manager().await;
    let controller = controller(&services, Some("s3cret"));
    let port = free_port();
    controller.apply_config(enabled_config(port, "listener")).await;
    controller.apply_config(SubsonicConfig::disabled_default()).await;
    assert_eq!(status(&controller).await, SubsonicStatus::Off);
    assert!(port_is_free(port));
}

#[tokio::test]
async fn shutdown_frees_the_port() {
    let (services, _temp) = test_manager().await;
    let controller = controller(&services, Some("s3cret"));
    let port = free_port();
    controller.apply_config(enabled_config(port, "listener")).await;
    controller.shutdown().await;
    assert!(port_is_free(port), "shutdown must release the bound port");
}
