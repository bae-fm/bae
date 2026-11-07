use super::*;
use bae_store::FormerDeviceState;

/// The line naming the device a release's audio is on follows coven's
/// registration of that device: active, removed, replaced when it was reset,
/// and removed when its person was removed from the library, whatever its own
/// registration still says.
#[test]
fn the_other_device_follows_its_registration_and_its_person() {
    let name = || "Device B".to_string();
    assert_eq!(other_device(name(), "active", false), Some(OtherDevice::Active { name: name() }));
    assert_eq!(
        other_device(name(), "removed", false),
        Some(OtherDevice::Former { name: name(), state: FormerDeviceState::Removed })
    );
    assert_eq!(
        other_device(name(), "replaced", false),
        Some(OtherDevice::Former { name: name(), state: FormerDeviceState::Replaced })
    );
    assert_eq!(
        other_device(name(), "active", true),
        Some(OtherDevice::Former { name: name(), state: FormerDeviceState::Removed }),
        "a removed person's devices are removed with them"
    );
    assert_eq!(other_device(name(), "lost", false), None);
}

/// A release file the library holds no row of, as one that left after the
/// read that found it on one device only, is on no one device.
#[tokio::test]
async fn a_file_on_no_one_device_names_no_other_device() {
    let (db, _temp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);

    let refused = records.release_file_other_device("gone").await;

    assert!(
        matches!(&refused, Err(OtherDeviceError::NotOnOneDevice(file)) if file == "gone"),
        "{refused:?}"
    );
}
