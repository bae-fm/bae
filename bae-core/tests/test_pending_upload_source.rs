#![cfg(feature = "test-utils")]
//! A cloud-only import (`Managed { pin: false }`) is playable and readable while
//! its upload is still queued. It lands as a real unmanaged import that ALSO
//! queues uploads: an unmanaged source pointing at the user's originals, so file
//! resolution reads them in place until the upload lands and the observer flips
//! the release managed (dropping the source). A cloud-only import recording no
//! source and no cache would be an unreadable, unplayable release mid-upload;
//! these tests pin that it isn't.

mod support;

use bae_core::album_detail::ReleaseStorageState;
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
            storage_mode: StorageMode::Managed { pin: false },
            identity_choice: IdentityChoice::Unknown,
            user_edit: None,
        })
        .unwrap();
    let mut progress_rx = f.handle.subscribe_import(import_id);
    let (release_id, _album_id) = support::wait_for_import_complete(&mut progress_rx).await;
    release_id
}

/// Issue #105: while its upload is queued, a cloud-only import resolves to its
/// original file in place (an unmanaged source), not to a cloud object that
/// doesn't exist yet. It reads `Unmanaged` and `Local`; only after the upload
/// lands and the observer flips it does it become `CloudOnly`.
#[tokio::test]
async fn cloud_only_import_is_playable_in_place_until_upload_lands() {
    support::tracing_init();
    let mut f = Fixture::new().await;
    f.mgr.set_force_sync_ready();
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    support::write_tagged_flac(&album_dir, "01.flac", "Track One");
    let release_id = import_managed_unpinned(&f, &album_dir).await;

    // The import lands as a real unmanaged import that also queued an upload:
    // an unmanaged source pointing at the originals, managed still false.
    assert!(
        f.mgr
            .get_unmanaged_source(&release_id)
            .await
            .unwrap()
            .is_some(),
        "a cloud-only import records an unmanaged source (issue #105)"
    );
    assert_eq!(
        f.mgr
            .count_pending_uploads_for_release(&release_id)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        f.mgr
            .find_release_storage_summary(&release_id)
            .await
            .unwrap()
            .unwrap()
            .storage_state,
        ReleaseStorageState::Unmanaged,
        "mid-upload the release is Unmanaged (readable in place), not an invalid state"
    );

    let tracks = f.mgr.get_tracks(&release_id).await.unwrap();
    let resolved = f.mgr.resolve_track_audio(&tracks[0].id).await.unwrap();
    assert_eq!(
        resolved.source,
        ReadableFileSource::Local(album_dir.join("01.flac")),
        "mid-upload, resolution reads the original file in place — never a cloud 404"
    );

    // Drive the upload: the observer flips the release managed and drops the
    // unmanaged source. Now (and only now) it resolves CloudOnly.
    let cloud = Arc::new(MockCloudHome::new());
    let enc = std::sync::RwLock::new(EncryptionService::new_with_key(&[7u8; 32]));
    let count = f
        .mgr
        .process_cloud_uploads_with(cloud.as_ref(), &enc)
        .await
        .unwrap();
    assert_eq!(count, 1);

    assert_eq!(
        f.mgr
            .find_release_storage_summary(&release_id)
            .await
            .unwrap()
            .unwrap()
            .storage_state,
        ReleaseStorageState::CloudOnly,
        "after the upload lands and the observer flips it, the release is CloudOnly"
    );
    let resolved = f.mgr.resolve_track_audio(&tracks[0].id).await.unwrap();
    assert_eq!(
        resolved.source,
        ReadableFileSource::CloudOnly,
        "post-flip resolution is CloudOnly (the source row was dropped)"
    );
}

/// The shared verified read (pin / export / unmanage) reads the cloud-only
/// import's original in place while the upload is queued — not the cloud.
#[tokio::test]
async fn read_release_file_bytes_uses_in_place_source() {
    support::tracing_init();
    let f = Fixture::new().await;
    let album_dir = f.temp_path().join("album");
    fs::create_dir_all(&album_dir).unwrap();
    let original_bytes = support::write_tagged_flac(&album_dir, "01.flac", "Track One");
    let release_id = import_managed_unpinned(&f, &album_dir).await;

    let files = f.mgr.get_files_for_release(&release_id).await.unwrap();
    // The mock cloud holds NO blob: success proves the in-place read.
    let bytes = bae_core::storage::local::transfer::read_release_file_bytes(&files[0], &f.mgr)
        .await
        .expect("the cloud-only import's original must be readable in place");
    assert_eq!(bytes, original_bytes);
}
