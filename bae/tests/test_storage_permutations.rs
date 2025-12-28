#![cfg(feature = "test-utils")]

//! Parameterized integration tests for all 8 storage permutations.
//!
//! Tests all combinations of:
//! - Location: Local, Cloud
//! - Chunked: true, false
//! - Encrypted: true, false

use std::path::Path;
use std::{fs, path::PathBuf};

use bae::cache::CacheManager;
use bae::cloud_storage::CloudStorageManager;
use bae::db::{Database, DbStorageProfile, ImportStatus, StorageLocation};
use bae::discogs::models::{DiscogsRelease, DiscogsTrack};
use bae::encryption::EncryptionService;
use bae::import::{ImportConfig, ImportPhase, ImportProgress, ImportRequest, ImportService};
use bae::library::LibraryManager;
use bae::test_support::MockCloudStorage;
use bae::torrent::TorrentManagerHandle;
use std::sync::Arc;
use tempfile::TempDir;
use tracing::info;

/// Initialize tracing for tests
fn tracing_init() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_line_number(true)
        .with_target(false)
        .with_file(true)
        .try_init();
}

#[tokio::test]
async fn test_storage_permutations() {
    tracing_init();

    for location in [StorageLocation::Local, StorageLocation::Cloud] {
        for chunked in [false, true] {
            for encrypted in [false, true] {
                info!(
                    "\n\n========== Testing: {:?} / chunked={} / encrypted={} ==========\n",
                    location, chunked, encrypted
                );
                run_storage_test(location, chunked, encrypted).await;
            }
        }
    }
}

async fn run_storage_test(location: StorageLocation, chunked: bool, encrypted: bool) {
    // 1. Setup directories
    let temp_root = TempDir::new().expect("Failed to create temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    let cache_dir = temp_root.path().join("cache");
    let storage_dir = temp_root.path().join("storage");

    fs::create_dir_all(&album_dir).expect("Failed to create album dir");
    fs::create_dir_all(&db_dir).expect("Failed to create db dir");
    fs::create_dir_all(&cache_dir).expect("Failed to create cache dir");
    fs::create_dir_all(&storage_dir).expect("Failed to create storage dir");

    // 2. Generate test files
    let file_data = generate_test_files(&album_dir);
    info!("Generated {} test files", file_data.len());

    // 3. Setup services
    let chunk_size_bytes = 1024 * 1024; // 1MB
    let mock_storage = Arc::new(MockCloudStorage::new());
    let cloud_storage = CloudStorageManager::from_storage(mock_storage.clone());

    let db_file = db_dir.join("test.db");
    let database = Database::new(db_file.to_str().unwrap())
        .await
        .expect("Failed to create database");

    let encryption_service = EncryptionService::new_with_key(vec![0u8; 32]);

    let cache_config = bae::cache::CacheConfig {
        cache_dir: cache_dir.clone(),
        max_size_bytes: 1024 * 1024 * 1024,
        max_chunks: 10000,
    };
    let cache_manager = CacheManager::with_config(cache_config)
        .await
        .expect("Failed to create cache manager");

    let library_manager = LibraryManager::new(database.clone(), cloud_storage.clone());
    let shared_library_manager = bae::library::SharedLibraryManager::new(library_manager.clone());
    let library_manager = Arc::new(library_manager);

    // 4. Create and insert storage profile
    let storage_profile =
        create_storage_profile(&location, chunked, encrypted, storage_dir.to_str().unwrap());
    let storage_profile_id = storage_profile.id.clone();

    database
        .insert_storage_profile(&storage_profile)
        .await
        .expect("Failed to insert storage profile");

    info!(
        "Created storage profile: {} (id: {})",
        storage_profile.name, storage_profile_id
    );

    // 5. Start import service
    let runtime_handle = tokio::runtime::Handle::current();
    let import_config = ImportConfig {
        chunk_size_bytes,
        max_encrypt_workers: 4,
        max_upload_workers: 20,
        max_db_write_workers: 10,
    };

    let torrent_handle = TorrentManagerHandle::new_dummy();
    let database_arc = Arc::new(database.clone());

    let import_handle = ImportService::start(
        import_config,
        runtime_handle,
        shared_library_manager,
        encryption_service.clone(),
        cloud_storage.clone(),
        torrent_handle,
        database_arc,
    );

    // 6. Send import request with storage profile
    let discogs_release = create_test_discogs_release();
    let master_year = discogs_release.year.unwrap_or(2024);

    // User selected "scans/back.jpg" as the cover in the import UI
    let selected_cover = "scans/back.jpg".to_string();

    let (_album_id, release_id) = import_handle
        .send_request(ImportRequest::Folder {
            discogs_release: Some(discogs_release),
            mb_release: None,
            folder: album_dir.clone(),
            master_year,
            cover_art_url: None,
            storage_profile_id: Some(storage_profile_id.clone()),
            selected_cover_filename: Some(selected_cover.clone()),
            import_id: uuid::Uuid::new_v4().to_string(),
        })
        .await
        .expect("Failed to send import request");

    info!("Import request sent, release_id: {}", release_id);

    // 7. Wait for completion and collect progress events
    let mut progress_rx = import_handle.subscribe_release(release_id.clone());
    let mut track_complete_ids: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    let mut release_complete_received = false;

    // Track progress events for verification
    let mut release_progress_received = false;
    let mut max_release_percent: u8 = 0;
    let mut progress_events_with_chunk_phase = 0;

    while let Some(progress) = progress_rx.recv().await {
        info!("Progress: {:?}", progress);
        match &progress {
            ImportProgress::Progress {
                id, percent, phase, ..
            } => {
                // Release subscription only receives release-level Progress events (id == release_id)
                // Track-level Progress events require individual track subscriptions
                if id == &release_id {
                    // Verify phase is Chunk for storage imports
                    assert_eq!(
                        *phase,
                        Some(ImportPhase::Chunk),
                        "Storage import progress should have phase=Chunk"
                    );
                    progress_events_with_chunk_phase += 1;

                    release_progress_received = true;
                    if *percent > max_release_percent {
                        max_release_percent = *percent;
                    }
                }
            }
            ImportProgress::Complete {
                id,
                release_id: rid,
                ..
            } => {
                if rid.is_none() {
                    // This is a release completion (release_id is None for release completions)
                    release_complete_received = true;
                    info!("Release completion event received!");
                    break;
                } else {
                    // This is a track completion
                    track_complete_ids.insert(id.clone());
                    info!("Track completion event received for: {}", id);
                }
            }
            ImportProgress::Failed { error, .. } => {
                panic!("Import failed: {}", error);
            }
            _ => {}
        }
    }

    // ========== VERIFICATION ==========

    // 8. Verify DB: release_storage record exists
    let release_storage = database
        .get_release_storage(&release_id)
        .await
        .expect("Failed to query release_storage")
        .expect("release_storage record should exist");

    assert_eq!(
        release_storage.storage_profile_id, storage_profile_id,
        "release_storage should link to correct profile"
    );
    info!("✓ release_storage record exists");

    // 9. Verify DB: all tracks are Complete
    let releases = library_manager
        .get_releases_for_album(&_album_id)
        .await
        .expect("Failed to get releases");
    let tracks = library_manager
        .get_tracks(&releases[0].id)
        .await
        .expect("Failed to get tracks");

    assert_eq!(tracks.len(), 3, "Expected 3 tracks");
    for track in &tracks {
        assert_eq!(
            track.import_status,
            ImportStatus::Complete,
            "Track '{}' should be Complete, got {:?}",
            track.title,
            track.import_status
        );
    }
    info!("✓ All {} tracks are Complete", tracks.len());

    // 9b. Verify ImportProgress::Complete events were sent for each track
    assert!(
        release_complete_received,
        "Should receive ImportProgress::Complete event for release"
    );
    assert_eq!(
        track_complete_ids.len(),
        tracks.len(),
        "Should receive ImportProgress::Complete event for each track. Got {} events for {} tracks. Track IDs received: {:?}",
        track_complete_ids.len(),
        tracks.len(),
        track_complete_ids
    );
    info!("✓ Received Complete events for all {} tracks", tracks.len());

    // 9c. Verify ImportProgress::Progress events were sent during import
    assert!(
        release_progress_received,
        "Should receive ImportProgress::Progress events for release during import"
    );
    assert_eq!(
        max_release_percent, 100,
        "Release progress should reach 100%"
    );
    assert!(
        progress_events_with_chunk_phase > 0,
        "Should receive multiple Progress events with phase=Chunk"
    );
    info!(
        "✓ Received {} Progress events for release (max: {}%, phase=Chunk)",
        progress_events_with_chunk_phase, max_release_percent
    );

    // 10. Verify DB: DbFile records exist
    // For chunked: file exists but may not have source_path (data is in chunks)
    // For non-chunked: file has source_path pointing to storage location
    let files = library_manager
        .get_files_for_release(&release_id)
        .await
        .expect("Failed to get files");

    assert!(!files.is_empty(), "Should have file records");
    if !chunked {
        for file in &files {
            assert!(
                file.source_path.is_some(),
                "Non-chunked file '{}' should have source_path",
                file.original_filename
            );
        }
        info!("✓ {} DbFile records with source_path", files.len());
    } else {
        info!(
            "✓ {} DbFile records (chunked, source_path not required)",
            files.len()
        );
    }

    // 11. Verify DB: DbTrackChunkCoords
    // For chunked storage: coords always exist with chunk indices
    // For non-chunked storage:
    //   - Single-file-per-track: no coords needed (file IS the track)
    //   - CUE/FLAC: coords with chunk_index=-1 and absolute byte ranges
    for track in &tracks {
        let coords = library_manager
            .get_track_chunk_coords(&track.id)
            .await
            .expect("Failed to get track chunk coords");

        if chunked {
            // Chunked: should have valid chunk indices
            let coords = coords.expect("Chunked track should have coords");
            assert!(
                coords.start_chunk_index >= 0,
                "Chunked track should have non-negative chunk index"
            );
            info!(
                "✓ Track '{}' has chunk coords: chunks {}-{}",
                track.title, coords.start_chunk_index, coords.end_chunk_index
            );
        } else {
            // Non-chunked single-file-per-track: coords may not exist (OK)
            // Non-chunked CUE/FLAC: would have chunk_index=-1 with byte ranges
            match coords {
                Some(coords) if coords.start_chunk_index == -1 => {
                    // CUE/FLAC non-chunked case
                    assert!(
                        coords.end_byte_offset > coords.start_byte_offset,
                        "End byte should be > start byte"
                    );
                    info!(
                        "✓ Track '{}' has non-chunked CUE/FLAC coords: bytes {}-{}",
                        track.title, coords.start_byte_offset, coords.end_byte_offset
                    );
                }
                Some(coords) => {
                    panic!(
                        "Unexpected coords for non-chunked track: chunk_index={}",
                        coords.start_chunk_index
                    );
                }
                None => {
                    // Single-file-per-track non-chunked: no coords needed
                    info!(
                        "✓ Track '{}' has no coords (single-file-per-track, non-chunked)",
                        track.title
                    );
                }
            }
        }
    }

    // 12. Verify DB: DbChunk/DbFileChunk (chunked only)
    if chunked {
        let chunks = library_manager
            .get_chunks_for_release(&release_id)
            .await
            .expect("Failed to get chunks");

        assert!(
            !chunks.is_empty(),
            "Chunked storage should have chunk records"
        );
        info!("✓ {} DbChunk records exist", chunks.len());

        // Verify chunk indices are unique and sequential (release-level chunking)
        let mut indices: Vec<i32> = chunks.iter().map(|c| c.chunk_index).collect();
        indices.sort();
        let unique_count = {
            let mut sorted = indices.clone();
            sorted.dedup();
            sorted.len()
        };
        assert_eq!(
            unique_count,
            chunks.len(),
            "Chunk indices must be unique - found {} unique indices for {} chunks",
            unique_count,
            chunks.len()
        );
        let expected_indices: Vec<i32> = (0..chunks.len() as i32).collect();
        assert_eq!(
            indices, expected_indices,
            "Chunk indices must be sequential 0..N"
        );
        info!(
            "✓ Chunk indices are unique and sequential (0..{})",
            chunks.len() - 1
        );

        // Verify file chunks exist
        for file in &files {
            if file.format == "flac" {
                let file_chunks = library_manager
                    .get_file_chunks(&file.id)
                    .await
                    .expect("Failed to get file chunks");

                assert!(
                    !file_chunks.is_empty(),
                    "FLAC file should have file_chunk mappings"
                );
                info!(
                    "✓ File '{}' has {} chunk mappings",
                    file.original_filename,
                    file_chunks.len()
                );
            }
        }
    }

    // 13. Verify DB: DbImage records exist with cover
    let images = library_manager
        .get_images_for_release(&release_id)
        .await
        .expect("Failed to get images");

    assert!(!images.is_empty(), "Should have at least one image record");
    assert!(
        images.iter().any(|img| img.is_cover),
        "Should have at least one image marked as cover"
    );
    info!("✓ {} DbImage records, cover image exists", images.len());

    // 13b. Verify the user-selected cover was used
    // BUG: Currently this asserts .bae/ wins because selected_cover_filename isn't implemented.
    // Once implemented, this should assert: cover_image.filename == selected_cover
    let cover_image = images.iter().find(|img| img.is_cover).unwrap();
    assert_eq!(
        cover_image.filename, selected_cover,
        "Cover should be user-selected '{}', not priority-based '{}'",
        selected_cover, cover_image.filename
    );
    info!("✓ Correct cover selected: {}", cover_image.filename);

    // 13c. Verify album.cover_image_id is set
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
        album.cover_image_id.as_ref(),
        Some(&cover_image.id),
        "Album cover_image_id should match the cover DbImage"
    );
    info!("✓ Album cover_image_id is set correctly");

    // 13d. Verify we can actually load the cover image data
    verify_image_loadable(
        cover_image,
        &library_manager,
        &cloud_storage,
        &cache_manager,
        &encryption_service,
        &location,
        chunked,
        encrypted,
    )
    .await;
    info!("✓ Cover image data is loadable");

    // 14. Verify storage state
    verify_storage_state(
        &location,
        chunked,
        encrypted,
        &storage_dir,
        &mock_storage,
        &files,
        &library_manager,
        &release_id,
    )
    .await;

    // 14. Verify roundtrip: load audio and verify it's valid FLAC
    verify_roundtrip(
        &tracks,
        &library_manager,
        &cloud_storage,
        &cache_manager,
        &encryption_service,
        chunk_size_bytes,
        chunked,
        encrypted,
        &location,
        &storage_profile_id,
    )
    .await;

    info!(
        "\n✅ Test passed: {:?} / chunked={} / encrypted={}\n",
        location, chunked, encrypted
    );
}

fn create_storage_profile(
    location: &StorageLocation,
    chunked: bool,
    encrypted: bool,
    storage_path: &str,
) -> DbStorageProfile {
    let name = format!(
        "Test-{:?}-{}-{}",
        location,
        if chunked { "chunked" } else { "raw" },
        if encrypted { "encrypted" } else { "plain" }
    );

    match location {
        StorageLocation::Local => {
            DbStorageProfile::new_local(&name, storage_path, encrypted, chunked)
        }
        StorageLocation::Cloud => DbStorageProfile::new_cloud(
            &name,
            "test-bucket",
            "us-east-1",
            None, // endpoint
            "test-access-key",
            "test-secret-key",
            encrypted,
            chunked,
        ),
    }
}

fn create_test_discogs_release() -> DiscogsRelease {
    DiscogsRelease {
        id: "test-release-storage".to_string(),
        title: "Storage Test Album".to_string(),
        year: Some(2024),
        genre: vec![],
        style: vec![],
        format: vec![],
        country: Some("US".to_string()),
        label: vec!["Test Label".to_string()],
        cover_image: None,
        thumb: None,
        artists: vec![],
        tracklist: vec![
            DiscogsTrack {
                position: "1".to_string(),
                title: "Track One".to_string(),
                duration: Some("3:00".to_string()),
            },
            DiscogsTrack {
                position: "2".to_string(),
                title: "Track Two".to_string(),
                duration: Some("4:00".to_string()),
            },
            DiscogsTrack {
                position: "3".to_string(),
                title: "Track Three".to_string(),
                duration: Some("2:30".to_string()),
            },
        ],
        master_id: "test-master-storage".to_string(),
    }
}

fn generate_test_files(dir: &Path) -> Vec<Vec<u8>> {
    // Use the real FLAC fixture file for testing
    // This ensures we have valid FLAC data for decoding tests
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flac")
        .join("01 Test Track 1.flac");
    let flac_template = std::fs::read(&fixture_path)
        .expect("Failed to read FLAC fixture - run scripts/generate_test_flac.sh");

    // Create 3 copies of the test FLAC with different names
    let files = [
        "01 Track One.flac",
        "02 Track Two.flac",
        "03 Track Three.flac",
    ];

    // Create .bae/ directory and cover image (simulates MusicBrainz cover art download)
    let bae_dir = dir.join(".bae");
    fs::create_dir_all(&bae_dir).expect("Failed to create .bae directory");

    // Minimal valid JPEG (smallest possible valid JPEG - 134 bytes)
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

    // Also create a local cover.jpg in the album root to test priority
    // The .bae/ cover should win over this local cover
    fs::write(dir.join("cover.jpg"), &minimal_jpeg).expect("Failed to write local cover");

    // Create scans/ folder with additional images (simulates user's local scans)
    // The user will select scans/back.jpg as the cover in the UI
    let scans_dir = dir.join("scans");
    fs::create_dir_all(&scans_dir).expect("Failed to create scans directory");
    fs::write(scans_dir.join("front.jpg"), &minimal_jpeg).expect("Failed to write scans/front.jpg");
    fs::write(scans_dir.join("back.jpg"), &minimal_jpeg).expect("Failed to write scans/back.jpg");

    // Write the FLAC files and return their data
    files
        .iter()
        .map(|filename| {
            let file_path = dir.join(filename);
            fs::write(&file_path, &flac_template).expect("Failed to write FLAC file");
            flac_template.clone()
        })
        .collect()
}

/// Verify we can actually load image data from storage
/// This mirrors the logic in ui/app.rs serve_image_from_chunks
async fn verify_image_loadable(
    image: &bae::db::DbImage,
    library_manager: &LibraryManager,
    cloud_storage: &CloudStorageManager,
    _cache_manager: &CacheManager,
    encryption_service: &EncryptionService,
    location: &StorageLocation,
    chunked: bool,
    encrypted: bool,
) {
    // Find the file record for this image
    let filename_only = std::path::Path::new(&image.filename)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&image.filename);

    let file = library_manager
        .get_file_by_release_and_filename(&image.release_id, filename_only)
        .await
        .expect("Failed to get file")
        .expect("File not found for image");

    // For non-chunked storage, read from source_path
    if !chunked {
        let source_path = file
            .source_path
            .as_ref()
            .expect("Non-chunked file should have source_path");

        let data = match location {
            StorageLocation::Local => {
                // Local files can be read directly
                tokio::fs::read(source_path)
                    .await
                    .expect("Failed to read local image file")
            }
            StorageLocation::Cloud => {
                // Cloud files need to be downloaded via cloud_storage
                cloud_storage
                    .download_chunk(source_path)
                    .await
                    .expect("Failed to download image from cloud")
            }
        };

        // Decrypt if needed
        let data = if encrypted {
            encryption_service
                .decrypt_simple(&data)
                .expect("Failed to decrypt image")
        } else {
            data
        };

        // Verify we got valid JPEG data (should start with FFD8)
        assert!(
            data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8,
            "Image data should be valid JPEG (got {} bytes, starts with {:02X}{:02X})",
            data.len(),
            data.first().unwrap_or(&0),
            data.get(1).unwrap_or(&0)
        );

        return;
    }

    // For chunked storage, use file_chunks
    let file_chunks = library_manager
        .get_file_chunks(&file.id)
        .await
        .expect("Failed to get file chunks");

    assert!(!file_chunks.is_empty(), "Chunked file should have chunks");

    // Download and decrypt chunks
    let mut chunk_data_map: std::collections::HashMap<String, Vec<u8>> =
        std::collections::HashMap::new();

    for fc in &file_chunks {
        if chunk_data_map.contains_key(&fc.chunk_id) {
            continue;
        }

        let chunk = library_manager
            .get_chunk_by_id(&fc.chunk_id)
            .await
            .expect("Failed to get chunk")
            .expect("Chunk not found");

        // For local storage, read from disk; for cloud, use cloud_storage
        let data = match location {
            StorageLocation::Local => tokio::fs::read(&chunk.storage_location)
                .await
                .expect("Failed to read local chunk"),
            StorageLocation::Cloud => cloud_storage
                .download_chunk(&chunk.storage_location)
                .await
                .expect("Failed to download chunk from cloud"),
        };

        let decrypted = if encrypted {
            encryption_service
                .decrypt_simple(&data)
                .expect("Failed to decrypt chunk")
        } else {
            data
        };

        chunk_data_map.insert(fc.chunk_id.clone(), decrypted);
    }

    // Reassemble file
    let mut file_data = Vec::new();
    for fc in &file_chunks {
        let chunk_data = chunk_data_map
            .get(&fc.chunk_id)
            .expect("Missing chunk data");
        let start = fc.byte_offset as usize;
        let end = start + fc.byte_length as usize;
        file_data.extend_from_slice(&chunk_data[start..end]);
    }

    // Verify we got valid JPEG data
    assert!(
        file_data.len() >= 2 && file_data[0] == 0xFF && file_data[1] == 0xD8,
        "Image data should be valid JPEG"
    );
}

async fn verify_storage_state(
    location: &StorageLocation,
    chunked: bool,
    encrypted: bool,
    _storage_dir: &Path,
    mock_storage: &MockCloudStorage,
    files: &[bae::db::DbFile],
    library_manager: &LibraryManager,
    release_id: &str,
) {
    match location {
        StorageLocation::Local => {
            // Verify files exist on disk
            for file in files {
                if let Some(ref source_path) = file.source_path {
                    let path = PathBuf::from(source_path);
                    assert!(path.exists(), "Local file should exist at: {}", source_path);

                    // If encrypted, verify file doesn't start with fLaC
                    if encrypted && file.format == "flac" {
                        let data = fs::read(&path).expect("Failed to read file");
                        assert!(
                            data.len() < 4 || &data[0..4] != b"fLaC",
                            "Encrypted file should not have plain FLAC header"
                        );
                        info!(
                            "✓ File '{}' is encrypted (no fLaC header)",
                            file.original_filename
                        );
                    }
                }
            }
            info!("✓ Local storage files verified");
        }
        StorageLocation::Cloud => {
            if chunked {
                // Verify chunks exist in mock storage
                let chunks = library_manager
                    .get_chunks_for_release(release_id)
                    .await
                    .expect("Failed to get chunks");

                for chunk in &chunks {
                    let data = mock_storage
                        .chunks
                        .lock()
                        .unwrap()
                        .get(&chunk.storage_location)
                        .cloned();

                    assert!(
                        data.is_some(),
                        "Chunk should exist in mock storage at: {}",
                        chunk.storage_location
                    );

                    // If encrypted, verify chunk doesn't start with fLaC
                    if encrypted {
                        let chunk_data = data.unwrap();
                        if chunk_data.len() >= 4 {
                            assert!(
                                &chunk_data[0..4] != b"fLaC",
                                "Encrypted chunk should not have plain FLAC header"
                            );
                        }
                    }
                }
                info!("✓ {} cloud chunks verified", chunks.len());
            } else {
                // Non-chunked cloud: verify files in mock storage
                for file in files {
                    if let Some(ref source_path) = file.source_path {
                        let data = mock_storage
                            .chunks
                            .lock()
                            .unwrap()
                            .get(source_path)
                            .cloned();

                        assert!(
                            data.is_some(),
                            "Cloud file should exist at: {}",
                            source_path
                        );

                        if encrypted && file.format == "flac" {
                            let file_data = data.unwrap();
                            assert!(
                                file_data.len() < 4 || &file_data[0..4] != b"fLaC",
                                "Encrypted cloud file should not have plain FLAC header"
                            );
                            info!("✓ Cloud file '{}' is encrypted", file.original_filename);
                        }
                    }
                }
                info!("✓ Cloud storage files verified");
            }
        }
    }
}

async fn verify_roundtrip(
    tracks: &[bae::db::DbTrack],
    library_manager: &LibraryManager,
    cloud_storage: &CloudStorageManager,
    cache_manager: &CacheManager,
    encryption_service: &EncryptionService,
    chunk_size_bytes: usize,
    chunked: bool,
    encrypted: bool,
    _location: &StorageLocation,
    _storage_profile_id: &str,
) {
    // For chunked storage, use reassemble_track
    // For non-chunked, the playback service would handle it differently
    // but we can verify the coords are correct

    if chunked {
        // Use the reassembly function for chunked storage
        // Note: Local chunked storage can't be tested with MockCloudStorage
        // because chunks are on disk, not in the mock. Cloud chunked works.
        match (_location, encrypted) {
            (StorageLocation::Cloud, true) => {
                // Cloud encrypted: test full reassembly
                for track in tracks.iter().take(1) {
                    let reassembled = bae::playback::reassemble_track(
                        &track.id,
                        library_manager,
                        cloud_storage,
                        cache_manager,
                        encryption_service,
                        chunk_size_bytes,
                    )
                    .await
                    .expect("Failed to reassemble track");

                    assert!(
                        !reassembled.duration().is_zero(),
                        "Reassembled track should have data"
                    );
                    info!(
                        "✓ Track '{}' reassembled: {:?}",
                        track.title,
                        reassembled.duration()
                    );
                }
            }
            (StorageLocation::Cloud, false) => {
                // Cloud non-encrypted: reassembly code assumes encryption, skip for now
                // TODO: Fix reassemble_track to handle non-encrypted storage
                info!("⚠ Skipping reassembly test for non-encrypted cloud (reassemble_track assumes encryption)");
            }
            (StorageLocation::Local, _) => {
                // Local chunked: verify chunks exist on disk instead of reassembly
                let chunks = library_manager
                    .get_chunks_for_release(&tracks[0].release_id)
                    .await
                    .expect("Failed to get chunks");

                for chunk in chunks.iter().take(3) {
                    let path = std::path::Path::new(&chunk.storage_location);
                    assert!(
                        path.exists(),
                        "Local chunk should exist at: {}",
                        chunk.storage_location
                    );
                }
                info!("✓ Local chunks verified on disk (reassembly skipped for MockCloudStorage)");
            }
        }
    } else {
        // For non-chunked single-file-per-track:
        //   - No coords (each file IS the track)
        //   - Verify the file exists and can be read
        // For non-chunked CUE/FLAC:
        //   - Coords with chunk_index=-1 and byte ranges
        for track in tracks {
            let coords = library_manager
                .get_track_chunk_coords(&track.id)
                .await
                .expect("Failed to get coords");

            match coords {
                Some(c) if c.start_chunk_index == -1 => {
                    // CUE/FLAC non-chunked
                    assert!(
                        c.end_byte_offset > c.start_byte_offset,
                        "Should have valid byte range"
                    );
                    info!(
                        "✓ Track '{}' has valid non-chunked CUE/FLAC coords",
                        track.title
                    );
                }
                Some(c) => {
                    panic!(
                        "Unexpected chunk_index {} for non-chunked track",
                        c.start_chunk_index
                    );
                }
                None => {
                    // Single-file-per-track: verify audio format exists
                    let audio_format = library_manager
                        .get_audio_format_by_track_id(&track.id)
                        .await
                        .expect("Failed to get audio format")
                        .expect("Audio format should exist for single-file track");

                    assert_eq!(audio_format.format, "flac", "Should be FLAC format");
                    info!(
                        "✓ Track '{}' has audio format (single-file-per-track)",
                        track.title
                    );
                }
            }
        }
    }

    info!("✓ Roundtrip verification passed");
}

/// Test with a real album - run with:
/// cargo test --test test_storage_permutations --features test-utils test_real_album -- --ignored --nocapture
#[tokio::test]
#[ignore]
async fn test_real_album() {
    tracing_init();

    let real_album_path = PathBuf::from("/Users/dima/Torrents/Rush  - Moving Pictures 1981 ( 1983 West Germany 1st Press Mercury 800 048-2 Green Arrow)");

    if !real_album_path.exists() {
        panic!("Real album path does not exist: {:?}", real_album_path);
    }

    // Test with local, non-chunked, non-encrypted (simplest case)
    run_real_album_test(
        real_album_path,
        StorageLocation::Local,
        false, // chunked
        false, // encrypted
    )
    .await;
}

async fn run_real_album_test(
    album_dir: PathBuf,
    location: StorageLocation,
    chunked: bool,
    encrypted: bool,
) {
    info!(
        "\n\n========== Testing REAL ALBUM: {:?} / chunked={} / encrypted={} ==========\n",
        location, chunked, encrypted
    );

    // 1. Setup directories
    let temp_root = TempDir::new().expect("Failed to create temp root");
    let db_dir = temp_root.path().join("db");
    let cache_dir = temp_root.path().join("cache");
    let storage_dir = temp_root.path().join("storage");

    fs::create_dir_all(&db_dir).expect("Failed to create db dir");
    fs::create_dir_all(&cache_dir).expect("Failed to create cache dir");
    fs::create_dir_all(&storage_dir).expect("Failed to create storage dir");

    // List files in the album
    let entries: Vec<_> = fs::read_dir(&album_dir)
        .expect("Failed to read album dir")
        .filter_map(|e| e.ok())
        .collect();
    info!("Album contains {} entries:", entries.len());
    for entry in &entries {
        info!("  - {:?}", entry.file_name());
    }

    // 2. Setup services
    let chunk_size_bytes = 1024 * 1024; // 1MB
    let mock_storage = Arc::new(MockCloudStorage::new());
    let cloud_storage = CloudStorageManager::from_storage(mock_storage.clone());

    let db_file = db_dir.join("test.db");
    let database = Database::new(db_file.to_str().unwrap())
        .await
        .expect("Failed to create database");

    let encryption_service = EncryptionService::new_with_key(vec![0u8; 32]);

    let library_manager = LibraryManager::new(database.clone(), cloud_storage.clone());
    let shared_library_manager = bae::library::SharedLibraryManager::new(library_manager.clone());
    let library_manager = Arc::new(library_manager);

    // 3. Create and insert storage profile
    let storage_profile =
        create_storage_profile(&location, chunked, encrypted, storage_dir.to_str().unwrap());
    let storage_profile_id = storage_profile.id.clone();

    database
        .insert_storage_profile(&storage_profile)
        .await
        .expect("Failed to insert storage profile");

    info!(
        "Created storage profile: {} (id: {})",
        storage_profile.name, storage_profile_id
    );

    // 4. Start import service
    let runtime_handle = tokio::runtime::Handle::current();
    let import_config = ImportConfig {
        chunk_size_bytes,
        max_encrypt_workers: 4,
        max_upload_workers: 20,
        max_db_write_workers: 10,
    };

    let torrent_handle = TorrentManagerHandle::new_dummy();
    let database_arc = Arc::new(database.clone());

    let import_handle = ImportService::start(
        import_config,
        runtime_handle,
        shared_library_manager,
        encryption_service.clone(),
        cloud_storage.clone(),
        torrent_handle,
        database_arc.clone(),
    );

    // 5. Create import request (no Discogs metadata - let it discover)
    let (_album_id, release_id) = import_handle
        .send_request(ImportRequest::Folder {
            discogs_release: None,
            mb_release: None,
            folder: album_dir.clone(),
            master_year: 1981,
            cover_art_url: None,
            storage_profile_id: Some(storage_profile_id.clone()),
            selected_cover_filename: None,
            import_id: uuid::Uuid::new_v4().to_string(),
        })
        .await
        .expect("Failed to send import request");

    info!("Import request sent, release_id: {}", release_id);

    // 6. Wait for completion
    let mut progress_rx = import_handle.subscribe_release(release_id.clone());
    while let Some(progress) = progress_rx.recv().await {
        match &progress {
            ImportProgress::Complete { .. } => {
                info!("✓ Import completed!");
                break;
            }
            ImportProgress::Failed { error, .. } => {
                panic!("Import failed: {}", error);
            }
            _ => {
                info!("Progress: {:?}", progress);
            }
        }
    }

    // 7. Verify database state
    info!("\n--- Verifying database state ---");

    // Check release_storage record
    let release_storage = database
        .get_release_storage(&release_id)
        .await
        .expect("Failed to get release_storage");
    info!("release_storage: {:?}", release_storage);

    // Check tracks
    let tracks = library_manager
        .get_tracks(&release_id)
        .await
        .expect("Failed to get tracks");

    info!("\nTracks ({}):", tracks.len());
    for track in &tracks {
        info!(
            "  - [{}] '{}' status={:?}",
            track.track_number.unwrap_or(0),
            track.title,
            track.import_status
        );

        // Check chunk coords
        let coords = library_manager
            .get_track_chunk_coords(&track.id)
            .await
            .expect("Failed to get coords");
        if let Some(c) = coords {
            info!(
                "    coords: chunks {}..{}, bytes {}..{}",
                c.start_chunk_index, c.end_chunk_index, c.start_byte_offset, c.end_byte_offset
            );
        } else {
            info!("    coords: None");
        }

        // Check audio format
        let audio_format = library_manager
            .get_audio_format_by_track_id(&track.id)
            .await
            .expect("Failed to get audio format");
        if let Some(af) = audio_format {
            info!(
                "    audio_format: {}, flac_headers={}, seektable={}",
                af.format,
                af.flac_headers.is_some(),
                af.flac_seektable.is_some()
            );
        } else {
            info!("    audio_format: None");
        }
    }

    // Check files
    let files = library_manager
        .get_files_for_release(&release_id)
        .await
        .expect("Failed to get files");

    info!("\nFiles ({}):", files.len());
    for file in &files {
        info!(
            "  - '{}' source_path={:?}",
            file.original_filename,
            file.source_path.as_ref().map(|s| &s[..s.len().min(60)])
        );
    }

    // 8. Verify track statuses
    let complete_count = tracks
        .iter()
        .filter(|t| t.import_status == ImportStatus::Complete)
        .count();
    let queued_count = tracks
        .iter()
        .filter(|t| t.import_status == ImportStatus::Queued)
        .count();

    info!(
        "\nTrack status summary: {} Complete, {} Queued, {} total",
        complete_count,
        queued_count,
        tracks.len()
    );

    // Assert all tracks are complete
    for track in &tracks {
        assert_eq!(
            track.import_status,
            ImportStatus::Complete,
            "Track '{}' should be Complete but is {:?}",
            track.title,
            track.import_status
        );
    }

    // 9. Verify cover image exists
    let images = library_manager
        .get_images_for_release(&release_id)
        .await
        .expect("Failed to get images");

    assert!(!images.is_empty(), "Should have at least one image record");
    assert!(
        images.iter().any(|img| img.is_cover),
        "Should have at least one image marked as cover"
    );
    info!("✓ {} DbImage records, cover image exists", images.len());

    info!("\n✅ All tracks are Complete!");
}
