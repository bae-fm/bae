//! Cast discovery: mDNS record → [`RendererDevice`] mapping. The session and
//! channel logic is tested generically in `crate::renderer`.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::discovery::map_device;
use crate::renderer::{RendererConnection, RendererDevice, RendererKind};

fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(a, b, c, d))
}

/// The Cast connection's address and port, for asserting on a mapped device.
fn cast_addr(device: &RendererDevice) -> (IpAddr, u16) {
    match device.connection {
        RendererConnection::Cast { addr, port } => (addr, port),
        RendererConnection::Dlna { .. } => panic!("cast discovery must map to a Cast connection"),
    }
}

#[test]
fn resolved_service_maps_to_device() {
    let device = map_device(
        "Living Room._googlecast._tcp.local.",
        8009,
        [v4(192, 168, 1, 40)].into_iter(),
        Some("abcd1234"),
        Some("Living Room Speaker"),
    )
    .expect("a resolved cast service maps to a device");

    assert_eq!(device.id, "abcd1234");
    assert_eq!(device.name, "Living Room Speaker");
    assert_eq!(device.kind(), RendererKind::Cast);
    assert_eq!(cast_addr(&device), (v4(192, 168, 1, 40), 8009));
}

#[test]
fn resolved_service_without_id_is_unusable() {
    assert!(
        map_device(
            "No Id._googlecast._tcp.local.",
            8009,
            [v4(192, 168, 1, 41)].into_iter(),
            None,
            Some("No Id"),
        )
        .is_none(),
        "a service with no device id can't be routed to, so it is dropped"
    );
}

#[test]
fn resolved_service_falls_back_to_instance_name_without_fn() {
    let device = map_device(
        "Kitchen._googlecast._tcp.local.",
        8009,
        [v4(192, 168, 1, 42)].into_iter(),
        Some("kitchen-id"),
        None,
    )
    .expect("maps with a fallback name");
    assert_eq!(device.name, "Kitchen");
}

#[test]
fn resolved_service_prefers_ipv4() {
    let device = map_device(
        "Dual._googlecast._tcp.local.",
        8009,
        [IpAddr::V6(Ipv6Addr::LOCALHOST), v4(192, 168, 1, 43)].into_iter(),
        Some("dual-id"),
        Some("Dual Stack"),
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
            "Addrless._googlecast._tcp.local.",
            8009,
            std::iter::empty(),
            Some("addrless-id"),
            Some("Addrless"),
        )
        .is_none(),
        "a service with no address can't be reached, so it is dropped"
    );
}
