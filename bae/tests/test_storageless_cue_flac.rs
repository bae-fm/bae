#![cfg(feature = "test-utils")]
//! Tests for storage-less CUE/FLAC imports.
//!
//! When importing a CUE/FLAC album without a storage profile, the files stay in place.
//! The import must still record track positions so playback can seek to the correct
//! position within the single FLAC file for each track.
mod support;
use crate::support::tracing_init;
use bae::db::{Database, ImportStatus};
use bae::discogs::models::{DiscogsRelease, DiscogsTrack};
use bae::encryption::EncryptionService;
use bae::import::{ImportConfig, ImportProgress, ImportRequest, ImportService};
use bae::library::{LibraryManager, SharedLibraryManager};
use bae::torrent::TorrentManagerHandle;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;
use tracing::info;
/// Test that storage-less CUE/FLAC imports record track positions correctly.
///
/// This is a regression test for the bug where:
/// 1. CUE/FLAC imports with no storage profile completed successfully
/// 2. But track positions were not recorded in the database
/// 3. So playback of any track except the first would fail or play wrong audio
#[tokio::test]
async fn test_storageless_cue_flac_records_track_positions() {
    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    std::fs::create_dir_all(&album_dir).expect("album dir");
    std::fs::create_dir_all(&db_dir).expect("db dir");
    generate_cue_flac_files(&album_dir);
    let chunk_size_bytes = 1024 * 1024;
    let db_file = db_dir.join("test.db");
    let database = Database::new(db_file.to_str().unwrap())
        .await
        .expect("database");
    let encryption_service = EncryptionService::new_with_key(vec![0u8; 32]);
    let library_manager = LibraryManager::new(database.clone());
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
        torrent_handle,
        database_arc,
    );
    let discogs_release = create_test_discogs_release();
    let import_id = uuid::Uuid::new_v4().to_string();
    let (_album_id, release_id) = import_handle
        .send_request(ImportRequest::Folder {
            import_id,
            discogs_release: Some(discogs_release),
            mb_release: None,
            folder: album_dir,
            master_year: 2024,
            cover_art_url: None,
            storage_profile_id: None,
            selected_cover_filename: None,
        })
        .await
        .expect("send request");
    info!("Import request sent, release_id: {}", release_id);
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
            track.title,
        );
    }
    for (i, track) in tracks.iter().enumerate() {
        let coords = library_manager
            .get_track_chunk_coords(&track.id)
            .await
            .expect("get coords");
        let coords = coords.unwrap_or_else(|| {
            panic!(
                "Track {} '{}' should have chunk coords recorded",
                i + 1,
                track.title,
            )
        });
        assert_eq!(
            coords.start_chunk_index, -1,
            "Storage-less track should have chunk_index=-1 (non-chunked sentinel)",
        );
        assert!(
            coords.start_time_ms >= 0,
            "Track {} should have valid start_time_ms",
            i + 1,
        );
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
                prev_coords.start_time_ms,
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
    for track in &tracks {
        let audio_format = library_manager
            .get_audio_format_by_track_id(&track.id)
            .await
            .expect("get audio format");
        assert!(
            audio_format.is_some(),
            "Track '{}' should have audio format recorded",
            track.title,
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
/// Test that playback of track 2 loads audio from the correct byte range.
///
/// This is a regression test for the bug where:
/// 1. CUE/FLAC imports with no storage profile recorded track positions correctly
/// 2. But playback ignored the positions and always played from the beginning
/// 3. So all tracks sounded like track 1
///
/// The test verifies that load_audio_from_source_path uses DbTrackChunkCoords
/// to extract the correct byte range for CUE/FLAC tracks.
#[tokio::test]
async fn test_storageless_cue_flac_playback_uses_track_positions() {
    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    let cache_dir = temp_root.path().join("cache");
    std::fs::create_dir_all(&album_dir).expect("album dir");
    std::fs::create_dir_all(&db_dir).expect("db dir");
    std::fs::create_dir_all(&cache_dir).expect("cache dir");
    generate_cue_flac_files_with_two_tracks(&album_dir);
    let chunk_size_bytes = 1024 * 1024;
    let db_file = db_dir.join("test.db");
    let database = Database::new(db_file.to_str().unwrap())
        .await
        .expect("database");
    let encryption_service = EncryptionService::new_with_key(vec![0u8; 32]);
    let cache_config = bae::cache::CacheConfig {
        cache_dir,
        max_size_bytes: 1024 * 1024 * 1024,
        max_chunks: 10000,
    };
    let cache_manager = bae::cache::CacheManager::with_config(cache_config)
        .await
        .expect("cache");
    let library_manager = LibraryManager::new(database.clone());
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
        runtime_handle.clone(),
        shared_library_manager,
        encryption_service.clone(),
        torrent_handle,
        database_arc,
    );
    let discogs_release = create_two_track_discogs_release();
    let import_id = uuid::Uuid::new_v4().to_string();
    let (_album_id, release_id) = import_handle
        .send_request(ImportRequest::Folder {
            import_id,
            discogs_release: Some(discogs_release),
            mb_release: None,
            folder: album_dir,
            master_year: 2024,
            cover_art_url: None,
            storage_profile_id: None,
            selected_cover_filename: None,
        })
        .await
        .expect("send request");
    let mut progress_rx = import_handle.subscribe_release(release_id.clone());
    while let Some(progress) = progress_rx.recv().await {
        match &progress {
            ImportProgress::Complete {
                release_id: rid, ..
            } if rid.is_none() => break,
            ImportProgress::Failed { error, .. } => panic!("Import failed: {}", error),
            _ => {}
        }
    }
    let tracks = library_manager
        .get_tracks(&release_id)
        .await
        .expect("get tracks");
    assert_eq!(tracks.len(), 2, "Should have exactly 2 tracks");
    let track1 = &tracks[0];
    let track2 = &tracks[1];
    let track1_coords = library_manager
        .get_track_chunk_coords(&track1.id)
        .await
        .expect("get track1 coords")
        .expect("track1 should have coords");
    let track2_coords = library_manager
        .get_track_chunk_coords(&track2.id)
        .await
        .expect("get track2 coords")
        .expect("track2 should have coords");
    info!(
        "Track 1 '{}': bytes {}..{}, time {}..{}ms",
        track1.title,
        track1_coords.start_byte_offset,
        track1_coords.end_byte_offset,
        track1_coords.start_time_ms,
        track1_coords.end_time_ms
    );
    info!(
        "Track 2 '{}': bytes {}..{}, time {}..{}ms",
        track2.title,
        track2_coords.start_byte_offset,
        track2_coords.end_byte_offset,
        track2_coords.start_time_ms,
        track2_coords.end_time_ms
    );
    assert!(
        track2_coords.end_byte_offset > track2_coords.start_byte_offset,
        "Track 2 must have a valid (non-empty) byte range. \
         Got {}..{}. The test FLAC file may be too short.",
        track2_coords.start_byte_offset,
        track2_coords.end_byte_offset,
    );
    assert!(
        track2_coords.start_byte_offset > track1_coords.start_byte_offset,
        "Track 2 should start after track 1",
    );
    let track2_format = library_manager
        .get_audio_format_by_track_id(&track2.id)
        .await
        .expect("get track2 format")
        .expect("track2 should have audio format");
    assert!(
        track2_format.flac_headers.is_some(),
        "Track 2 must have FLAC headers stored for byte-range playback",
    );
    std::env::set_var("MUTE_TEST_AUDIO", "1");
    let playback_handle = bae::playback::PlaybackService::start(
        library_manager.as_ref().clone(),
        cache_manager,
        encryption_service,
        chunk_size_bytes,
        runtime_handle,
    );
    playback_handle.set_volume(0.0);
    let mut progress_rx = playback_handle.subscribe_progress();
    playback_handle.play(track2.id.clone());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut track2_decoded_duration = None;
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(std::time::Duration::from_millis(100), progress_rx.recv()).await
        {
            Ok(Some(bae::playback::PlaybackProgress::StateChanged { state })) => {
                if let bae::playback::PlaybackState::Playing {
                    track,
                    decoded_duration,
                    ..
                } = &state
                {
                    if track.id == track2.id {
                        track2_decoded_duration = Some(*decoded_duration);
                        break;
                    }
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    let track2_decoded_duration =
        track2_decoded_duration.expect("Should get track 2 decoded_duration from playback");
    let track2_decoded_duration_ms = track2_decoded_duration.as_millis() as i64;
    info!(
        "Track 2 decoded_duration: {}ms (actual audio length)",
        track2_decoded_duration_ms
    );
    let full_album_duration_ms = 5000;
    assert!(
        track2_decoded_duration_ms < full_album_duration_ms - 500,
        "BUG: Track 2 decoded_duration is {}ms, which is close to the full album ({}ms). \
         This means load_audio_from_source_path is NOT using track coords. \
         It's reading the whole FLAC file instead of extracting track 2's byte range.",
        track2_decoded_duration_ms,
        full_album_duration_ms,
    );
    info!("✅ Storage-less CUE/FLAC playback correctly uses track positions");
}
fn create_two_track_discogs_release() -> DiscogsRelease {
    DiscogsRelease {
        id: "test-two-track".to_string(),
        title: "Two Track Test".to_string(),
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
                title: "First Half".to_string(),
                duration: Some("0:02".to_string()),
            },
            DiscogsTrack {
                position: "2".to_string(),
                title: "Second Half".to_string(),
                duration: Some("0:03".to_string()),
            },
        ],
        master_id: "test-master-two".to_string(),
    }
}
/// Generate CUE/FLAC with 2 tracks that fit within our ~5 second test file.
fn generate_cue_flac_files_with_two_tracks(dir: &Path) {
    use std::fs;
    let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flac");
    let flac_data = fs::read(fixture_dir.join("01 Test Track 1.flac"))
        .expect("Failed to read fixture - run scripts/generate_test_flac.sh");
    fs::write(dir.join("Two Track Test.flac"), &flac_data).expect("write flac");
    let cue_content = r#"REM GENRE "Test"
REM DATE 2024
PERFORMER "Test Artist"
TITLE "Two Track Test"
FILE "Two Track Test.flac" WAVE
  TRACK 01 AUDIO
    TITLE "First Half"
    PERFORMER "Test Artist"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Second Half"
    PERFORMER "Test Artist"
    INDEX 01 00:02:00
"#;
    fs::write(dir.join("Two Track Test.cue"), cue_content).expect("write cue");
    info!(
        "Generated 2-track CUE/FLAC: {} bytes FLAC, track 2 starts at 2 seconds",
        flac_data.len()
    );
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
    let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flac");
    let track1_data = fs::read(fixture_dir.join("01 Test Track 1.flac"))
        .expect("Failed to read fixture 01 - run scripts/generate_test_flac.sh");
    fs::write(dir.join("Test Album.flac"), &track1_data).expect("write flac");
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
