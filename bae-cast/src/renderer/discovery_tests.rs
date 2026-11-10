use super::*;
use crate::renderer::RendererConnection;
use bae_playback::renderer::RendererKind;

fn cast_service(instance: &str, id: &str, addr: &str) -> ReportedRenderer {
    ReportedRenderer {
        service_type: RendererServiceType::GoogleCast,
        instance_name: instance.to_string(),
        addr: addr.to_string(),
        port: 8009,
        txt: [("id", id), ("fn", instance), ("MD", "Chromecast")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}

fn airplay_service(instance: &str, device_id: &str) -> ReportedRenderer {
    ReportedRenderer {
        service_type: RendererServiceType::AirPlay,
        instance_name: instance.to_string(),
        addr: "10.0.0.8".to_string(),
        port: 7000,
        txt: [("deviceid", device_id), ("features", "0x0,0x10000")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}

/// A reported service becomes a picker device, and is gone once the host says
/// its browser lost it.
#[test]
fn reported_services_appear_and_disappear() {
    let mut discovery = RendererDiscovery::reported();
    discovery.start(&bae_diagnostics::Diagnostics::noop());

    discovery.report(cast_service("Kitchen", "cast-1", "192.168.1.7"));
    discovery.report(airplay_service("Studio", "AA:BB:CC:DD:EE:FF"));
    let devices = discovery.devices();
    assert_eq!(
        devices.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        vec!["Kitchen", "Studio"],
        "both reported services are in the picker list, sorted by name"
    );
    assert_eq!(devices[0].kind(), RendererKind::Cast);
    assert_eq!(devices[1].kind(), RendererKind::AirPlay);
    assert!(matches!(devices[0].connection, RendererConnection::Cast { port: 8009, .. }));
    assert_eq!(devices[0].model.as_deref(), Some("Chromecast"), "TXT keys match any case");
    assert_eq!(devices[0].receiver().address, "192.168.1.7");
    assert_eq!(devices[1].model, None, "the AirPlay service stated no model");

    discovery.forget(RendererServiceType::GoogleCast, "Kitchen");
    assert_eq!(
        discovery.devices().iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        vec!["Studio"],
        "a lost service leaves the list"
    );
}

/// Reports outside a browse are dropped, and starting a browse clears what
/// the previous one found — the picker never opens on a stale list.
#[test]
fn reports_need_a_live_browse() {
    let mut discovery = RendererDiscovery::reported();

    discovery.report(cast_service("Kitchen", "cast-1", "192.168.1.7"));
    assert!(discovery.devices().is_empty(), "a report outside a browse is dropped");

    discovery.start(&bae_diagnostics::Diagnostics::noop());
    discovery.report(cast_service("Kitchen", "cast-1", "192.168.1.7"));
    assert_eq!(discovery.devices().len(), 1);

    discovery.stop();
    discovery.start(&bae_diagnostics::Diagnostics::noop());
    assert!(discovery.devices().is_empty(), "a fresh browse starts from nothing");
}

/// A service that can't be reached or routed to is dropped rather than
/// listed: an unparseable address, or a Cast service with no device id.
#[test]
fn unusable_services_are_dropped() {
    let mut discovery = RendererDiscovery::reported();
    discovery.start(&bae_diagnostics::Diagnostics::noop());

    discovery.report(cast_service("Bad Address", "cast-2", "not-an-address"));
    let mut no_id = cast_service("No Id", "", "192.168.1.9");
    no_id.txt.remove("id");
    discovery.report(no_id);

    assert!(discovery.devices().is_empty());
}
