#![cfg(feature = "test-utils")]
//! Integration tests for the cloud outbox (upload/delete queueing and processing).

mod support;

use async_trait::async_trait;
use bae_core::clock::SystemClock;
use bae_core::db::Database;
use bae_core::encryption::EncryptionService;
use bae_core::storage::cloud::{CloudHome, CloudHomeError, CloudHomeJoinInfo};
use bae_core::sync::outbox;
use std::sync::{Mutex, RwLock};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Mock CloudHome
// ---------------------------------------------------------------------------

/// Records writes and deletes for assertion. Can be configured to fail.
struct MockCloudHome {
    writes: Mutex<Vec<(String, Vec<u8>)>>,
    deletes: Mutex<Vec<String>>,
    fail_writes: bool,
    fail_deletes: bool,
}

impl MockCloudHome {
    fn new() -> Self {
        Self {
            writes: Mutex::new(Vec::new()),
            deletes: Mutex::new(Vec::new()),
            fail_writes: false,
            fail_deletes: false,
        }
    }

    fn failing_writes() -> Self {
        Self {
            writes: Mutex::new(Vec::new()),
            deletes: Mutex::new(Vec::new()),
            fail_writes: true,
            fail_deletes: false,
        }
    }

    fn failing_deletes() -> Self {
        Self {
            writes: Mutex::new(Vec::new()),
            deletes: Mutex::new(Vec::new()),
            fail_writes: false,
            fail_deletes: true,
        }
    }
}

#[async_trait]
impl CloudHome for MockCloudHome {
    async fn write(
        &self,
        key: &str,
        data: Vec<u8>,
        progress: &bae_core::storage::cloud::UploadProgress<'_>,
    ) -> Result<(), CloudHomeError> {
        if self.fail_writes {
            return Err(CloudHomeError::Storage("mock write failure".into()));
        }
        let total = data.len() as u64;
        self.writes.lock().unwrap().push((key.to_string(), data));
        progress(total);
        Ok(())
    }

    async fn read(&self, _key: &str) -> Result<Vec<u8>, CloudHomeError> {
        unimplemented!()
    }

    async fn read_range(&self, _: &str, _: u64, _: u64) -> Result<Vec<u8>, CloudHomeError> {
        unimplemented!()
    }

    async fn list(&self, _: &str) -> Result<Vec<String>, CloudHomeError> {
        unimplemented!()
    }

    async fn delete(&self, key: &str) -> Result<(), CloudHomeError> {
        if self.fail_deletes {
            return Err(CloudHomeError::Storage("mock delete failure".into()));
        }
        self.deletes.lock().unwrap().push(key.to_string());
        Ok(())
    }

    async fn exists(&self, _: &str) -> Result<bool, CloudHomeError> {
        unimplemented!()
    }

    async fn grant_access(&self, _: &str) -> Result<CloudHomeJoinInfo, CloudHomeError> {
        unimplemented!()
    }

    async fn revoke_access(&self, _: &str) -> Result<(), CloudHomeError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn setup_db() -> (Database, TempDir) {
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().join("test.db");
    let db = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(bae_core::clock::SystemClock),
    )
    .await
    .expect("create database");
    (db, tmp)
}

// ---------------------------------------------------------------------------
// DB method tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_add_and_get_pending_uploads_fifo() {
    let (db, _tmp) = setup_db().await;

    db.add_cloud_outbox_upload("file-aaa", "storage/aa/bb/file-aaa", None)
        .await
        .unwrap();
    db.add_cloud_outbox_upload("file-bbb", "storage/bb/cc/file-bbb", None)
        .await
        .unwrap();
    db.add_cloud_outbox_upload("file-ccc", "storage/cc/dd/file-ccc", None)
        .await
        .unwrap();

    let uploads = db.get_pending_cloud_uploads().await.unwrap();
    assert_eq!(uploads.len(), 3);
    assert_eq!(upload_file_id(&uploads[0]), "file-aaa");
    assert_eq!(upload_file_id(&uploads[1]), "file-bbb");
    assert_eq!(upload_file_id(&uploads[2]), "file-ccc");
}

/// The `file_id` an upload entry carries, panicking if the entry is a delete.
/// coven nests it inside `OutboxOperation::Upload`; the tests assert FIFO order
/// by it.
fn upload_file_id(entry: &coven::db::OutboxEntry) -> &str {
    match &entry.operation {
        coven::db::OutboxOperation::Upload { file_id, .. } => file_id,
        coven::db::OutboxOperation::Delete => panic!("expected an upload entry"),
    }
}

/// The master scope round-trips through the outbox: enqueue an upload, read back
/// the `BlobScope::Master` on the `OutboxEntry`. coven encrypts with the library
/// master key at drain, so a lost or wrong scope would silently mis-encrypt.
/// Deletes carry no scope.
#[tokio::test]
async fn test_outbox_upload_round_trips_master_scope() {
    use coven::blob::BlobScope;
    use coven::db::OutboxOperation;

    let (db, _tmp) = setup_db().await;

    db.add_cloud_outbox_upload("file-k", "storage/aa/bb/file-k", None)
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
        coven::db::OutboxOperation::Delete
    ));
    assert_eq!(deletes[1].cloud_key, "storage/bb/cc/file-bbb");
    assert!(matches!(
        deletes[1].operation,
        coven::db::OutboxOperation::Delete
    ));
}

#[tokio::test]
async fn test_remove_outbox_entry() {
    let (db, _tmp) = setup_db().await;

    db.add_cloud_outbox_upload("file-aaa", "storage/aa/bb/file-aaa", None)
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

    db.add_cloud_outbox_upload("file-aaa", "storage/aa/bb/file-aaa", None)
        .await
        .unwrap();
    db.add_cloud_outbox_upload("file-bbb", "storage/bb/cc/file-bbb", None)
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

    db.add_cloud_outbox_upload("file-aaa", "storage/aa/bb/file-aaa", None)
        .await
        .unwrap();

    // Same key again — should not error (INSERT OR IGNORE)
    db.add_cloud_outbox_upload("file-aaa", "storage/aa/bb/file-aaa", None)
        .await
        .unwrap();

    let uploads = db.get_pending_cloud_uploads().await.unwrap();
    assert_eq!(uploads.len(), 1, "duplicate insert should be ignored");
}

// ---------------------------------------------------------------------------
// process_uploads tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_process_uploads_success() {
    let (db, tmp) = setup_db().await;
    let library_dir = tmp.path().to_path_buf();

    // Write a local file that the uploader will read
    let file_id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    let cloud_key = bae_core::storage::local::storage_path(file_id);
    let local_path = library_dir.join(&cloud_key);
    std::fs::create_dir_all(local_path.parent().unwrap()).unwrap();
    std::fs::write(&local_path, b"hello world").unwrap();

    // The upload encrypts with the library master key (`BlobScope::Master`).
    db.add_cloud_outbox_upload(file_id, &cloud_key, None)
        .await
        .unwrap();

    let cloud = MockCloudHome::new();
    let enc = RwLock::new(EncryptionService::new_with_key(&[0u8; 32]));

    let count = outbox::process_uploads(
        db.coven_db(),
        &cloud,
        &enc,
        &library_dir,
        &SystemClock,
        None,
    )
    .await
    .unwrap()
    .uploaded;

    assert_eq!(count, 1);

    // Cloud should have received one encrypted write
    {
        let writes = cloud.writes.lock().unwrap();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].0, cloud_key);
        // Data should be encrypted (not plaintext)
        assert_ne!(writes[0].1, b"hello world");
    }

    // Outbox should be empty
    assert!(db.get_pending_cloud_uploads().await.unwrap().is_empty());
}

#[tokio::test]
async fn test_process_uploads_failure_retains_entry() {
    let (db, tmp) = setup_db().await;
    let library_dir = tmp.path().to_path_buf();

    let file_id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    let cloud_key = bae_core::storage::local::storage_path(file_id);
    let local_path = library_dir.join(&cloud_key);
    std::fs::create_dir_all(local_path.parent().unwrap()).unwrap();
    std::fs::write(&local_path, b"hello world").unwrap();

    db.add_cloud_outbox_upload(file_id, &cloud_key, None)
        .await
        .unwrap();

    let cloud = MockCloudHome::failing_writes();
    let enc = RwLock::new(EncryptionService::new_with_key(&[0u8; 32]));

    let count = outbox::process_uploads(
        db.coven_db(),
        &cloud,
        &enc,
        &library_dir,
        &SystemClock,
        None,
    )
    .await
    .unwrap()
    .uploaded;

    assert_eq!(count, 0, "no uploads should succeed");

    // Entry should still be in the outbox
    assert!(!db.get_pending_cloud_uploads().await.unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// process_deletes tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_process_deletes_removes_queued_blob() {
    let (db, _tmp) = setup_db().await;

    db.add_cloud_outbox_delete("storage/aa/bb/file-aaa")
        .await
        .unwrap();

    let cloud = MockCloudHome::new();

    // The delete drains as soon as the cloud is reachable — no wait on your other devices.
    let count = outbox::process_deletes(db.coven_db(), &cloud)
        .await
        .unwrap();

    assert_eq!(count, 1);

    {
        let deletes = cloud.deletes.lock().unwrap();
        assert_eq!(deletes.len(), 1);
        assert_eq!(deletes[0], "storage/aa/bb/file-aaa");
    }

    // Outbox should be empty
    let remaining = db.get_pending_cloud_deletes().await.unwrap();
    assert!(remaining.is_empty());
}

#[tokio::test]
async fn test_process_deletes_failure_retains_entry() {
    let (db, _tmp) = setup_db().await;

    db.add_cloud_outbox_delete("storage/aa/bb/file-aaa")
        .await
        .unwrap();

    let cloud = MockCloudHome::failing_deletes();

    let count = outbox::process_deletes(db.coven_db(), &cloud)
        .await
        .unwrap();

    assert_eq!(count, 0, "failed delete should not count");

    // Entry should still be in the outbox
    let remaining = db.get_pending_cloud_deletes().await.unwrap();
    assert_eq!(remaining.len(), 1);
}
