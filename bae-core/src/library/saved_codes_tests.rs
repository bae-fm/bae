//! Listing the restore codes saved for the person on a device that may have
//! nowhere to keep them.

use super::keeps_no_saved_codes;

/// A phone without Google Play services has no Block Store: coven answers
/// `Unsupported`, and the welcome lists no saved codes rather than an error.
#[test]
fn a_device_with_nowhere_to_keep_saved_codes_keeps_none() {
    assert!(keeps_no_saved_codes(&coven::BootstrapError::SecureStorage(
        coven::KeyError::Unsupported
    )));
}

/// Any other failure to read the saved codes stays a failure, so a refused
/// read never looks like a device with none.
#[test]
fn a_failed_read_of_the_saved_codes_stays_a_failure() {
    assert!(!keeps_no_saved_codes(
        &coven::BootstrapError::SecureStorage(coven::KeyError::RestoreCodeStoreId)
    ));
}
