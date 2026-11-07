use super::*;
use bae_transfers::{DownloadEntry, DownloadKind, DownloadStep, QueueWait};

fn entry(kind: DownloadKind, step: DownloadStep) -> Option<DownloadEntry> {
    Some(DownloadEntry { kind, step })
}

fn queued(kind: DownloadKind, ahead: u32) -> Option<DownloadEntry> {
    entry(kind, DownloadStep::Queued { ahead, wait: Some(QueueWait::Paused) })
}

fn failed(kind: DownloadKind, error: &str) -> Option<DownloadEntry> {
    entry(kind, DownloadStep::Failed { error: error.to_string() })
}

/// The queues' places show a pin or a make-local by its entry, and a
/// release at rest shows where its files are.
#[test]
fn a_running_entry_shows_by_what_it_does() {
    assert_eq!(
        StorageRowStatus::of(
            StoragePlace::Pinning,
            entry(DownloadKind::Pin, DownloadStep::Running { wait: None }),
            None
        ),
        Ok(StorageRowStatus::Pinning(DownloadEntryStep::Running { wait: None }))
    );
    assert_eq!(
        StorageRowStatus::of(
            StoragePlace::Pinning,
            entry(DownloadKind::MakeLocal, DownloadStep::Running { wait: None }),
            None
        ),
        Ok(StorageRowStatus::MakingLocal(DownloadEntryStep::Running { wait: None }))
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

/// An entry waiting in the queue shows how many entries are ahead of it and
/// what holds the queue back; a failed one shows its error, by what it does.
#[test]
fn a_download_shows_its_place_in_the_queue_and_its_failure() {
    let waiting = DownloadEntryStep::Queued { ahead: 2, wait: Some(QueueWait::Paused) };
    assert_eq!(
        StorageRowStatus::of(StoragePlace::Pinning, queued(DownloadKind::Pin, 2), None),
        Ok(StorageRowStatus::Pinning(waiting.clone()))
    );
    assert_eq!(
        StorageRowStatus::of(StoragePlace::Pinning, queued(DownloadKind::MakeLocal, 2), None),
        Ok(StorageRowStatus::MakingLocal(waiting))
    );
    assert_eq!(
        StorageRowStatus::of(StoragePlace::Failed, failed(DownloadKind::Pin, "offline"), None),
        Ok(StorageRowStatus::DownloadFailed { error: "offline".to_string() })
    );
    assert_eq!(
        StorageRowStatus::of(
            StoragePlace::Failed,
            failed(DownloadKind::MakeLocal, "disk full"),
            None
        ),
        Ok(StorageRowStatus::MakeLocalFailed { error: "disk full".to_string() })
    );
}

/// A place that names the download queue without its entry is a row the
/// listing and the queue disagree on, not one to show.
#[test]
fn a_download_place_without_its_entry_is_refused() {
    assert!(StorageRowStatus::of(StoragePlace::Pinning, None, None).is_err());
    assert!(StorageRowStatus::of(StoragePlace::Failed, queued(DownloadKind::Pin, 0), None).is_err());
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
