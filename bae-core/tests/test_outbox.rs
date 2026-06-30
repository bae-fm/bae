#![cfg(feature = "test-utils")]
//! Integration tests for bae's cloud-outbox DB wrappers (`db/client.rs`): the
//! delete enqueue and the remove/cancel seam bae keeps over coven's
//! `cloud_outbox`, plus the `BlobScope::Master` default bae stamps when it seeds
//! an upload. Pending-queue reads go through bae's database wrapper over coven's
//! SQL context. The drains themselves (coven's upload
//! drain and the tombstone delete path) are coven's and covered by coven's own
//! blob tests.

use bae_core::db::Database;
use tempfile::TempDir;

async fn setup_db() -> (Database, TempDir) {
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().join("test.db");
    let db = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .expect("create database");
    (db, tmp)
}

/// The `file_id` an upload entry carries, panicking if the entry is not an
/// upload. coven nests it inside `OutboxOperation::Upload`; the tests assert FIFO
/// order by it.
fn upload_file_id(entry: &coven::OutboxEntry) -> &str {
    match &entry.operation {
        coven::OutboxOperation::Upload { file_id, .. } => file_id,
        other => panic!("expected an upload entry, got {other:?}"),
    }
}

#[tokio::test]
async fn test_add_and_get_pending_uploads_fifo() {
    let (db, _tmp) = setup_db().await;

    db.add_cloud_outbox_upload("file-aaa", "storage/aa/bb/file-aaa", None, false)
        .await
        .unwrap();
    db.add_cloud_outbox_upload("file-bbb", "storage/bb/cc/file-bbb", None, false)
        .await
        .unwrap();
    db.add_cloud_outbox_upload("file-ccc", "storage/cc/dd/file-ccc", None, false)
        .await
        .unwrap();

    let uploads = db.get_pending_cloud_uploads().await.unwrap();
    assert_eq!(uploads.len(), 3);
    assert_eq!(upload_file_id(&uploads[0]), "file-aaa");
    assert_eq!(upload_file_id(&uploads[1]), "file-bbb");
    assert_eq!(upload_file_id(&uploads[2]), "file-ccc");
}

/// The master scope round-trips through the outbox: bae's `add_cloud_outbox_upload`
/// stamps `BlobScope::Master` at enqueue, read back on the `OutboxEntry`. coven
/// encrypts with the library master key at drain, so a lost or wrong scope would
/// silently mis-encrypt. Deletes carry no scope.
#[tokio::test]
async fn test_outbox_upload_round_trips_master_scope() {
    use coven::BlobScope;
    use coven::OutboxOperation;

    let (db, _tmp) = setup_db().await;

    db.add_cloud_outbox_upload("file-k", "storage/aa/bb/file-k", None, false)
        .await
        .unwrap();

    let uploads = db.get_pending_cloud_uploads().await.unwrap();
    assert_eq!(uploads.len(), 1);
    let OutboxOperation::Upload { scope, .. } = &uploads[0].operation else {
        panic!("expected an upload operation");
    };
    assert_eq!(
        *scope,
        BlobScope::Master,
        "the master scope the drain encrypts under round-trips through the outbox"
    );

    db.add_cloud_outbox_delete("storage/aa/bb/file-k")
        .await
        .unwrap();
    let deletes = db.get_pending_cloud_deletes().await.unwrap();
    assert_eq!(deletes.len(), 1);
    assert!(
        matches!(deletes[0].operation, OutboxOperation::Delete),
        "a delete carries no encryption scope"
    );
}

#[tokio::test]
async fn test_add_and_get_pending_deletes() {
    let (db, _tmp) = setup_db().await;

    db.add_cloud_outbox_delete("storage/aa/bb/file-aaa")
        .await
        .unwrap();
    db.add_cloud_outbox_delete("storage/bb/cc/file-bbb")
        .await
        .unwrap();

    let deletes = db.get_pending_cloud_deletes().await.unwrap();
    assert_eq!(deletes.len(), 2);
    assert_eq!(deletes[0].cloud_key, "storage/aa/bb/file-aaa");
    assert!(matches!(
        deletes[0].operation,
        coven::OutboxOperation::Delete
    ));
    assert_eq!(deletes[1].cloud_key, "storage/bb/cc/file-bbb");
    assert!(matches!(
        deletes[1].operation,
        coven::OutboxOperation::Delete
    ));
}

#[tokio::test]
async fn test_remove_outbox_entry() {
    let (db, _tmp) = setup_db().await;

    db.add_cloud_outbox_upload("file-aaa", "storage/aa/bb/file-aaa", None, false)
        .await
        .unwrap();

    let uploads = db.get_pending_cloud_uploads().await.unwrap();
    assert_eq!(uploads.len(), 1);
    let id = uploads[0].id;

    db.remove_cloud_outbox_entry(id).await.unwrap();

    let uploads = db.get_pending_cloud_uploads().await.unwrap();
    assert!(uploads.is_empty());
}

#[tokio::test]
async fn test_remove_uploads_for_key() {
    let (db, _tmp) = setup_db().await;

    db.add_cloud_outbox_upload("file-aaa", "storage/aa/bb/file-aaa", None, false)
        .await
        .unwrap();
    db.add_cloud_outbox_upload("file-bbb", "storage/bb/cc/file-bbb", None, false)
        .await
        .unwrap();

    db.remove_cloud_outbox_uploads_for_key("storage/aa/bb/file-aaa")
        .await
        .unwrap();

    let uploads = db.get_pending_cloud_uploads().await.unwrap();
    assert_eq!(uploads.len(), 1);
    assert_eq!(upload_file_id(&uploads[0]), "file-bbb");
}

#[tokio::test]
async fn test_insert_or_ignore_idempotency() {
    let (db, _tmp) = setup_db().await;

    db.add_cloud_outbox_upload("file-aaa", "storage/aa/bb/file-aaa", None, false)
        .await
        .unwrap();

    // Same key again — should not error (INSERT OR IGNORE)
    db.add_cloud_outbox_upload("file-aaa", "storage/aa/bb/file-aaa", None, false)
        .await
        .unwrap();

    let uploads = db.get_pending_cloud_uploads().await.unwrap();
    assert_eq!(uploads.len(), 1, "duplicate insert should be ignored");
}

/// The UI snapshot query reads only upload/delete rows. coven's upload drain
/// queues `cancel` rows for tombstone removal; `outbox_items` must skip them, not
/// render an operation the snapshot doesn't display. The `WHERE IN
/// ('upload','delete')` filter keeps cancel rows out of the result.
#[tokio::test]
async fn test_outbox_items_skips_tombstone_cancels() {
    let (db, _tmp) = setup_db().await;

    db.add_cloud_outbox_upload("file-aaa", "storage/aa/bb/file-aaa", None, false)
        .await
        .unwrap();
    db.add_cloud_outbox_delete("storage/bb/cc/file-bbb")
        .await
        .unwrap();
    db.add_cloud_outbox_cancel("storage/cc/dd/file-ccc")
        .await
        .unwrap();

    let items = db.outbox_items().await.unwrap();
    assert_eq!(
        items.len(),
        2,
        "cancel row must be excluded from the snapshot"
    );
    assert!(
        items
            .iter()
            .all(|r| r.cloud_key != "storage/cc/dd/file-ccc"),
        "no cancel row should reach the snapshot"
    );
}
