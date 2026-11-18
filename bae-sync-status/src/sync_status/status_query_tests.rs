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
