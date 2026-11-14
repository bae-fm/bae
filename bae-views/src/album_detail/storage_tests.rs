use super::*;
use bae_transfers::{DownloadEntry, DownloadKind, DownloadStep};

fn entry(kind: DownloadKind, step: DownloadStep) -> Option<DownloadEntry> {
    Some(DownloadEntry { kind, step })
}

fn queued(kind: DownloadKind) -> Option<DownloadEntry> {
    entry(kind, DownloadStep::Queued)
}

fn failed(kind: DownloadKind, error: &str) -> Option<DownloadEntry> {
    entry(kind, DownloadStep::Failed { error: error.to_string() })
}

/// The queues' places show a pin or a move-to-this-device by its entry, and a
/// release at rest shows where its files are.
#[test]
fn a_running_entry_shows_by_what_it_does() {
    assert_eq!(
        StorageRowStatus::of(
            StoragePlace::Pinning,
            entry(DownloadKind::Pin, DownloadStep::Running),
            None
        ),
        Ok(StorageRowStatus::Pinning(DownloadEntryStep::Running))
    );
    assert_eq!(
        StorageRowStatus::of(
            StoragePlace::Pinning,
            entry(DownloadKind::MoveToThisDevice, DownloadStep::Running),
            None
        ),
        Ok(StorageRowStatus::MovingToThisDevice(DownloadEntryStep::Running))
    );
    assert_eq!(
        StorageRowStatus::of(StoragePlace::Uploading, None, None),
        Ok(StorageRowStatus::Uploading)
    );
    assert_eq!(
        StorageRowStatus::of(StoragePlace::InStorage, None, None),
        Ok(StorageRowStatus::InStorage)
    );
}

/// An entry waiting in the queue shows by what it does; a failed one shows
/// its error, by what it does.
#[test]
fn a_waiting_download_shows_by_what_it_does_and_a_failed_one_its_error() {
    let waiting = DownloadEntryStep::Queued;
    assert_eq!(
        StorageRowStatus::of(StoragePlace::Pinning, queued(DownloadKind::Pin), None),
        Ok(StorageRowStatus::Pinning(waiting))
    );
    assert_eq!(
        StorageRowStatus::of(StoragePlace::Pinning, queued(DownloadKind::MoveToThisDevice), None),
        Ok(StorageRowStatus::MovingToThisDevice(waiting))
    );
    assert_eq!(
        StorageRowStatus::of(StoragePlace::Failed, failed(DownloadKind::Pin, "offline"), None),
        Ok(StorageRowStatus::DownloadFailed { error: "offline".to_string() })
    );
    assert_eq!(
        StorageRowStatus::of(
            StoragePlace::Failed,
            failed(DownloadKind::MoveToThisDevice, "disk full"),
            None
        ),
        Ok(StorageRowStatus::MoveToThisDeviceFailed { error: "disk full".to_string() })
    );
}

/// A place that names the download queue without its entry is a row the
/// listing and the queue disagree on, not one to show.
#[test]
fn a_download_place_without_its_entry_is_refused() {
    assert!(StorageRowStatus::of(StoragePlace::Pinning, None, None).is_err());
    assert!(StorageRowStatus::of(StoragePlace::Failed, queued(DownloadKind::Pin), None).is_err());
    assert!(
        StorageRowStatus::of(StoragePlace::Pinning, failed(DownloadKind::Pin, "x"), None).is_err()
    );
}

/// A failed move shows the file whose original changed; a failed download
/// reads first when both failed; a failure place with neither is refused.
#[test]
fn a_failed_move_names_the_file_that_changed() {
    assert_eq!(
        StorageRowStatus::of(StoragePlace::Failed, None, Some("a.flac".to_string())),
        Ok(StorageRowStatus::MoveFailed { file_name: "a.flac".to_string() })
    );
    assert_eq!(
        StorageRowStatus::of(
            StoragePlace::Failed,
            failed(DownloadKind::Pin, "offline"),
            Some("a.flac".to_string())
        ),
        Ok(StorageRowStatus::DownloadFailed { error: "offline".to_string() })
    );
    assert!(StorageRowStatus::of(StoragePlace::Failed, None, None).is_err());
}
