use super::track::playback_info_from_track_release;
use super::*;
use crate::config::Config;
use crate::db::{
    DbAlbum, DbFile, DbLibraryImage, DbRelease, DbTrackWork, DbWork, LibraryImageType,
};
use crate::import::MetadataSource;
#[cfg(feature = "test-utils")]
use crate::sync::CloudCipher;
use crate::util::content_type::ContentType;
use chrono::Utc;
#[cfg(feature = "test-utils")]
use coven::InMemoryCloudHome;
use tempfile::TempDir;
use uuid::Uuid;

async fn setup_test_manager() -> (LibraryManager, TempDir) {
    // A unique id per test so keyring entries don't collide in the shared
    // process-global mock store (see `install_test_keyring`).
    setup_test_manager_with_library_id(&format!("test-{}", Uuid::new_v4())).await
}

/// The keyring entries the mock keyring stores are namespaced by library id
/// (`<base>:<library_id>`) in one process-global store. Tests that read or
/// write keyring secrets (the Discogs key) must use a unique library id so
/// parallel tests don't clobber each other's entries.
async fn setup_test_manager_with_library_id(library_id: &str) -> (LibraryManager, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Database::new_test(db_path.to_str().unwrap(), Arc::new(coven::SystemClock))
        .await
        .unwrap();

    // Insert the test artist that create_test_album() references
    let artist = DbArtist {
        id: "test-artist-id".to_string(),
        name: "Test Artist".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: Utc::now(),
    };
    database.insert_artist(&artist).await.unwrap();

    let library_dir = LibraryDir::new(temp_dir.path().to_path_buf());
    let config = Config::with_defaults(
        library_id.to_string(),
        "test-device".to_string(),
        library_dir.clone(),
        "Test Library".to_string(),
    );
    let config_handle = Arc::new(ConfigHandle::new(config));
    crate::config::install_test_keyring();
    let key_service = KeyService::new(library_id.to_string());
    let manager = LibraryManager::new(
        database,
        library_dir,
        config_handle,
        key_service,
        Arc::new(coven::SystemClock),
        Arc::new(coven::UuidProvider),
        tokio::runtime::Handle::current(),
    );
    (manager, temp_dir)
}

#[tokio::test]
async fn set_export_presets_rejects_removing_selected_default() {
    let (manager, _temp_dir) = setup_test_manager().await;
    manager
        .set_default_track_export_selection(crate::config::ExportSelection::Preset {
            preset_id: "mp3".to_string(),
        })
        .unwrap();

    let presets_without_mp3: Vec<_> = manager
        .export_presets()
        .into_iter()
        .filter(|preset| preset.id != "mp3")
        .collect();
    let err = manager
        .set_export_presets(presets_without_mp3)
        .expect_err("selected default preset cannot be removed");

    assert!(err.to_string().contains("unknown export preset mp3"));
    assert!(manager
        .export_presets()
        .iter()
        .any(|preset| preset.id == "mp3"));
}

async fn rename_table_for_test(manager: &LibraryManager, from: &str, to: &str) {
    let statement = format!("ALTER TABLE {from} RENAME TO {to}");
    manager
        .database
        .handle()
        .sql(move |sql| {
            sql.connection().execute(&statement, [])?;
            Ok::<(), coven::CovenError>(())
        })
        .await
        .unwrap();
}

async fn store_test_cover_image(manager: &LibraryManager, release_id: &str) {
    manager
        .store_library_image_blob(
            &DbLibraryImage {
                id: release_id.to_string(),
                image_type: LibraryImageType::Cover,
                content_type: crate::util::content_type::ContentType::Jpeg,
                file_size: 5,
                width: None,
                height: None,
                source: "local".to_string(),
                source_url: None,
                cloud_path: None,
                created_at: manager.clock.now(),
            },
            b"image",
        )
        .await
        .unwrap();
}

async fn setup_forget_library_manager(library_id: &str, home: &std::path::Path) -> LibraryManager {
    let bae_dir = home.join(".bae");
    let library_dir = crate::config::registered_library_path(&bae_dir, library_id);
    setup_forget_library_manager_at(library_id, library_dir, home).await
}

async fn setup_forget_library_manager_at(
    library_id: &str,
    library_dir: std::path::PathBuf,
    home: &std::path::Path,
) -> LibraryManager {
    let db_path = home.join("manager.db");
    let database = Database::new_test(db_path.to_str().unwrap(), Arc::new(coven::SystemClock))
        .await
        .unwrap();
    let config = Config::with_defaults(
        library_id.to_string(),
        "test-device".to_string(),
        LibraryDir::new(library_dir.clone()),
        "Test Library".to_string(),
    );
    let config_handle = Arc::new(ConfigHandle::new(config));
    crate::config::install_test_keyring();
    let key_service = KeyService::new(library_id.to_string());
    LibraryManager::new(
        database,
        LibraryDir::new(library_dir),
        config_handle,
        key_service,
        Arc::new(coven::SystemClock),
        Arc::new(coven::UuidProvider),
        tokio::runtime::Handle::current(),
    )
}

fn setup_forget_library_home(library_id: &str) -> (TempDir, std::path::PathBuf) {
    let home = TempDir::new().unwrap();
    let bae_dir = home.path().join(".bae");
    let library_dir = crate::config::registered_library_path(&bae_dir, library_id);
    (home, library_dir)
}

#[tokio::test]
async fn forget_library_returns_error_when_registered_path_cannot_be_removed() {
    let library_id = format!("forget-fails-{}", Uuid::new_v4());
    let (home, library_path) = setup_forget_library_home(&library_id);
    let bae_dir = home.path().join(".bae");
    std::fs::create_dir_all(library_path.parent().unwrap()).unwrap();
    std::fs::write(&library_path, b"not a directory").unwrap();
    std::fs::write(bae_dir.join("active-library"), &library_id).unwrap();
    let manager = setup_forget_library_manager(&library_id, home.path()).await;
    manager.key_service.set_encryption_key("00").unwrap();

    let err = manager
        .forget_library()
        .expect_err("directory removal failure must surface");

    assert!(
        err.contains("Failed to remove library data"),
        "error should name the failed library data deletion: {err}"
    );
    assert_eq!(
        std::fs::read_to_string(bae_dir.join("active-library")).unwrap(),
        library_id
    );
    assert_eq!(
        manager.key_service.get_encryption_key().unwrap().as_deref(),
        Some("00")
    );
}

#[tokio::test]
async fn forget_library_removes_registered_path_active_pointer_and_key() {
    let library_id = format!("forget-succeeds-{}", Uuid::new_v4());
    let (home, library_path) = setup_forget_library_home(&library_id);
    let bae_dir = home.path().join(".bae");
    std::fs::create_dir_all(&library_path).unwrap();
    std::fs::write(library_path.join("config.yaml"), b"library data").unwrap();
    std::fs::write(bae_dir.join("active-library"), &library_id).unwrap();
    let manager = setup_forget_library_manager(&library_id, home.path()).await;
    manager.key_service.set_encryption_key("00").unwrap();

    manager.forget_library().unwrap();

    assert!(!library_path.exists());
    assert!(!bae_dir.join("active-library").exists());
    assert!(manager.key_service.get_encryption_key().unwrap().is_none());
}

#[tokio::test]
async fn forget_library_accepts_missing_directory_and_pointer_on_retry() {
    let library_id = format!("forget-retry-{}", Uuid::new_v4());
    let (home, library_path) = setup_forget_library_home(&library_id);
    let bae_dir = home.path().join(".bae");
    std::fs::create_dir_all(&bae_dir).unwrap();
    let manager = setup_forget_library_manager(&library_id, home.path()).await;
    manager.key_service.set_encryption_key("00").unwrap();

    manager.forget_library().unwrap();

    assert!(!library_path.exists());
    assert!(!bae_dir.join("active-library").exists());
    assert!(manager.key_service.get_encryption_key().unwrap().is_none());
}

#[tokio::test]
async fn forget_library_returns_error_when_active_pointer_cannot_be_read() {
    let library_id = format!("forget-pointer-fails-{}", Uuid::new_v4());
    let (home, library_path) = setup_forget_library_home(&library_id);
    let bae_dir = home.path().join(".bae");
    std::fs::create_dir_all(&library_path).unwrap();
    std::fs::create_dir(bae_dir.join("active-library")).unwrap();
    let manager = setup_forget_library_manager(&library_id, home.path()).await;
    manager.key_service.set_encryption_key("00").unwrap();

    let err = manager
        .forget_library()
        .expect_err("active pointer read failure must surface");

    assert!(
        err.contains("Failed to read active-library pointer"),
        "error should name the failed active pointer read: {err}"
    );
    assert!(library_path.exists());
    assert!(bae_dir.join("active-library").is_dir());
    assert_eq!(
        manager.key_service.get_encryption_key().unwrap().as_deref(),
        Some("00")
    );
}

#[tokio::test]
async fn forget_library_returns_error_when_active_pointer_names_another_library() {
    let library_id = format!("forget-pointer-mismatch-{}", Uuid::new_v4());
    let (home, library_path) = setup_forget_library_home(&library_id);
    let bae_dir = home.path().join(".bae");
    std::fs::create_dir_all(&library_path).unwrap();
    std::fs::write(bae_dir.join("active-library"), "different-library").unwrap();
    let manager = setup_forget_library_manager(&library_id, home.path()).await;
    manager.key_service.set_encryption_key("00").unwrap();

    let err = manager
        .forget_library()
        .expect_err("active pointer mismatch must surface");

    assert!(
        err.contains("points at different-library"),
        "error should name the active pointer mismatch: {err}"
    );
    assert!(library_path.exists());
    assert_eq!(
        std::fs::read_to_string(bae_dir.join("active-library")).unwrap(),
        "different-library"
    );
    assert_eq!(
        manager.key_service.get_encryption_key().unwrap().as_deref(),
        Some("00")
    );
}

#[tokio::test]
async fn forget_library_rejects_unregistered_library_dir() {
    let library_id = format!("forget-unregistered-{}", Uuid::new_v4());
    let home = TempDir::new().unwrap();
    let library_path = home.path().join("external-library");
    std::fs::create_dir_all(&library_path).unwrap();
    let manager =
        setup_forget_library_manager_at(&library_id, library_path.clone(), home.path()).await;
    manager.key_service.set_encryption_key("00").unwrap();

    let err = manager
        .forget_library()
        .expect_err("unregistered library directory must fail loudly");

    assert!(
        err.contains("does not match library id"),
        "error should name the unregistered library directory: {err}"
    );
    assert!(library_path.exists());
    assert_eq!(
        manager.key_service.get_encryption_key().unwrap().as_deref(),
        Some("00")
    );
}

#[tokio::test]
async fn mcp_config_rejects_port_zero_and_persists_valid_config() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let invalid = crate::config::McpConfig {
        enabled: true,
        port: 0,
    };
    assert!(manager.set_mcp_config(invalid).is_err());
    assert_eq!(
        manager.get_config().mcp,
        crate::config::McpConfig::disabled_default()
    );

    let valid = crate::config::McpConfig {
        enabled: true,
        port: crate::config::MCP_DEFAULT_PORT + 1,
    };
    manager.set_mcp_config(valid).unwrap();
    assert_eq!(manager.get_config().mcp, valid);
}

#[tokio::test]
async fn mcp_token_is_keyring_backed_and_sets_target() {
    let (manager, _temp_dir) = setup_test_manager().await;
    assert!(manager.get_mcp_token().unwrap().is_none());

    let token = manager.ensure_mcp_token().unwrap();
    assert_eq!(token.len(), 64);
    assert!(token.chars().all(|ch| ch.is_ascii_hexdigit()));
    assert_eq!(manager.ensure_mcp_token().unwrap(), token);

    let replacement = "a".repeat(64);
    manager.set_mcp_token(replacement.clone()).unwrap();
    assert_eq!(
        manager.get_mcp_token().unwrap().as_deref(),
        Some(replacement.as_str())
    );
}

fn create_test_album() -> DbAlbum {
    DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: "Test Album".to_string(),
        artist_id: "test-artist-id".to_string(),
        year: Some(2024),
        primary_release_id: None,
        is_compilation: false,
        created_at: Utc::now(),
    }
}

fn create_test_release(album_id: &str) -> DbRelease {
    DbRelease {
        id: Uuid::new_v4().to_string(),
        album_id: album_id.to_string(),
        release_name: None,
        pressing: Pressing {
            year: Some(2024),
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
        },
        disc_id: None,
        metadata_source: crate::db::ReleaseMetadataSource::FileTags,
        metadata_source_release_id: None,
        remote: true,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: Utc::now(),
    }
}

#[tokio::test]
async fn work_detail_release_rows_are_display_ready() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.pressing.format = Some("CD".to_string());
    let track = crate::db::DbTrack::new_test(&release.id, "track-a", "Track Title", Some(1));
    let now = Utc::now();
    let work = DbWork::new("work-a", "Work Title", None, Some("work".to_string()), now);
    let track_work = DbTrackWork::new(
        &track.id,
        &work.id,
        0,
        MetadataSource::MusicBrainz,
        "track-work-a".to_string(),
        now,
    );
    let cover = DbLibraryImage {
        id: release.id.clone(),
        image_type: LibraryImageType::Cover,
        content_type: ContentType::Jpeg,
        file_size: 100,
        width: None,
        height: None,
        source: "local".to_string(),
        source_url: None,
        cloud_path: None,
        created_at: now,
    };

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager.database.insert_track(&track).await.unwrap();
    manager
        .database
        .insert_composition_fixture_rows(std::slice::from_ref(&work), &[track_work], &[cover])
        .await
        .unwrap();

    let detail = manager
        .get_work_detail(&work.id)
        .await
        .unwrap()
        .expect("work detail");

    assert_eq!(detail.releases.len(), 1);
    let row = &detail.releases[0];
    assert_eq!(row.release_id, release.id);
    assert_eq!(row.album_id, album.id);
    assert_eq!(row.album_title, album.title);
    assert_eq!(row.display_name, "2024 CD");
    assert_eq!(row.format.as_deref(), Some("CD"));
    let cover = row.cover.as_ref().expect("work release cover");
    assert_eq!(cover.id, release.id);
    assert!(!cover.version.is_empty());
    assert_eq!(cover.image_type, crate::db::LibraryImageType::Cover);
}

#[tokio::test]
async fn test_delete_release_with_single_release_deletes_album() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    manager.delete_release(&release.id).await.unwrap();

    let album_result = manager.database.find_album_by_id(&album.id).await.unwrap();
    assert!(album_result.is_none());
    let releases = manager
        .database
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert!(releases.is_empty());
}

#[tokio::test]
async fn test_delete_release_with_multiple_releases_preserves_album() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();

    manager.delete_release(&release1.id).await.unwrap();

    let album_result = manager.database.find_album_by_id(&album.id).await.unwrap();
    assert!(album_result.is_some());
    let releases = manager
        .database
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].id, release2.id);
}

#[tokio::test]
async fn delete_releases_with_content_hash_removes_only_matching() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut matching1 = create_test_release(&album.id);
    matching1.content_hash = Some("hash-shared".to_string());
    let mut matching2 = create_test_release(&album.id);
    matching2.content_hash = Some("hash-shared".to_string());
    let mut other = create_test_release(&album.id);
    other.content_hash = Some("hash-other".to_string());

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&matching1).await.unwrap();
    manager.database.insert_release(&matching2).await.unwrap();
    manager.database.insert_release(&other).await.unwrap();

    manager
        .delete_releases_with_content_hash("hash-shared")
        .await
        .unwrap();

    // Both releases carrying the re-imported folder's hash are gone; the
    // unrelated release survives. This is the overwrite the import worker
    // performs before inserting a re-import of the same folder tree.
    let remaining = manager
        .database
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, other.id);
}

/// Deleting one release of a multi-release album must tombstone its remote
/// cloud blobs — delete_release has to queue the cloud-outbox deletes like
/// delete_album/unmanage, or the remote blobs leak in the cloud (nothing else
/// processes the release once its rows are gone).
#[tokio::test]
async fn delete_release_tombstones_remote_cloud_blobs() {
    let (mut manager, _temp_dir) = setup_test_manager().await;
    manager.set_cleanup_delay(std::time::Duration::ZERO);

    let album = create_test_album();
    // Two releases so delete_release takes the album-survives branch.
    let release1 = create_test_release(&album.id); // remote: true
    let release2 = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();

    // release1 is remote with one cloud blob (the tombstone is enqueued for a
    // remote release's blob regardless of whether it's cached on this device).
    let file = DbFile::new(
        &release1.id,
        "track1.flac",
        5,
        crate::util::content_type::ContentType::Flac,
        Uuid::new_v4().to_string(),
        Utc::now(),
    );
    manager.add_file(&file).await.unwrap();

    manager.delete_release(&release1.id).await.unwrap();

    // delete_release awaits the deletion queueing, so by now the remote
    // blob's cloud-outbox tombstone is enqueued.
    let deletes = manager.database.get_pending_cloud_deletes().await.unwrap();
    assert_eq!(
        deletes.len(),
        1,
        "deleting a remote release tombstones its cloud blob"
    );
}

#[tokio::test]
async fn delete_release_fails_before_rows_are_deleted_when_file_cleanup_lookup_fails() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let file = DbFile::new(
        &release.id,
        "track1.flac",
        5,
        crate::util::content_type::ContentType::Flac,
        Uuid::new_v4().to_string(),
        Utc::now(),
    );
    manager.add_file(&file).await.unwrap();

    rename_table_for_test(&manager, "release_files", "release_files_unavailable").await;

    let error = manager.delete_release(&release.id).await.unwrap_err();
    assert!(matches!(error, LibraryError::Database(_)));
    assert!(manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn delete_release_fails_before_rows_are_deleted_when_file_tombstone_enqueue_fails() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let file = DbFile::new(
        &release.id,
        "track1.flac",
        5,
        crate::util::content_type::ContentType::Flac,
        Uuid::new_v4().to_string(),
        Utc::now(),
    );
    manager.add_file(&file).await.unwrap();

    rename_table_for_test(&manager, "cloud_outbox", "cloud_outbox_unavailable").await;

    let error = manager.delete_release(&release.id).await.unwrap_err();
    assert!(matches!(error, LibraryError::Database(_)));
    assert!(manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn delete_release_fails_before_rows_are_deleted_when_local_external_ref_cleanup_fails() {
    let (manager, temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .set_remote_for_test(&release.id, false)
        .await
        .unwrap();

    let file = DbFile::new(
        &release.id,
        "track1.flac",
        5,
        crate::util::content_type::ContentType::Flac,
        Uuid::new_v4().to_string(),
        Utc::now(),
    );
    manager.add_file(&file).await.unwrap();
    manager
        .database
        .register_release_external_refs_for_test(&release.id, temp_dir.path().to_str().unwrap())
        .await
        .unwrap();

    rename_table_for_test(&manager, "local_blob_refs", "local_blob_refs_unavailable").await;

    let error = manager.delete_release(&release.id).await.unwrap_err();
    assert!(matches!(error, LibraryError::Database(_)));
    assert!(manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn delete_album_fails_before_rows_are_deleted_when_track_lookup_fails() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    rename_table_for_test(&manager, "tracks", "tracks_unavailable").await;

    let error = manager.delete_album(&album.id).await.unwrap_err();
    assert!(matches!(error, LibraryError::Database(_)));
    assert!(manager
        .database
        .find_album_by_id(&album.id)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn delete_album_fails_before_rows_are_deleted_when_file_cleanup_lookup_fails() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    rename_table_for_test(&manager, "release_files", "release_files_unavailable").await;

    let error = manager.delete_album(&album.id).await.unwrap_err();
    assert!(matches!(error, LibraryError::Database(_)));
    assert!(manager
        .database
        .find_album_by_id(&album.id)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn playback_info_from_track_release_rejects_missing_album() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    let track = crate::db::DbTrack::new_test(&release.id, "track-a", "Track Title", Some(1));
    let track_artist = crate::db::DbTrackArtist::new(
        &track.id,
        "test-artist-id",
        0,
        Uuid::new_v4().to_string(),
        Utc::now(),
    );
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager.database.insert_track(&track).await.unwrap();
    manager
        .database
        .insert_track_artist(&track_artist)
        .await
        .unwrap();

    let mut broken_release = release.clone();
    broken_release.album_id = "missing-album".to_string();

    let error = playback_info_from_track_release(&manager.database, &track, &broken_release)
        .await
        .unwrap_err();
    assert!(
        matches!(error, LibraryError::TrackMapping(message) if message.contains("missing-album"))
    );
}

#[tokio::test]
async fn delete_release_fails_before_rows_are_deleted_when_cover_lookup_fails() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    store_test_cover_image(&manager, &release.id).await;

    rename_table_for_test(&manager, "covers", "covers_unavailable").await;

    let error = manager.delete_release(&release.id).await.unwrap_err();
    assert!(matches!(error, LibraryError::Database(_)));
    assert!(manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn delete_release_fails_before_rows_are_deleted_when_cover_tombstone_enqueue_fails() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    store_test_cover_image(&manager, &release.id).await;

    rename_table_for_test(&manager, "cloud_outbox", "cloud_outbox_unavailable").await;

    let error = manager.delete_release(&release.id).await.unwrap_err();
    assert!(matches!(error, LibraryError::Database(_)));
    assert!(manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .is_some());
}

/// Deleting a release cascade-deletes its `covers` row (the FK on `covers.id`
/// to `releases`), and the delete path cleans up the cover blob: a Remote
/// release's cover is tombstoned in the cloud and dropped from the cache.
#[tokio::test]
async fn delete_release_removes_its_cover_image() {
    let (mut manager, _temp_dir) = setup_test_manager().await;
    manager.set_cleanup_delay(std::time::Duration::ZERO);

    let album = create_test_album();
    // Two releases so the album survives the single-release delete.
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();

    // Give release1 a cover: a `covers` row plus its blob in one coven batch.
    // release1 is Remote (`create_test_release` defaults remote=true), so the
    // cover blob is in the cloud + cache.
    manager
        .store_library_image_blob(
            &crate::db::DbLibraryImage {
                id: release1.id.clone(),
                image_type: LibraryImageType::Cover,
                content_type: crate::util::content_type::ContentType::Jpeg,
                file_size: 5,
                width: None,
                height: None,
                source: "local".to_string(),
                source_url: None,
                cloud_path: None,
                created_at: manager.clock.now(),
            },
            b"image",
        )
        .await
        .unwrap();

    manager.delete_release(&release1.id).await.unwrap();

    // Row removed (the `covers` FK to `releases` cascade-deletes it).
    assert!(manager
        .get_library_image(&release1.id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .is_none());

    // Cloud blob delete enqueued under the `covers` namespace — the same key
    // the delete path derives, through the handle.
    let cloud_key = manager
        .handle()
        .blob_cloud_key(&LibraryManager::image_blob_ref(
            crate::sync::COVERS_NAMESPACE,
            &release1.id,
            None,
        ))
        .unwrap();
    let deletes = manager.database.get_pending_cloud_deletes().await.unwrap();
    assert!(
        deletes.iter().any(|d| d.cloud_key == cloud_key),
        "cover blob delete must be enqueued"
    );
}

/// delete_album removes each release's cover too (same helper, second wiring
/// site): the cover row is gone and its blob delete is enqueued.
#[tokio::test]
async fn delete_album_removes_release_covers() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager
        .store_library_image_blob(
            &crate::db::DbLibraryImage {
                id: release.id.clone(),
                image_type: LibraryImageType::Cover,
                content_type: crate::util::content_type::ContentType::Jpeg,
                file_size: 5,
                width: None,
                height: None,
                source: "local".to_string(),
                source_url: None,
                cloud_path: None,
                created_at: manager.clock.now(),
            },
            b"image",
        )
        .await
        .unwrap();

    manager.delete_album(&album.id).await.unwrap();

    assert!(manager
        .get_library_image(&release.id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .is_none());
    let cloud_key = manager
        .handle()
        .blob_cloud_key(&LibraryManager::image_blob_ref(
            crate::sync::COVERS_NAMESPACE,
            &release.id,
            None,
        ))
        .unwrap();
    let deletes = manager.database.get_pending_cloud_deletes().await.unwrap();
    assert!(deletes.iter().any(|d| d.cloud_key == cloud_key));
}

/// Drain the manager's library event channel and return the first
/// `ReleaseRemoved` seen, failing if the channel closes first.
async fn next_release_removed(
    rx: &mut broadcast::Receiver<LibraryEvent>,
) -> (String, String, Option<AlbumSummary>) {
    loop {
        match rx.recv().await {
            Ok(LibraryEvent::ReleaseRemoved {
                album_id,
                release_id,
                album,
            }) => return (album_id, release_id, album),
            Ok(_) => continue,
            Err(e) => panic!("library event channel closed before ReleaseRemoved: {e}"),
        }
    }
}

#[tokio::test]
async fn release_removed_carries_post_removal_parent_album() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();

    let mut rx = manager.subscribe_events();
    manager.delete_release(&release1.id).await.unwrap();

    let (album_id, release_id, summary) = next_release_removed(&mut rx).await;
    assert_eq!(album_id, album.id);
    assert_eq!(release_id, release1.id);
    let summary = summary.expect("album survives, so the event carries its summary");
    assert!(
        !summary.release_ids.contains(&release1.id),
        "deleted release must be gone from the post-removal summary"
    );
    assert!(
        summary.release_ids.contains(&release2.id),
        "surviving release must remain in the post-removal summary"
    );
}

#[tokio::test]
async fn release_removed_carries_none_when_album_no_longer_exists() {
    // The album-cascade case: the sync path (emit_sync_entity_changes) calls
    // emit_release_removed for a release whose album was already removed when
    // its last release went. delete_release's local path takes the
    // album-removed branch instead, so exercise emit_release_removed directly
    // against a missing album — the same call the sync path makes — and
    // assert it ships album: None rather than panicking in resolve.
    let (manager, _temp_dir) = setup_test_manager().await;

    let mut rx = manager.subscribe_events();
    manager
        .emit_release_removed("gone-album-id", "gone-release-id")
        .await;

    let (album_id, release_id, summary) = next_release_removed(&mut rx).await;
    assert_eq!(album_id, "gone-album-id");
    assert_eq!(release_id, "gone-release-id");
    assert!(
        summary.is_none(),
        "a removed album yields no summary, not a panic in AlbumSummary::from_raw"
    );
}

#[tokio::test]
async fn test_delete_album_deletes_all_releases() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();

    manager.delete_album(&album.id).await.unwrap();

    let album_result = manager.database.find_album_by_id(&album.id).await.unwrap();
    assert!(album_result.is_none());
    let releases = manager
        .database
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert!(releases.is_empty());
}

/// Insert (or overwrite) a release's `covers` row. The cover reference reads
/// the row (not the bytes), and each upsert stamps a fresh `_updated_at`, so
/// re-calling this moves the cover's version — what `change_cover` does when it
/// replaces a cover in place.
async fn add_cover_row(manager: &LibraryManager, release_id: &str) {
    manager
        .upsert_library_image(&crate::db::DbLibraryImage {
            id: release_id.to_string(),
            image_type: LibraryImageType::Cover,
            content_type: crate::util::content_type::ContentType::Jpeg,
            file_size: 5,
            width: None,
            height: None,
            source: "local".to_string(),
            source_url: None,
            cloud_path: None,
            created_at: manager.clock.now(),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn release_detail_has_no_cover_without_a_cover_row() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    let detail = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .expect("detail present for known release");
    assert!(detail.summary.cover.is_none());
}

/// A peer can ship `releases`/`release_files` rows with any `id` — bae's
/// apply path mints no id and validates none. A path-traversal id
/// (`../../etc/x`) is not a valid storage path token, so the display
/// resolver that fires on every sync cycle must treat it as a missing asset,
/// never panic. Before the fix, `find_release_detail` panics inside
/// `image_path`/`storage_path` on the bad id, which a synced row makes
/// durable — crash-looping every device each cycle (a denial of service).
#[tokio::test]
async fn find_release_detail_does_not_panic_on_traversal_ids_from_a_peer() {
    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    // A release whose id is a path-traversal token, as if synced from a peer.
    let release = DbRelease {
        id: "../../etc/x".to_string(),
        ..create_test_release(&album.id)
    };
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // An image file whose id is also a traversal token — it drives the
    // gallery's `image_path(&file.id)` resolution, and (as the release's
    // representative blob) the remote release's `is_pinned` pinned-cache
    // check; both must reject the bad id rather than panic.
    let file = DbFile::new(
        &release.id,
        "cover.jpg",
        5,
        crate::util::content_type::ContentType::Jpeg,
        "../../etc/y".to_string(),
        Utc::now(),
    );
    manager.add_file(&file).await.unwrap();

    // The resolver that fires when a synced release surfaces in the UI. The
    // bad ids must resolve to "no cover" / "no local gallery path", not a
    // panic.
    let detail = manager
        .find_release_detail(&release.id)
        .await
        .expect("resolving a release with a traversal id must not error")
        .expect("the inserted release must resolve to a detail");
    assert!(
        detail.summary.cover.is_none(),
        "a traversal release id has no cover row"
    );
    assert!(
        detail.gallery_items.iter().all(|item| matches!(
            item.source,
            crate::album_detail::GallerySource::ReleaseFile { .. }
        )),
        "with no cover row there is no cover slot; the traversal image file is a \
             release-file item, read by id"
    );
}

/// A release's cover reference carries the `covers` row's `_updated_at` as its
/// version, and overwriting the cover (re-upserting the row) moves that
/// version — the changed field that fires the UI's per-field re-render and
/// reloads the cover.
#[tokio::test]
async fn release_cover_version_moves_when_the_cover_row_is_reupserted() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    add_cover_row(&manager, &release.id).await;
    let before = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .unwrap()
        .summary
        .cover
        .expect("cover reference present once the row exists");
    assert_eq!(before.id, release.id, "the cover id is the release id");

    // Overwrite (what change_cover does): same row, fresh `_updated_at`.
    add_cover_row(&manager, &release.id).await;
    let after = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .unwrap()
        .summary
        .cover
        .expect("cover reference present after overwrite");
    assert_ne!(
        before.version, after.version,
        "overwriting the cover must move its version"
    );
}

/// Each storage-page row carries its own release's cover reference, not the
/// album's primary-release cover. Two releases of one album, each with its own
/// `covers` row, resolve to their own ids; a non-primary release's row resolves
/// to its own cover rather than the album's primary.
#[tokio::test]
async fn storage_page_rows_carry_each_releases_own_cover() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();
    // release1 is the album's primary, so its cover is the album-level cover.
    manager
        .set_album_primary_release(&album.id, &release1.id)
        .await
        .unwrap();

    add_cover_row(&manager, &release1.id).await;
    add_cover_row(&manager, &release2.id).await;

    let page = manager
        .get_storage_page(
            &crate::album_detail::StorageSort {
                field: crate::album_detail::StorageSortField::AlbumTitle,
                direction: crate::album_detail::StorageSortDirection::Ascending,
            },
            crate::album_detail::StorageFilter::All,
            0,
            100,
        )
        .await
        .unwrap();

    let row1 = page
        .rows
        .iter()
        .find(|r| r.release.id == release1.id)
        .expect("release1 row present");
    let row2 = page
        .rows
        .iter()
        .find(|r| r.release.id == release2.id)
        .expect("release2 row present");

    // Each release row resolves to that release's own cover.
    assert_eq!(row1.release.cover.as_ref().unwrap().id, release1.id);
    assert_eq!(row2.release.cover.as_ref().unwrap().id, release2.id);

    // The album carries the primary release's cover; the non-primary release's
    // row carries its own, distinct from the album's.
    assert_eq!(row2.album.cover.as_ref().unwrap().id, release1.id);
    assert_ne!(
        row2.release.cover.as_ref().unwrap().id,
        row2.album.cover.as_ref().unwrap().id
    );
}

#[tokio::test]
async fn album_detail_cover_is_versioned_and_moves_on_overwrite() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // No cover row yet: the detail carries no cover reference.
    let detail = manager
        .find_album_detail(&album.id)
        .await
        .unwrap()
        .expect("detail present for known album");
    assert!(detail.cover.is_none());

    add_cover_row(&manager, &release.id).await;
    let before = manager
        .find_album_detail(&album.id)
        .await
        .unwrap()
        .unwrap()
        .cover
        .expect("cover reference present once the row exists");
    assert_eq!(before.id, release.id);

    // Overwrite the cover (what change_cover does): fresh `_updated_at`.
    add_cover_row(&manager, &release.id).await;
    let after = manager
        .find_album_detail(&album.id)
        .await
        .unwrap()
        .unwrap()
        .cover
        .expect("cover reference present after overwrite");

    // The summary the UI re-renders against carries a changed version, which
    // fires the per-field re-render and reloads the cover.
    assert_ne!(before.version, after.version);
}

#[tokio::test]
async fn find_album_detail_returns_none_when_releases_vanish() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    manager.database.insert_album(&album).await.unwrap();

    let detail = manager
        .find_album_detail(&album.id)
        .await
        .expect("empty album aggregate must resolve without an error");

    assert!(detail.is_none());
}

#[tokio::test]
async fn find_release_detail_returns_some_for_known_id() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.pressing.year = None;
    release.pressing.format = None;
    release.release_name = None;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let detail = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .expect("detail present for known release");
    assert_eq!(detail.summary.id, release.id);
    assert_eq!(detail.summary.album_id, album.id);
    // Only release in album, no year/format/release_name → "Release 1".
    assert_eq!(detail.display_name, "Release 1");
    assert!(detail.tracks.is_empty());
    assert!(detail.files.is_empty());
}

#[tokio::test]
async fn find_release_detail_surfaces_seeded_tracks() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // Seed two tracks; the detail resolver must surface both with their
    // titles and track numbers (not just report emptiness).
    let t1 = crate::db::DbTrack::new_test(&release.id, "track-1", "Opening", Some(1));
    let t2 = crate::db::DbTrack::new_test(&release.id, "track-2", "Closing", Some(2));
    manager.database.insert_track(&t1).await.unwrap();
    manager.database.insert_track(&t2).await.unwrap();

    let detail = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .expect("detail present");
    assert_eq!(detail.tracks.len(), 2);
    let opening = detail
        .tracks
        .iter()
        .find(|t| t.title == "Opening")
        .expect("opening track surfaced");
    assert_eq!(opening.track_number, Some(1));
    assert!(
        detail.tracks.iter().any(|t| t.title == "Closing"),
        "closing track surfaced"
    );
}

#[tokio::test]
async fn gallery_includes_cloud_only_image_files_with_no_local_path() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // An image file for the release with no local copy on this device — the
    // release's images live only in the cloud here.
    let image = crate::db::DbFile::new(
        &release.id,
        "back.jpg",
        1234,
        crate::util::content_type::ContentType::Jpeg,
        Uuid::new_v4().to_string(),
        Utc::now(),
    );
    manager.add_file(&image).await.unwrap();

    let detail = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .expect("detail present");

    // The lightbox shows every image the release has: the image file is
    // surfaced as a gallery item read by file id (fetched on demand), a
    // release-file source, not the cover slot.
    let item = detail
        .gallery_items
        .iter()
        .find(|g| g.id == image.id)
        .expect("image file surfaced in gallery");
    assert_eq!(item.label, "back.jpg");
    assert!(
        matches!(
            item.source,
            crate::album_detail::GallerySource::ReleaseFile { .. }
        ),
        "a release-file image is read by file id, not as the cover"
    );
}

/// `change_cover` resizes whatever the user picks to a ≤600 JPEG thumbnail
/// before storing it: a 900×300 PNG release image lands as a 600×200 JPEG blob
/// (downscaled to fit 600, aspect kept), and the `covers` row records JPEG.
#[tokio::test]
async fn change_cover_stores_a_resized_jpeg_thumbnail() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.remote = false;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // An oversized non-JPEG release image on disk, registered as the release's
    // user-provided file so `change_cover` reads it back through coven.
    let source_dir = TempDir::new().unwrap();
    let cover_bytes = {
        let img = ::image::RgbImage::from_pixel(900, 300, ::image::Rgb([20, 160, 90]));
        let mut buf = std::io::Cursor::new(Vec::new());
        ::image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, ::image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    };
    std::fs::write(source_dir.path().join("art.png"), &cover_bytes).unwrap();
    let file = DbFile::new(
        &release.id,
        "art.png",
        cover_bytes.len() as i64,
        ContentType::Png,
        Uuid::new_v4().to_string(),
        Utc::now(),
    );
    manager.add_file(&file).await.unwrap();
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &source_dir.path().to_string_lossy())
        .await
        .unwrap();

    manager
        .change_cover(
            &album.id,
            &release.id,
            CoverSelection::ReleaseImage {
                file_id: file.id.clone(),
            },
        )
        .await
        .unwrap();

    // The stored blob decodes as a ≤600 JPEG, not the 900×300 PNG source.
    let stored = manager
        .read_cover_image_blob(&release.id)
        .await
        .unwrap()
        .expect("cover blob stored");
    assert_eq!(
        ::image::guess_format(&stored).unwrap(),
        ::image::ImageFormat::Jpeg
    );
    let decoded = ::image::load_from_memory(&stored).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (600, 200));

    // The row describes the stored thumbnail: JPEG, and its size matches.
    let row = manager
        .get_library_image(&release.id, &LibraryImageType::Cover)
        .await
        .unwrap()
        .expect("cover row stored");
    assert_eq!(row.content_type, ContentType::Jpeg);
    assert_eq!(row.file_size, stored.len() as i64);
}

/// Queueing an album expands to its PRIMARY release's tracks, not the
/// earliest-imported one. When the user picks a non-default primary (e.g. a
/// later remaster over the original vinyl rip), enqueueing the album must
/// play the chosen release.
#[tokio::test]
async fn resolve_to_track_ids_expands_album_to_primary_release() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    // release1 is imported first (older created_at, so it sorts first);
    // release2 is the user's chosen primary.
    let mut release1 = create_test_release(&album.id);
    release1.created_at = Utc::now() - chrono::Duration::days(1);
    let release2 = create_test_release(&album.id);

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();

    let old = crate::db::DbTrack::new_test(&release1.id, "r1-t1", "Old A", Some(1));
    let p1 = crate::db::DbTrack::new_test(&release2.id, "r2-t1", "New A", Some(1));
    let p2 = crate::db::DbTrack::new_test(&release2.id, "r2-t2", "New B", Some(2));
    manager.database.insert_track(&old).await.unwrap();
    manager.database.insert_track(&p1).await.unwrap();
    manager.database.insert_track(&p2).await.unwrap();

    manager
        .set_album_primary_release(&album.id, &release2.id)
        .await
        .unwrap();

    let resolved = manager
        .resolve_to_track_ids(std::slice::from_ref(&album.id))
        .await
        .unwrap();
    assert!(resolved.contains(&"r2-t1".to_string()));
    assert!(resolved.contains(&"r2-t2".to_string()));
    assert!(
        !resolved.contains(&"r1-t1".to_string()),
        "must not expand to the non-primary release's tracks"
    );
    assert_eq!(resolved.len(), 2);
}

#[tokio::test]
async fn resolve_to_track_ids_rejects_unknown_id() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let err = manager
        .resolve_to_track_ids(&["missing-id".to_string()])
        .await
        .unwrap_err();
    assert!(
        matches!(err, LibraryError::TrackMapping(message) if message.contains("missing-id")),
        "unknown ids must fail instead of being treated as track ids"
    );
}

#[tokio::test]
async fn find_release_detail_display_name_uses_year_format_fallback() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.pressing.year = Some(2024);
    release.pressing.format = Some("CD".to_string());
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let detail = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.display_name, "2024 CD");
}

#[tokio::test]
async fn find_release_detail_display_name_prefers_release_name() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.release_name = Some("Deluxe Edition".to_string());
    release.pressing.year = Some(2024);
    release.pressing.format = Some("CD".to_string());
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let detail = manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.display_name, "Deluxe Edition");
}

#[tokio::test]
async fn find_release_detail_uses_position_for_second_release() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release1 = create_test_release(&album.id);
    release1.pressing.year = None;
    release1.pressing.format = None;
    let mut release2 = create_test_release(&album.id);
    release2.pressing.year = None;
    release2.pressing.format = None;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();

    let detail2 = manager
        .find_release_detail(&release2.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail2.display_name, "Release 2");
}

#[tokio::test]
async fn find_release_detail_returns_none_for_unknown_id() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let detail = manager.find_release_detail("nonexistent-id").await.unwrap();
    assert!(detail.is_none());
}

// ── Storage page tests ───────────────────────────────────────────

/// Insert N albums each with one release; return `(albums, releases)`.
/// Each album's title is `"Album {i}"` so ordering is deterministic.
async fn seed_albums(manager: &LibraryManager, count: usize) -> (Vec<DbAlbum>, Vec<DbRelease>) {
    let mut albums = Vec::new();
    let mut releases = Vec::new();
    for i in 0..count {
        let mut album = create_test_album();
        album.title = format!("Album {i}");
        let release = create_test_release(&album.id);
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
        albums.push(album);
        releases.push(release);
    }
    (albums, releases)
}

fn sort_by_album_title_asc() -> StorageSort {
    StorageSort {
        field: StorageSortField::AlbumTitle,
        direction: StorageSortDirection::Ascending,
    }
}

#[tokio::test]
async fn storage_page_returns_all_rows_for_all_filter() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_albums(&manager, 3).await;

    let page = manager
        .get_storage_page(&sort_by_album_title_asc(), StorageFilter::All, 0, 10)
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 3);
    assert_eq!(page.total_count, 3);
}

#[tokio::test]
async fn storage_page_paginates() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_albums(&manager, 5).await;

    let page1 = manager
        .get_storage_page(&sort_by_album_title_asc(), StorageFilter::All, 0, 2)
        .await
        .unwrap();
    let page2 = manager
        .get_storage_page(&sort_by_album_title_asc(), StorageFilter::All, 2, 2)
        .await
        .unwrap();
    let page3 = manager
        .get_storage_page(&sort_by_album_title_asc(), StorageFilter::All, 4, 2)
        .await
        .unwrap();

    assert_eq!(page1.rows.len(), 2);
    assert_eq!(page2.rows.len(), 2);
    assert_eq!(page3.rows.len(), 1);
    // total_count is the full filtered universe, not the page.
    assert_eq!(page1.total_count, 5);
    assert_eq!(page2.total_count, 5);
    assert_eq!(page3.total_count, 5);
}

#[tokio::test]
async fn storage_page_sorts_album_title_ascending() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_albums(&manager, 3).await;

    let page = manager
        .get_storage_page(&sort_by_album_title_asc(), StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Album 0", "Album 1", "Album 2"]);
}

#[tokio::test]
async fn storage_page_sorts_album_title_descending() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_albums(&manager, 3).await;

    let sort = StorageSort {
        field: StorageSortField::AlbumTitle,
        direction: StorageSortDirection::Descending,
    };
    let page = manager
        .get_storage_page(&sort, StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Album 2", "Album 1", "Album 0"]);
}

/// Each storage-page row carries the state-appropriate `storage_actions`
/// the Storage Manager row context menu renders — pinned offers unpin +
/// unmanage, cloud-only offers pin + unmanage, local offers manage.
/// With a cloud home present every remote/local transition is open.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn storage_page_rows_carry_state_appropriate_actions() {
    use crate::album_detail::ReleaseStorageAction::{MakeLocal, MakeRemote, Pin, Unpin};

    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;

    // Pinned: made Remote with pin, so its blob lands in coven's offline cache.
    let pinned = make_remote_release(
        &manager,
        &temp_dir.path().join("pinned"),
        "Pinned Album",
        true,
    )
    .await;
    // Cloud-only: made Remote without pin, so its blob is evictable, not pinned.
    let cloud_only = make_remote_release(
        &manager,
        &temp_dir.path().join("cloud"),
        "Cloud Album",
        false,
    )
    .await;

    // Local: not remote, files at a local path.
    let mut local_album = create_test_album();
    local_album.title = "Local Album".to_string();
    let mut local = create_test_release(&local_album.id);
    local.remote = false;
    manager.database.insert_album(&local_album).await.unwrap();
    manager.database.insert_release(&local).await.unwrap();
    manager
        .database
        .register_release_external_refs_for_test(&local.id, "/tmp/local")
        .await
        .unwrap();

    let page = manager
        .get_storage_page(&sort_by_album_title_asc(), StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let actions: std::collections::HashMap<_, _> = page
        .rows
        .iter()
        .map(|r| (r.release.id.clone(), r.release.storage_actions.clone()))
        .collect();

    assert_eq!(actions[&pinned], vec![Unpin, MakeLocal]);
    assert_eq!(actions[&cloud_only], vec![Pin, MakeLocal]);
    assert_eq!(actions[&local.id], vec![MakeRemote]);
}

/// With no cloud home, no remote storage exists, so the rows offer no
/// transitions — the context menu is empty everywhere.
#[tokio::test]
async fn storage_page_rows_have_no_actions_without_cloud_home() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.remote = false;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .register_release_external_refs_for_test(&release.id, "/tmp/local")
        .await
        .unwrap();

    let page = manager
        .get_storage_page(&sort_by_album_title_asc(), StorageFilter::All, 0, 10)
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    assert!(page.rows[0].release.storage_actions.is_empty());
}

#[tokio::test]
async fn storage_page_local_filter_matches_local_path() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album_remote = create_test_album();
    let mut album_local = create_test_album();
    album_local.title = "Local Album".to_string();
    let remote_release = create_test_release(&album_remote.id);
    let mut local_release = create_test_release(&album_local.id);
    local_release.remote = false;

    manager.database.insert_album(&album_remote).await.unwrap();
    manager.database.insert_album(&album_local).await.unwrap();
    manager
        .database
        .insert_release(&remote_release)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&local_release)
        .await
        .unwrap();
    manager
        .database
        .register_release_external_refs_for_test(&local_release.id, "/tmp/local")
        .await
        .unwrap();

    let local = manager
        .get_storage_page(&sort_by_album_title_asc(), StorageFilter::Local, 0, 10)
        .await
        .unwrap();
    assert_eq!(local.rows.len(), 1);
    assert_eq!(local.total_count, 1);
    assert_eq!(local.rows[0].release.id, local_release.id);
    assert_eq!(
        local.rows[0].release.storage_state,
        crate::album_detail::ReleaseStorageState::Local
    );

    let remote = manager
        .get_storage_page(&sort_by_album_title_asc(), StorageFilter::Remote, 0, 10)
        .await
        .unwrap();
    assert_eq!(remote.rows.len(), 1);
    assert_eq!(remote.rows[0].release.id, remote_release.id);
    assert_ne!(
        remote.rows[0].release.storage_state,
        crate::album_detail::ReleaseStorageState::Local
    );
}

#[tokio::test]
async fn storage_count_matches_filtered_page_total() {
    let (manager, _temp_dir) = setup_test_manager().await;
    // Three albums, one release each. Mark the second release
    // local at insert time so filters produce distinct counts.
    let mut inserted_local = None;
    for i in 0..3 {
        let mut album = create_test_album();
        album.title = format!("Album {i}");
        let mut release = create_test_release(&album.id);
        let local = i == 1;
        if local {
            release.remote = false;
            inserted_local = Some(release.id.clone());
        }
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
        if local {
            manager
                .database
                .register_release_external_refs_for_test(&release.id, "/tmp/local")
                .await
                .unwrap();
        }
    }

    assert_eq!(
        manager.get_storage_count(StorageFilter::All).await.unwrap(),
        3
    );
    assert_eq!(
        manager
            .get_storage_count(StorageFilter::Local)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        manager
            .get_storage_count(StorageFilter::Remote)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        manager
            .get_storage_count(StorageFilter::Uploading)
            .await
            .unwrap(),
        0
    );

    let all_page = manager
        .get_storage_page(&sort_by_album_title_asc(), StorageFilter::All, 0, 10)
        .await
        .unwrap();
    assert_eq!(all_page.total_count, 3);

    let local_page = manager
        .get_storage_page(&sort_by_album_title_asc(), StorageFilter::Local, 0, 10)
        .await
        .unwrap();
    assert_eq!(local_page.rows.len(), 1);
    assert_eq!(local_page.rows[0].release.id, inserted_local.unwrap());
}

/// Connect a real `SyncManager` over an in-memory cloud home (opaque,
/// encrypted) so the manager's cloud read/write/transition paths run against
/// it — the in-module counterpart of the integration tests' `setup_with_cloud`.
/// After this, `has_cloud_home()` and `is_sync_ready()` both hold.
#[cfg(feature = "test-utils")]
async fn connect_test_cloud(manager: &LibraryManager) {
    manager
        .connect_test_cloud_home(
            Arc::new(InMemoryCloudHome::new()),
            CloudCipher::Encrypted(EncryptionService::new_with_key(&[7u8; 32])),
        )
        .await
        .expect("connect in-memory cloud home");
}

/// Create a Remote release the real way: a Local release with one source file
/// on disk (coven's external ref), made Remote (`pin` keeps its blob in the
/// offline cache) and drained so the gate flips. Returns its id. The manager
/// must already be connected via [`connect_test_cloud`].
#[cfg(feature = "test-utils")]
async fn make_remote_release(
    manager: &LibraryManager,
    dir: &std::path::Path,
    album_title: &str,
    pin: bool,
) -> String {
    make_remote_release_with_files(
        manager,
        dir,
        album_title,
        &[("track.flac", b"track-bytes")],
        pin,
    )
    .await
}

#[cfg(feature = "test-utils")]
async fn make_remote_release_with_files(
    manager: &LibraryManager,
    dir: &std::path::Path,
    album_title: &str,
    files: &[(&str, &[u8])],
    pin: bool,
) -> String {
    let mut album = create_test_album();
    album.title = album_title.to_string();
    let mut release = create_test_release(&album.id);
    release.remote = false;
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    std::fs::create_dir_all(dir).unwrap();
    let created_at = Utc::now();
    for (index, (name, bytes)) in files.iter().enumerate() {
        std::fs::write(dir.join(name), bytes).unwrap();
        let file = DbFile::new(
            &release.id,
            name,
            bytes.len() as i64,
            crate::util::content_type::ContentType::Flac,
            format!("{}-test-file-{index}", release.id),
            created_at,
        );
        manager.add_file(&file).await.unwrap();
    }
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &dir.to_string_lossy())
        .await
        .unwrap();
    manager.coven_make_remote(&release.id, pin).await.unwrap();
    let n = manager.drain_uploads_for_test().await.unwrap();
    assert_eq!(n as usize, files.len(), "each release blob uploaded");
    release.id
}

/// Insert a local release whose `local_path` points at a
/// nonexistent directory, so no local copy resolves on this device. Seeds
/// a `DbFile` row so the release is otherwise complete.
async fn insert_local_release_without_local_files(
    manager: &LibraryManager,
    album_id: &str,
) -> DbRelease {
    let mut release = create_test_release(album_id);
    release.remote = false;
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .register_release_external_refs_for_test(
            &release.id,
            &format!("/nonexistent/origin-device/{}", Uuid::new_v4()),
        )
        .await
        .unwrap();

    let file = DbFile::new(
        &release.id,
        "track1.flac",
        5,
        crate::util::content_type::ContentType::Flac,
        Uuid::new_v4().to_string(),
        Utc::now(),
    );
    manager.add_file(&file).await.unwrap();
    release
}

/// The read layer surfaces a local release even when no local copy
/// resolves on this device — there is no availability filter to hide one.
/// The substrate gate (coven's `gated_by_descendants`) prunes such a
/// release's album from a *peer's* sync entirely, so a receiver never
/// materializes an orphan album; nothing is hidden on read here.
#[tokio::test]
async fn surfaces_local_release_with_no_local_files() {
    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    manager.database.insert_album(&album).await.unwrap();
    let release = insert_local_release_without_local_files(&manager, &album.id).await;

    // Grid and count include the album.
    let page = manager.get_album_page(&[], 0, 10).await.unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].release_ids, vec![release.id.clone()]);
    assert_eq!(manager.get_album_count().await.unwrap(), 1);

    // Album detail carries the release.
    let detail = manager
        .find_album_detail(&album.id)
        .await
        .unwrap()
        .expect("album surfaces");
    let detail_ids: Vec<_> = detail
        .releases
        .iter()
        .map(|r| r.summary.id.clone())
        .collect();
    assert_eq!(detail_ids, vec![release.id.clone()]);

    // The release-level resolver returns it.
    assert!(manager
        .find_release_detail(&release.id)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn storage_page_sort_by_artist_names() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Two artists with distinct sort-orderings; ArtistNames sort triggers
    // the `needs_artist_sort_join` branch.
    for (artist_id, artist_name) in &[("a-zulu", "Zulu"), ("a-alpha", "Alpha")] {
        let artist = DbArtist {
            id: artist_id.to_string(),
            name: artist_name.to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: Utc::now(),
        };
        manager.database.insert_artist(&artist).await.unwrap();
        let mut album = create_test_album();
        album.title = format!("Album by {artist_name}");
        album.artist_id = artist_id.to_string();
        let release = create_test_release(&album.id);
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
    }

    let asc = StorageSort {
        field: StorageSortField::ArtistNames,
        direction: StorageSortDirection::Ascending,
    };
    let page = manager
        .get_storage_page(&asc, StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let names: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(names, vec!["Album by Alpha", "Album by Zulu"]);

    let desc = StorageSort {
        field: StorageSortField::ArtistNames,
        direction: StorageSortDirection::Descending,
    };
    let page = manager
        .get_storage_page(&desc, StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let names: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(names, vec!["Album by Zulu", "Album by Alpha"]);
}

#[tokio::test]
async fn storage_page_sort_by_format_nulls_last() {
    let (manager, _temp_dir) = setup_test_manager().await;

    for (title, format) in &[
        ("Album No Format", None),
        ("Album CD", Some("CD")),
        ("Album Vinyl", Some("Vinyl")),
    ] {
        let mut album = create_test_album();
        album.title = title.to_string();
        let mut release = create_test_release(&album.id);
        release.pressing.format = format.map(str::to_string);
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
    }

    let asc = StorageSort {
        field: StorageSortField::Format,
        direction: StorageSortDirection::Ascending,
    };
    let page = manager
        .get_storage_page(&asc, StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    // NULL format sorts last in both directions.
    assert_eq!(titles, vec!["Album CD", "Album Vinyl", "Album No Format"]);
}

#[tokio::test]
async fn storage_page_sort_by_file_count() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Three releases, each with a distinct number of files.
    for (title, file_count) in &[("Album A", 1usize), ("Album B", 3), ("Album C", 2)] {
        let mut album = create_test_album();
        album.title = title.to_string();
        let release = create_test_release(&album.id);
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
        for i in 0..*file_count {
            let file = DbFile {
                id: format!("{}-file-{i}", release.id),
                release_id: release.id.clone(),
                original_filename: format!("{i}.flac"),
                file_size: 1000,
                content_type: crate::util::content_type::ContentType::Flac,
                cloud_path: None,
                created_at: Utc::now(),
            };
            manager.database.insert_file(&file).await.unwrap();
        }
    }

    let desc = StorageSort {
        field: StorageSortField::FileCount,
        direction: StorageSortDirection::Descending,
    };
    let page = manager
        .get_storage_page(&desc, StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Album B", "Album C", "Album A"]);
}

#[tokio::test]
async fn storage_page_sort_by_total_size() {
    let (manager, _temp_dir) = setup_test_manager().await;

    for (title, file_size) in &[("Small", 100i64), ("Big", 10_000), ("Medium", 1_000)] {
        let mut album = create_test_album();
        album.title = title.to_string();
        let release = create_test_release(&album.id);
        manager.database.insert_album(&album).await.unwrap();
        manager.database.insert_release(&release).await.unwrap();
        let file = DbFile {
            id: format!("{}-file", release.id),
            release_id: release.id.clone(),
            original_filename: "a.flac".to_string(),
            file_size: *file_size,
            content_type: crate::util::content_type::ContentType::Flac,
            cloud_path: None,
            created_at: Utc::now(),
        };
        manager.database.insert_file(&file).await.unwrap();
    }

    let asc = StorageSort {
        field: StorageSortField::TotalSize,
        direction: StorageSortDirection::Ascending,
    };
    let page = manager
        .get_storage_page(&asc, StorageFilter::All, 0, 10)
        .await
        .unwrap();
    let titles: Vec<_> = page.rows.iter().map(|r| r.album.title.clone()).collect();
    assert_eq!(titles, vec!["Small", "Medium", "Big"]);
}

#[tokio::test]
async fn storage_page_uploading_filter_matches_cloud_outbox() {
    let (manager, _temp_dir) = setup_test_manager().await;

    let album_uploading = create_test_album();
    let album_quiet = create_test_album();
    let release_uploading = create_test_release(&album_uploading.id);
    let release_quiet = create_test_release(&album_quiet.id);

    manager
        .database
        .insert_album(&album_uploading)
        .await
        .unwrap();
    manager.database.insert_album(&album_quiet).await.unwrap();
    manager
        .database
        .insert_release(&release_uploading)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_quiet)
        .await
        .unwrap();

    // Seed a file + outbox upload entry on one release only.
    let uploading_file = DbFile {
        id: format!("{}-file", release_uploading.id),
        release_id: release_uploading.id.clone(),
        original_filename: "a.flac".to_string(),
        file_size: 1000,
        content_type: crate::util::content_type::ContentType::Flac,
        cloud_path: None,
        created_at: Utc::now(),
    };
    manager.database.insert_file(&uploading_file).await.unwrap();
    manager
        .database
        .add_cloud_outbox_upload(&uploading_file.id, "cloud-key", None, false)
        .await
        .unwrap();

    let sort = StorageSort {
        field: StorageSortField::AlbumTitle,
        direction: StorageSortDirection::Ascending,
    };
    let page = manager
        .get_storage_page(&sort, StorageFilter::Uploading, 0, 10)
        .await
        .unwrap();
    assert_eq!(page.total_count, 1);
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].release.id, release_uploading.id);
    assert_eq!(
        manager
            .get_storage_count(StorageFilter::Uploading)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn cancel_release_transition_fires_a_registered_transfer_token() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // A registered unmanage token is fired by the unified cancel.
    let token = crate::library::CancellationToken::new();
    manager
        .transfer_cancels
        .lock()
        .unwrap()
        .insert("rel-x".to_string(), token.clone());
    manager.cancel_release_transition("rel-x").await.unwrap();
    assert!(token.is_cancelled(), "transfer token fired");

    // Nothing in progress for an unknown release → no-op, no error.
    manager.cancel_release_transition("rel-none").await.unwrap();
}

// Needs the test-utils mock cloud home: a Remote release implies a connected
// home, which the make-Local read storage is built over (the cancel fires
// before any blob is read, so the home is never actually called).
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn unmanage_cancelled_before_copy_leaves_release_remote() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let album = create_test_album();
    let release = create_test_release(&album.id); // remote: true
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .insert_file(&DbFile {
            id: format!("{}-f", release.id),
            release_id: release.id.clone(),
            original_filename: "a.flac".to_string(),
            file_size: 10,
            content_type: crate::util::content_type::ContentType::Flac,
            cloud_path: None,
            created_at: Utc::now(),
        })
        .await
        .unwrap();

    // A token cancelled before the materialize loop runs: coven aborts at the
    // first check, before reading/writing any blob, and never flips state. A
    // cancelled make-Local is a clean stop (Ok), not a failure.
    let token = crate::library::CancellationToken::new();
    token.cancel();
    let dest = temp_dir.path().join("out");
    manager
        .coven_make_local(&release.id, dest.to_str().unwrap(), &token)
        .await
        .expect("a cancelled make-Local ends cleanly");

    let after = manager
        .get_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        after.remote,
        "cancelled make-Local leaves the release remote"
    );
}

#[tokio::test]
async fn outbox_snapshot_tracks_queued_active_failed_and_cancel() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    let file = DbFile {
        id: format!("{}-file", release.id),
        release_id: release.id.clone(),
        original_filename: "a.flac".to_string(),
        file_size: 1000,
        content_type: crate::util::content_type::ContentType::Flac,
        cloud_path: None,
        created_at: Utc::now(),
    };
    manager.database.insert_file(&file).await.unwrap();
    manager
        .add_cloud_outbox_upload(&file.id, "cloud-key", None, false)
        .await
        .unwrap();

    // Freshly queued: per-release count is 1 queued, joined to the album title.
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.total.queued, 1);
    assert_eq!(snap.total.active, 0);
    assert_eq!(snap.total.failed, 0);
    assert_eq!(snap.total.bytes_total, 1000);
    assert_eq!(snap.total.bytes_done, 0);
    assert_eq!(snap.upload_groups.len(), 1);
    let group = &snap.upload_groups[0];
    assert_eq!(group.display_title, "Test Album");
    assert_eq!(group.release_id.as_deref(), Some(release.id.as_str()));
    assert_eq!(group.file_count, 1);
    assert_eq!(group.progress.queued, 1);
    assert_eq!(group.progress.bytes_total, 1000);
    let item_id = manager.database.get_pending_cloud_uploads().await.unwrap()[0].id;

    // In flight now: the in-memory map flips it to active, starting at zero
    // bytes done.
    manager
        .sync
        .outbox_in_flight()
        .lock()
        .unwrap()
        .insert(file.id.clone(), 0);
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.total.active, 1);
    assert_eq!(snap.total.queued, 0);
    assert_eq!(snap.upload_groups[0].progress.active, 1);
    assert_eq!(snap.total.bytes_done, 0);

    // Mid-upload progress advances the live byte count: the snapshot's
    // per-release and aggregate bytes_done climb without the file
    // completing.
    manager
        .sync
        .outbox_in_flight()
        .lock()
        .unwrap()
        .insert(file.id.clone(), 400);
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.upload_groups[0].progress.active, 1);
    assert_eq!(snap.upload_groups[0].progress.bytes_done, 400);
    assert_eq!(snap.total.bytes_done, 400);
    assert_eq!(snap.total.bytes_total, 1000);
    manager
        .sync
        .outbox_in_flight()
        .lock()
        .unwrap()
        .remove(&file.id);

    // A recorded failure: failed with the stored error + attempt.
    manager
        .database
        .record_cloud_upload_failure(item_id, "boom", "2024-06-01T00:00:00Z")
        .await
        .unwrap();
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.total.failed, 1);
    assert_eq!(snap.total.queued, 0);
    assert_eq!(snap.upload_groups[0].progress.failed, 1);
    let rows = manager.database.outbox_items().await.unwrap();
    assert_eq!(rows[0].attempt_count, 1);
    assert_eq!(rows[0].last_error.as_deref(), Some("boom"));

    // Reset backoff clears the timestamp but keeps the failure record.
    manager.database.reset_cloud_outbox_backoff().await.unwrap();
    let uploads = manager.database.get_pending_cloud_uploads().await.unwrap();
    assert_eq!(uploads[0].attempt_count, 1);
    assert!(uploads[0].last_attempt_at.is_none());
    let rows = manager.database.outbox_items().await.unwrap();
    assert_eq!(rows[0].last_error.as_deref(), Some("boom"));

    // Cancel dequeues the entry; the snapshot empties.
    manager.cancel_outbox_item(item_id).await.unwrap();
    let snap = manager.outbox_snapshot().await.unwrap();
    assert!(snap.upload_groups.is_empty());
    assert_eq!(snap.total.failed, 0);
}

/// The real `ReleaseUploadObserver` drives the snapshot's live byte count:
/// `on_blob_upload_progress` advances an in-flight `Active` file's
/// `bytes_done` so the aggregate and per-release bars move mid-file.
#[tokio::test]
async fn observer_progress_advances_snapshot_bytes_done() {
    use coven::BlobTransitionObserver;

    let (manager, _temp_dir) = setup_test_manager().await;
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    let file = DbFile {
        id: format!("{}-file", release.id),
        release_id: release.id.clone(),
        original_filename: "a.flac".to_string(),
        file_size: 1000,
        content_type: crate::util::content_type::ContentType::Flac,
        cloud_path: None,
        created_at: Utc::now(),
    };
    manager.database.insert_file(&file).await.unwrap();
    manager
        .add_cloud_outbox_upload(&file.id, "cloud-key", None, false)
        .await
        .unwrap();

    // The observer shares the manager's in-flight map and throughput tracker,
    // exactly as production wires it in `build_sync_manager`.
    let observer = crate::sync::upload_observer::ReleaseUploadObserver::new(
        manager.sync.outbox_in_flight(),
        manager.sync.upload_throughput(),
        manager.sync.sync_paused(),
        manager.event_tx.clone(),
    );
    observer.set_database(Arc::new(manager.database.clone()));

    observer.on_blob_upload_started(&file.id).await;
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.upload_groups[0].progress.active, 1);
    assert_eq!(snap.upload_groups[0].progress.bytes_done, 0);
    assert_eq!(snap.total.bytes_done, 0);

    // A mid-upload progress report advances the live count without the file
    // completing.
    observer.on_blob_upload_progress(&file.id, 600, 1000).await;
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.upload_groups[0].progress.active, 1);
    assert_eq!(snap.upload_groups[0].progress.bytes_done, 600);
    assert_eq!(snap.total.bytes_done, 600);
    // The rolling-window tracker saw the 600-byte delta, so the rate is
    // non-zero before the file even finishes.
    assert!(manager.sync.upload_throughput().bytes_per_sec() > 0);

    // Completion clears the in-flight entry; the row's still queued in the
    // DB (this test drives only the observer, not coven's removal), so it
    // reads back as queued with bytes_done reset to 0.
    observer.on_blob_uploaded(&file.id).await;
    let snap = manager.outbox_snapshot().await.unwrap();
    assert_eq!(snap.total.active, 0);
    assert_eq!(snap.upload_groups[0].progress.queued, 1);
}

/// Insert a remote, not-pinned release with one file and return its id.
/// `remote: true` + no pinned cache copy makes it eligible for pinning.
async fn insert_pinnable_release(manager: &LibraryManager) -> String {
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    let file = DbFile {
        id: format!("{}-file", release.id),
        release_id: release.id.clone(),
        original_filename: "a.flac".to_string(),
        file_size: 1000,
        content_type: crate::util::content_type::ContentType::Flac,
        cloud_path: None,
        created_at: Utc::now(),
    };
    manager.database.insert_file(&file).await.unwrap();
    release.id
}

/// Pausing before the first enqueue parks the worker, so the queue's
/// in-memory state (enqueue, dedup, snapshot counts, cancel) is observable
/// deterministically without the download path racing the assertions.
#[tokio::test]
async fn download_queue_enqueue_dedups_and_cancels_while_paused() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let release_id = insert_pinnable_release(&manager).await;

    // Park the worker up front so nothing drains while we inspect state.
    manager.set_downloads_paused(true);

    manager.enqueue_pins(vec![release_id.clone()]).await;
    let snap = manager.download_snapshot();
    assert_eq!(snap.total.queued, 1);
    assert_eq!(snap.ops.len(), 1);
    assert_eq!(snap.ops[0].title, "Test Album");
    assert_eq!(snap.ops[0].file_count, 1);
    assert_eq!(snap.ops[0].state, crate::library::DownloadState::Queued);
    assert!(snap.paused);

    // Re-enqueuing the same release is a no-op: still one entry.
    manager.enqueue_pins(vec![release_id.clone()]).await;
    assert_eq!(manager.download_snapshot().ops.len(), 1);

    // Cancel drops the entry.
    manager.cancel_download(&release_id);
    let snap = manager.download_snapshot();
    assert!(snap.ops.is_empty());
}

/// An already-pinned release is skipped at enqueue rather than re-downloaded.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn download_queue_skips_already_pinned() {
    let (manager, temp_dir) = setup_test_manager().await;
    manager.set_downloads_paused(true);

    // A genuinely pinned release: made Remote with pin, so its blob lands in
    // coven's offline cache.
    connect_test_cloud(&manager).await;
    let release_id = make_remote_release(
        &manager,
        &temp_dir.path().join("pinned"),
        "Test Album",
        true,
    )
    .await;
    let summary = manager
        .find_release_storage_summary(&release_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        summary.storage_state,
        crate::album_detail::ReleaseStorageState::Remote
    );
    assert!(
        summary.pinned,
        "the offline-cached blob makes it read as pinned"
    );

    manager.enqueue_pins(vec![release_id.clone()]).await;
    assert!(manager.download_snapshot().ops.is_empty());
}

/// A pin that fails (no cloud home for a cloud-only release) lands `Failed`
/// and stays in the queue; `retry_downloads` flips it back to `Queued`.
#[tokio::test]
async fn download_queue_failed_pin_retries() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let release_id = insert_pinnable_release(&manager).await;

    // No cloud home + no local copy ⇒ the pin can't read the file and fails.
    manager.enqueue_pins(vec![release_id.clone()]).await;

    // Let the worker pick it up, fail, and mark it Failed. Poll the snapshot
    // rather than sleeping a fixed interval.
    let failed = wait_for(|| {
        matches!(
            manager.download_snapshot().ops.first().map(|op| &op.state),
            Some(crate::library::DownloadState::Failed { .. })
        )
    })
    .await;
    assert!(failed, "the pin should land Failed without a cloud home");
    assert_eq!(manager.download_snapshot().total.failed, 1);

    // Retry flips it back to Queued; with no cloud home it'll fail again,
    // but the immediate post-retry state is Queued (or already re-failed).
    manager.retry_downloads();
    let snap = manager.download_snapshot();
    assert!(
        snap.ops.first().is_some_and(|op| matches!(
            op.state,
            crate::library::DownloadState::Queued
                | crate::library::DownloadState::Active { .. }
                | crate::library::DownloadState::Failed { .. }
        )),
        "after retry the release is still tracked"
    );

    // Cancelling clears it regardless of the in-flight retry.
    manager.cancel_download(&release_id);
    let cleared = wait_for(|| manager.download_snapshot().ops.is_empty()).await;
    assert!(cleared, "cancel removes the entry");
}

#[cfg(feature = "test-utils")]
#[tokio::test]
async fn download_queue_active_pin_reports_file_progress() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release_id = make_remote_release_with_files(
        &manager,
        &temp_dir.path().join("download-source"),
        "Test Album",
        &[("a.flac", b"aaa"), ("b.flac", b"bbbb")],
        false,
    )
    .await;
    let mut events = manager.subscribe_events();

    manager.enqueue_pins(vec![release_id.clone()]).await;

    let mut saw_initial = false;
    let mut saw_completed_progress = false;
    for _ in 0..20 {
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
            .await
            .expect("download queue event")
            .expect("event channel stays open");
        let LibraryEvent::DownloadQueueChanged { snapshot } = event else {
            continue;
        };
        for op in snapshot.ops {
            if op.release_id != release_id {
                continue;
            }
            if let crate::library::DownloadState::Active { progress } = op.state {
                if progress
                    == (crate::library::DownloadTransferProgress {
                        bytes_done: 0,
                        bytes_total: 7,
                        fraction: 0.0,
                    })
                {
                    saw_initial = true;
                }
                if progress
                    == (crate::library::DownloadTransferProgress {
                        bytes_done: 7,
                        bytes_total: 7,
                        fraction: 1.0,
                    })
                {
                    saw_completed_progress = true;
                }
            }
        }
        if saw_initial && saw_completed_progress {
            break;
        }
    }

    assert!(saw_initial, "active download starts with known totals");
    assert!(
        saw_completed_progress,
        "active download reports completed file bytes before leaving the queue"
    );
}

// ── Export queue ─────────────────────────────────────────────────

/// Create a Remote, pinned, exportable release: a Local release with
/// known-byte source files on disk (coven external refs) and a
/// `source_folder_name`, made Remote with pin so its blobs stay readable from
/// the offline cache. Returns its id. The manager must already be connected via
/// [`connect_test_cloud`].
#[cfg(feature = "test-utils")]
async fn make_exportable_release(
    manager: &LibraryManager,
    source_dir: &std::path::Path,
    folder_name: &str,
    files: &[(&str, &[u8])],
) -> String {
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.remote = false;
    release.source_folder_name = Some(folder_name.to_string());
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    std::fs::create_dir_all(source_dir).unwrap();
    for (name, bytes) in files {
        let path = source_dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        let file = DbFile::new(
            &release.id,
            name,
            bytes.len() as i64,
            crate::util::content_type::ContentType::Flac,
            Uuid::new_v4().to_string(),
            Utc::now(),
        );
        manager.add_file(&file).await.unwrap();
    }
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &source_dir.to_string_lossy())
        .await
        .unwrap();
    manager.coven_make_remote(&release.id, true).await.unwrap();
    let n = manager.drain_uploads_for_test().await.unwrap();
    assert_eq!(n as usize, files.len(), "each release blob uploaded");
    release.id
}

/// Pausing before the first enqueue parks the worker, so the queue's in-memory
/// state (enqueue, dedup, target_dir, cancel) is observable deterministically
/// without the export path racing the assertions.
#[tokio::test]
async fn export_queue_enqueue_dedups_and_cancels_while_paused() {
    let (manager, temp_dir) = setup_test_manager().await;
    let release_id = insert_pinnable_release(&manager).await;

    manager.set_exports_paused(true);

    let target = temp_dir.path().join("export-out");
    manager
        .enqueue_export(
            &release_id,
            target.clone(),
            crate::config::ExportSelection::Original,
        )
        .await
        .unwrap();
    let snap = manager.export_snapshot();
    assert_eq!(snap.total.queued, 1);
    assert_eq!(snap.ops.len(), 1);
    assert_eq!(snap.ops[0].title, "Test Album");
    assert_eq!(snap.ops[0].file_count, 1);
    assert_eq!(snap.ops[0].payload.target_dir, target);
    assert_eq!(snap.ops[0].state, crate::library::ExportState::Queued);
    assert!(snap.paused);

    // Re-enqueuing the same release is a no-op: still one entry.
    manager
        .enqueue_export(
            &release_id,
            target.clone(),
            crate::config::ExportSelection::Original,
        )
        .await
        .unwrap();
    assert_eq!(manager.export_snapshot().ops.len(), 1);

    // Cancel drops the entry.
    manager.cancel_export(&release_id);
    assert!(manager.export_snapshot().ops.is_empty());
}

/// The verbatim copy-out: exported bytes equal the source bytes, laid out at
/// `<target>/<source_folder_name>/<original_filename>` (including nested
/// subfolders), and the export changes no release state — it stays Remote with
/// no new cloud-outbox rows.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn export_writes_exact_bytes_in_source_folder_and_leaves_release_remote() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;

    let source_dir = temp_dir.path().join("source");
    let files: &[(&str, &[u8])] = &[
        ("cover.jpg", b"cover-bytes-abc"),
        ("CD1/track.flac", b"flac-bytes-0123456789"),
    ];
    let release_id =
        make_exportable_release(&manager, &source_dir, "Album Title (2020)", files).await;

    // Precondition: Remote, no pending uploads after the drain.
    let before = manager
        .get_release_by_id(&release_id)
        .await
        .unwrap()
        .unwrap();
    assert!(before.remote);
    assert!(!manager
        .database
        .has_pending_uploads_for_release(&release_id)
        .await
        .unwrap());

    let target = temp_dir.path().join("export-out");
    manager
        .enqueue_export(
            &release_id,
            target.clone(),
            crate::config::ExportSelection::Original,
        )
        .await
        .unwrap();

    // Success removes the entry from the queue.
    let done = wait_for(|| manager.export_snapshot().ops.is_empty()).await;
    assert!(done, "the export should complete and clear the queue");

    // Byte-accuracy + folder layout.
    for (name, bytes) in files {
        let written = target.join("Album Title (2020)").join(name);
        let got = std::fs::read(&written).unwrap_or_else(|e| panic!("read exported {name}: {e}"));
        assert_eq!(&got, bytes, "exported bytes for {name} match the source");
    }

    // The staging directory was renamed into place, leaving nothing behind it.
    let leftover = std::fs::read_dir(&target)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        leftover,
        vec!["Album Title (2020)".to_string()],
        "only the final export folder remains under the target; no staging dir"
    );

    // Export changed no release state: still Remote, no new outbox rows.
    let after = manager
        .get_release_by_id(&release_id)
        .await
        .unwrap()
        .unwrap();
    assert!(after.remote, "export leaves the release Remote");
    assert!(
        !manager
            .database
            .has_pending_uploads_for_release(&release_id)
            .await
            .unwrap(),
        "export enqueues no cloud uploads"
    );
}

/// A write error (an unwritable target) marks the export `Failed` with a message
/// and keeps it in the queue; `retry_exports` flips it back to `Queued`.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn export_write_error_marks_failed_and_retries() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;

    let source_dir = temp_dir.path().join("source");
    let release_id = make_exportable_release(
        &manager,
        &source_dir,
        "Album Title",
        &[("track.flac", b"track-bytes")],
    )
    .await;

    // Target a path that is actually a file, so creating the release subfolder
    // under it fails with an I/O error (the read succeeds; the write doesn't).
    let blocker = temp_dir.path().join("blocker");
    std::fs::write(&blocker, b"a file, not a directory").unwrap();

    manager
        .enqueue_export(
            &release_id,
            blocker.clone(),
            crate::config::ExportSelection::Original,
        )
        .await
        .unwrap();

    let failed = wait_for(|| {
        matches!(
            manager.export_snapshot().ops.first().map(|op| &op.state),
            Some(crate::library::ExportState::Failed { .. })
        )
    })
    .await;
    assert!(failed, "an unwritable target marks the export Failed");
    assert_eq!(manager.export_snapshot().total.failed, 1);

    // Retry flips it back to Queued (it'll fail again, but stays tracked).
    manager.retry_exports();
    assert!(manager
        .export_snapshot()
        .ops
        .first()
        .is_some_and(|op| matches!(
            op.state,
            crate::library::ExportState::Queued
                | crate::library::ExportState::Active { .. }
                | crate::library::ExportState::Failed { .. }
        )));

    manager.cancel_export(&release_id);
    let cleared = wait_for(|| manager.export_snapshot().ops.is_empty()).await;
    assert!(cleared, "cancel removes the entry");
}

/// A failure partway through the export (a read error on a later file, after an
/// earlier file has already been written to staging) leaves NO output at the
/// final `<target>/<source_folder_name>/` path — the staging directory is
/// removed, so the export is all-or-nothing.
#[cfg(feature = "test-utils")]
#[tokio::test]
async fn export_mid_failure_leaves_no_partial_output_at_final_path() {
    let (manager, temp_dir) = setup_test_manager().await;

    // A Local release whose two source files live on disk as coven external refs
    // (UserProvided reads straight from the user's own file). Keeping it Local —
    // never made Remote — means the export reads these files directly, so removing
    // one forces a read error on a later file mid-export.
    let source_dir = temp_dir.path().join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.remote = false;
    release.source_folder_name = Some("Album Title".to_string());
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    let files: &[(&str, &[u8])] = &[("01.flac", b"first-ok"), ("02.flac", b"second-fails")];
    for (name, bytes) in files {
        std::fs::write(source_dir.join(name), bytes).unwrap();
        let file = DbFile::new(
            &release.id,
            name,
            bytes.len() as i64,
            crate::util::content_type::ContentType::Flac,
            Uuid::new_v4().to_string(),
            Utc::now(),
        );
        manager.add_file(&file).await.unwrap();
    }
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &source_dir.to_string_lossy())
        .await
        .unwrap();

    // Pause so the source file can be deleted before the worker runs, making the
    // later read fail deterministically rather than racing the copy.
    manager.set_exports_paused(true);
    let target = temp_dir.path().join("export-out");
    std::fs::create_dir_all(&target).unwrap();
    manager
        .enqueue_export(
            &release.id,
            target.clone(),
            crate::config::ExportSelection::Original,
        )
        .await
        .unwrap();
    std::fs::remove_file(source_dir.join("02.flac")).unwrap();
    manager.set_exports_paused(false);

    let failed = wait_for(|| {
        matches!(
            manager.export_snapshot().ops.first().map(|op| &op.state),
            Some(crate::library::ExportState::Failed { .. })
        )
    })
    .await;
    assert!(
        failed,
        "a read error on a later file marks the export Failed"
    );

    // The all-or-nothing guarantee: nothing at the final path, and the staging
    // directory was removed — the target holds no export output at all.
    assert!(
        !target.join("Album Title").exists(),
        "no partial output at the final export path"
    );
    assert_eq!(
        std::fs::read_dir(&target).unwrap().count(),
        0,
        "staging directory cleaned up; target left empty"
    );
}

/// Poll `predicate` up to ~2s (40 × 50ms), returning whether it became true.
/// Used by the async download-worker tests instead of a fixed sleep.
async fn wait_for(predicate: impl Fn() -> bool) -> bool {
    for _ in 0..40 {
        if predicate() {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    predicate()
}

#[tokio::test]
async fn storage_page_id_tiebreaker_stable_across_pages() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Two releases sharing album title + created_at — the ORDER BY clause
    // falls through to the `r.id` tiebreaker.
    let now = Utc::now();
    let mut album = create_test_album();
    album.title = "Same Title".to_string();
    manager.database.insert_album(&album).await.unwrap();
    let mut release_a = create_test_release(&album.id);
    release_a.id = "aaaa".to_string();
    release_a.created_at = now;
    let mut release_b = create_test_release(&album.id);
    release_b.id = "bbbb".to_string();
    release_b.created_at = now;
    manager.database.insert_release(&release_a).await.unwrap();
    manager.database.insert_release(&release_b).await.unwrap();

    let sort = StorageSort {
        field: StorageSortField::AlbumTitle,
        direction: StorageSortDirection::Ascending,
    };
    let first_page = manager
        .get_storage_page(&sort, StorageFilter::All, 0, 1)
        .await
        .unwrap();
    let second_page = manager
        .get_storage_page(&sort, StorageFilter::All, 1, 1)
        .await
        .unwrap();

    assert_eq!(first_page.rows.len(), 1);
    assert_eq!(second_page.rows.len(), 1);
    assert_eq!(first_page.rows[0].release.id, "aaaa");
    assert_eq!(second_page.rows[0].release.id, "bbbb");
}

// =========================================================================
// set_identity
// =========================================================================

fn mb_identity(group: &str, release: Option<&str>) -> crate::import::ReleaseIdentity {
    crate::import::ReleaseIdentity {
        source: crate::import::MetadataSource::MusicBrainz,
        source_group_id: group.to_string(),
        source_release_id: release.map(|s| s.to_string()),
    }
}

fn discogs_identity(group: &str, release: Option<&str>) -> crate::import::ReleaseIdentity {
    crate::import::ReleaseIdentity {
        source: crate::import::MetadataSource::Discogs,
        source_group_id: group.to_string(),
        source_release_id: release.map(|s| s.to_string()),
    }
}

#[tokio::test]
async fn set_identity_to_unknown_moves_release_to_fresh_album() {
    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.metadata_source = crate::db::ReleaseMetadataSource::MusicBrainz;
    release.metadata_source_release_id = Some("mb-rel-1".to_string());

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .insert_release_identities(&release.id, &[mb_identity("g1", Some("mb-rel-1"))])
        .await
        .unwrap();

    manager
        .set_identity(
            &release.id,
            vec![],
            crate::import::MetadataPointer::FileTags,
            &[],
        )
        .await
        .unwrap();

    // The original album was a one-release album → deleted now.
    assert!(manager
        .database
        .find_album_by_id(&album.id)
        .await
        .unwrap()
        .is_none());

    // Release moved to a brand-new album, holds nothing else.
    let new_album_id = manager
        .database
        .find_album_id_for_release(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(new_album_id, album.id);
    let siblings = manager
        .database
        .get_releases_for_album(&new_album_id)
        .await
        .unwrap();
    assert_eq!(siblings.len(), 1);
    assert_eq!(siblings[0].id, release.id);

    // Identity rows wiped, metadata source flipped to file_tags.
    let identities = manager
        .database
        .get_release_identities(&release.id)
        .await
        .unwrap();
    assert!(identities.is_empty());
    let updated = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_source,
        crate::db::ReleaseMetadataSource::FileTags
    );
    assert_eq!(updated.metadata_source_release_id, None);
}

#[tokio::test]
async fn set_identity_replaces_rows_when_new_identity_fits_current_album() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Album has two releases, both Approximate-MB on group g1.
    let album = create_test_album();
    let release1 = create_test_release(&album.id);
    let release2 = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release1).await.unwrap();
    manager.database.insert_release(&release2).await.unwrap();
    manager
        .database
        .insert_release_identities(&release1.id, &[mb_identity("g1", None)])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release2.id, &[mb_identity("g1", None)])
        .await
        .unwrap();

    // Promote release1 from Approximate to Exact within g1. New row
    // still agrees with release2's group, so release1 stays put.
    manager
        .set_identity(
            &release1.id,
            vec![mb_identity("g1", Some("mb-rel-99"))],
            crate::import::MetadataPointer::External {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-99".to_string(),
            },
            &[],
        )
        .await
        .unwrap();

    let landing_album_id = manager
        .database
        .find_album_id_for_release(&release1.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(landing_album_id, album.id);

    let identities = manager
        .database
        .get_release_identities(&release1.id)
        .await
        .unwrap();
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].source_group_id, "g1");
    assert_eq!(
        identities[0].source_release_id.as_deref(),
        Some("mb-rel-99")
    );

    let updated = manager
        .database
        .find_release_by_id(&release1.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_source,
        crate::db::ReleaseMetadataSource::MusicBrainz
    );
    assert_eq!(
        updated.metadata_source_release_id.as_deref(),
        Some("mb-rel-99"),
    );

    // Source album still holds both releases.
    let siblings = manager
        .database
        .get_releases_for_album(&album.id)
        .await
        .unwrap();
    assert_eq!(siblings.len(), 2);
}

#[tokio::test]
async fn set_identity_creates_new_album_when_no_existing_album_fits() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Two albums, neither matching the new MB group g2.
    let album_a = create_test_album();
    let mut album_b = create_test_album();
    album_b.title = "Other Album".to_string();
    manager.database.insert_album(&album_a).await.unwrap();
    manager.database.insert_album(&album_b).await.unwrap();

    let release_alpha = create_test_release(&album_a.id);
    let release_beta = create_test_release(&album_a.id);
    let release_other = create_test_release(&album_b.id);
    manager
        .database
        .insert_release(&release_alpha)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_beta)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_other)
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", None)])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_beta.id, &[mb_identity("g1", None)])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_other.id, &[mb_identity("g3", None)])
        .await
        .unwrap();

    // release_alpha takes on a brand-new MB group (g2). Its current
    // album (album_a) holds release_beta on g1, so it can't stay.
    // No other album holds g2 either → fresh album.
    manager
        .set_identity(
            &release_alpha.id,
            vec![mb_identity("g2", None)],
            crate::import::MetadataPointer::External {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-g2".to_string(),
            },
            &[],
        )
        .await
        .unwrap();

    let landing_album_id = manager
        .database
        .find_album_id_for_release(&release_alpha.id)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(landing_album_id, album_a.id);
    assert_ne!(landing_album_id, album_b.id);

    // Source album loses release_alpha but keeps release_beta.
    let source_siblings = manager
        .database
        .get_releases_for_album(&album_a.id)
        .await
        .unwrap();
    assert_eq!(source_siblings.len(), 1);
    assert_eq!(source_siblings[0].id, release_beta.id);
}

#[tokio::test]
async fn set_identity_moves_release_to_matching_album() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Source album (album_a) carries release_alpha solo at MB g1.
    // Target album (album_b) carries release_other at MB g2 — that
    // matches the new identity we'll set on release_alpha.
    let album_a = create_test_album();
    let mut album_b = create_test_album();
    album_b.title = "Other Album".to_string();
    manager.database.insert_album(&album_a).await.unwrap();
    manager.database.insert_album(&album_b).await.unwrap();

    let release_alpha = create_test_release(&album_a.id);
    let release_other = create_test_release(&album_b.id);
    manager
        .database
        .insert_release(&release_alpha)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_other)
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", None)])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_other.id, &[mb_identity("g2", None)])
        .await
        .unwrap();

    manager
        .set_identity(
            &release_alpha.id,
            vec![mb_identity("g2", Some("mb-rel-pressing"))],
            crate::import::MetadataPointer::External {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-pressing".to_string(),
            },
            &[],
        )
        .await
        .unwrap();

    // release_alpha now lives in album_b alongside release_other.
    let landing_album_id = manager
        .database
        .find_album_id_for_release(&release_alpha.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(landing_album_id, album_b.id);
    let target_siblings = manager
        .database
        .get_releases_for_album(&album_b.id)
        .await
        .unwrap();
    assert_eq!(target_siblings.len(), 2);

    // album_a was a single-release album → deleted now.
    assert!(manager
        .database
        .find_album_by_id(&album_a.id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn set_identity_keeps_vacated_album_when_other_releases_remain() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // album_a holds two releases, both on MB g1. Move release_alpha
    // out by giving it a different group.
    let album_a = create_test_album();
    manager.database.insert_album(&album_a).await.unwrap();
    let release_alpha = create_test_release(&album_a.id);
    let release_beta = create_test_release(&album_a.id);
    manager
        .database
        .insert_release(&release_alpha)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_beta)
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", None)])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_beta.id, &[mb_identity("g1", None)])
        .await
        .unwrap();

    manager
        .set_identity(
            &release_alpha.id,
            vec![mb_identity("g2", None)],
            crate::import::MetadataPointer::External {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-g2".to_string(),
            },
            &[],
        )
        .await
        .unwrap();

    // album_a still exists, holds release_beta only.
    let surviving = manager
        .database
        .find_album_by_id(&album_a.id)
        .await
        .unwrap();
    assert!(surviving.is_some());
    let surviving_releases = manager
        .database
        .get_releases_for_album(&album_a.id)
        .await
        .unwrap();
    assert_eq!(surviving_releases.len(), 1);
    assert_eq!(surviving_releases[0].id, release_beta.id);
}

#[tokio::test]
async fn set_identity_does_not_touch_metadata_columns() {
    let (manager, _temp_dir) = setup_test_manager().await;

    let mut album = create_test_album();
    album.title = "Initial Title".to_string();
    album.year = Some(1999);
    manager.database.insert_album(&album).await.unwrap();

    let mut release = create_test_release(&album.id);
    release.pressing.format = Some("Vinyl".to_string());
    release.pressing.label = Some("My Label".to_string());
    release.pressing.catalog_number = Some("CAT-123".to_string());
    release.pressing.country = Some("US".to_string());
    release.pressing.barcode = Some("1234567890".to_string());
    release.pressing.year = Some(1999);
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .insert_release_identities(&release.id, &[mb_identity("g1", Some("mb-rel-1"))])
        .await
        .unwrap();

    // Insert a track too — we want to verify it survives.
    let track = crate::db::DbTrack {
        id: Uuid::new_v4().to_string(),
        release_id: release.id.clone(),
        title: "My Track".to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms: Some(180_000),
        discogs_position: None,
        created_at: Utc::now(),
    };
    manager.database.insert_track(&track).await.unwrap();

    manager
        .set_identity(
            &release.id,
            vec![discogs_identity("dg1", Some("dg-rel-1"))],
            crate::import::MetadataPointer::External {
                source: crate::import::MetadataSource::Discogs,
                release_id: "dg-rel-1".to_string(),
            },
            &[],
        )
        .await
        .unwrap();

    // Pressing fields untouched.
    let after = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.pressing.format.as_deref(), Some("Vinyl"));
    assert_eq!(after.pressing.label.as_deref(), Some("My Label"));
    assert_eq!(after.pressing.catalog_number.as_deref(), Some("CAT-123"));
    assert_eq!(after.pressing.country.as_deref(), Some("US"));
    assert_eq!(after.pressing.barcode.as_deref(), Some("1234567890"));
    assert_eq!(after.pressing.year, Some(1999));

    // Album-level fields untouched (still in the same album, since
    // both old and new identities are in the only release).
    let after_album = manager
        .database
        .find_album_by_id(&after.album_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after_album.title, "Initial Title");
    assert_eq!(after_album.year, Some(1999));

    // Track survived.
    let tracks = manager
        .database
        .get_tracks_for_release(&release.id)
        .await
        .unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].title, "My Track");
}

#[tokio::test]
async fn set_identity_to_fresh_album_preserves_album_artists() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // Two extra artists so the album carries multiple album_artists
    // rows beyond the primary (which lives on `albums.artist_id`).
    let primary = DbArtist {
        id: "primary-artist".to_string(),
        name: "Primary".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: Utc::now(),
    };
    let secondary = DbArtist {
        id: "secondary-artist".to_string(),
        name: "Secondary".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: Utc::now(),
    };
    manager.database.insert_artist(&primary).await.unwrap();
    manager.database.insert_artist(&secondary).await.unwrap();

    // album_a holds release_alpha and release_beta on g1, with both
    // primary + secondary as album artists. We're going to move
    // release_alpha out via a non-fitting identity, forcing the
    // creation of a fresh album.
    let mut album_a = create_test_album();
    album_a.artist_id = primary.id.clone();
    manager.database.insert_album(&album_a).await.unwrap();
    manager
        .database
        .insert_album_artist(&DbAlbumArtist::new(
            &album_a.id,
            &primary.id,
            0,
            Uuid::new_v4().to_string(),
            Utc::now(),
        ))
        .await
        .unwrap();
    manager
        .database
        .insert_album_artist(&DbAlbumArtist::new(
            &album_a.id,
            &secondary.id,
            1,
            Uuid::new_v4().to_string(),
            Utc::now(),
        ))
        .await
        .unwrap();

    let release_alpha = create_test_release(&album_a.id);
    let release_beta = create_test_release(&album_a.id);
    manager
        .database
        .insert_release(&release_alpha)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_beta)
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", None)])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_beta.id, &[mb_identity("g1", None)])
        .await
        .unwrap();

    // release_alpha takes a different group → can't stay in album_a
    // (g1 disagrees with g2), no other album holds g2 → fresh album.
    manager
        .set_identity(
            &release_alpha.id,
            vec![mb_identity("g2", None)],
            crate::import::MetadataPointer::External {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-g2".to_string(),
            },
            &[],
        )
        .await
        .unwrap();

    let new_album_id = manager
        .database
        .find_album_id_for_release(&release_alpha.id)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(new_album_id, album_a.id);

    // The fresh album carries the same album_artists as the source.
    // get_artists_for_album joins both the primary (via albums.artist_id)
    // and album_artists rows, ordered by position.
    let new_album_artists = manager
        .database
        .get_artists_for_album(&new_album_id)
        .await
        .unwrap();
    let names: Vec<&str> = new_album_artists.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["Primary", "Primary", "Secondary"]);
}

#[tokio::test]
async fn set_identity_moves_primary_release_id_to_remaining_release() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // album_a carries two releases on g1 and points
    // primary_release_id at release_alpha. Move release_alpha out
    // and the pointer should be repaired to release_beta — anything
    // less leaves the FK pointing at a release that no longer
    // belongs to the album.
    let album_a = create_test_album();
    manager.database.insert_album(&album_a).await.unwrap();
    let release_alpha = create_test_release(&album_a.id);
    // Older `created_at` for beta so it wins the
    // ORDER BY created_at ASC tiebreak.
    let mut release_beta = create_test_release(&album_a.id);
    release_beta.created_at = release_alpha.created_at - chrono::Duration::seconds(60);

    manager
        .database
        .insert_release(&release_alpha)
        .await
        .unwrap();
    manager
        .database
        .insert_release(&release_beta)
        .await
        .unwrap();

    // Point album_a.primary_release_id at release_alpha — the
    // release we're about to move out.
    manager
        .database
        .set_album_primary_release(&album_a.id, &release_alpha.id)
        .await
        .unwrap();

    manager
        .database
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", None)])
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_beta.id, &[mb_identity("g1", None)])
        .await
        .unwrap();

    manager
        .set_identity(
            &release_alpha.id,
            vec![mb_identity("g2", None)],
            crate::import::MetadataPointer::External {
                source: crate::import::MetadataSource::MusicBrainz,
                release_id: "mb-rel-g2".to_string(),
            },
            &[],
        )
        .await
        .unwrap();

    // album_a survives, primary_release_id moved to release_beta.
    let surviving_album = manager
        .database
        .find_album_by_id(&album_a.id)
        .await
        .unwrap()
        .expect("album should still exist with release_beta");
    assert_eq!(
        surviving_album.primary_release_id.as_deref(),
        Some(release_beta.id.as_str()),
        "primary_release_id should repoint to the remaining release",
    );
}

#[tokio::test]
async fn set_identity_atomic_rechecks_source_count_inside_transaction() {
    // Models the TOCTOU window: a separate writer lands a release
    // into the source album between `set_identity`'s pre-flight
    // read and its atomic call. We invoke the atomic API directly
    // with `current_album_id` set to the source album, after seeding
    // an extra release into that album. The atomic call must NOT
    // delete the source — its in-transaction recheck sees the
    // surviving release.
    let (manager, _temp_dir) = setup_test_manager().await;

    let album_a = create_test_album();
    manager.database.insert_album(&album_a).await.unwrap();

    let release_alpha = create_test_release(&album_a.id);
    manager
        .database
        .insert_release(&release_alpha)
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release_alpha.id, &[mb_identity("g1", None)])
        .await
        .unwrap();

    // Build the fresh-album row the manager would have produced —
    // we're driving the atomic API by hand.
    let now = chrono::Utc::now();
    let fresh_album = DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: album_a.title.clone(),
        artist_id: album_a.artist_id.clone(),
        year: album_a.year,
        primary_release_id: None,
        is_compilation: album_a.is_compilation,
        created_at: now,
    };

    // Race window: another writer lands release_intruder into
    // album_a after the (hypothetical) pre-flight read but before
    // the atomic call.
    let release_intruder = create_test_release(&album_a.id);
    manager
        .database
        .insert_release(&release_intruder)
        .await
        .unwrap();

    let outcome = manager
        .database
        .set_identity_atomic(
            &release_alpha.id,
            &[mb_identity("g2", None)],
            crate::db::ReleaseMetadataSource::MusicBrainz,
            Some("mb-rel-g2"),
            &album_a.id,
            &fresh_album.id,
            Some(&fresh_album),
            &[],
        )
        .await
        .unwrap();

    assert!(
        !outcome.source_album_deleted,
        "atomic recheck must protect the late-arriving release"
    );

    // Source album survives, holding only release_intruder.
    let survivors = manager
        .database
        .get_releases_for_album(&album_a.id)
        .await
        .unwrap();
    assert_eq!(survivors.len(), 1);
    assert_eq!(survivors[0].id, release_intruder.id);

    // release_alpha landed in the fresh album.
    let fresh_releases = manager
        .database
        .get_releases_for_album(&fresh_album.id)
        .await
        .unwrap();
    assert_eq!(fresh_releases.len(), 1);
    assert_eq!(fresh_releases[0].id, release_alpha.id);
}

// ── re_identify_release ────────────────────────────────────────────
//
// Exact / Approximate fetch through MB / Discogs; the tests seed the
// release cache first so `prepare_release` reads locally instead of
// hitting the network. The Unknown path needs no seeding — it makes
// no source claim.

#[tokio::test]
async fn re_identify_to_unknown_clears_identities_and_moves_album() {
    use lofty::config::WriteOptions;
    use lofty::prelude::*;
    use lofty::tag::{Tag, TagType};
    use std::fs;

    let (manager, _temp_dir) = setup_test_manager().await;

    // Local audio files so the post-`set_identity` reseed can read tags.
    let media = TempDir::new().unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flac");
    let mut filenames = Vec::new();
    for (name, title) in [("01.flac", "Tag One"), ("02.flac", "Tag Two")] {
        let dest = media.path().join(name);
        fs::copy(fixtures.join("01 Test Track 1.flac"), &dest).unwrap();
        let mut tagged = lofty::read_from_path(&dest).unwrap();
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.set_title(title.to_string());
        tag.set_artist("Tag Artist".to_string());
        tag.set_album("Tag Album".to_string());
        tagged.insert_tag(tag);
        tagged.save_to_path(&dest, WriteOptions::default()).unwrap();
        filenames.push(name.to_string());
    }

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.metadata_source = crate::db::ReleaseMetadataSource::MusicBrainz;
    release.metadata_source_release_id = Some("mb-rel-1".to_string());
    release.remote = false;

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    // Two existing track rows align positionally with the two files.
    insert_n_tracks(&manager.database, &release.id, 2).await;
    let now = Utc::now();
    for name in &filenames {
        let file = crate::db::DbFile::new(
            &release.id,
            name,
            0,
            crate::util::content_type::ContentType::Flac,
            Uuid::new_v4().to_string(),
            now,
        );
        manager.database.insert_file(&file).await.unwrap();
    }
    // Register the files as coven external refs (in-place files of a Local
    // release) AFTER inserting them, so the file-tag re-read resolves paths.
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &media.path().to_string_lossy())
        .await
        .unwrap();
    manager
        .database
        .insert_release_identities(&release.id, &[mb_identity("g1", Some("mb-rel-1"))])
        .await
        .unwrap();

    // Seed a stale cache row to verify Unknown clears it.
    manager
        .database
        .insert_release_metadata(&crate::db::DbReleaseMetadata::new(
            &release.id,
            "musicbrainz",
            r#"{"id":"mb-rel-1"}"#.to_string(),
            Uuid::new_v4().to_string(),
            Utc::now(),
        ))
        .await
        .unwrap();

    manager
        .re_identify_release(&release.id, crate::import::IdentityChoice::Unknown)
        .await
        .unwrap();

    // Original (single-release) album is gone; release sits on a
    // fresh one.
    assert!(manager
        .database
        .find_album_by_id(&album.id)
        .await
        .unwrap()
        .is_none());
    let new_album_id = manager
        .database
        .find_album_id_for_release(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(new_album_id, album.id);

    // Identity rows wiped, metadata pointer flipped to file_tags,
    // cache rows cleared (file_tags has no source payload).
    let identities = manager
        .database
        .get_release_identities(&release.id)
        .await
        .unwrap();
    assert!(identities.is_empty());
    let updated = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_source,
        crate::db::ReleaseMetadataSource::FileTags
    );
    assert_eq!(updated.metadata_source_release_id, None);
    let cache_after = manager
        .database
        .get_release_metadata_by_source(&release.id)
        .await
        .unwrap();
    assert!(
        cache_after.is_empty(),
        "Unknown commit must clear stale cached payloads"
    );
}

// ── re_identify_release Exact / Approximate (MB cache-seeded) ────
//
// Drive the network-side `prepare_release` through the MB LRU cache
// (`seed_release_cache` + `seed_release_group_json_cache`) so the
// tests don't hit the network. Each test uses a unique MB release
// ID so other tests' cache seeds don't bleed in (the caches are
// process-global LRUs).

/// Build a synthetic MB release response with `n` track rows on a
/// single CD medium, plus a release group reference. Suitable for
/// driving `prepare_release` via cache seeding.
fn make_mb_release_for_re_identify(
    release_id: &str,
    release_group_id: &str,
    track_count: usize,
) -> crate::musicbrainz::MbReleaseResponse {
    use crate::musicbrainz::{
        MbArtistCredit, MbArtistRef, MbMedium, MbReleaseGroupRef, MbReleaseResponse, MbTrack,
    };
    MbReleaseResponse {
        id: release_id.to_string(),
        title: "Album Title".to_string(),
        date: Some("2024-01-01".to_string()),
        country: None,
        barcode: None,
        artist_credit: vec![MbArtistCredit {
            name: "Artist Name".to_string(),
            artist: Some(MbArtistRef {
                id: Some("mb-artist-1".to_string()),
                name: Some("Artist Name".to_string()),
                sort_name: Some("Artist Name".to_string()),
            }),
        }],
        release_group: Some(MbReleaseGroupRef {
            id: release_group_id.to_string(),
            first_release_date: Some("2024-01-01".to_string()),
            relations: Some(vec![]),
        }),
        label_info: vec![],
        media: vec![MbMedium {
            format: Some("CD".to_string()),
            tracks: (1..=track_count)
                .map(|n| MbTrack {
                    position: Some(n as i64),
                    number: Some(n.to_string()),
                    title: Some(format!("Track {n}")),
                    length: None,
                    recording: None,
                    artist_credit: vec![],
                })
                .collect(),
        }],
        relations: vec![],
    }
}

fn empty_mb_external_urls() -> crate::musicbrainz::ExternalUrls {
    crate::musicbrainz::ExternalUrls {
        discogs_release_url: None,
    }
}

/// Insert `n` plain track rows for a release. Mirrors the row shape
/// `prepared.parsed.tracks` would produce so the track-count check
/// in `re_identify_release` accepts the picked release.
async fn insert_n_tracks(database: &Database, release_id: &str, n: usize) {
    for i in 1..=n {
        let track = crate::db::DbTrack {
            id: Uuid::new_v4().to_string(),
            release_id: release_id.to_string(),
            title: format!("Track {i}"),
            side: 1,
            track_number: Some(i as i32),
            duration_ms: None,
            discogs_position: None,
            created_at: Utc::now(),
        };
        database.insert_track(&track).await.unwrap();
    }
}

#[tokio::test]
async fn re_identify_release_exact_writes_cache() {
    use crate::import::{IdentityChoice, MetadataRef, MetadataSource};
    use crate::musicbrainz::{seed_release_cache, seed_release_group_json_cache};

    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.metadata_source = crate::db::ReleaseMetadataSource::MusicBrainz;
    release.metadata_source_release_id = Some("mb-rel-old".to_string());

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .insert_release_identities(&release.id, &[mb_identity("g-old", Some("mb-rel-old"))])
        .await
        .unwrap();
    insert_n_tracks(&manager.database, &release.id, 3).await;

    // Seed an old cached payload so the test can assert it was
    // replaced (not just augmented).
    manager
        .database
        .insert_release_metadata(&crate::db::DbReleaseMetadata::new(
            &release.id,
            "musicbrainz",
            r#"{"id":"mb-rel-old"}"#.to_string(),
            Uuid::new_v4().to_string(),
            Utc::now(),
        ))
        .await
        .unwrap();

    // Cache the picked release so `prepare_release` skips the
    // network. The raw JSON payload is what the cache replacement
    // step writes into `release_metadata`.
    let new_release_id = "exact-re-identify-mb-rel-new";
    let new_group_id = "exact-re-identify-mb-group-new";
    let new_response = make_mb_release_for_re_identify(new_release_id, new_group_id, 3);
    let new_raw_json = r#"{"id":"exact-re-identify-mb-rel-new"}"#.to_string();
    seed_release_cache(
        new_release_id,
        (new_response, empty_mb_external_urls(), new_raw_json.clone()),
    );
    seed_release_group_json_cache(
        new_group_id,
        r#"{"id":"exact-re-identify-mb-group-new"}"#.to_string(),
    );

    manager
        .re_identify_release(
            &release.id,
            IdentityChoice::Exact {
                release_ref: MetadataRef {
                    source: MetadataSource::MusicBrainz,
                    id: new_release_id.to_string(),
                },
            },
        )
        .await
        .unwrap();

    // Identity row updated to Exact at the new pressing.
    let identities = manager
        .database
        .get_release_identities(&release.id)
        .await
        .unwrap();
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].source, MetadataSource::MusicBrainz);
    assert_eq!(identities[0].source_group_id, new_group_id);
    assert_eq!(
        identities[0].source_release_id.as_deref(),
        Some(new_release_id)
    );

    // Pointer columns flipped to the new source release.
    let updated = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_source,
        crate::db::ReleaseMetadataSource::MusicBrainz
    );
    assert_eq!(
        updated.metadata_source_release_id.as_deref(),
        Some(new_release_id)
    );

    // Cache rows aligned with the new pointer: stale "mb-rel-old"
    // payload gone, fresh payload + release-group JSON in.
    let cache_after = manager
        .database
        .get_release_metadata_by_source(&release.id)
        .await
        .unwrap();
    assert_eq!(
        cache_after.get("musicbrainz").map(String::as_str),
        Some(new_raw_json.as_str()),
        "cache must hold the freshly-fetched MB JSON, not the stale row"
    );
    assert!(
        cache_after.contains_key("musicbrainz_release_group"),
        "release-group JSON must be cached alongside the release JSON"
    );
}

#[tokio::test]
async fn re_identify_release_approximate_writes_cache() {
    use crate::import::{IdentityChoice, MetadataRef, MetadataSource};
    use crate::musicbrainz::{seed_release_cache, seed_release_group_json_cache};

    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.metadata_source = crate::db::ReleaseMetadataSource::FileTags;
    release.metadata_source_release_id = None;

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    insert_n_tracks(&manager.database, &release.id, 4).await;

    let new_release_id = "approx-re-identify-mb-rel-new";
    let new_group_id = "approx-re-identify-mb-group-new";
    let new_response = make_mb_release_for_re_identify(new_release_id, new_group_id, 4);
    let new_raw_json = r#"{"id":"approx-re-identify-mb-rel-new"}"#.to_string();
    seed_release_cache(
        new_release_id,
        (new_response, empty_mb_external_urls(), new_raw_json.clone()),
    );
    seed_release_group_json_cache(
        new_group_id,
        r#"{"id":"approx-re-identify-mb-group-new"}"#.to_string(),
    );

    manager
        .re_identify_release(
            &release.id,
            IdentityChoice::Approximate {
                release_ref: MetadataRef {
                    source: MetadataSource::MusicBrainz,
                    id: new_release_id.to_string(),
                },
            },
        )
        .await
        .unwrap();

    // Approximate clears `source_release_id` on the identity row
    // (group-only claim) but the metadata pointer still names the
    // picked pressing — reset-to-source reads cached payload through it.
    let identities = manager
        .database
        .get_release_identities(&release.id)
        .await
        .unwrap();
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].source, MetadataSource::MusicBrainz);
    assert_eq!(identities[0].source_group_id, new_group_id);
    assert_eq!(identities[0].source_release_id, None);

    let updated = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_source,
        crate::db::ReleaseMetadataSource::MusicBrainz
    );
    assert_eq!(
        updated.metadata_source_release_id.as_deref(),
        Some(new_release_id)
    );

    let cache_after = manager
        .database
        .get_release_metadata_by_source(&release.id)
        .await
        .unwrap();
    assert_eq!(
        cache_after.get("musicbrainz").map(String::as_str),
        Some(new_raw_json.as_str())
    );
    assert!(cache_after.contains_key("musicbrainz_release_group"));
}

#[tokio::test]
async fn re_identify_release_rejects_track_count_mismatch() {
    // The folder-import path enforces this through prefetch's
    // `track_count_mismatch` flag (which disables the commit
    // button). Re-identify bypasses prefetch — the user picks a
    // row directly — so the check belongs in the bae-core commit.
    // A 12-track release can't replace a 10-track rip.
    use crate::import::{IdentityChoice, MetadataRef, MetadataSource};
    use crate::musicbrainz::{seed_release_cache, seed_release_group_json_cache};

    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();

    // Local release has 10 tracks; picked release has 12.
    insert_n_tracks(&manager.database, &release.id, 10).await;

    let new_release_id = "mismatch-re-identify-mb-rel-new";
    let new_group_id = "mismatch-re-identify-mb-group-new";
    let new_response = make_mb_release_for_re_identify(new_release_id, new_group_id, 12);
    seed_release_cache(
        new_release_id,
        (
            new_response,
            empty_mb_external_urls(),
            r#"{"id":"mismatch-re-identify-mb-rel-new"}"#.to_string(),
        ),
    );
    seed_release_group_json_cache(
        new_group_id,
        r#"{"id":"mismatch-re-identify-mb-group-new"}"#.to_string(),
    );

    let err = manager
        .re_identify_release(
            &release.id,
            IdentityChoice::Exact {
                release_ref: MetadataRef {
                    source: MetadataSource::MusicBrainz,
                    id: new_release_id.to_string(),
                },
            },
        )
        .await
        .expect_err("track-count mismatch must error before identity write");
    let msg = err.to_string();
    assert!(
        msg.contains("Track count mismatch") && msg.contains("10") && msg.contains("12"),
        "error must name both counts so the UI can render a useful banner: {msg}"
    );

    // No identity row written.
    let identities = manager
        .database
        .get_release_identities(&release.id)
        .await
        .unwrap();
    assert!(
        identities.is_empty(),
        "mismatched commit must not leave a partial identity row"
    );
}

#[tokio::test]
async fn re_identify_release_followed_by_reset_succeeds() {
    // End-to-end check of the cache-alignment invariant: after a re-identify
    // commit, `reset_metadata_to_source` projects through the new
    // pointer + new cached payload without hitting the cache-
    // divergence guard. A regression here means re-identify left
    // the cache stale relative to `metadata_source_release_id`.
    use crate::import::{IdentityChoice, MetadataRef, MetadataSource};
    use crate::musicbrainz::{seed_release_cache, seed_release_group_json_cache};

    let (manager, _temp_dir) = setup_test_manager().await;

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    release.metadata_source = crate::db::ReleaseMetadataSource::FileTags;
    release.metadata_source_release_id = None;

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    insert_n_tracks(&manager.database, &release.id, 2).await;

    let new_release_id = "reset-re-identify-mb-rel-new";
    let new_group_id = "reset-re-identify-mb-group-new";
    let new_response = make_mb_release_for_re_identify(new_release_id, new_group_id, 2);
    seed_release_cache(
            new_release_id,
            (
                new_response,
                empty_mb_external_urls(),
                r#"{"id":"reset-re-identify-mb-rel-new","title":"Album Title","date":"2024-01-01","artist-credit":[{"name":"Artist Name","artist":{"id":"mb-artist-1","name":"Artist Name","sort-name":"Artist Name"}}],"release-group":{"id":"reset-re-identify-mb-group-new"},"media":[{"format":"CD","tracks":[{"position":1,"number":"1","title":"Track 1"},{"position":2,"number":"2","title":"Track 2"}]}]}"#.to_string(),
            ),
        );
    seed_release_group_json_cache(
        new_group_id,
        r#"{"id":"reset-re-identify-mb-group-new"}"#.to_string(),
    );

    manager
        .re_identify_release(
            &release.id,
            IdentityChoice::Exact {
                release_ref: MetadataRef {
                    source: MetadataSource::MusicBrainz,
                    id: new_release_id.to_string(),
                },
            },
        )
        .await
        .unwrap();

    // Reset replays the seed through the new pointer. A stale
    // cache would surface here as a missing key, a
    // parse error, or a divergence-guard `Err`. Success means
    // re_identify_release left the cache aligned.
    let edit = manager
        .reset_metadata_to_source(&release.id)
        .await
        .expect("reset must replay through aligned cache after re-identify");
    assert_eq!(edit.album_title, "Album Title");
    assert_eq!(edit.tracks.len(), 2);
}

#[tokio::test]
async fn re_identify_to_unknown_reseeds_rows_from_file_tags() {
    // A release carrying MusicBrainz-shaped rows, with local audio
    // files whose embedded tags say something different. Re-identifying
    // as Unknown must reseed the album/track rows from those tags — not
    // leave the old MB metadata displayed under a "use my files" claim.
    use crate::import::IdentityChoice;
    use lofty::config::WriteOptions;
    use lofty::prelude::*;
    use lofty::tag::{Tag, TagType};
    use std::fs;

    let (manager, _temp_dir) = setup_test_manager().await;

    // Local files live in a local folder so `local_file_path`
    // resolves to disk where lofty can read the embedded tags.
    let media = TempDir::new().unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flac");

    let tag_file = |name: &str, title: &str| -> String {
        let src = fixtures.join("01 Test Track 1.flac");
        let dest = media.path().join(name);
        fs::copy(&src, &dest).unwrap();
        let mut tagged = lofty::read_from_path(&dest).unwrap();
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.set_title(title.to_string());
        tag.set_artist("Tagged Artist".to_string());
        tag.set_album("Tagged Album".to_string());
        tagged.insert_tag(tag);
        tagged.save_to_path(&dest, WriteOptions::default()).unwrap();
        name.to_string()
    };
    let f1 = tag_file("01.flac", "Tagged One");
    let f2 = tag_file("02.flac", "Tagged Two");

    let album = create_test_album();
    let mut release = create_test_release(&album.id);
    // MusicBrainz-shaped pointer; the rows below carry MB metadata.
    release.metadata_source = crate::db::ReleaseMetadataSource::MusicBrainz;
    release.metadata_source_release_id = Some("mb-rel-1".to_string());
    release.remote = false;

    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&release).await.unwrap();
    manager
        .database
        .insert_release_identities(&release.id, &[mb_identity("g1", Some("mb-rel-1"))])
        .await
        .unwrap();

    // MB-shaped track rows — distinct from the embedded tags.
    for (i, (id, title)) in [("t1", "MB Track One"), ("t2", "MB Track Two")]
        .into_iter()
        .enumerate()
    {
        let track = crate::db::DbTrack {
            id: id.to_string(),
            release_id: release.id.clone(),
            title: title.to_string(),
            side: 1,
            track_number: Some(i as i32 + 1),
            duration_ms: None,
            discogs_position: None,
            created_at: Utc::now(),
        };
        manager.database.insert_track(&track).await.unwrap();
    }
    let now = Utc::now();
    for name in [&f1, &f2] {
        let file = crate::db::DbFile::new(
            &release.id,
            name,
            0,
            crate::util::content_type::ContentType::Flac,
            Uuid::new_v4().to_string(),
            now,
        );
        manager.database.insert_file(&file).await.unwrap();
    }
    // Register the in-place files as coven external refs after inserting them.
    manager
        .database
        .register_release_external_refs_for_test(&release.id, &media.path().to_string_lossy())
        .await
        .unwrap();

    manager
        .re_identify_release(&release.id, IdentityChoice::Unknown)
        .await
        .unwrap();

    // Pointer flipped to file_tags.
    let updated = manager
        .database
        .find_release_by_id(&release.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated.metadata_source,
        crate::db::ReleaseMetadataSource::FileTags
    );

    // Album + track rows now reflect the embedded tags, not the MB seed.
    let landing_album_id = manager
        .database
        .find_album_id_for_release(&release.id)
        .await
        .unwrap()
        .unwrap();
    let landing_album = manager
        .database
        .find_album_by_id(&landing_album_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(landing_album.title, "Tagged Album");

    let tracks = manager
        .database
        .get_tracks_for_release(&release.id)
        .await
        .unwrap();
    let titles: Vec<&str> = tracks.iter().map(|t| t.title.as_str()).collect();
    assert!(
        titles.contains(&"Tagged One") && titles.contains(&"Tagged Two"),
        "track rows must carry the embedded tag titles, got {titles:?}"
    );
    assert!(
        !titles.iter().any(|t| t.starts_with("MB ")),
        "old MusicBrainz track titles must be gone, got {titles:?}"
    );
}

#[tokio::test]
async fn discogs_client_withheld_when_rejected() {
    use crate::config::DiscogsValidation;
    let (manager, _temp_dir) = setup_test_manager_with_library_id("discogs-withheld-test").await;
    manager.save_discogs_key("a-key").unwrap();

    manager
        .set_discogs_key_stored(DiscogsValidation::Valid)
        .unwrap();
    assert!(
        manager.discogs_client().unwrap().is_some(),
        "a Valid key is served"
    );

    manager
        .set_discogs_validation(DiscogsValidation::Unvalidated)
        .unwrap();
    assert!(
        manager.discogs_client().unwrap().is_some(),
        "an Unvalidated key is served optimistically"
    );

    manager
        .set_discogs_validation(DiscogsValidation::Rejected)
        .unwrap();
    assert!(
        manager.discogs_client().unwrap().is_none(),
        "a Rejected key is withheld"
    );
}

#[tokio::test]
async fn discogs_validation_observer_confirms_and_rejects() {
    use crate::config::DiscogsValidation;
    use crate::discogs::client::DiscogsKeySignal;
    let (manager, _temp_dir) = setup_test_manager().await;
    let observe = manager.discogs_validation_observer();

    // A success confirms a stored Unvalidated key.
    manager
        .set_discogs_key_stored(DiscogsValidation::Unvalidated)
        .unwrap();
    observe(DiscogsKeySignal::Accepted);
    assert_eq!(manager.discogs_validation(), Some(DiscogsValidation::Valid));

    // A 401 rejects, from any prior state.
    observe(DiscogsKeySignal::Rejected);
    assert_eq!(
        manager.discogs_validation(),
        Some(DiscogsValidation::Rejected)
    );

    // A success does NOT flip an already-Rejected key back to Valid.
    observe(DiscogsKeySignal::Accepted);
    assert_eq!(
        manager.discogs_validation(),
        Some(DiscogsValidation::Rejected)
    );

    // A success while already Valid is a no-op (only Unvalidated -> Valid).
    manager
        .set_discogs_validation(DiscogsValidation::Valid)
        .unwrap();
    observe(DiscogsKeySignal::Accepted);
    assert_eq!(manager.discogs_validation(), Some(DiscogsValidation::Valid));
}

/// Aborting the transfer driver mid-flight (the bridge future is abortable)
/// must still emit `ReleaseTransferEnded` so the UI's transfer indicator
/// clears. The drop guard inside `drive_transfer` fires it when the future
/// is dropped between progress events.
#[tokio::test]
async fn aborted_transfer_still_emits_transfer_ended() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let mut rx = manager.subscribe_events();

    // A channel whose sender we hold open: `drive_transfer` parks in
    // `rx.recv().await` forever, so the only way out is the abort below.
    let (tx, progress_rx) = tokio::sync::mpsc::unbounded_channel();

    let driver = manager.clone();
    let handle = tokio::spawn(async move {
        let result = driver
            .drive_transfer("rel-abort", ReleaseStorageAction::Pin, progress_rx)
            .await;
        panic!("the parked driver must only exit by abort, returned {result:?}");
    });

    // Let the task reach its parked `recv()` before aborting.
    tokio::task::yield_now().await;
    handle.abort();
    let join = handle.await;
    assert!(
        join.expect_err("the parked driver can only exit by abort")
            .is_cancelled(),
        "the driver must end by cancellation, not a panic"
    );
    drop(tx);

    let event = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("ReleaseTransferEnded must arrive after abort")
        .expect("event channel stays open");
    assert!(
        matches!(
            &event,
            LibraryEvent::ReleaseTransferEnded { release_id } if release_id == "rel-abort"
        ),
        "the drop guard must emit ReleaseTransferEnded for the aborted release, got {event:?}"
    );
}

/// Seed an album with two releases, each holding two tracks with explicit
/// side/track-number so the library order is deterministic. Track ids are
/// chosen so the `(release_id, side, track_number, id)` order is unambiguous.
async fn seed_two_release_library(manager: &LibraryManager) -> (String, String) {
    use crate::db::DbTrack;
    let mut album = create_test_album();
    album.id = "alb-1".to_string();
    let mut rel1 = create_test_release(&album.id);
    rel1.id = "rel-1".to_string();
    let mut rel2 = create_test_release(&album.id);
    rel2.id = "rel-2".to_string();
    manager.database.insert_album(&album).await.unwrap();
    manager.database.insert_release(&rel1).await.unwrap();
    manager.database.insert_release(&rel2).await.unwrap();

    let track = |release_id: &str, id: &str, side: i32, number: i32| {
        let t = DbTrack {
            side,
            ..DbTrack::new_test(release_id, id, "Track Title", Some(number))
        };
        let database = &manager.database;
        async move { database.insert_track(&t).await.unwrap() }
    };
    // rel-1: side 1 then side 2; rel-2: two side-1 tracks.
    track("rel-1", "r1-t1", 1, 1).await;
    track("rel-1", "r1-t2", 2, 1).await;
    track("rel-2", "r2-t1", 1, 1).await;
    track("rel-2", "r2-t2", 1, 2).await;
    (rel1.id, rel2.id)
}

/// `get_all_track_ids` returns every library track in the deterministic base
/// order — by release, then side, track number, id — so a shuffle seed
/// permutes a stable list.
#[tokio::test]
async fn test_get_all_track_ids_returns_library_in_base_order() {
    let (manager, _temp_dir) = setup_test_manager().await;
    seed_two_release_library(&manager).await;
    let all = manager.get_all_track_ids().await.unwrap();
    assert_eq!(all, vec!["r1-t1", "r1-t2", "r2-t1", "r2-t2"]);
}

/// The two track-id queries the service's source dispatcher routes between:
/// a release's own ordered tracks (`get_track_ids`) vs the whole library
/// (`get_all_track_ids`). The library is the union of the releases, so a
/// release's tracks are a strict subset of it.
#[tokio::test]
async fn test_release_and_library_track_id_queries_return_their_sets() {
    let (manager, _temp_dir) = setup_test_manager().await;
    let (rel1, _rel2) = seed_two_release_library(&manager).await;
    let release_tracks = manager.get_track_ids(&rel1).await.unwrap();
    assert_eq!(release_tracks, vec!["r1-t1", "r1-t2"]);
    let library_tracks = manager.get_all_track_ids().await.unwrap();
    assert_eq!(library_tracks, vec!["r1-t1", "r1-t2", "r2-t1", "r2-t2"]);
    assert!(release_tracks.iter().all(|t| library_tracks.contains(t)));
}

/// A `playback_state` row carrying a library source survives save → load: the
/// `source` column stores the library sentinel and reads back unchanged, and a
/// release row stores/reads its id. (Decoding the sentinel back to the source
/// enum is covered in `playback::persisted`.)
#[tokio::test]
async fn test_playback_state_source_column_round_trips_both_kinds() {
    use crate::db::{DbPlaybackContext, DbPlaybackState};
    use crate::playback::source_to_str;
    use crate::playback::ContextSource;
    let (manager, _temp_dir) = setup_test_manager().await;

    for source in [
        ContextSource::Library,
        ContextSource::Release("rel-1".to_string()),
    ] {
        let row = DbPlaybackState {
            context: Some(DbPlaybackContext {
                source: source_to_str(&source),
                shuffle_seed: Some(11),
                cursor: 0,
            }),
            manual: "[]".to_string(),
            repeat: "off".to_string(),
            current_track_id: None,
            position_ms: None,
            volume: 1.0,
            is_muted: false,
        };
        manager.save_playback_state(&row).await.unwrap();
        let loaded = manager
            .load_playback_state()
            .await
            .unwrap()
            .expect("a saved row loads");
        assert_eq!(
            loaded.context.unwrap().source,
            source_to_str(&source),
            "the source column round-trips for {source:?}"
        );
    }
}
