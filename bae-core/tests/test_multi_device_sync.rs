#![cfg(feature = "test-utils")]
//! End-to-end "macOS → cloud → device" sync round-trip on bae's real catalog
//! schema, driven through coven's `run_single_sync_cycle` against a shared
//! in-memory cloud.
//!
//! Device A creates a *managed* album/release/tracks and runs a sync cycle that
//! pushes the catalog to the cloud. Device B (a fresh library) runs a sync cycle
//! that pulls it. B must end up with A's managed release and its tracks — the
//! property the empty-library-after-import bug violated.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use async_trait::async_trait;
use bae_core::clock::SystemClock;
use bae_core::db::Database;
use bae_core::storage::cloud::{CloudHome, CloudHomeError, CloudHomeJoinInfo, UploadProgress};

use coven::blob::{BlobPlan, BlobRef};
use coven::changeset::RowChange;
use coven::encryption::EncryptionService;
use coven::keys::UserKeypair;
use coven::library_dir::LibraryDir;
use coven::sync::cloud_storage::{BlobPathScheme, CloudCipher, CloudSyncStorage};
use coven::sync::cycle::run_single_sync_cycle;
use coven::sync::hlc::Hlc;

/// An in-memory `CloudHome` whose state is shared across clones, so two devices
/// can talk to the same "cloud".
#[derive(Clone)]
struct SharedCloud {
    blobs: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl SharedCloud {
    fn new() -> Self {
        Self {
            blobs: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl CloudHome for SharedCloud {
    async fn write(
        &self,
        key: &str,
        data: Vec<u8>,
        progress: &UploadProgress<'_>,
    ) -> Result<(), CloudHomeError> {
        let total = data.len() as u64;
        self.blobs.lock().unwrap().insert(key.to_string(), data);
        progress(total);
        Ok(())
    }

    async fn read(&self, key: &str) -> Result<Vec<u8>, CloudHomeError> {
        self.blobs
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(|| CloudHomeError::NotFound(key.to_string()))
    }

    async fn read_range(&self, key: &str, start: u64, end: u64) -> Result<Vec<u8>, CloudHomeError> {
        let blobs = self.blobs.lock().unwrap();
        let data = blobs
            .get(key)
            .ok_or_else(|| CloudHomeError::NotFound(key.to_string()))?;
        let (s, e) = (start as usize, (end as usize).min(data.len()));
        Ok(data.get(s..e).unwrap_or(&[]).to_vec())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, CloudHomeError> {
        Ok(self
            .blobs
            .lock()
            .unwrap()
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }

    async fn delete(&self, key: &str) -> Result<(), CloudHomeError> {
        self.blobs.lock().unwrap().remove(key);
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, CloudHomeError> {
        Ok(self.blobs.lock().unwrap().contains_key(key))
    }

    async fn grant_access(&self, _: &str) -> Result<CloudHomeJoinInfo, CloudHomeError> {
        unimplemented!("not used by this changeset-pull test")
    }

    async fn revoke_access(&self, _: &str) -> Result<(), CloudHomeError> {
        unimplemented!()
    }
}

/// No blobs in this test — the catalog (rows) is what must cross, not audio.
struct NoopBlobPlan;
impl BlobPlan for NoopBlobPlan {
    fn blobs_to_push(&self, _: &[RowChange]) -> Vec<BlobRef> {
        Vec::new()
    }
    fn blobs_to_pull(&self, _: &[RowChange]) -> Vec<BlobRef> {
        Vec::new()
    }
    fn blobs_in_db(
        &self,
        _: &coven::rusqlite::Connection,
    ) -> coven::rusqlite::Result<Vec<BlobRef>> {
        Ok(Vec::new())
    }
}

async fn exec(db: &Database, sql: &str) {
    let sql = sql.to_string();
    db.coven_db()
        .call(move |conn| {
            conn.execute_batch(&sql)
                .map_err(|e| coven::database::DbError(e.to_string()))
        })
        .await
        .expect("exec");
}

async fn count(db: &Database, sql: &str) -> i64 {
    let sql = sql.to_string();
    db.coven_db()
        .call(move |conn| {
            conn.query_row(&sql, [], |r| r.get::<_, i64>(0))
                .map_err(|e| coven::database::DbError(e.to_string()))
        })
        .await
        .expect("count")
}

/// Insert a managed album/release/tracks (a 2-track release marked managed=1).
async fn insert_managed_catalog(db: &Database) {
    exec(
        db,
        "INSERT INTO artists (id, name, _updated_at, created_at) \
         VALUES ('ar1', 'Artist Name', '0000000001000-0000-test-device', '2026-01-01');\n\
         INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at) \
         VALUES ('al1', 'Album Title', 'ar1', 0, '0000000001001-0000-test-device', '2026-01-01');\n\
         INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at) \
         VALUES ('aa1', 'al1', 'ar1', 0, '0000000001002-0000-test-device', '2026-01-01');\n\
         INSERT INTO releases (id, album_id, metadata_source, managed, _updated_at, created_at) \
         VALUES ('re1', 'al1', 'file_tags', 1, '0000000001003-0000-test-device', '2026-01-01');\n\
         INSERT INTO tracks (id, release_id, title, side, _updated_at, created_at) \
         VALUES ('tr1', 're1', 'Track One', 1, '0000000001004-0000-test-device', '2026-01-01');\n\
         INSERT INTO tracks (id, release_id, title, side, _updated_at, created_at) \
         VALUES ('tr2', 're1', 'Track Two', 1, '0000000001005-0000-test-device', '2026-01-01');",
    )
    .await;
}

/// Whether the cloud holds any changeset object for `device_id`.
fn cloud_has_changeset(cloud: &SharedCloud, device_id: &str) -> bool {
    let prefix = format!("changes/{device_id}/");
    cloud
        .blobs
        .lock()
        .unwrap()
        .keys()
        .any(|k| k.starts_with(&prefix))
}

/// Run one sync cycle for `device_id` over `storage` (no cloud_home → no outbox
/// processing; no blobs).
async fn sync_cycle(db: &Database, storage: &CloudSyncStorage, device_id: &str, lib: &LibraryDir) {
    let hlc = Hlc::new(device_id.to_string());
    let cipher = RwLock::new(CloudCipher::Encrypted(EncryptionService::new_with_key(
        &[9u8; 32],
    )));
    let keypair = UserKeypair::generate();
    run_single_sync_cycle(
        storage,
        device_id,
        &hlc,
        &SystemClock,
        db.coven_db(),
        &cipher,
        &keypair,
        lib,
        None,
        &NoopBlobPlan,
        None,
    )
    .await
    .expect("sync cycle");
}

#[tokio::test]
async fn managed_catalog_pushed_by_device_a_reaches_device_b() {
    let cloud = SharedCloud::new();
    let key = [9u8; 32];

    // Device A: a fresh library with a managed album/release/tracks.
    let tmp_a = tempfile::tempdir().unwrap();
    let lib_a = LibraryDir::new(tmp_a.path().join("a"));
    std::fs::create_dir_all(&*lib_a).unwrap();
    let db_a = Database::new_test(lib_a.db_path().to_str().unwrap(), Arc::new(SystemClock))
        .await
        .unwrap();
    let storage_a = CloudSyncStorage::new(
        Box::new(cloud.clone()),
        CloudCipher::Encrypted(EncryptionService::new_with_key(&key)),
        BlobPathScheme::Hashed,
    );

    insert_managed_catalog(&db_a).await;

    // A syncs: the managed release flips into the gated set, so its whole subtree
    // is pushed to the cloud.
    sync_cycle(&db_a, &storage_a, "device-a", &lib_a).await;

    // Device B: a fresh empty library on the same cloud.
    let tmp_b = tempfile::tempdir().unwrap();
    let lib_b = LibraryDir::new(tmp_b.path().join("b"));
    std::fs::create_dir_all(&*lib_b).unwrap();
    let db_b = Database::new_test(lib_b.db_path().to_str().unwrap(), Arc::new(SystemClock))
        .await
        .unwrap();
    let storage_b = CloudSyncStorage::new(
        Box::new(cloud.clone()),
        CloudCipher::Encrypted(EncryptionService::new_with_key(&key)),
        BlobPathScheme::Hashed,
    );
    assert_eq!(
        count(&db_b, "SELECT count(*) FROM releases").await,
        0,
        "device B starts empty"
    );

    // B syncs: it pulls A's catalog changeset.
    sync_cycle(&db_b, &storage_b, "device-b", &lib_b).await;

    // B must now hold A's managed release, its album/artist, and both tracks.
    assert_eq!(
        count(
            &db_b,
            "SELECT count(*) FROM releases WHERE id='re1' AND managed=1"
        )
        .await,
        1,
        "device B must receive device A's managed release",
    );
    assert_eq!(
        count(&db_b, "SELECT count(*) FROM albums WHERE id='al1'").await,
        1,
        "device B must receive the album",
    );
    assert_eq!(
        count(&db_b, "SELECT count(*) FROM tracks WHERE release_id='re1'").await,
        2,
        "device B must receive the release's tracks",
    );
}

/// Insert two albums/releases, each landed `managed = 0` — the shape of "import
/// two releases, their audio still uploading". A release reaches `managed = 1`
/// only once its own audio uploads (the upload observer flips it); here the test
/// performs that flip directly, since this exercises sync propagation, not the
/// observer.
async fn insert_two_uploading_releases(db: &Database) {
    exec(
        db,
        "INSERT INTO artists (id, name, _updated_at, created_at) \
         VALUES ('ar1', 'Artist One', '0000000001000-0000-test-device', '2026-01-01');\n\
         INSERT INTO artists (id, name, _updated_at, created_at) \
         VALUES ('ar2', 'Artist Two', '0000000001001-0000-test-device', '2026-01-01');\n\
         INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at) \
         VALUES ('al1', 'Album One', 'ar1', 0, '0000000001002-0000-test-device', '2026-01-01');\n\
         INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at) \
         VALUES ('al2', 'Album Two', 'ar2', 0, '0000000001003-0000-test-device', '2026-01-01');\n\
         INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at) \
         VALUES ('aa1', 'al1', 'ar1', 0, '0000000001004-0000-test-device', '2026-01-01');\n\
         INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at) \
         VALUES ('aa2', 'al2', 'ar2', 0, '0000000001005-0000-test-device', '2026-01-01');\n\
         INSERT INTO releases (id, album_id, metadata_source, managed, _updated_at, created_at) \
         VALUES ('re1', 'al1', 'file_tags', 0, '0000000001006-0000-test-device', '2026-01-01');\n\
         INSERT INTO releases (id, album_id, metadata_source, managed, _updated_at, created_at) \
         VALUES ('re2', 'al2', 'file_tags', 0, '0000000001007-0000-test-device', '2026-01-01');\n\
         INSERT INTO tracks (id, release_id, title, side, _updated_at, created_at) \
         VALUES ('tr1', 're1', 'Track One', 1, '0000000001008-0000-test-device', '2026-01-01');\n\
         INSERT INTO tracks (id, release_id, title, side, _updated_at, created_at) \
         VALUES ('tr2', 're2', 'Track Two', 1, '0000000001009-0000-test-device', '2026-01-01');",
    )
    .await;
}

/// Each release's metadata propagates as its own audio finishes — not batched
/// behind the slowest upload. Two releases land `managed = 0` (audio uploading);
/// `re1` flips `managed = 1` first (its upload finished) and must reach device B
/// while `re2` is still uploading; `re2` reaches B only once it flips too.
#[tokio::test]
async fn each_release_propagates_when_its_own_upload_flips_managed() {
    let cloud = SharedCloud::new();
    let key = [9u8; 32];

    let tmp_a = tempfile::tempdir().unwrap();
    let lib_a = LibraryDir::new(tmp_a.path().join("a"));
    std::fs::create_dir_all(&*lib_a).unwrap();
    let db_a = Database::new_test(lib_a.db_path().to_str().unwrap(), Arc::new(SystemClock))
        .await
        .unwrap();
    let storage_a = CloudSyncStorage::new(
        Box::new(cloud.clone()),
        CloudCipher::Encrypted(EncryptionService::new_with_key(&key)),
        BlobPathScheme::Hashed,
    );

    insert_two_uploading_releases(&db_a).await;

    // Cycle 1: both releases are still `managed = 0` (audio uploading), so the
    // gate cuts them — nothing reaches the cloud.
    sync_cycle(&db_a, &storage_a, "device-a", &lib_a).await;
    assert!(
        !cloud_has_changeset(&cloud, "device-a"),
        "a release whose audio isn't uploaded yet (managed=0) is gated out",
    );

    // re1's audio finishes; the observer flips it managed (cloud-only — no local
    // copy), through the same DB transition the live observer calls. re2 is still
    // uploading.
    db_a.set_release_managed_cloud_only("re1").await.unwrap();
    sync_cycle(&db_a, &storage_a, "device-a", &lib_a).await;

    // Device B pulls: it gets re1 (and its album/artist/track) but NOT re2 —
    // re1 reached B without waiting for re2's upload.
    let tmp_b = tempfile::tempdir().unwrap();
    let lib_b = LibraryDir::new(tmp_b.path().join("b"));
    std::fs::create_dir_all(&*lib_b).unwrap();
    let db_b = Database::new_test(lib_b.db_path().to_str().unwrap(), Arc::new(SystemClock))
        .await
        .unwrap();
    let storage_b = CloudSyncStorage::new(
        Box::new(cloud.clone()),
        CloudCipher::Encrypted(EncryptionService::new_with_key(&key)),
        BlobPathScheme::Hashed,
    );
    sync_cycle(&db_b, &storage_b, "device-b", &lib_b).await;
    assert_eq!(
        count(
            &db_b,
            "SELECT count(*) FROM releases WHERE id='re1' AND managed=1"
        )
        .await,
        1,
        "re1 must reach device B as soon as its own upload flips it managed",
    );
    assert_eq!(
        count(&db_b, "SELECT count(*) FROM releases WHERE id='re2'").await,
        0,
        "re2 must NOT reach B while it is still uploading (managed=0)",
    );

    // re2's audio finishes and flips it managed; now it reaches B too.
    db_a.set_release_managed_cloud_only("re2").await.unwrap();
    sync_cycle(&db_a, &storage_a, "device-a", &lib_a).await;
    sync_cycle(&db_b, &storage_b, "device-b", &lib_b).await;
    assert_eq!(
        count(
            &db_b,
            "SELECT count(*) FROM releases WHERE id='re2' AND managed=1"
        )
        .await,
        1,
        "re2 reaches B once its own upload flips it managed",
    );
    assert_eq!(
        count(&db_b, "SELECT count(*) FROM tracks").await,
        2,
        "device B ends with both releases' tracks",
    );
}
