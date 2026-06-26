#![cfg(feature = "test-utils")]
//! End-to-end "macOS → cloud → device" sync round-trip on bae's real catalog
//! schema, driven through coven's `run_single_sync_cycle` against a shared
//! in-memory cloud.
//!
//! Device A creates a *remote* album/release/tracks and runs a sync cycle that
//! pushes the catalog to the cloud. Device B (a fresh library) runs a sync cycle
//! that pulls it. B must end up with A's remote release and its tracks — the
//! property the empty-library-after-import bug violated.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use async_trait::async_trait;
use bae_core::clock::SystemClock;
use bae_core::db::Database;
use bae_core::storage::cloud::{CloudHome, CloudHomeError, CloudHomeJoinInfo, UploadProgress};

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

/// Insert a remote album/release/tracks (a 2-track release marked remote=1).
async fn insert_remote_catalog(db: &Database) {
    exec(
        db,
        "INSERT INTO artists (id, name, _updated_at, created_at) \
         VALUES ('ar1', 'Artist Name', '0000000001000-0000-test-device', '2026-01-01');\n\
         INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at) \
         VALUES ('al1', 'Album Title', 'ar1', 0, '0000000001001-0000-test-device', '2026-01-01');\n\
         INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at) \
         VALUES ('aa1', 'al1', 'ar1', 0, '0000000001002-0000-test-device', '2026-01-01');\n\
         INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at) \
         VALUES ('re1', 'al1', 'file_tags', 1, '0000000001003-0000-test-device', '2026-01-01');\n\
         INSERT INTO tracks (id, release_id, title, side, _updated_at, created_at) \
         VALUES ('tr1', 're1', 'Track One', 1, '0000000001004-0000-test-device', '2026-01-01');\n\
         INSERT INTO tracks (id, release_id, title, side, _updated_at, created_at) \
         VALUES ('tr2', 're1', 'Track Two', 1, '0000000001005-0000-test-device', '2026-01-01');",
    )
    .await;
}

/// Insert a `covers` row (a host-provided cover asset) for `release_id`. Its `id`
/// IS the release id (the FK to `releases`), so the cover rides that release's gate.
async fn insert_cover(db: &Database, release_id: &str, stamp: &str) {
    exec(
        db,
        &format!(
            "INSERT INTO covers (id, content_type, file_size, source, cloud_path, _updated_at, created_at) \
             VALUES ('{release_id}', 'image/jpeg', 5, 'local', NULL, '{stamp}', '2026-01-01');"
        ),
    )
    .await;
}

/// Insert an `artist_images` row (a host-provided artist-image asset) for
/// `artist_id`. Its `id` IS the artist id (the FK to `artists`), so it rides the
/// artist's gate but — being an asset — never keeps the artist alive on its own.
async fn insert_artist_image(db: &Database, artist_id: &str, stamp: &str) {
    exec(
        db,
        &format!(
            "INSERT INTO artist_images (id, content_type, file_size, source, cloud_path, _updated_at, created_at) \
             VALUES ('{artist_id}', 'image/jpeg', 5, 'discogs', NULL, '{stamp}', '2026-01-01');"
        ),
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

/// The one library both devices sync. coven authorizes a changeset against the
/// library it names, so the cycle and the storage that signs the device's control
/// objects must agree on it.
const LIBRARY_ID: &str = "test-lib";

/// Run one sync cycle for `device_id` over `storage` (no cloud_home → no outbox
/// processing; no blobs). `keypair` is the device's identity — the same key its
/// `storage` signs control objects with, so the cloud accepts what it reads back.
async fn sync_cycle(
    db: &Database,
    storage: &CloudSyncStorage,
    device_id: &str,
    keypair: &UserKeypair,
    lib: &LibraryDir,
) {
    let hlc = Hlc::new(device_id.to_string());
    let cipher = RwLock::new(CloudCipher::Encrypted(EncryptionService::new_with_key(
        &[9u8; 32],
    )));
    run_single_sync_cycle(
        storage,
        LIBRARY_ID,
        device_id,
        &hlc,
        &SystemClock,
        db.coven_db(),
        &cipher,
        keypair,
        lib,
        None,
        None,
    )
    .await
    .expect("sync cycle");
}

#[tokio::test]
async fn remote_catalog_pushed_by_device_a_reaches_device_b() {
    let cloud = SharedCloud::new();
    let key = [9u8; 32];

    // Device A: a fresh library with a remote album/release/tracks.
    let tmp_a = tempfile::tempdir().unwrap();
    let lib_a = LibraryDir::new(tmp_a.path().join("a"));
    std::fs::create_dir_all(&*lib_a).unwrap();
    let db_a = Database::new_test(lib_a.db_path().to_str().unwrap(), Arc::new(SystemClock))
        .await
        .unwrap();
    let keypair_a = UserKeypair::generate();
    let storage_a = CloudSyncStorage::new(
        Arc::new(cloud.clone()),
        CloudCipher::Encrypted(EncryptionService::new_with_key(&key)),
        BlobPathScheme::Hashed,
        keypair_a.clone(),
    );

    insert_remote_catalog(&db_a).await;

    // A syncs: the remote release flips into the gated set, so its whole subtree
    // is pushed to the cloud.
    sync_cycle(&db_a, &storage_a, "device-a", &keypair_a, &lib_a).await;

    // Device B: a fresh empty library on the same cloud.
    let tmp_b = tempfile::tempdir().unwrap();
    let lib_b = LibraryDir::new(tmp_b.path().join("b"));
    std::fs::create_dir_all(&*lib_b).unwrap();
    let db_b = Database::new_test(lib_b.db_path().to_str().unwrap(), Arc::new(SystemClock))
        .await
        .unwrap();
    let keypair_b = UserKeypair::generate();
    let storage_b = CloudSyncStorage::new(
        Arc::new(cloud.clone()),
        CloudCipher::Encrypted(EncryptionService::new_with_key(&key)),
        BlobPathScheme::Hashed,
        keypair_b.clone(),
    );
    assert_eq!(
        count(&db_b, "SELECT count(*) FROM releases").await,
        0,
        "device B starts empty"
    );

    // B syncs: it pulls A's catalog changeset.
    sync_cycle(&db_b, &storage_b, "device-b", &keypair_b, &lib_b).await;

    // B must now hold A's remote release, its album/artist, and both tracks.
    assert_eq!(
        count(
            &db_b,
            "SELECT count(*) FROM releases WHERE id='re1' AND remote=1"
        )
        .await,
        1,
        "device B must receive device A's remote release",
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

/// Insert two albums/releases, each landed `remote = 0` — the shape of "import
/// two releases, their audio still uploading". A release reaches `remote = 1`
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
         INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at) \
         VALUES ('re1', 'al1', 'file_tags', 0, '0000000001006-0000-test-device', '2026-01-01');\n\
         INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at) \
         VALUES ('re2', 'al2', 'file_tags', 0, '0000000001007-0000-test-device', '2026-01-01');\n\
         INSERT INTO tracks (id, release_id, title, side, _updated_at, created_at) \
         VALUES ('tr1', 're1', 'Track One', 1, '0000000001008-0000-test-device', '2026-01-01');\n\
         INSERT INTO tracks (id, release_id, title, side, _updated_at, created_at) \
         VALUES ('tr2', 're2', 'Track Two', 1, '0000000001009-0000-test-device', '2026-01-01');",
    )
    .await;
}

/// Each release's metadata propagates as its own audio finishes — not batched
/// behind the slowest upload. Two releases land `remote = 0` (audio uploading);
/// `re1` flips `remote = 1` first (its upload finished) and must reach device B
/// while `re2` is still uploading; `re2` reaches B only once it flips too.
#[tokio::test]
async fn each_release_propagates_when_its_own_upload_flips_remote() {
    let cloud = SharedCloud::new();
    let key = [9u8; 32];

    let tmp_a = tempfile::tempdir().unwrap();
    let lib_a = LibraryDir::new(tmp_a.path().join("a"));
    std::fs::create_dir_all(&*lib_a).unwrap();
    let db_a = Database::new_test(lib_a.db_path().to_str().unwrap(), Arc::new(SystemClock))
        .await
        .unwrap();
    let keypair_a = UserKeypair::generate();
    let storage_a = CloudSyncStorage::new(
        Arc::new(cloud.clone()),
        CloudCipher::Encrypted(EncryptionService::new_with_key(&key)),
        BlobPathScheme::Hashed,
        keypair_a.clone(),
    );

    insert_two_uploading_releases(&db_a).await;

    // Cycle 1: both releases are still `remote = 0` (audio uploading), so the
    // gate cuts them — nothing reaches the cloud.
    sync_cycle(&db_a, &storage_a, "device-a", &keypair_a, &lib_a).await;
    assert!(
        !cloud_has_changeset(&cloud, "device-a"),
        "a release whose audio isn't uploaded yet (remote=0) is gated out",
    );

    // re1's audio finishes; the observer flips it remote (cloud-only — no local
    // copy), through the same DB transition the live observer calls. re2 is still
    // uploading.
    db_a.set_remote_for_test("re1", true).await.unwrap();
    sync_cycle(&db_a, &storage_a, "device-a", &keypair_a, &lib_a).await;

    // Device B pulls: it gets re1 (and its album/artist/track) but NOT re2 —
    // re1 reached B without waiting for re2's upload.
    let tmp_b = tempfile::tempdir().unwrap();
    let lib_b = LibraryDir::new(tmp_b.path().join("b"));
    std::fs::create_dir_all(&*lib_b).unwrap();
    let db_b = Database::new_test(lib_b.db_path().to_str().unwrap(), Arc::new(SystemClock))
        .await
        .unwrap();
    let keypair_b = UserKeypair::generate();
    let storage_b = CloudSyncStorage::new(
        Arc::new(cloud.clone()),
        CloudCipher::Encrypted(EncryptionService::new_with_key(&key)),
        BlobPathScheme::Hashed,
        keypair_b.clone(),
    );
    sync_cycle(&db_b, &storage_b, "device-b", &keypair_b, &lib_b).await;
    assert_eq!(
        count(
            &db_b,
            "SELECT count(*) FROM releases WHERE id='re1' AND remote=1"
        )
        .await,
        1,
        "re1 must reach device B as soon as its own upload flips it remote",
    );
    assert_eq!(
        count(&db_b, "SELECT count(*) FROM releases WHERE id='re2'").await,
        0,
        "re2 must NOT reach B while it is still uploading (remote=0)",
    );

    // re2's audio finishes and flips it remote; now it reaches B too.
    db_a.set_remote_for_test("re2", true).await.unwrap();
    sync_cycle(&db_a, &storage_a, "device-a", &keypair_a, &lib_a).await;
    sync_cycle(&db_b, &storage_b, "device-b", &keypair_b, &lib_b).await;
    assert_eq!(
        count(
            &db_b,
            "SELECT count(*) FROM releases WHERE id='re2' AND remote=1"
        )
        .await,
        1,
        "re2 reaches B once its own upload flips it remote",
    );
    assert_eq!(
        count(&db_b, "SELECT count(*) FROM tracks").await,
        2,
        "device B ends with both releases' tracks",
    );
}

/// Gate-retract round-trip (Part 2 "Tests"): A makes a release Remote and a peer B
/// pulls its whole subtree (release + tracks). When A makes the release Local again
/// (the gate flips `remote` true→false, exactly what `make_local` does), coven's
/// gate retract emits DELETEs for the subtree, so B loses the release and its
/// tracks — while A keeps the rows locally (gated-false). Exercises bae's
/// `synced_tables()` gate declaration end to end across two devices. (The cover
/// asset's ride + retract is covered by the asset test below, which sets up the
/// cover blob bytes the inline push needs.)
#[tokio::test]
async fn make_local_retracts_the_release_subtree_from_a_peer() {
    let cloud = SharedCloud::new();
    let key = [9u8; 32];

    let tmp_a = tempfile::tempdir().unwrap();
    let lib_a = LibraryDir::new(tmp_a.path().join("a"));
    std::fs::create_dir_all(&*lib_a).unwrap();
    let db_a = Database::new_test(lib_a.db_path().to_str().unwrap(), Arc::new(SystemClock))
        .await
        .unwrap();
    let keypair_a = UserKeypair::generate();
    let storage_a = CloudSyncStorage::new(
        Arc::new(cloud.clone()),
        CloudCipher::Encrypted(EncryptionService::new_with_key(&key)),
        BlobPathScheme::Hashed,
        keypair_a.clone(),
    );

    insert_remote_catalog(&db_a).await;
    sync_cycle(&db_a, &storage_a, "device-a", &keypair_a, &lib_a).await;

    let tmp_b = tempfile::tempdir().unwrap();
    let lib_b = LibraryDir::new(tmp_b.path().join("b"));
    std::fs::create_dir_all(&*lib_b).unwrap();
    let db_b = Database::new_test(lib_b.db_path().to_str().unwrap(), Arc::new(SystemClock))
        .await
        .unwrap();
    let keypair_b = UserKeypair::generate();
    let storage_b = CloudSyncStorage::new(
        Arc::new(cloud.clone()),
        CloudCipher::Encrypted(EncryptionService::new_with_key(&key)),
        BlobPathScheme::Hashed,
        keypair_b.clone(),
    );
    sync_cycle(&db_b, &storage_b, "device-b", &keypair_b, &lib_b).await;
    assert_eq!(
        count(&db_b, "SELECT count(*) FROM releases WHERE id='re1'").await,
        1,
        "B receives the remote release",
    );
    assert_eq!(
        count(&db_b, "SELECT count(*) FROM tracks WHERE release_id='re1'").await,
        2,
        "B receives the release's tracks",
    );

    // A makes re1 Local (the gate flips true→false, as `make_local` does), then
    // syncs: the gate retract emits DELETEs for the subtree.
    db_a.set_remote_for_test("re1", false).await.unwrap();
    sync_cycle(&db_a, &storage_a, "device-a", &keypair_a, &lib_a).await;

    // B applies the retract and loses the whole subtree.
    sync_cycle(&db_b, &storage_b, "device-b", &keypair_b, &lib_b).await;
    assert_eq!(
        count(&db_b, "SELECT count(*) FROM releases WHERE id='re1'").await,
        0,
        "B's release is retracted when A makes it Local",
    );
    assert_eq!(
        count(&db_b, "SELECT count(*) FROM tracks WHERE release_id='re1'").await,
        0,
        "B's tracks are retracted with the release",
    );

    // A keeps the rows locally (a gated-false Local release is private, not deleted).
    assert_eq!(
        count(&db_a, "SELECT count(*) FROM releases WHERE id='re1'").await,
        1,
        "A keeps the release locally after making it Local",
    );
}

// Real-length ids: coven shards a blob by the first two byte-pairs of its
// dash-stripped id, so an asset blob (uploaded with its row) needs an id long
// enough to form that prefix — production ids are UUIDs.
const AR1: &str = "a0000000-0000-0000-0000-000000000001"; // has a Remote release
const AR2: &str = "a0000000-0000-0000-0000-000000000002"; // has a Local release
const AR3: &str = "a0000000-0000-0000-0000-000000000003"; // orphan: only an image
const AL1: &str = "b0000000-0000-0000-0000-000000000001";
const AL2: &str = "b0000000-0000-0000-0000-000000000002";
const RE1: &str = "c0000000-0000-0000-0000-000000000001"; // Remote
const RE2: &str = "c0000000-0000-0000-0000-000000000002"; // Local

/// Asset keep/leak (Part 2 "Tests"): a `covers` / `artist_images` asset rides its
/// FK subject's gate when that subject is Remote, but never grants keep on its own.
/// A holds (a) a Remote release whose artist also has an artist image, (b) a Local
/// release with a cover, and (c) an orphan artist whose only row is an artist image
/// (no Remote release references it). The cover/artist-image BYTES are seeded via
/// `local_files::store` (coven's host-provided local store) so the inline push can
/// upload the Remote ones. After A→B sync, B gets the Remote release + its cover +
/// the artist + the artist's image, but NOT the Local release's cover, NOT the
/// Local release, and NOT the orphan artist or its image.
#[tokio::test]
async fn assets_ride_remote_subjects_but_local_and_orphan_assets_do_not_leak() {
    let cloud = SharedCloud::new();
    let key = [9u8; 32];

    let tmp_a = tempfile::tempdir().unwrap();
    let lib_a = LibraryDir::new(tmp_a.path().join("a"));
    std::fs::create_dir_all(&*lib_a).unwrap();
    let db_a = Database::new_test(lib_a.db_path().to_str().unwrap(), Arc::new(SystemClock))
        .await
        .unwrap();
    let keypair_a = UserKeypair::generate();
    let storage_a = CloudSyncStorage::new(
        Arc::new(cloud.clone()),
        CloudCipher::Encrypted(EncryptionService::new_with_key(&key)),
        BlobPathScheme::Hashed,
        keypair_a.clone(),
    );

    exec(
        &db_a,
        &format!(
            "INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('{AR1}', 'Artist One', '0000000001000-0000-test-device', '2026-01-01');\n\
             INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('{AR2}', 'Artist Two', '0000000001001-0000-test-device', '2026-01-01');\n\
             INSERT INTO artists (id, name, _updated_at, created_at) VALUES ('{AR3}', 'Artist Three', '0000000001002-0000-test-device', '2026-01-01');\n\
             INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at) VALUES ('{AL1}', 'Album One', '{AR1}', 0, '0000000001003-0000-test-device', '2026-01-01');\n\
             INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at) VALUES ('{AL2}', 'Album Two', '{AR2}', 0, '0000000001004-0000-test-device', '2026-01-01');\n\
             INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at) VALUES ('aa1', '{AL1}', '{AR1}', 0, '0000000001005-0000-test-device', '2026-01-01');\n\
             INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at) VALUES ('aa2', '{AL2}', '{AR2}', 0, '0000000001006-0000-test-device', '2026-01-01');\n\
             INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at) VALUES ('{RE1}', '{AL1}', 'file_tags', 1, '0000000001007-0000-test-device', '2026-01-01');\n\
             INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at) VALUES ('{RE2}', '{AL2}', 'file_tags', 0, '0000000001008-0000-test-device', '2026-01-01');"
        ),
    )
    .await;
    // Seed the cover / artist-image bytes in coven's host-provided local store, so
    // the inline push can upload the Remote ones. (The Local release's cover and
    // the orphan artist's image are gated out, never uploaded.)
    coven::blob::local_files::store(&lib_a, "covers", RE1, b"cover-bytes-1")
        .await
        .unwrap();
    coven::blob::local_files::store(&lib_a, "covers", RE2, b"cover-bytes-2")
        .await
        .unwrap();
    coven::blob::local_files::store(&lib_a, "artist_images", AR1, b"artist-img-1")
        .await
        .unwrap();
    coven::blob::local_files::store(&lib_a, "artist_images", AR3, b"artist-img-3")
        .await
        .unwrap();
    insert_cover(&db_a, RE1, "0000000001020-0000-test-device").await;
    insert_cover(&db_a, RE2, "0000000001021-0000-test-device").await;
    insert_artist_image(&db_a, AR1, "0000000001022-0000-test-device").await;
    insert_artist_image(&db_a, AR3, "0000000001023-0000-test-device").await;

    sync_cycle(&db_a, &storage_a, "device-a", &keypair_a, &lib_a).await;

    let tmp_b = tempfile::tempdir().unwrap();
    let lib_b = LibraryDir::new(tmp_b.path().join("b"));
    std::fs::create_dir_all(&*lib_b).unwrap();
    let db_b = Database::new_test(lib_b.db_path().to_str().unwrap(), Arc::new(SystemClock))
        .await
        .unwrap();
    let keypair_b = UserKeypair::generate();
    let storage_b = CloudSyncStorage::new(
        Arc::new(cloud.clone()),
        CloudCipher::Encrypted(EncryptionService::new_with_key(&key)),
        BlobPathScheme::Hashed,
        keypair_b.clone(),
    );
    sync_cycle(&db_b, &storage_b, "device-b", &keypair_b, &lib_b).await;

    // Rides when Remote: B gets the remote release, its cover, its artist, and the
    // artist's image (the artist is kept by its remote release; the image rides).
    assert_eq!(
        count(
            &db_b,
            &format!("SELECT count(*) FROM releases WHERE id='{RE1}'")
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &db_b,
            &format!("SELECT count(*) FROM covers WHERE id='{RE1}'")
        )
        .await,
        1,
        "cover rides the remote release",
    );
    assert_eq!(
        count(
            &db_b,
            &format!("SELECT count(*) FROM artists WHERE id='{AR1}'")
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &db_b,
            &format!("SELECT count(*) FROM artist_images WHERE id='{AR1}'")
        )
        .await,
        1,
        "the kept artist's image rides its gate",
    );

    // Does NOT leak while Local: the Local release, its cover, its artist.
    assert_eq!(
        count(
            &db_b,
            &format!("SELECT count(*) FROM releases WHERE id='{RE2}'")
        )
        .await,
        0,
        "a Local release does not sync"
    );
    assert_eq!(
        count(
            &db_b,
            &format!("SELECT count(*) FROM covers WHERE id='{RE2}'")
        )
        .await,
        0,
        "a Local release's cover does not leak"
    );
    assert_eq!(
        count(
            &db_b,
            &format!("SELECT count(*) FROM artists WHERE id='{AR2}'")
        )
        .await,
        0
    );

    // An asset never grants keep: the orphan artist (only an artist image, no
    // remote release) is not kept, and neither is its image.
    assert_eq!(
        count(
            &db_b,
            &format!("SELECT count(*) FROM artists WHERE id='{AR3}'")
        )
        .await,
        0,
        "an artist with only an image is not kept"
    );
    assert_eq!(
        count(
            &db_b,
            &format!("SELECT count(*) FROM artist_images WHERE id='{AR3}'")
        )
        .await,
        0,
        "the orphan artist's image does not leak"
    );
}
