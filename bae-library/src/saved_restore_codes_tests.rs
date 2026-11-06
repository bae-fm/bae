//! The restore codes saved for the person: on a device that may have nowhere
//! to keep them, and only for libraries not on this device.

use super::*;

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
    assert!(!keeps_no_saved_codes(&coven::BootstrapError::SecureStorage(
        coven::KeyError::RestoreCodeStoreId
    )));
}

/// A library on this device is opened, not restored, so neither its readable
/// code nor an entry coven can't read is offered; elsewhere both are, the
/// unreadable one for the person to delete.
#[tokio::test]
async fn saved_codes_for_libraries_on_this_device_are_not_offered() {
    let temp = tempfile::TempDir::new().unwrap();
    let installation =
        coven::Coven::in_memory(bae_app_dir::AppDir::at(temp.path()).unwrap().coven_app_dir());
    let here = installation
        .create_closed_store("On This Device", std::sync::Arc::new(coven::UuidIds))
        .await
        .unwrap()
        .id();
    let elsewhere = coven::StoreId(uuid::Uuid::new_v4());
    let unreadable_elsewhere = coven::StoreId(uuid::Uuid::new_v4());
    let code = |store_id| {
        coven::SavedRestoreEntry::Readable(coven::SavedRestoreCode {
            name: "Library".into(),
            store_id,
            location: coven::StorageConfig::CloudKit {
                container: "iCloud.fm.bae.desktop".into(),
                owner: "owner".into(),
                zone: "zone".into(),
            },
            has_s3_credentials: false,
        })
    };
    let unreadable = |store_id| coven::SavedRestoreEntry::Unreadable {
        store_id,
        reason: coven::UnreadableSavedCode::Format,
    };
    let offered = restorable_codes(
        vec![code(here), unreadable(here), code(elsewhere), unreadable(unreadable_elsewhere)],
        &installation,
        &bae_diagnostics::Diagnostics::noop(),
    )
    .await
    .unwrap();
    assert_eq!(offered, vec![code(elsewhere), unreadable(unreadable_elsewhere)]);
}
