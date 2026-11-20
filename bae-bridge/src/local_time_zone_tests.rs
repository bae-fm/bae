use super::*;
use bae_playback::LocalZone as _;

/// A zone the way a host sets one: an offset in seconds east of UTC that
/// changes at a moment, as daylight saving time ends.
struct SummerTime {
    ends_at_millis: i64,
}

impl BridgeLocalTimeZone for SummerTime {
    fn utc_offset_seconds(&self, unix_millis: i64) -> i32 {
        if unix_millis < self.ends_at_millis {
            -4 * 3_600
        } else {
            -5 * 3_600
        }
    }
}

struct Throws;

impl BridgeLocalTimeZone for Throws {
    fn utc_offset_seconds(&self, _unix_millis: i64) -> i32 {
        panic!("Callback interface failure")
    }
}

fn at(rfc3339: &str) -> time::UtcDateTime {
    bae_util::timestamp::parse_rfc3339(rfc3339).unwrap()
}

/// The host answers for the moment asked, so a change of offset between two
/// plays gives each its own.
#[test]
fn the_offset_is_the_hosts_at_the_moment_asked() {
    let zone = HostZone::new(
        Box::new(SummerTime { ends_at_millis: 1_793_512_800_000 }),
        bae_diagnostics::Diagnostics::noop(),
    );

    assert_eq!(zone.offset_at(at("2026-11-01T02:00:00Z")).unwrap().whole_seconds(), -4 * 3_600);
    assert_eq!(zone.offset_at(at("2026-11-02T02:00:00Z")).unwrap().whole_seconds(), -5 * 3_600);
}

/// A host that throws gives no offset, and the throw is reported.
#[test]
fn a_host_that_throws_gives_no_offset() {
    let zone = HostZone::new(Box::new(Throws), bae_diagnostics::Diagnostics::noop());

    assert!(zone.offset_at(at("2026-11-01T02:00:00Z")).is_err());
}

struct OutOfRange;

impl BridgeLocalTimeZone for OutOfRange {
    fn utc_offset_seconds(&self, _unix_millis: i64) -> i32 {
        i32::MAX
    }
}

/// An answer that is no offset is an error, not UTC.
#[test]
fn an_answer_that_is_no_offset_is_an_error() {
    let zone = HostZone::new(Box::new(OutOfRange), bae_diagnostics::Diagnostics::noop());

    assert!(zone.offset_at(at("2026-11-01T02:00:00Z")).is_err());
}
