use super::*;
use crate::library::CountLabel;
use crate::library::{DeviceArrivals, OutboxSnapshot, ReleaseArrival, UploadProgress};

fn status() -> SyncStatusSnapshot {
    SyncStatusSnapshot {
        status: coven::SyncStatus::Synced {
            finished_at: std::time::UNIX_EPOCH,
        },
        pending: Some(Ok(Vec::new())),
        arrivals: Some(Ok(Arrivals::default())),
        error: None,
    }
}

/// Nothing moving and nothing to fix: the indicator is absent.
#[test]
fn a_settled_library_is_quiet() {
    let activity = SyncActivity::new(
        &status(),
        &OutboxSnapshot::default(),
        &DownloadQueueContents::default(),
    );
    assert!(!activity.moving());
    assert!(!activity.needs_attention());
}

/// A pass that runs while nothing transfers doesn't spin the indicator; the
/// list still says sync is running.
#[test]
fn a_pass_alone_is_not_movement() {
    let mut syncing = status();
    syncing.status = coven::SyncStatus::Syncing;
    let activity = SyncActivity::new(
        &syncing,
        &OutboxSnapshot::default(),
        &DownloadQueueContents::default(),
    );
    assert!(activity.syncing);
    assert!(!activity.moving());
}

/// Files being sent and files arriving each move; a paused queue's waiting
/// files don't.
#[test]
fn transfers_and_arrivals_are_movement() {
    let mut outbox = OutboxSnapshot {
        total: UploadProgress {
            queued: 4,
            uploading: 2,
            upload_bytes_done: 10,
            upload_bytes_total: 40,
        },
        ..OutboxSnapshot::default()
    };
    let uploading = SyncActivity::new(&status(), &outbox, &DownloadQueueContents::default());
    assert!(uploading.moving());
    assert_eq!(
        uploading.uploads,
        Some(UploadsInFlight {
            sending: 2,
            waiting: 4,
            bytes_done: 10,
            bytes_total: 40,
        })
    );
    outbox.pause_state = OutboxPauseState::Paused;
    outbox.total.uploading = 0;
    let paused = SyncActivity::new(&status(), &outbox, &DownloadQueueContents::default());
    assert_eq!(paused.uploads, None);

    let mut arriving = status();
    arriving.arrivals = Some(Ok(Arrivals {
        devices: vec![DeviceArrivals {
            device: coven::DeviceId(1),
            device_name: "Studio Mac".into(),
            releases: vec![ReleaseArrival {
                release_id: "release".into(),
                album_id: "album".into(),
                album_title: "Blue".into(),
                cover: true,
                track_ids: Vec::new(),
            }],
            other_files: 0,
        }],
    }));
    let activity = SyncActivity::new(
        &arriving,
        &OutboxSnapshot::default(),
        &DownloadQueueContents::default(),
    );
    assert!(activity.moving());
    assert!(!activity.needs_attention(), "arriving files need nothing");
}

/// A sync failure or a pending record that needs the person marks the
/// indicator; a failed read of either list does too.
#[test]
fn failures_and_pending_records_need_attention() {
    let mut failed = status();
    failed.status = coven::SyncStatus::Offline {
        error: std::sync::Arc::new(coven::StorageFailure::Network.into()),
    };
    let new = |status: &SyncStatusSnapshot| {
        SyncActivity::new(
            status,
            &OutboxSnapshot::default(),
            &DownloadQueueContents::default(),
        )
    };
    assert!(new(&failed).needs_attention());
    let mut waiting = status();
    waiting.pending = Some(Ok(vec![PendingItem::before_open(
        coven::PendingRecord::new(
            coven::PendingSubject::Connection,
            coven::PendingReason::NoStorage,
        ),
    )]));
    assert!(new(&waiting).needs_attention());
    let mut unread = status();
    unread.arrivals = Some(Err(UiError::internal("no devices")));
    assert!(new(&unread).needs_attention());
}

/// The list's lines name what is sending or downloading, then what waits,
/// and drop a zero.
#[test]
fn transfer_lines_name_the_moving_count_first() {
    let label = |key: &str, count| CountLabel {
        key: key.into(),
        count,
    };
    assert_eq!(
        UploadsInFlight {
            sending: 2,
            waiting: 4,
            bytes_done: 0,
            bytes_total: 0,
        }
        .summary_parts(),
        vec![
            label("core.queue.uploading", 2),
            label("core.queue.queued", 4)
        ]
    );
    assert_eq!(
        DownloadsInFlight {
            active: 1,
            queued: 0
        }
        .summary_parts(),
        vec![label("core.queue.downloading", 1)]
    );
}
