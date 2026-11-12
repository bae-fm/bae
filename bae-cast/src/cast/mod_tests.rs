//! Cast discovery: mDNS record → [`RendererDevice`] mapping. The session and
//! channel logic is tested generically in `crate::renderer`.

use std::net::{IpAddr, Ipv6Addr};

use super::channel::receiver_time;
use super::discovery::{instance_name, map_device};
use crate::renderer::{RendererConnection, RendererDevice};
use crate::test_support::v4;
use bae_playback::renderer::RendererKind;

/// The Cast connection's address and port, for asserting on a mapped device.
fn cast_addr(device: &RendererDevice) -> (IpAddr, u16) {
    match device.connection {
        RendererConnection::Cast { addr, port } => (addr, port),
        _ => panic!("cast discovery must map to a Cast connection"),
    }
}

#[test]
fn resolved_service_maps_to_device() {
    let device = map_device(
        "Living Room",
        8009,
        [v4(192, 168, 1, 40)].into_iter(),
        Some("abcd1234"),
        Some("Living Room Speaker"),
        Some("Chromecast Audio"),
    )
    .expect("a resolved cast service maps to a device");

    assert_eq!(device.id, "abcd1234");
    assert_eq!(device.name, "Living Room Speaker");
    assert_eq!(device.kind(), RendererKind::Cast);
    assert_eq!(cast_addr(&device), (v4(192, 168, 1, 40), 8009));
    assert_eq!(device.model.as_deref(), Some("Chromecast Audio"), "the TXT `md` record");
    assert_eq!(device.receiver().address, "192.168.1.40", "where discovery found it");
}

#[test]
fn cast_device_that_states_no_model_has_none() {
    for md in [None, Some(""), Some("  ")] {
        let device = map_device(
            "Den",
            8009,
            [v4(192, 168, 1, 44)].into_iter(),
            Some("den-id"),
            Some("Den"),
            md,
        )
        .expect("maps");
        assert_eq!(device.model, None, "`md` {md:?} states no model");
    }
}

#[test]
fn resolved_service_without_id_is_unusable() {
    assert!(
        map_device("No Id", 8009, [v4(192, 168, 1, 41)].into_iter(), None, Some("No Id"), None)
            .is_none(),
        "a service with no device id can't be routed to, so it is dropped"
    );
}

#[test]
fn service_fullname_reduces_to_its_instance_label() {
    assert_eq!(
        instance_name("Living Room._googlecast._tcp.local."),
        "Living Room",
        "the instance label is what a host browser reports and what names a device with no `fn`"
    );
}

#[test]
fn resolved_service_falls_back_to_instance_name_without_fn() {
    let device = map_device(
        "Kitchen",
        8009,
        [v4(192, 168, 1, 42)].into_iter(),
        Some("kitchen-id"),
        None,
        None,
    )
    .expect("maps with a fallback name");
    assert_eq!(device.name, "Kitchen");
}

#[test]
fn resolved_service_prefers_ipv4() {
    let device = map_device(
        "Dual",
        8009,
        [IpAddr::V6(Ipv6Addr::LOCALHOST), v4(192, 168, 1, 43)].into_iter(),
        Some("dual-id"),
        Some("Dual Stack"),
        None,
    )
    .expect("maps");
    assert_eq!(
        cast_addr(&device).0,
        v4(192, 168, 1, 43),
        "an IPv4 address is preferred over IPv6 for the LAN media fetch"
    );
}

#[test]
fn resolved_service_without_address_is_unusable() {
    assert!(
        map_device(
            "Addrless",
            8009,
            std::iter::empty(),
            Some("addrless-id"),
            Some("Addrless"),
            None
        )
        .is_none(),
        "a service with no address can't be reached, so it is dropped"
    );
}

/// A receiver's time is taken only when it is a time: finite, not negative and
/// within a year.
#[test]
fn a_receiver_time_that_is_not_a_time_is_none() {
    assert_eq!(receiver_time(12.5), Some(std::time::Duration::from_millis(12_500)));
    for seconds in [f32::INFINITY, f32::NEG_INFINITY, f32::NAN, -1.0, 1e30, 1e12] {
        assert_eq!(receiver_time(seconds), None, "{seconds}");
    }
}
