//! The status query reads a library with nothing recorded as one row.

use super::*;
use bae_store::test_fixtures::temp_db;

#[tokio::test]
async fn a_library_with_nothing_recorded_reads_as_not_synced() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let status = db.read(|sql| sync_status_on(&sql)).await.unwrap();
    assert_eq!(status.coven.state, coven::SyncState::NotSynced);
    assert_eq!(status.coven.last_synced, None);
    assert!(status.pending.is_empty());
    assert_eq!(status.attention_count, 0);
    assert_eq!(status.error, None);
    assert_eq!(status.storage_setup, None);
    assert_eq!(status.retry_effect, coven::RetryEffect::NotSyncing);
    assert!(!status.failure_recorded);
}

/// Whether `FAILURE_RECORDED` finds a record for `state`, when `held` is the
/// kind of subject and reason and the JSON of the one pending record, as
/// coven keeps them in `coven_pending`.
fn records_failure(
    state: &coven::SyncState,
    held: Option<(&str, &str, &coven::PendingRecord)>,
) -> bool {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "CREATE TABLE coven_pending (record BLOB, subject_kind TEXT, reason_kind TEXT)",
        )
        .unwrap();
    if let Some((subject_kind, reason_kind, record)) = held {
        connection
            .execute(
                "INSERT INTO coven_pending VALUES (?1, ?2, ?3)",
                (serde_json::to_vec(record).unwrap(), subject_kind, reason_kind),
            )
            .unwrap();
    }
    let kind = match state {
        coven::SyncState::Removed { .. } => "removed",
        coven::SyncState::NeedsReset { .. } => "needs_reset",
        coven::SyncState::Offline(_) => "offline",
        coven::SyncState::Failed(_) => "failed",
        coven::SyncState::StorageFull(_) => "storage_full",
        _ => panic!("a state these tests don't name"),
    };
    connection
        .query_row(
            &format!(
                "SELECT {FAILURE_RECORDED} FROM \
                 (SELECT sync.*, {SHOWN_PASS} AS pass \
                  FROM (SELECT ?1 AS state, ?2 AS detail) AS sync) AS state"
            ),
            (kind, serde_json::to_string(state).unwrap()),
            |row| row.get(0),
        )
        .unwrap()
}

fn storage_reason(failure: coven::StorageFailure) -> coven::PendingReason {
    coven::PendingReason::Storage { provider: coven::CloudProvider::S3, failure, answered: true }
}

#[test]
fn a_connection_record_of_the_failure_shows_a_failed_pass() {
    use coven::StorageFailure as F;
    let record =
        coven::PendingRecord::new(coven::PendingSubject::Connection, storage_reason(F::Network));
    let held = Some(("connection", "storage", &record));
    let offline = |failure| {
        coven::SyncState::Offline(coven::SyncCause::Pass(coven::LastPass::Offline {
            failure,
            error: "unreachable".into(),
        }))
    };
    assert!(records_failure(&offline(F::Network), held));
    assert!(!records_failure(&offline(F::Protocol), held));
    assert!(!records_failure(&offline(F::Network), None));
    let refused = coven::SyncState::Failed(coven::SyncCause::Pass(coven::LastPass::Failed {
        failure: coven::StatusFailure::Storage(F::Network),
        error: "refused".into(),
    }));
    assert!(records_failure(&refused, held));
}

#[test]
fn a_slot_record_of_the_reason_shows_a_blocked_pass() {
    use coven::StorageFailure as F;
    let blocked = |reason| {
        coven::SyncState::StorageFull(coven::SyncCause::Pass(coven::LastPass::Blocked {
            slot: coven::Slot::new(3).unwrap(),
            reason,
            error: Some("full".into()),
        }))
    };
    let record = coven::PendingRecord::new(
        coven::PendingSubject::Slot(coven::Slot::new(3).unwrap()),
        storage_reason(F::QuotaExceeded),
    );
    let held = Some(("slot", "storage", &record));
    assert!(records_failure(&blocked(storage_reason(F::QuotaExceeded)), held));
    assert!(!records_failure(&blocked(storage_reason(F::Protocol)), held));
}

#[test]
fn an_operation_record_of_the_failed_attempt_shows_a_failed_pass() {
    use coven::StorageFailure as F;
    let operation = |kind| coven::PendingSubject::Operation { id: coven::OperationId(5), kind };
    let record = coven::PendingRecord::new(
        operation(coven::OperationKind::Invite),
        storage_reason(F::Network),
    );
    let failed = |record: &coven::PendingRecord| {
        coven::SyncState::Failed(coven::SyncCause::Pass(coven::LastPass::Failed {
            failure: coven::StatusFailure::Operation(record.clone()),
            error: "the invite failed".into(),
        }))
    };
    let held = Some(("operation", "storage", &record));
    assert!(records_failure(&failed(&record), held));
    let other_reason = coven::PendingRecord::new(
        operation(coven::OperationKind::Invite),
        storage_reason(F::Protocol),
    );
    assert!(!records_failure(&failed(&other_reason), held));
}

#[test]
fn a_record_of_a_removal_shows_a_removed_device() {
    let nothing_unsent = coven::UnsentWork { writes: 0, files: 0 };
    let removed =
        coven::SyncState::Removed { removal: coven::Removal::Device, unsent: nothing_unsent };
    let record = coven::PendingRecord::new(
        coven::PendingSubject::Connection,
        coven::PendingReason::Removed(coven::Removal::Device),
    );
    assert!(records_failure(&removed, Some(("connection", "removed", &record))));
    assert!(!records_failure(&removed, None));
}
