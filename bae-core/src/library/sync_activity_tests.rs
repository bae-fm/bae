use super::*;
use crate::library::{CountLabel, ReleaseQueueCounts};

/// An empty, running upload queue.
fn idle_uploads() -> UploadQueueCounts {
    UploadQueueCounts {
        queued: 0,
        sending: 0,
        paused: false,
    }
}

fn status() -> SyncStatusSnapshot {
    SyncStatusSnapshot {
        status: coven::SyncStatus::Synced {
            finished_at: std::time::UNIX_EPOCH,
        },
        pending: Some(Ok(Vec::new())),
        abandoned: Some(Ok(Vec::new())),
        error: None,
    }
}

/// Nothing moving and nothing to fix: the indicator is absent.
#[test]
fn a_settled_library_is_quiet() {
    let activity = SyncActivity::new(&status(), &idle_uploads(), &ReleaseQueueCounts::default());
    assert!(!activity.moving());
    assert!(!activity.needs_attention());
}

/// A pass that runs while nothing transfers doesn't spin the indicator; the
/// list still says sync is running.
#[test]
fn a_pass_alone_is_not_movement() {
    let mut syncing = status();
    syncing.status = coven::SyncStatus::Syncing;
    let activity = SyncActivity::new(&syncing, &idle_uploads(), &ReleaseQueueCounts::default());
    assert!(activity.syncing);
    assert!(!activity.moving());
}

/// Files being sent move; a paused queue's waiting files don't move.
#[test]
fn transfers_are_movement() {
    let mut uploads = UploadQueueCounts {
        queued: 6,
        sending: 2,
        paused: false,
    };
    let uploading = SyncActivity::new(&status(), &uploads, &ReleaseQueueCounts::default());
    assert!(uploading.moving());
    assert_eq!(
        uploading.uploads,
        Some(UploadsInFlight {
            sending: 2,
            waiting: 4,
        })
    );
    uploads.paused = true;
    uploads.sending = 0;
    let paused = SyncActivity::new(&status(), &uploads, &ReleaseQueueCounts::default());
    assert_eq!(paused.uploads, None);
}

/// A sync failure, a pending record that needs the person or an abandoned
/// upload marks the indicator; a failed read of any list does too.
#[test]
fn failures_and_pending_records_need_attention() {
    let mut failed = status();
    failed.status = coven::SyncStatus::Offline {
        error: std::sync::Arc::new(coven::StorageFailure::Network.into()),
    };
    let new = |status: &SyncStatusSnapshot| {
        SyncActivity::new(status, &idle_uploads(), &ReleaseQueueCounts::default())
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
    unread.pending = Some(Err(UiError::internal("no records")));
    assert!(new(&unread).needs_attention());
    let mut abandoned = status();
    abandoned.abandoned = Some(Ok(vec![crate::library::AbandonedItem {
        subject: crate::library::PendingSubjectName::ArtistImage,
        file: coven::FileId(uuid::Uuid::from_u128(1)),
        reason: coven::AbandonReason::Replaced,
    }]));
    assert!(new(&abandoned).needs_attention());
    let mut abandoned_unread = status();
    abandoned_unread.abandoned = Some(Err(UiError::internal("no records")));
    assert!(new(&abandoned_unread).needs_attention());
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

/// An S3 failure the provider answered, as coven reports the whole pass's.
fn answered_s3(failure: coven::StorageFailure) -> std::sync::Arc<coven::StorageError> {
    std::sync::Arc::new(coven::StorageError::Provider {
        provider: coven::CloudProvider::S3,
        failure,
        source: Box::new(std::io::Error::other("S3 answered HTTP 503")),
    })
}

fn pending(record: coven::PendingRecord) -> Option<Result<Vec<PendingItem>, UiError>> {
    Some(Ok(vec![PendingItem::before_open(record)]))
}

fn connection(failure: coven::StorageFailure) -> coven::PendingRecord {
    coven::PendingRecord::new(
        coven::PendingSubject::Connection,
        coven::PendingReason::Storage {
            provider: coven::CloudProvider::S3,
            failure,
            answered: true,
        },
    )
}

fn activity(status: &SyncStatusSnapshot) -> SyncActivity {
    SyncActivity::new(status, &idle_uploads(), &ReleaseQueueCounts::default())
}

/// A pass that fails on the connection leaves coven's record of that
/// failure: the list shows the record, which names what failed and what the
/// person can do, and not the same failure again as a second line.
#[test]
fn a_failed_connection_shows_once_as_its_record() {
    for failure in [
        coven::SyncStatus::Failed {
            error: coven::SyncFailure::Storage(answered_s3(coven::StorageFailure::Protocol)),
        },
        coven::SyncStatus::Offline {
            error: answered_s3(coven::StorageFailure::Protocol),
        },
    ] {
        let mut failed = status();
        failed.status = failure;
        failed.pending = pending(connection(coven::StorageFailure::Protocol));
        let shown = activity(&failed);
        assert_eq!(shown.failure, None, "{:?}", failed.status);
        assert!(shown.needs_attention());
        assert!(matches!(&shown.attention, Some(Ok(items)) if items.len() == 1));
    }
}

/// A blocked slot and coven's own failed operation each have their record
/// in the list, which shows the failure the status holds.
#[test]
fn a_blocked_slot_or_failed_operation_shows_once_as_its_record() {
    let reason = coven::PendingReason::Storage {
        provider: coven::CloudProvider::S3,
        failure: coven::StorageFailure::Network,
        answered: true,
    };
    let mut blocked = status();
    blocked.status = coven::SyncStatus::Blocked {
        slot: coven::Slot(3),
        reason: reason.clone(),
        error: Some(answered_s3(coven::StorageFailure::Network)),
    };
    blocked.pending = pending(coven::PendingRecord::new(
        coven::PendingSubject::Slot(coven::Slot(3)),
        reason.clone(),
    ));
    assert_eq!(activity(&blocked).failure, None);

    let operation = coven::PendingRecord::new(
        coven::PendingSubject::Operation {
            id: coven::OperationId(5),
            kind: coven::OperationKind::Invite,
        },
        reason,
    );
    let mut failed = status();
    failed.status = coven::SyncStatus::Failed {
        error: coven::SyncFailure::Operation(Box::new(operation.clone())),
    };
    failed.pending = pending(operation);
    assert_eq!(activity(&failed).failure, None);
}

/// A failure no record shows is the list's line: another copy of this
/// device, which coven records nothing for, beside an older connection
/// record; or a storage failure whose record the list hasn't read yet.
#[test]
fn a_failure_no_record_shows_is_its_own_line() {
    let mut copied = status();
    copied.status = coven::SyncStatus::Failed {
        error: coven::SyncFailure::DeviceIdInUse(coven::DeviceIdEvidence::Replaced),
    };
    copied.pending = pending(connection(coven::StorageFailure::Network));
    assert!(activity(&copied).failure.is_some());

    let mut unrecorded = status();
    unrecorded.status = coven::SyncStatus::Failed {
        error: coven::SyncFailure::Storage(answered_s3(coven::StorageFailure::Protocol)),
    };
    unrecorded.pending = pending(connection(coven::StorageFailure::Network));
    assert!(activity(&unrecorded).failure.is_some());
}
