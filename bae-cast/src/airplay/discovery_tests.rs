use super::*;
use crate::airplay::capabilities::ReceiverAccess;
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
        AirPlayService::AirPlay,
        "Kitchen",
        7000,
        [v4(10, 0, 0, 4)].into_iter(),
        &txt(&[("deviceid", "AA:BB:CC:DD:EE:FF"), ("features", AIRPLAY2_FEATURES)]),
    )
    .expect("a service with an address maps");
    assert_eq!(device.id, "AABBCCDDEEFF");
    assert_eq!(device.name, "Kitchen");
    assert_eq!(device.addr, v4(10, 0, 0, 4));
    assert_eq!(device.port, 7000);
    assert!(matches!(device.capabilities.as_ref().unwrap().dialect, Dialect::AirPlay2 { .. }));
}

/// A RAOP instance name `MAC@Name` yields the MAC as the id and the suffix as
/// the name.
#[test]
fn raop_instance_name_splits_mac_and_name() {
    let device = map_device(
        AirPlayService::Raop,
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
        AirPlayService::AirPlay,
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
        AirPlayService::AirPlay,
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
        AirPlayService::Raop,
        "AABBCCDDEEFF@Den",
        5000,
        [v4(10, 0, 0, 7)].into_iter(),
        &txt(&[("et", "1"), ("cn", "1")]),
    )
    .unwrap();
    let airplay = map_device(
        AirPlayService::AirPlay,
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
    assert!(matches!(snap[0].capabilities.as_ref().unwrap().dialect, Dialect::AirPlay2 { .. }));
    assert_eq!(snap[0].port, 7000);
}

/// A Mac advertises `_airplay._tcp` with a colon-separated `deviceid` and
/// `_raop._tcp` as `AABBCCDDEEFF@Name` with no `deviceid`: the two are one
/// receiver, whatever case or separators the id was written in.
#[test]
fn colon_deviceid_and_raop_name_prefix_are_one_receiver() {
    let raop = map_device(
        AirPlayService::Raop,
        "A483E771D10D@Den",
        5000,
        [v4(10, 0, 0, 7)].into_iter(),
        &txt(&[("et", "1"), ("cn", "1")]),
    )
    .unwrap();
    let airplay = map_device(
        AirPlayService::AirPlay,
        "Den",
        7000,
        [v4(10, 0, 0, 7)].into_iter(),
        &txt(&[("deviceid", "a4:83:e7:71:d1:0d"), ("features", AIRPLAY2_FEATURES)]),
    )
    .unwrap();
    assert_eq!(airplay.id, raop.id);

    let snap = dedupe_by_id([&raop, &airplay].into_iter());
    assert_eq!(snap.len(), 1, "one entry per receiver");
    assert!(matches!(snap[0].capabilities.as_ref().unwrap().dialect, Dialect::AirPlay2 { .. }));
}

/// A receiver whose AirPlay advertisement bae can't read shows as one it
/// can't connect to, even when its RAOP advertisement reads: bae can't tell
/// which dialect it speaks best.
#[test]
fn an_unreadable_advertisement_wins_over_a_readable_one() {
    let raop = map_device(
        AirPlayService::Raop,
        "AABBCCDDEEFF@Den",
        5000,
        [v4(10, 0, 0, 7)].into_iter(),
        &txt(&[("et", "1"), ("cn", "1")]),
    )
    .unwrap();
    let airplay = map_device(
        AirPlayService::AirPlay,
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

/// An `_airplay._tcp` service states its model in the `model` record.
#[test]
fn airplay_service_states_its_model_in_model() {
    let device = map_device(
        AirPlayService::AirPlay,
        "Kitchen",
        7000,
        [v4(10, 0, 0, 4)].into_iter(),
        &txt(&[("deviceid", "AA:BB:CC:DD:EE:FF"), ("model", "AudioAccessory5,1")]),
    )
    .unwrap();
    assert_eq!(device.model.as_deref(), Some("AudioAccessory5,1"));
    assert_eq!(device.to_renderer_device().receiver().address, "10.0.0.4");
}

/// A `_raop._tcp` service states its model in the `am` record.
#[test]
fn raop_service_states_its_model_in_am() {
    let device = map_device(
        AirPlayService::Raop,
        "001122334455@Studio",
        5000,
        [v4(10, 0, 0, 9)].into_iter(),
        &txt(&[("am", "Sonos One"), ("et", "0,1")]),
    )
    .unwrap();
    assert_eq!(device.model.as_deref(), Some("Sonos One"));
}

/// A receiver that states no model, or an empty one, has none.
#[test]
fn airplay_service_without_a_model_has_none() {
    for records in [vec![("deviceid", "ID")], vec![("deviceid", "ID"), ("model", " ")]] {
        let device = map_device(
            AirPlayService::AirPlay,
            "Bare",
            7000,
            [v4(10, 0, 0, 5)].into_iter(),
            &txt(&records),
        )
        .unwrap();
        assert_eq!(device.model, None);
    }
}

/// `acl` and `act` are on a receiver's AirPlay advertisement only. An AirPlay 1
/// receiver chosen at its RAOP advertisement still plays only from who its
/// AirPlay advertisement says, whichever of the two arrived first.
#[test]
fn an_airplay_advertisements_access_holds_for_its_raop_twin() {
    let raop = map_device(
        AirPlayService::Raop,
        "AABBCCDDEEFF@Den",
        5000,
        [v4(10, 0, 0, 7)].into_iter(),
        &txt(&[("et", "0,1")]),
    )
    .unwrap();
    // No AirPlay 2 bit: an AirPlay 1 receiver advertising on both services.
    let airplay = map_device(
        AirPlayService::AirPlay,
        "Den",
        7000,
        [v4(10, 0, 0, 7)].into_iter(),
        &txt(&[("deviceid", "AABBCCDDEEFF"), ("features", "0x39f7"), ("acl", "1")]),
    )
    .unwrap();

    for devices in [[&raop, &airplay], [&airplay, &raop]] {
        let snap = dedupe_by_id(devices.into_iter());
        assert_eq!(snap.len(), 1, "one entry per receiver");
        assert_eq!(
            snap[0].capabilities.as_ref().unwrap().access,
            ReceiverAccess::HomeMembers,
            "the receiver keeps the restriction its AirPlay advertisement states"
        );
    }
}
