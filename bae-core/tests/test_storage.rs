#![cfg(feature = "test-utils")]
//! Integration tests for import storage.
//!
//! Tests:
//! - Import with cover selection: verifies cover, audio formats, progress events
//! - Local import: files stay in original location
//! - Local delete preserves files on disk
use crate::support::{seed_discogs_test_release, test_config_and_keys, wait_for_import_complete};
use bae_core::db::{Database, LibraryImageType};
use bae_core::discogs::models::{DiscogsArtist, DiscogsRelease, DiscogsTrack};
use bae_core::import::{
    CoverSelection, IdentityChoice, ImportCommand, ImportProgress, ImportService, MetadataRef,
    MetadataSource, StorageMode,
};
use bae_core::library::LibraryManager;
use bae_core::util::content_type::ContentType;
use bae_test_support as support;
use coven::StoreDir;
use std::fs;
use std::path::Path;
use tempfile::TempDir;
use tracing::info;

fn start_test_import(
    runtime_handle: tokio::runtime::Handle,
    library_manager: LibraryManager,
) -> bae_core::import::ImportServiceHandle {
    ImportService::start(
        runtime_handle.clone(),
        library_manager,
        bae_core::import::cover_art::CoverArtArchiveClient::new(),
    )
}

/// Initialize tracing for tests
fn tracing_init() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_line_number(true)
        .with_target(false)
        .with_file(true)
        .try_init();
}

/// Test import with cover selection: files stay in place (no cloud home),
/// but cover selection, audio format records, and progress events all work.
#[tokio::test]
async fn test_import_with_cover_selection() {
    tracing_init();
    run_import_with_cover_test().await;
}

/// Test local import: files stay in original location.
#[tokio::test]
async fn test_local_import() {
    tracing_init();

    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    fs::create_dir_all(&album_dir).expect("album dir");
    fs::create_dir_all(&db_dir).expect("db dir");

    let file_data = generate_test_files(&album_dir);
    let original_files: Vec<_> = [
        "01 Track One.flac",
        "02 Track Two.flac",
        "03 Track Three.flac",
    ]
    .iter()
    .map(|f| album_dir.join(f))
    .collect();

    // Verify files exist before import
    for path in &original_files {
        assert!(path.exists(), "Test file should exist: {:?}", path);
    }

    let db_file = db_dir.join("test.db");
    let database = Database::new_test(
        db_file.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .expect("database");
    let library_dir = StoreDir::new(db_dir.clone());
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::new(
        database.clone(),
        config_handle,
        key_service,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        tokio::runtime::Handle::current(),
    );
    let runtime_handle = tokio::runtime::Handle::current();

    let discogs_release = create_test_discogs_release();
    let release_id_key = seed_discogs_test_release(discogs_release);
    let import_handle = start_test_import(runtime_handle, library_manager.clone());

    let import_id = uuid::Uuid::new_v4().to_string();
    import_handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir.clone(),
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .expect("send command");
    let mut progress_rx = import_handle.subscribe_import(import_id);
    let (release_id, _album_id) = wait_for_import_complete(&mut progress_rx).await;

    info!("Import completed, release_id: {}", release_id);

    // Verify tracks created
    let tracks = library_manager
        .get_tracks_for_release(&release_id)
        .await
        .expect("get tracks");
    assert_eq!(tracks.len(), file_data.len(), "Should have all tracks");

    info!("All {} tracks created", tracks.len());

    // Verify audio_format records exist with segment file linkage
    for track in &tracks {
        let audio_format = library_manager
            .get_audio_format_by_track_id(&track.id)
            .await
            .expect("get audio_format");
        assert!(
            audio_format.is_some(),
            "Track '{}' should have an audio_format record",
            track.title
        );
        let _af = audio_format.unwrap();
        let resolved = library_manager
            .resolve_track_audio(&track.id)
            .await
            .expect("resolve track audio");
        assert!(
            resolved
                .segments
                .iter()
                .any(|segment| !segment.file_id.is_empty()),
            "Track '{}' should have a segment file_id",
            track.title
        );
    }

    info!("All tracks have audio_format records with segment file links");

    // Verify release is local with this device's in-place local copy.
    let release = database
        .find_release_by_id(&release_id)
        .await
        .expect("query")
        .expect("release should exist");
    assert!(!release.remote, "Local import should not be remote");
    // A Local release's files are coven user-provided external refs at their
    // in-place location; that external ref IS the local state.
    let files = library_manager
        .get_files_for_release(&release_id)
        .await
        .expect("get files");
    let local_path = library_manager
        .file_local_path(&files[0].id)
        .await
        .expect("query")
        .expect("local import should register an in-place external ref");
    assert!(
        local_path.starts_with(&album_dir),
        "Local import should keep files in place under {album_dir:?}, got {local_path:?}"
    );

    info!("Release is correctly local");

    // Verify original files still exist in place
    for path in &original_files {
        assert!(
            path.exists(),
            "Original file should still exist after local import: {:?}",
            path
        );
    }

    info!("Original files preserved in place");
}

/// Test that deleting a local release preserves the original files on disk.
///
/// When a release is local, the files live at their original location.
/// Deleting the release should only remove database records, NOT the actual files.
#[tokio::test]
async fn test_local_delete_preserves_files() {
    tracing_init();

    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    fs::create_dir_all(&album_dir).expect("album dir");
    fs::create_dir_all(&db_dir).expect("db dir");

    let _file_data = generate_test_files(&album_dir);
    let original_files: Vec<_> = [
        "01 Track One.flac",
        "02 Track Two.flac",
        "03 Track Three.flac",
    ]
    .iter()
    .map(|f| album_dir.join(f))
    .collect();

    // Verify files exist before import
    for path in &original_files {
        assert!(path.exists(), "Test file should exist: {:?}", path);
    }

    let db_file = db_dir.join("test.db");
    let database = Database::new_test(
        db_file.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .expect("database");
    let library_dir = StoreDir::new(db_dir.clone());
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::new(
        database.clone(),
        config_handle,
        key_service,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        tokio::runtime::Handle::current(),
    );
    let runtime_handle = tokio::runtime::Handle::current();

    let discogs_release = create_test_discogs_release();
    let release_id_key = seed_discogs_test_release(discogs_release);
    let import_handle = start_test_import(runtime_handle, library_manager.clone());

    let import_id = uuid::Uuid::new_v4().to_string();
    import_handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir.clone(),
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .expect("send command");
    let mut progress_rx = import_handle.subscribe_import(import_id);
    let (release_id, album_id) = wait_for_import_complete(&mut progress_rx).await;

    info!("Import completed, release_id: {}", release_id);

    // Verify import succeeded
    let tracks = library_manager
        .get_tracks_for_release(&release_id)
        .await
        .expect("get tracks");
    assert_eq!(tracks.len(), 3, "Should have 3 tracks after import");

    // Verify audio_format records exist.
    for track in &tracks {
        let audio_format = library_manager
            .get_audio_format_by_track_id(&track.id)
            .await
            .expect("get audio_format");
        assert!(
            audio_format.is_some(),
            "Track '{}' should have an audio_format record",
            track.title
        );
    }

    // Files should still exist after import
    for path in &original_files {
        assert!(
            path.exists(),
            "File should still exist after local import: {:?}",
            path
        );
    }

    // Now delete the release
    info!("Deleting release {}", release_id);
    library_manager
        .delete_release(&release_id)
        .await
        .expect("delete release");

    // Verify database records are gone
    let tracks_after = library_manager
        .get_tracks_for_release(&release_id)
        .await
        .expect("get tracks after delete");
    assert!(
        tracks_after.is_empty(),
        "Tracks should be deleted from database"
    );

    let album_after = library_manager
        .get_album_by_id(&album_id)
        .await
        .expect("get album after delete");
    assert!(
        album_after.is_none(),
        "Album should be deleted (was last release)"
    );

    // THE KEY ASSERTION: Original files must still exist on disk
    for path in &original_files {
        assert!(
            path.exists(),
            "Original file must be preserved after deleting local release: {:?}",
            path
        );
    }

    info!("Local delete preserves original files");
}

async fn run_import_with_cover_test() {
    let temp_root = TempDir::new().expect("Failed to create temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    fs::create_dir_all(&album_dir).expect("Failed to create album dir");
    fs::create_dir_all(&db_dir).expect("Failed to create db dir");
    let file_data = generate_test_files(&album_dir);
    info!("Generated {} test files", file_data.len());
    let db_file = db_dir.join("test.db");
    let database = Database::new_test(
        db_file.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .expect("Failed to create database");
    let library_dir = StoreDir::new(db_dir.clone());
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::new(
        database.clone(),
        config_handle,
        key_service,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        tokio::runtime::Handle::current(),
    );
    let runtime_handle = tokio::runtime::Handle::current();

    let discogs_release = create_test_discogs_release();
    let release_id_key = seed_discogs_test_release(discogs_release);
    let import_handle = start_test_import(runtime_handle, library_manager.clone());
    let selected_cover = "scans/back.jpg".to_string();
    let import_id = uuid::Uuid::new_v4().to_string();
    import_handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir.clone(),
            selected_cover: Some(CoverSelection::Local(selected_cover.clone())),
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .expect("send command");
    let mut progress_rx = import_handle.subscribe_import(import_id);
    let mut release_id = String::new();
    while let Some(progress) = progress_rx.recv().await {
        info!("Progress: {:?}", progress);
        match &progress {
            ImportProgress::Complete { id, .. } => {
                release_id = id.clone();
                info!("Release completion event received!");
                break;
            }
            ImportProgress::Failed { error, .. } => {
                panic!("Import failed: {}", error);
            }
            _ => {}
        }
    }
    assert!(!release_id.is_empty(), "Should receive release completion");

    // Verify release storage state — without cloud home, this is a local
    // (local) import, so it's not remote and records an in-place local copy.
    let release = database
        .find_release_by_id(&release_id)
        .await
        .expect("Failed to query release")
        .expect("release should exist");
    assert!(!release.remote, "Import without cloud home should be local");
    let local_files = library_manager
        .get_files_for_release(&release_id)
        .await
        .expect("get files");
    let local_path = library_manager
        .file_local_path(&local_files[0].id)
        .await
        .expect("query")
        .expect("local import should register an in-place external ref");
    assert!(
        local_path.starts_with(&album_dir),
        "Import without cloud home should keep files in place under {album_dir:?}, got {local_path:?}"
    );
    info!("Release has in-place external refs");

    let releases = library_manager
        .get_releases_for_album(&release.album_id)
        .await
        .expect("Failed to get releases");
    let tracks = library_manager
        .get_tracks_for_release(&releases[0].id)
        .await
        .expect("Failed to get tracks");
    assert_eq!(tracks.len(), 3, "Expected 3 tracks");
    info!("All {} tracks created", tracks.len());

    let files = library_manager
        .get_files_for_release(&release_id)
        .await
        .expect("Failed to get files");
    assert!(!files.is_empty(), "Should have file records");

    // Verify original files still exist (local import keeps files in place)
    for file in &files {
        let original_path = album_dir.join(&file.original_filename);
        assert!(
            original_path.exists(),
            "File '{}' should still exist at original location: {:?}",
            file.original_filename,
            original_path,
        );
    }
    info!(
        "{} DbFile records with original files in place",
        files.len()
    );

    // Verify audio format records for tracks
    for track in &tracks {
        let audio_format = library_manager
            .get_audio_format_by_track_id(&track.id)
            .await
            .expect("Failed to get audio format")
            .expect("Audio format should exist for track");
        assert_eq!(
            audio_format.content_type,
            ContentType::Flac,
            "Should be FLAC format"
        );
        let resolved = library_manager
            .resolve_track_audio(&track.id)
            .await
            .expect("Failed to resolve track audio");
        assert!(
            resolved
                .segments
                .iter()
                .any(|segment| !segment.file_id.is_empty()),
            "Track '{}' should have a segment file_id",
            track.title
        );
        info!(
            "Track '{}' has audio format with segment file link",
            track.title
        );
    }

    let cover = library_manager
        .get_library_image(&release_id, &LibraryImageType::Cover)
        .await
        .expect("Failed to get cover")
        .expect("Cover should exist in the covers table");
    assert_eq!(cover.content_type, ContentType::Jpeg);
    assert_eq!(cover.source, "local");
    let source_url = cover.source_url.as_ref().expect("source_url should be set");
    assert!(
        source_url.starts_with("release://"),
        "source_url should start with release://, got: {}",
        source_url,
    );
    assert!(
        source_url.contains(&selected_cover),
        "source_url should contain selected cover '{}', got: {}",
        selected_cover,
        source_url,
    );
    info!("Cover library_image record exists with correct source");
    let album_id = library_manager
        .get_album_id_for_release(&release_id)
        .await
        .expect("Failed to get album_id");
    let album = library_manager
        .get_album_by_id(&album_id)
        .await
        .expect("Failed to get album")
        .expect("Album should exist");
    assert_eq!(
        album.primary_release_id.as_ref(),
        Some(&release_id),
        "Album primary_release_id should match the release",
    );
    info!("Album primary_release_id is set correctly");

    // Verify roundtrip: audio format records exist with segment file linkage
    for track in &tracks {
        let audio_format = library_manager
            .get_audio_format_by_track_id(&track.id)
            .await
            .expect("Failed to get audio format")
            .expect("Audio format should exist for single-file track");
        assert_eq!(
            audio_format.content_type,
            ContentType::Flac,
            "Should be FLAC format"
        );
        let resolved = library_manager
            .resolve_track_audio(&track.id)
            .await
            .expect("Failed to resolve track audio");
        assert!(
            resolved
                .segments
                .iter()
                .any(|segment| !segment.file_id.is_empty()),
            "Track '{}' should have a segment file_id",
            track.title
        );
    }
    info!("Import with cover selection test passed");
}

fn create_test_discogs_release() -> DiscogsRelease {
    DiscogsRelease {
        id: "test-release-storage".to_string(),
        title: "Storage Test Album".to_string(),
        year: Some(2024),
        format: vec![],
        country: Some("US".to_string()),
        label: vec!["Test Label".to_string()],
        cover_image: None,
        thumb: None,
        catno: None,
        artists: vec![DiscogsArtist {
            id: "discogs-artist-1".to_string(),
            name: "Artist Name".to_string(),
        }],
        extraartists: Some(vec![]),
        tracklist: vec![
            DiscogsTrack {
                type_: "track".to_string(),
                position: "1".to_string(),
                title: "Track One".to_string(),
                duration: Some("3:00".to_string()),
                artists: vec![],
                extraartists: None,
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "2".to_string(),
                title: "Track Two".to_string(),
                duration: Some("4:00".to_string()),
                artists: vec![],
                extraartists: None,
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "3".to_string(),
                title: "Track Three".to_string(),
                duration: Some("2:30".to_string()),
                artists: vec![],
                extraartists: None,
            },
        ],
        master_id: Some("test-master-storage".to_string()),
    }
}

fn generate_test_files(dir: &Path) -> Vec<Vec<u8>> {
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flac")
        .join("01 Test Track 1.flac");
    let flac_template = std::fs::read(&fixture_path)
        .expect("Failed to read FLAC fixture - run scripts/generate_test_flac.sh");
    let files = [
        "01 Track One.flac",
        "02 Track Two.flac",
        "03 Track Three.flac",
    ];
    let bae_dir = dir.join(".bae");
    fs::create_dir_all(&bae_dir).expect("Failed to create .bae directory");
    let minimal_jpeg: Vec<u8> = vec![
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00,
        0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06, 0x07, 0x06,
        0x05, 0x08, 0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0A, 0x0C, 0x14, 0x0D, 0x0C, 0x0B, 0x0B,
        0x0C, 0x19, 0x12, 0x13, 0x0F, 0x14, 0x1D, 0x1A, 0x1F, 0x1E, 0x1D, 0x1A, 0x1C, 0x1C, 0x20,
        0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C, 0x23, 0x1C, 0x1C, 0x28, 0x37, 0x29, 0x2C, 0x30, 0x31,
        0x34, 0x34, 0x34, 0x1F, 0x27, 0x39, 0x3D, 0x38, 0x32, 0x3C, 0x2E, 0x33, 0x34, 0x32, 0xFF,
        0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01, 0x01, 0x01, 0x11, 0x00, 0xFF, 0xC4, 0x00,
        0x1F, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B,
        0xFF, 0xC4, 0x00, 0xB5, 0x10, 0x00, 0x02, 0x01, 0x03, 0x03, 0x02, 0x04, 0x03, 0x05, 0x05,
        0x04, 0x04, 0x00, 0x00, 0x01, 0x7D, 0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21,
        0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xA1, 0x08,
        0x23, 0x42, 0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0A,
        0x16, 0x17, 0x18, 0x19, 0x1A, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x34, 0x35, 0x36, 0x37,
        0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x53, 0x54, 0x55, 0x56,
        0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x73, 0x74, 0x75,
        0x76, 0x77, 0x78, 0x79, 0x7A, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x92, 0x93,
        0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9,
        0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6,
        0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2,
        0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7,
        0xF8, 0xF9, 0xFA, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00, 0xFB, 0xD5,
        0xDB, 0x20, 0xA8, 0xF1, 0x7E, 0xFF, 0xD9,
    ];
    fs::write(bae_dir.join("cover-mb.jpg"), &minimal_jpeg).expect("Failed to write cover image");
    fs::write(dir.join("cover.jpg"), &minimal_jpeg).expect("Failed to write local cover");
    let scans_dir = dir.join("scans");
    fs::create_dir_all(&scans_dir).expect("Failed to create scans directory");
    fs::write(scans_dir.join("front.jpg"), &minimal_jpeg).expect("Failed to write scans/front.jpg");
    fs::write(scans_dir.join("back.jpg"), &minimal_jpeg).expect("Failed to write scans/back.jpg");
    files
        .iter()
        .map(|filename| {
            let file_path = dir.join(filename);
            fs::write(&file_path, &flac_template).expect("Failed to write FLAC file");
            flac_template.clone()
        })
        .collect()
}

/// A local (local-only) import records `local_path` at the folder the
/// files actually live in, not the system temp directory.
///
/// The user imports a folder in place; `local_path` must reflect that
/// on-disk location so playback, pin, and export read the originals.
#[tokio::test]
async fn test_local_import_not_in_temp_dir() {
    tracing_init();

    // The folder lives in the CWD (not system temp) to simulate a user's
    // Music folder.
    let user_music_dir =
        TempDir::with_prefix_in("bae-test-music-", std::env::current_dir().unwrap())
            .expect("create user music dir");
    let album_dir = user_music_dir.path().join("test-album");
    fs::create_dir_all(&album_dir).expect("create album dir");
    let _file_data = generate_test_files(&album_dir);

    // Set up library WITHOUT cloud home (local-only / local)
    let lib_root = TempDir::new().expect("create lib root");
    let db_dir = lib_root.path().join("db");
    fs::create_dir_all(&db_dir).expect("create db dir");

    let db_file = db_dir.join("test.db");
    let database = Database::new_test(
        db_file.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .expect("create database");
    let library_dir = StoreDir::new(db_dir.clone());
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::new(
        database.clone(),
        config_handle,
        key_service,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        tokio::runtime::Handle::current(),
    );
    let runtime_handle = tokio::runtime::Handle::current();

    let discogs_release = create_test_discogs_release();
    let release_id_key = seed_discogs_test_release(discogs_release);
    let import_handle = start_test_import(runtime_handle, library_manager.clone());
    let import_id = uuid::Uuid::new_v4().to_string();
    import_handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir.clone(),
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .expect("send command");
    let mut progress_rx = import_handle.subscribe_import(import_id);
    let (release_id, _album_id) = wait_for_import_complete(&mut progress_rx).await;

    info!("Import completed, release_id: {release_id}");

    database
        .find_release_by_id(&release_id)
        .await
        .expect("query release")
        .expect("release should exist");
    let files = library_manager
        .get_files_for_release(&release_id)
        .await
        .expect("get files");
    let local_path = library_manager
        .file_local_path(&files[0].id)
        .await
        .expect("query local external ref")
        .expect("local library import should register an in-place external ref")
        .to_string_lossy()
        .into_owned();

    // The local_path must NOT be in the system temp directory — it must
    // point at the folder the user imported in place.
    assert!(
        !Path::new(&local_path).starts_with(std::env::temp_dir()),
        "Local files must not live in the system temp directory.\n\
         Got local_path: {local_path}\n\
         Temp: {:?}",
        std::env::temp_dir()
    );

    // Verify it's under the user-chosen download directory
    assert!(
        Path::new(&local_path).starts_with(user_music_dir.path()),
        "local_path should be under the user-chosen save directory.\n\
         Got: {local_path}\n\
         Expected prefix: {:?}",
        user_music_dir.path()
    );
}
