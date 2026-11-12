use super::*;

/// The database's own failures are in this device's copy; a busy database
/// passes by itself, and the network or the keychain fail the same on a new
/// copy.
#[test]
fn only_a_failure_in_the_devices_copy_is_replaced_by_a_reset() {
    assert!(UiErrorCategory::Database.reset_from_cloud_replaces());
    assert!(UiErrorCategory::DatabaseDamaged.reset_from_cloud_replaces());
    assert!(!UiErrorCategory::DatabaseBusy.reset_from_cloud_replaces());
    assert!(!UiErrorCategory::Network.reset_from_cloud_replaces());
    assert!(!UiErrorCategory::KeyringLocked.reset_from_cloud_replaces());
    assert!(!UiErrorCategory::Internal.reset_from_cloud_replaces());
}
