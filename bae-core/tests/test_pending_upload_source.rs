#![cfg(feature = "test-utils")]
//! A managed import that keeps no local copy (cloud-only) is playable and
//! readable while its upload is still queued: the outbox rows carry the
//! original files' paths, and file resolution falls back to them until the
//! upload lands. Without the fallback, playback issues cloud reads for
//! objects that don't exist yet ("Cloud nonce-header read failed: not
//! found").

mod support;

use bae_core::encryption::EncryptionService;
use bae_core::import::{IdentityChoice, ImportCommand, StorageMode};
use bae_core::library::manager::ReadableFileSource;
use bae_core::library::LibraryManager;
use bae_core::library_dir::LibraryDir;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use support::MockCloudHome;
use tempfile::TempDir;

struct Fixture {
    mgr: LibraryManager,
    handle: bae_core::import::ImportServiceHandle,
    _temp: TempDir,
}

impl Fixture {
    async fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let db_dir = temp.path().join("db");
        fs::create_dir_all(&db_dir).unwrap();

        let db = bae_core::db::Database::new_test(
            db_dir.join("test.db").to_str().unwrap(),
            Arc::new(bae_core::clock::SystemClock),
        )
        .await
        .unwrap();
        let library_dir = LibraryDir::new(db_dir.clone());
        let (config_handle, key_service) = support::test_config_and_keys(&library_dir);
        let mut mgr = LibraryManager::new(
            db,
            library_dir,
            config_handle,
            key_service,
            Arc::new(bae_core::clock::SystemClock),
            Arc::new(bae_core::id_provider::UuidProvider),
            tokio::runtime::Handle::current(),
            None,
        );
        // An (empty) cloud home: resolution must prefer the pending upload's
        // original file, never reaching for a cloud object that doesn't
        // exist yet.
        mgr.set_cloud_override(
            Arc::new(MockCloudHome::new()),
            EncryptionService::new_with_key(&[7u8; 32]),
        );

        let handle = bae_core::import::ImportService::start(
            tokio::runtime::Handle::current(),
            mgr.clone(),
            bae_core::import::cover_art::CoverArtArchiveClient::new(),
        );

        Self {
            mgr,
            handle,
            _temp: temp,
        }
    }

    fn temp_path(&self) -> &Path {
        self._temp.path()
    }
}

/// Import one album as Managed{pin: false}: managed, no local-copy row, one
/// queued upload per file whose source is the original on disk.
async fn import_managed_unpinned(f: &Fixture, album_dir: &Path) -> String {
    let import_id = uuid::Uuid::new_v4().to_string();
    f.handle
        .send_command(ImportCommand::Folder {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir.to_path_buf(),
            selected_cover: None,
            storage_mode: StorageMode::Managed,
            pin: false,
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .unwrap();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut progress_rx).await;
    release_id
}

/// Playback resolution of a managed-unpinned import lands on the original
/// file while its upload is queued — not on a cloud object that doesn't
/// exist yet.
#[tokio::test]
async fn pending_upload_source_resolves_for_playback() {
    support::tracing_init();
    let f = Fixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    support::write_tagged_flac(&album_dir, "01.flac", "Track One");
    let release_id = import_managed_unpinned(&f, &album_dir).await;

    // The import landed Unmanaged with its upload queued: it keeps its in-place
    // source row (managed flips only once the upload drains) and a pending upload.
    assert!(
        f.mgr
            .get_release_unmanaged_source(&release_id)
            .await
            .unwrap()
            .is_some(),
        "a managed import stays Unmanaged-with-source until its upload drains"
    );
    assert_eq!(
        f.mgr
            .count_pending_uploads_for_release(&release_id)
            .await
            .unwrap(),
        1
    );

    let tracks = f.mgr.get_tracks(&release_id).await.unwrap();
    let resolved = f.mgr.resolve_track_audio(&tracks[0].id).await.unwrap();

    assert_eq!(
        resolved.source,
        ReadableFileSource::Local(album_dir.join("01.flac")),
        "resolution must fall back to the pending upload's original file"
    );
}

/// When the pending upload's original is gone too, resolution reports the
/// pending upload so playback can explain the state instead of issuing a
/// cloud read that 404s.
#[tokio::test]
async fn missing_pending_source_reports_upload_pending() {
    support::tracing_init();
    let f = Fixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    support::write_tagged_flac(&album_dir, "01.flac", "Track One");
    let release_id = import_managed_unpinned(&f, &album_dir).await;

    fs::remove_dir_all(&album_dir).unwrap();

    let tracks = f.mgr.get_tracks(&release_id).await.unwrap();
    let resolved = f.mgr.resolve_track_audio(&tracks[0].id).await.unwrap();

    assert_eq!(
        resolved.source,
        ReadableFileSource::UploadPendingSourceMissing,
        "a queued upload with a missing source must be reported, not 404 in the cloud"
    );
}

/// The shared verified read (pin / export / unmanage) gains the same
/// fallback: it reads the pending upload's original instead of the cloud.
#[tokio::test]
async fn read_release_file_bytes_uses_pending_source() {
    support::tracing_init();
    let f = Fixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let original_bytes = support::write_tagged_flac(&album_dir, "01.flac", "Track One");
    let release_id = import_managed_unpinned(&f, &album_dir).await;

    let files = f.mgr.get_files_for_release(&release_id).await.unwrap();
    // The mock cloud holds NO blob: success proves the local fallback.
    let bytes =
        bae_core::storage::local::transfer::read_release_file_bytes(None, &files[0], &f.mgr)
            .await
            .expect("pending-upload original must be readable");
    assert_eq!(bytes, original_bytes);
}
