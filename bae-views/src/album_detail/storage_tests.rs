use super::*;
use bae_transfers::{DownloadEntry, QueueWait};

fn queued(ahead: u32) -> Option<DownloadEntry> {
    Some(DownloadEntry::Queued { ahead, wait: Some(QueueWait::Paused) })
}

/// A transition this process runs shows while the files rest, and the
/// queues' places show over it.
#[test]
fn a_running_transition_shows_only_while_the_files_rest() {
    let detach = Some(ReleaseStorageAction::DetachAudio);
    assert_eq!(
        StorageRowStatus::of(StoragePlace::Pinned, detach, None, None),
        Ok(StorageRowStatus::Transferring(ReleaseStorageAction::DetachAudio))
    );
    assert_eq!(
        StorageRowStatus::of(
            StoragePlace::Pinning,
            Some(ReleaseStorageAction::Pin),
            Some(DownloadEntry::Running { wait: None }),
            None
        ),
        Ok(StorageRowStatus::Pinning(DownloadEntryStep::Running { wait: None }))
    );
    assert_eq!(
        StorageRowStatus::of(
            StoragePlace::Uploading,
            Some(ReleaseStorageAction::Upload),
            None,
            None
        ),
        Ok(StorageRowStatus::Uploading)
    );
    assert_eq!(
        StorageRowStatus::of(StoragePlace::InStorage, None, None, None),
        Ok(StorageRowStatus::InStorage)
    );
}

/// A pin waiting in the queue shows how many entries are ahead of it and
/// what holds the queue back; a failed one shows its error.
#[test]
fn a_download_shows_its_place_in_the_queue_and_its_failure() {
    assert_eq!(
        StorageRowStatus::of(StoragePlace::Pinning, None, queued(2), None),
        Ok(StorageRowStatus::Pinning(DownloadEntryStep::Queued {
            ahead: 2,
            wait: Some(QueueWait::Paused)
        }))
    );
    assert_eq!(
        StorageRowStatus::of(
            StoragePlace::Failed,
            None,
            Some(DownloadEntry::Failed { error: "offline".to_string() }),
            None
        ),
        Ok(StorageRowStatus::DownloadFailed { error: "offline".to_string() })
    );
}

/// A place that names the download queue without its entry is a row the
/// listing and the queue disagree on, not one to show.
#[test]
fn a_download_place_without_its_entry_is_refused() {
    assert!(StorageRowStatus::of(StoragePlace::Pinning, None, None, None).is_err());
    assert!(StorageRowStatus::of(StoragePlace::Failed, None, queued(0), None).is_err());
}

/// A failed move shows the file whose original changed; a failed download
/// reads first when both failed; a failure place with neither is refused.
#[test]
fn a_failed_move_names_the_file_that_changed() {
    assert_eq!(
        StorageRowStatus::of(StoragePlace::Failed, None, None, Some("a.flac".to_string())),
        Ok(StorageRowStatus::MoveFailed { file_name: "a.flac".to_string() })
    );
    assert_eq!(
        StorageRowStatus::of(
            StoragePlace::Failed,
            None,
            Some(DownloadEntry::Failed { error: "offline".to_string() }),
            Some("a.flac".to_string())
        ),
        Ok(StorageRowStatus::DownloadFailed { error: "offline".to_string() })
    );
    assert!(StorageRowStatus::of(StoragePlace::Failed, None, None, None).is_err());
}
