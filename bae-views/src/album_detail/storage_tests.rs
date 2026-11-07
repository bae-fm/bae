use super::*;

/// A transition this process runs shows while the files rest, and the
/// queues' places show over it.
#[test]
fn a_running_transition_shows_only_while_the_files_rest() {
    let detach = Some(ReleaseStorageAction::DetachAudio);
    assert_eq!(
        StorageRowStatus::of(StoragePlace::Pinned, detach),
        StorageRowStatus::Transferring(ReleaseStorageAction::DetachAudio)
    );
    assert_eq!(
        StorageRowStatus::of(StoragePlace::Pinning, Some(ReleaseStorageAction::Pin)),
        StorageRowStatus::Pinning
    );
    assert_eq!(
        StorageRowStatus::of(StoragePlace::Uploading, Some(ReleaseStorageAction::Upload)),
        StorageRowStatus::Uploading
    );
    assert_eq!(StorageRowStatus::of(StoragePlace::InStorage, None), StorageRowStatus::InStorage);
}
