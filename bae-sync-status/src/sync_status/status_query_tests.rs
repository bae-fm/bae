//! The status query reads the sync status in one row.

use super::*;
use bae_store::test_fixtures::temp_db;

#[tokio::test]
async fn a_library_with_nothing_recorded_reads_as_not_synced() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let status = db.read(|sql| sync_status_on(&sql)).await.unwrap();
    assert_eq!(status.coven.state, coven::SyncState::NotSynced);
    assert_eq!(status.coven.last_synced, None);
    assert_eq!(status.error, None);
    assert_eq!(status.storage_setup, None);
    assert_eq!(status.retry_effect, coven::RetryEffect::NotSyncing);
}

/// A bare library whose storage is connected, so that the sync state follows
/// its last pass and pending records.
fn connected_library() -> rusqlite::Connection {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    crate::test_fixtures::TEST_SCHEMA.open_bare_library(&connection);
    coven::record_connected_storage(
        &connection,
        coven::StorageConfig::S3 {
            bucket: "library".into(),
            region: "us-east-1".into(),
            endpoint: None,
            profile: None,
        },
    )
    .unwrap();
    connection
}

/// The status the statement gives `connection`.
fn status_of(connection: &rusqlite::Connection) -> SyncStatusSnapshot {
    snapshot_of(connection.query_row(&status_query(), [], StatusColumns::read).unwrap()).unwrap()
}

/// A pass that failed on coven's own operation reads as a failed state,
/// with the operation's failure as sync's.
#[test]
fn a_failed_operation_reads_as_a_failed_pass() {
    let connection = connected_library();
    let invite = coven::record_invite(
        &connection,
        coven::RecordedInvite {
            id: coven::InviteId(uuid::Uuid::from_bytes([1; 16])),
            member: coven::MemberId(uuid::Uuid::from_bytes([101; 16])),
            stage: coven::RecordedInviteStage::Granting,
        },
    )
    .unwrap();
    let record = coven::PendingRecord::new(
        coven::PendingSubject::Operation { id: invite, kind: coven::OperationKind::Invite },
        coven::PendingReason::NoStorage,
    );
    coven::record_pending(&connection, &record).unwrap();
    let failed = |record: &coven::PendingRecord| {
        coven::LastPass::failed(coven::SyncFailure::Operation(Box::new(record.clone())))
    };
    coven::record_last_pass(&connection, &failed(&record)).unwrap();
    let status = status_of(&connection);
    assert!(matches!(status.coven.state, coven::SyncState::Failed(_)));
    assert!(status.failure().is_some());
}

/// A record of this device's removal reads as a removed device, with the
/// removal as sync's failure.
#[test]
fn a_record_of_a_removal_reads_as_a_removed_device() {
    let connection = connected_library();
    let record = coven::PendingRecord::new(
        coven::PendingSubject::Connection,
        coven::PendingReason::Removed(coven::Removal::Device),
    );
    coven::record_pending(&connection, &record).unwrap();
    coven::record_last_pass(
        &connection,
        &coven::LastPass::failed(coven::SyncFailure::Removed(coven::Removal::Device)),
    )
    .unwrap();
    let status = status_of(&connection);
    assert!(matches!(status.coven.state, coven::SyncState::Removed { .. }));
    assert!(status.failure().is_some());
}

/// A record of the member's keys another installation replaced reads as the
/// reset the state needs, with that as sync's failure.
#[test]
fn a_record_of_replaced_member_keys_reads_as_the_reset_the_state_needs() {
    let connection = connected_library();
    let record = coven::PendingRecord::new(
        coven::PendingSubject::Slot(coven::Slot::new(3).unwrap()),
        coven::PendingReason::MemberKeysReplaced {
            member: coven::MemberId(uuid::Uuid::from_bytes([5; 16])),
        },
    );
    coven::record_pending(&connection, &record).unwrap();
    let status = status_of(&connection);
    assert!(matches!(status.coven.state, coven::SyncState::NeedsReset { .. }));
    assert!(status.failure().is_some());
}
