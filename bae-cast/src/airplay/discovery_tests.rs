use super::*;
use crate::test_support::{txt, v4};

/// A features string with bit 48 (CoreUtils pairing/encryption) set — the
/// AirPlay 2 marker. Bit 48 is bit 16 of the high word: `0xLOW,0xHIGH` =
/// `0x0,0x10000`.
const AIRPLAY2_FEATURES: &str = "0x0,0x10000";

/// An `_airplay._tcp` service maps to an AirPlay 2 device named by its
/// instance label, addressed at its IPv4 address.
#[test]
fn airplay2_service_maps_to_device() {
    let device = map_device(
        RendererServiceType::AirPlay,
        "Kitchen",
        7000,
        [v4(10, 0, 0, 4)].into_iter(),
        &txt(&[("deviceid", "AA:BB:CC:DD:EE:FF"), ("features", AIRPLAY2_FEATURES)]),
    )
    .expect("a service with an address maps");
    assert_eq!(device.id, "AA:BB:CC:DD:EE:FF");
    assert_eq!(device.name, "Kitchen");
    assert_eq!(device.addr, v4(10, 0, 0, 4));
    assert_eq!(device.port, 7000);
    assert_eq!(device.capabilities.as_ref().unwrap().dialect, Dialect::AirPlay2);
}

/// A RAOP instance name `MAC@Name` yields the MAC as the id and the suffix as
/// the name.
#[test]
fn raop_instance_name_splits_mac_and_name() {
    let device = map_device(
        RendererServiceType::Raop,
        "001122334455@Studio Monitor",
        5000,
        [v4(10, 0, 0, 9)].into_iter(),
        &txt(&[("et", "0,1"), ("cn", "0,1")]),
    )
    .expect("maps");
    assert_eq!(device.id, "001122334455");
    assert_eq!(device.name, "Studio Monitor");
    assert!(matches!(device.capabilities.as_ref().unwrap().dialect, Dialect::Raop { .. }));
}

/// A service with no resolvable address is dropped.
#[test]
fn service_without_address_is_dropped() {
    assert!(map_device(
        RendererServiceType::AirPlay,
        "Nowhere",
        7000,
        std::iter::empty(),
        &txt(&[("deviceid", "X")]),
    )
    .is_none());
}

/// IPv4 is chosen over an IPv6 address advertised for the same service.
#[test]
fn ipv4_is_preferred() {
    let device = map_device(
        RendererServiceType::AirPlay,
        "Dual",
        7000,
        ["fe80::1".parse().unwrap(), v4(192, 168, 1, 5)].into_iter(),
        &txt(&[("deviceid", "ID")]),
    )
    .unwrap();
    assert_eq!(device.addr, v4(192, 168, 1, 5));
}

/// A receiver seen on both `_raop._tcp` and `_airplay._tcp` collapses to one
/// entry, kept at the AirPlay 2 dialect.
#[test]
fn same_receiver_on_both_services_prefers_airplay2() {
    let raop = map_device(
        RendererServiceType::Raop,
        "AABBCCDDEEFF@Den",
        5000,
        [v4(10, 0, 0, 7)].into_iter(),
        &txt(&[("et", "1"), ("cn", "1")]),
    )
    .unwrap();
    let airplay = map_device(
        RendererServiceType::AirPlay,
        "Den",
        7000,
        [v4(10, 0, 0, 7)].into_iter(),
        &txt(&[("deviceid", "AABBCCDDEEFF"), ("features", AIRPLAY2_FEATURES)]),
    )
    .unwrap();

    let mut table = DeviceTable::default();
    table.by_fullname.insert("AABBCCDDEEFF@Den._raop._tcp.local.".to_string(), raop);
    table.by_fullname.insert("Den._airplay._tcp.local.".to_string(), airplay);

    let snap = snapshot(&table);
    assert_eq!(snap.len(), 1, "one entry per receiver");
    assert_eq!(snap[0].capabilities.as_ref().unwrap().dialect, Dialect::AirPlay2);
    assert_eq!(snap[0].port, 7000);
}

/// A receiver whose AirPlay advertisement bae can't read shows as one it
/// can't connect to, even when its RAOP advertisement reads: bae can't tell
/// which dialect it speaks best.
#[test]
fn an_unreadable_advertisement_wins_over_a_readable_one() {
    let raop = map_device(
        RendererServiceType::Raop,
        "AABBCCDDEEFF@Den",
        5000,
        [v4(10, 0, 0, 7)].into_iter(),
        &txt(&[("et", "1"), ("cn", "1")]),
    )
    .unwrap();
    let airplay = map_device(
        RendererServiceType::AirPlay,
        "Den",
        7000,
        [v4(10, 0, 0, 7)].into_iter(),
        &txt(&[("deviceid", "AABBCCDDEEFF"), ("features", "unreadable")]),
    )
    .unwrap();

    for order in [[raop.clone(), airplay.clone()], [airplay, raop]] {
        let snap = dedupe_by_id(order.iter());
        assert_eq!(snap.len(), 1, "one entry per receiver");
        assert_eq!(
            snap[0].capabilities,
            Err(UnreadableSetting {
                key: "features".to_string(),
                raw: Some("unreadable".to_string()),
            })
        );
    }
}
