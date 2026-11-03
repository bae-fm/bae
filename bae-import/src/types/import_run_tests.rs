//! Where an import's files go.

use super::*;

/// An import goes where the stored choice says, pinned as it says, and to the
/// cloud only when there is a home to go to.
#[test]
fn an_import_s_destination_is_the_stored_choice() {
    use bae_preferences::ImportStoragePreferences;
    let mut storage = ImportStoragePreferences::default();
    assert_eq!(
        ImportDestination::of(storage, false),
        ImportDestination::Local,
        "without storage, imports stay local"
    );
    assert_eq!(ImportDestination::of(storage, true), ImportDestination::Remote { pin: true });
    storage.pinned = false;
    assert_eq!(ImportDestination::of(storage, true), ImportDestination::Remote { pin: false });
    storage.cloud = false;
    assert_eq!(ImportDestination::of(storage, true), ImportDestination::Local);
}
