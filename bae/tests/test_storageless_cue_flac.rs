#![cfg(feature = "test-utils")]

//! Tests for storage-less CUE/FLAC imports.
//!
//! When importing a CUE/FLAC album without a storage profile, the files stay in place.
//! The import must still record track positions so playback can seek to the correct
//! position within the single FLAC file for each track.

mod support;

use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;
use tracing::info;

use bae::cache::{CacheConfig, CacheManager};
use bae::cloud_storage::CloudStorageManager;
use bae::db::{Database, ImportStatus};
use bae::discogs::models::{DiscogsRelease, DiscogsTrack};
use bae::encryption::EncryptionService;
use bae::import::{ImportConfig, ImportProgress, ImportRequest, ImportService};
use bae::library::{LibraryManager, SharedLibraryManager};
use bae::test_support::MockCloudStorage;
use bae::torrent::TorrentManagerHandle;

use crate::support::tracing_init;

/// Test that storage-less CUE/FLAC imports record track positions correctly.
///
/// This is a regression test for the bug where:
/// 1. CUE/FLAC imports with no storage profile completed successfully
/// 2. But track positions were not recorded in the database
/// 3. So playback of any track except the first would fail or play wrong audio
#[tokio::test]
async fn test_storageless_cue_flac_records_track_positions() {
    tracing_init();

    // Setup directories
    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    let cache_dir = temp_root.path().join("cache");

    std::fs::create_dir_all(&album_dir).expect("album dir");
    std::fs::create_dir_all(&db_dir).expect("db dir");
    std::fs::create_dir_all(&cache_dir).expect("cache dir");

    // Generate CUE/FLAC test files
    generate_cue_flac_files(&album_dir);

    // Setup services
    let chunk_size_bytes = 1024 * 1024;
    let mock_storage = Arc::new(MockCloudStorage::new());
    let cloud_storage = CloudStorageManager::from_storage(mock_storage);

    let db_file = db_dir.join("test.db");
    let database = Database::new(db_file.to_str().unwrap())
        .await
        .expect("database");

    let encryption_service = EncryptionService::new_with_key(vec![0u8; 32]);

    let cache_config = CacheConfig {
        cache_dir,
        max_size_bytes: 1024 * 1024 * 1024,
        max_chunks: 10000,
    };
    let cache_manager = CacheManager::with_config(cache_config)
        .await
        .expect("cache");

    let library_manager = LibraryManager::new(database.clone(), cloud_storage.clone());
    let shared_library_manager = SharedLibraryManager::new(library_manager.clone());
    let library_manager = Arc::new(library_manager);

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
        encryption_service,
        cloud_storage,
        cache_manager,
        torrent_handle,
        database_arc,
    );

    // Create Discogs release matching our CUE file
    let discogs_release = create_test_discogs_release();

    // Send import request WITHOUT storage profile (storage-less)
    let import_id = uuid::Uuid::new_v4().to_string();
    let (_album_id, release_id) = import_handle
        .send_request(ImportRequest::Folder {
            import_id,
            discogs_release: Some(discogs_release),
            mb_release: None,
            folder: album_dir,
            master_year: 2024,
            cover_art_url: None,
            storage_profile_id: None, // <-- No storage profile!
            selected_cover_filename: None,
        })
        .await
        .expect("send request");

    info!("Import request sent, release_id: {}", release_id);

    // Wait for completion
    let mut progress_rx = import_handle.subscribe_release(release_id.clone());
    while let Some(progress) = progress_rx.recv().await {
        match &progress {
            ImportProgress::Complete {
                release_id: rid, ..
            } if rid.is_none() => {
                info!("Import completed");
                break;
            }
            ImportProgress::Failed { error, .. } => {
                panic!("Import failed: {}", error);
            }
            _ => {}
        }
    }

    // Verify tracks are complete
    let tracks = library_manager
        .get_tracks(&release_id)
        .await
        .expect("get tracks");

    assert_eq!(tracks.len(), 3, "Should have 3 tracks");
    for track in &tracks {
        assert_eq!(
            track.import_status,
            ImportStatus::Complete,
            "Track '{}' should be Complete",
            track.title
        );
    }

    // THE KEY ASSERTION: Track positions must be recorded for CUE/FLAC
    // Without this, playback of tracks 2+ will fail or play wrong audio
    for (i, track) in tracks.iter().enumerate() {
        let coords = library_manager
            .get_track_chunk_coords(&track.id)
            .await
            .expect("get coords");

        // For storage-less CUE/FLAC, coords should exist with chunk_index=-1
        let coords = coords.unwrap_or_else(|| {
            panic!(
                "Track {} '{}' should have chunk coords recorded",
                i + 1,
                track.title
            )
        });

        assert_eq!(
            coords.start_chunk_index, -1,
            "Storage-less track should have chunk_index=-1 (non-chunked sentinel)"
        );

        // Verify time ranges are recorded (these come from the CUE sheet)
        // Note: end_time_ms may be 0 for the last track (end of file)
        assert!(
            coords.start_time_ms >= 0,
            "Track {} should have valid start_time_ms",
            i + 1
        );

        // Later tracks should start at or after previous tracks (by time)
        if i > 0 {
            let prev_coords = library_manager
                .get_track_chunk_coords(&tracks[i - 1].id)
                .await
                .expect("get prev coords")
                .expect("prev coords exist");

            assert!(
                coords.start_time_ms >= prev_coords.start_time_ms,
                "Track {} start_time ({}) should be >= track {} start_time ({})",
                i + 1,
                coords.start_time_ms,
                i,
                prev_coords.start_time_ms
            );
        }

        info!(
            "Track {} '{}': bytes {}..{}, time {}..{}ms",
            i + 1,
            track.title,
            coords.start_byte_offset,
            coords.end_byte_offset,
            coords.start_time_ms,
            coords.end_time_ms
        );
    }

    // Also verify audio format is recorded (needed for playback)
    for track in &tracks {
        let audio_format = library_manager
            .get_audio_format_by_track_id(&track.id)
            .await
            .expect("get audio format");

        assert!(
            audio_format.is_some(),
            "Track '{}' should have audio format recorded",
            track.title
        );

        let af = audio_format.unwrap();
        assert_eq!(af.format, "flac", "Should be FLAC format");
        assert!(
            af.flac_headers.is_some(),
            "Should have FLAC headers for seeking"
        );
    }

    info!("✅ All track positions recorded correctly for storage-less CUE/FLAC import");
}

fn create_test_discogs_release() -> DiscogsRelease {
    DiscogsRelease {
        id: "test-storageless-cue-flac".to_string(),
        title: "Test Album".to_string(),
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
        master_id: "test-master".to_string(),
    }
}

/// Generate a minimal CUE/FLAC test album.
///
/// Creates:
/// - Test Album.flac: A valid FLAC file (we use a pre-generated fixture)
/// - Test Album.cue: CUE sheet pointing to the FLAC with 3 tracks
fn generate_cue_flac_files(dir: &Path) {
    use std::fs;

    // Copy the real FLAC fixtures and concatenate them to simulate a single album file
    let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flac");

    let track1_data = fs::read(fixture_dir.join("01 Test Track 1.flac"))
        .expect("Failed to read fixture 01 - run scripts/generate_test_flac.sh");

    // For a real CUE/FLAC, we'd need a longer FLAC file with audio at all CUE positions.
    // For this test, we just use one fixture file as the "album" FLAC.
    // The CUE sheet timings extend beyond the file length, but that's fine for testing
    // that the import code records track positions (byte ranges will clamp to file size).
    fs::write(dir.join("Test Album.flac"), &track1_data).expect("write flac");

    // Create CUE sheet
    let cue_content = r#"REM GENRE "Test"
REM DATE 2024
PERFORMER "Test Artist"
TITLE "Test Album"
FILE "Test Album.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One"
    PERFORMER "Test Artist"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two"
    PERFORMER "Test Artist"
    INDEX 01 00:05:00
  TRACK 03 AUDIO
    TITLE "Track Three"
    PERFORMER "Test Artist"
    INDEX 01 00:09:00
"#;

    fs::write(dir.join("Test Album.cue"), cue_content).expect("write cue");

    info!(
        "Generated CUE/FLAC test files: {} bytes FLAC, {} bytes CUE",
        track1_data.len(),
        cue_content.len()
    );
}
