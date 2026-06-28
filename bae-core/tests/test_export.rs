#![cfg(feature = "test-utils")]
//! Export of cloud-only (unpinned remote) releases.
//!
//! Export must not require a local copy: when this device holds none, the
//! bytes are downloaded from the cloud home and decrypted with the release's
//! item key — the same verified read pin and unmanage use.

mod support;

use bae_core::db::Database;
use bae_core::import::{IdentityChoice, ImportCommand, StorageMode};
use bae_core::library::{ExportFormat, LibraryManager};
use coven::EncryptionService;
use coven::LibraryDir;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use support::MockCloudHome;
use tempfile::TempDir;

struct ExportFixture {
    db: Database,
    mgr: LibraryManager,
    handle: bae_core::import::ImportServiceHandle,
    cloud: Arc<MockCloudHome>,
    _temp: TempDir,
}

/// The cloud object key coven addresses a release file's blob by. The export
/// fixture's cloud home is the default opaque home, so the blob is sharded by id
/// under the `release_files` namespace (Hashed scheme, no readable cloud_path).
/// Seeding/removing the mock cloud at this key matches what coven reads.
async fn cloud_key(mgr: &LibraryManager, file: &bae_core::db::DbFile) -> String {
    mgr.resolve_track_cloud_key_for_test(&file.id).await
}

impl ExportFixture {
    async fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let db_dir = temp.path().join("db");
        fs::create_dir_all(&db_dir).unwrap();

        let db = Database::new_test(
            db_dir.join("test.db").to_str().unwrap(),
            Arc::new(coven::SystemClock),
        )
        .await
        .unwrap();
        let library_dir = LibraryDir::new(db_dir.clone());
        let (config_handle, key_service) = support::test_config_and_keys(&library_dir);
        let mgr = LibraryManager::new(
            db.clone(),
            library_dir,
            config_handle,
            key_service,
            Arc::new(coven::SystemClock),
            Arc::new(coven::UuidProvider),
            tokio::runtime::Handle::current(),
        );
        let cloud = Arc::new(MockCloudHome::new());
        mgr.connect_test_cloud_home(
            cloud.clone(),
            bae_core::sync::CloudCipher::Encrypted(EncryptionService::new_with_key(&[7u8; 32])),
        )
        .await
        .unwrap();

        let handle = bae_core::import::ImportService::start(
            tokio::runtime::Handle::current(),
            mgr.clone(),
            bae_core::import::cover_art::CoverArtArchiveClient::new(),
        );

        Self {
            db,
            mgr,
            handle,
            cloud,
            _temp: temp,
        }
    }

    fn temp_path(&self) -> &Path {
        self._temp.path()
    }
}

/// Import a one-track album from `album_dir` as Local with Unknown
/// identity (file tags only — no network), then flip it to cloud-only:
/// remote with no local copy, encrypted blobs seeded in the mock cloud,
/// originals deleted. This is the state export must handle: no local bytes,
/// audio only in the cloud.
async fn import_then_strand_in_cloud(f: &ExportFixture, album_dir: &Path) -> (String, Vec<u8>) {
    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand::Folder {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir.to_path_buf(),
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .unwrap();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut progress_rx).await;

    // Flip to cloud-only: remote, and drop the in-place external refs. A real
    // make-Remote uploads each file and drops its ref; this test seeds the cloud
    // by hand, so it flips the gate and clears the refs directly. With the refs
    // gone, coven's read resolves to the cloud (not the user's deleted file).
    f.db.set_remote_for_test(&release_id, true).await.unwrap();
    let files = f.mgr.get_files_for_release(&release_id).await.unwrap();
    assert_eq!(files.len(), 1);
    for file in &files {
        f.db.clear_external_blob(&file.id).await.unwrap();
    }

    // Seed the cloud home with each file's bytes encrypted under the library
    // master key (what the upload outbox would have produced).
    let master_enc = f
        .mgr
        .get_encryption_service()
        .expect("the library is unlocked");
    let original_bytes = fs::read(album_dir.join(&files[0].original_filename)).unwrap();
    f.cloud.put(
        &cloud_key(&f.mgr, &files[0]).await,
        master_enc.encrypt(&original_bytes),
    );

    // Remove the originals — nothing local remains.
    fs::remove_dir_all(album_dir).unwrap();

    (release_id, original_bytes)
}

/// Exporting a single track of a cloud-only release downloads + decrypts the
/// audio and re-encodes it — no "pin the release before exporting" dead-end.
#[tokio::test]
async fn export_track_from_cloud_only_release() {
    support::tracing_init();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let original_bytes = support::write_tagged_flac(&album_dir, "01.flac", "Track One");
    let (release_id, _) = import_then_strand_in_cloud(&f, &album_dir).await;

    let tracks = f.mgr.get_tracks(&release_id).await.unwrap();
    assert_eq!(tracks.len(), 1);

    let out = f.temp_path().join("exported.flac");
    f.mgr
        .export_track(&tracks[0].id, &out, ExportFormat::Flac)
        .await
        .expect("cloud-only track must export");

    // FLAC is lossless: the exported file decodes to the same PCM as the
    // cloud copy.
    let exported = fs::read(&out).unwrap();
    let exported_pcm = bae_core::audio_codec::decode_audio(&exported, None, None).unwrap();
    let original_pcm = bae_core::audio_codec::decode_audio(&original_bytes, None, None).unwrap();
    assert_eq!(exported_pcm.samples, original_pcm.samples);
}

/// Exporting a whole cloud-only release downloads each file and writes the
/// raw bytes — byte-identical to what was uploaded.
#[tokio::test]
async fn export_release_from_cloud_only_release() {
    support::tracing_init();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    support::write_tagged_flac(&album_dir, "01.flac", "Track One");
    let (release_id, original_bytes) = import_then_strand_in_cloud(&f, &album_dir).await;

    let target = f.temp_path().join("export-target");
    fs::create_dir_all(&target).unwrap();
    f.mgr
        .export_release(&release_id, &target)
        .await
        .expect("cloud-only release must export");

    // One subfolder, containing the file byte-identical to the cloud copy.
    let subdir = fs::read_dir(&target)
        .unwrap()
        .next()
        .expect("export wrote a release folder")
        .unwrap()
        .path();
    let written = fs::read(subdir.join("01.flac")).unwrap();
    assert_eq!(written, original_bytes);
}

/// A cloud-only release whose blob is missing exports nothing and errors —
/// no partial silent success.
#[tokio::test]
async fn export_release_missing_blob_is_hard_error() {
    support::tracing_init();
    let f = ExportFixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    support::write_tagged_flac(&album_dir, "01.flac", "Track One");
    let (release_id, _) = import_then_strand_in_cloud(&f, &album_dir).await;

    // Blow away the seeded blob.
    let files = f.mgr.get_files_for_release(&release_id).await.unwrap();
    f.cloud.remove(&cloud_key(&f.mgr, &files[0]).await);

    let target = f.temp_path().join("export-target");
    fs::create_dir_all(&target).unwrap();
    let result = f.mgr.export_release(&release_id, &target).await;
    assert!(result.is_err(), "missing blob must fail the export");
}
