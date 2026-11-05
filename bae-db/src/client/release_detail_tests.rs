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
