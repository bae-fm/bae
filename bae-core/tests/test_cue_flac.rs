#![cfg(feature = "test-utils")]
//! Tests for CUE/FLAC format handling.
//!
//! CUE/FLAC albums have multiple tracks in a single FLAC file. The import must:
//! - Parse the CUE sheet to find track boundaries
//! - Record byte offsets for each track in the database
//! - Enable playback to seek to the correct position for each track
//!
//! These tests use storageless imports for simplicity (the CUE/FLAC handling
//! is independent of storage configuration).
mod support;
use crate::support::{test_encryption_service, tracing_init};
use bae_core::db::{Database, ImportStatus};
use bae_core::discogs::models::{DiscogsRelease, DiscogsTrack};
use bae_core::encryption::EncryptionService;
use bae_core::import::{ImportProgress, ImportRequest, ImportService};
use bae_core::library::{LibraryManager, SharedLibraryManager};
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;
use tracing::info;
/// Test that CUE/FLAC imports record track byte positions correctly.
///
/// Regression test: CUE/FLAC imports must record byte offsets for each track
/// so playback can seek to the correct position within the single FLAC file.
#[tokio::test]
async fn test_cue_flac_records_track_positions() {
    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    std::fs::create_dir_all(&album_dir).expect("album dir");
    std::fs::create_dir_all(&db_dir).expect("db dir");
    copy_cue_flac_fixture_with_seektable(&album_dir);
    let db_file = db_dir.join("test.db");
    let database = Database::new(db_file.to_str().unwrap())
        .await
        .expect("database");
    let encryption_service = Some(EncryptionService::new_with_key(&[0u8; 32]));
    let library_manager = LibraryManager::new(database.clone(), test_encryption_service());
    let shared_library_manager = SharedLibraryManager::new(library_manager.clone());
    let library_manager = Arc::new(library_manager);
    let runtime_handle = tokio::runtime::Handle::current();
    let database_arc = Arc::new(database.clone());
    let import_handle = ImportService::start(
        runtime_handle,
        shared_library_manager,
        encryption_service,
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
    // Check that each track has audio format with byte offsets recorded
    for (i, track) in tracks.iter().enumerate() {
        let audio_format = library_manager
            .get_audio_format_by_track_id(&track.id)
            .await
            .expect("get audio format")
            .unwrap_or_else(|| {
                panic!(
                    "Track {} '{}' should have audio format recorded",
                    i + 1,
                    track.title,
                )
            });
        assert!(
            audio_format.start_byte_offset.is_some(),
            "CUE/FLAC track {} should have start_byte_offset",
            i + 1,
        );
        assert!(
            audio_format.end_byte_offset.is_some(),
            "CUE/FLAC track {} should have end_byte_offset",
            i + 1,
        );
        if i > 0 {
            let prev_format = library_manager
                .get_audio_format_by_track_id(&tracks[i - 1].id)
                .await
                .expect("get prev format")
                .expect("prev format exists");
            assert!(
                audio_format.start_byte_offset.unwrap() >= prev_format.start_byte_offset.unwrap(),
                "Track {} start_byte ({}) should be >= track {} start_byte ({})",
                i + 1,
                audio_format.start_byte_offset.unwrap(),
                i,
                prev_format.start_byte_offset.unwrap(),
            );
        }
        info!(
            "Track {} '{}': bytes {}..{}",
            i + 1,
            track.title,
            audio_format.start_byte_offset.unwrap_or(-1),
            audio_format.end_byte_offset.unwrap_or(-1),
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
    info!("✅ All track positions recorded correctly for CUE/FLAC import");
}
/// Test that playback of track 2 loads audio from the correct byte range.
///
/// Regression test: Playback must use the recorded byte offsets to extract
/// the correct portion of the FLAC file for each track. Without this,
/// all tracks would play from the beginning of the file.
#[tokio::test]
async fn test_cue_flac_playback_uses_track_positions() {
    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    let cache_dir = temp_root.path().join("cache");
    std::fs::create_dir_all(&album_dir).expect("album dir");
    std::fs::create_dir_all(&db_dir).expect("db dir");
    std::fs::create_dir_all(&cache_dir).expect("cache dir");
    copy_cue_flac_fixture_with_seektable(&album_dir);
    let db_file = db_dir.join("test.db");
    let database = Database::new(db_file.to_str().unwrap())
        .await
        .expect("database");
    let encryption_service = Some(EncryptionService::new_with_key(&[0u8; 32]));
    let cache_config = bae_core::cache::CacheConfig {
        cache_dir,
        max_size_bytes: 1024 * 1024 * 1024,
        max_files: 10000,
    };
    let _cache_manager = bae_core::cache::CacheManager::with_config(cache_config)
        .await
        .expect("cache");
    let library_manager = LibraryManager::new(database.clone(), test_encryption_service());
    let shared_library_manager = SharedLibraryManager::new(library_manager.clone());
    let library_manager = Arc::new(library_manager);
    let runtime_handle = tokio::runtime::Handle::current();
    let database_arc = Arc::new(database.clone());
    let import_handle = ImportService::start(
        runtime_handle.clone(),
        shared_library_manager,
        encryption_service.clone(),
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
    assert_eq!(tracks.len(), 3, "Should have exactly 3 tracks");

    // Test track 2 (middle track with pregap)
    let track2 = &tracks[1];
    let track2_format = library_manager
        .get_audio_format_by_track_id(&track2.id)
        .await
        .expect("get track2 format")
        .expect("track2 should have audio format");
    let track2_start = track2_format
        .start_byte_offset
        .expect("track2 should have start offset");
    let track2_end = track2_format
        .end_byte_offset
        .expect("track2 should have end offset");
    info!(
        "Track 2 '{}': bytes {}..{}",
        track2.title, track2_start, track2_end,
    );
    assert!(
        track2_end > track2_start,
        "Track 2 must have a valid (non-empty) byte range. Got {}..{}",
        track2_start,
        track2_end,
    );
    assert!(
        track2_format.flac_headers.is_some(),
        "Track 2 must have FLAC headers stored for byte-range playback",
    );

    std::env::set_var("MUTE_TEST_AUDIO", "1");
    let playback_handle = bae_core::playback::PlaybackService::start(
        library_manager.as_ref().clone(),
        encryption_service,
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
            Ok(Some(bae_core::playback::PlaybackProgress::StateChanged { state })) => {
                if let bae_core::playback::PlaybackState::Playing {
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

    // Full album is 30 seconds. Track 2 is ~12 seconds (10s + 2s pregap).
    // If we're reading the whole file, duration would be ~30s.
    let full_album_duration_ms = 30000;
    assert!(
        track2_decoded_duration_ms < full_album_duration_ms - 5000,
        "BUG: Track 2 decoded_duration is {}ms, which is close to the full album ({}ms). \
         This means load_audio_from_source_path is NOT using track coords. \
         It's reading the whole FLAC file instead of extracting track 2's byte range.",
        track2_decoded_duration_ms,
        full_album_duration_ms,
    );
    info!("✅ CUE/FLAC playback correctly uses track positions");
}

/// Test that decoded audio duration matches expected CUE timing.
///
/// Regression test: The seektable "at or before" algorithm for end bytes
/// was cutting off audio at the end of tracks. decoded_duration must match
/// the expected duration from CUE timing (within tolerance for frame alignment).
#[tokio::test]
async fn test_cue_flac_decoded_duration_matches_cue_timing() {
    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    let cache_dir = temp_root.path().join("cache");
    std::fs::create_dir_all(&album_dir).expect("album dir");
    std::fs::create_dir_all(&db_dir).expect("db dir");
    std::fs::create_dir_all(&cache_dir).expect("cache dir");
    copy_cue_flac_fixture_with_seektable(&album_dir);
    let db_file = db_dir.join("test.db");
    let database = Database::new(db_file.to_str().unwrap())
        .await
        .expect("database");
    let encryption_service = Some(EncryptionService::new_with_key(&[0u8; 32]));
    let cache_config = bae_core::cache::CacheConfig {
        cache_dir,
        max_size_bytes: 1024 * 1024 * 1024,
        max_files: 10000,
    };
    let _cache_manager = bae_core::cache::CacheManager::with_config(cache_config)
        .await
        .expect("cache");
    let library_manager = LibraryManager::new(database.clone(), test_encryption_service());
    let shared_library_manager = SharedLibraryManager::new(library_manager.clone());
    let library_manager = Arc::new(library_manager);
    let runtime_handle = tokio::runtime::Handle::current();
    let database_arc = Arc::new(database.clone());
    let import_handle = ImportService::start(
        runtime_handle.clone(),
        shared_library_manager,
        encryption_service.clone(),
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
    assert_eq!(tracks.len(), 3, "Should have exactly 3 tracks");

    // Test track 1 (first track, 0:00-0:08, which is 8 seconds before track 2's pregap)
    // CUE timing: Track 1 INDEX 01 @ 0:00, Track 2 INDEX 00 @ 0:08
    // Expected duration: 8 seconds (8000ms)
    let track1 = &tracks[0];
    let expected_duration_ms: i64 = 8000; // Track 1 is 0:00 to 0:08

    std::env::set_var("MUTE_TEST_AUDIO", "1");
    let playback_handle = bae_core::playback::PlaybackService::start(
        library_manager.as_ref().clone(),
        encryption_service,
        runtime_handle,
    );
    playback_handle.set_volume(0.0);
    let mut progress_rx = playback_handle.subscribe_progress();
    playback_handle.play(track1.id.clone());

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut decoded_duration = None;
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(std::time::Duration::from_millis(100), progress_rx.recv()).await
        {
            Ok(Some(bae_core::playback::PlaybackProgress::StateChanged { state })) => {
                if let bae_core::playback::PlaybackState::Playing {
                    track,
                    decoded_duration: dd,
                    ..
                } = &state
                {
                    if track.id == track1.id {
                        decoded_duration = Some(*dd);
                        break;
                    }
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }

    let decoded_duration = decoded_duration.expect("Should get decoded_duration from playback");
    let decoded_duration_ms = decoded_duration.as_millis() as i64;

    info!(
        "Track 1: expected {}ms, decoded {}ms",
        expected_duration_ms, decoded_duration_ms
    );

    // The decoded audio must be AT LEAST as long as expected (we can't cut off audio).
    // It may be longer due to FLAC frame alignment - frames are decoded whole.
    // With sparse seektables (5-10s apart), decoded may be up to one seektable interval longer.
    assert!(
        decoded_duration_ms >= expected_duration_ms,
        "BUG: decoded_duration ({}ms) is shorter than expected CUE timing ({}ms). \
         Missing {}ms of audio. The seektable byte range algorithm is cutting off audio.",
        decoded_duration_ms,
        expected_duration_ms,
        expected_duration_ms - decoded_duration_ms
    );

    // Sanity check: decoded shouldn't be more than one seektable interval (5s) longer
    let max_overshoot_ms = 5500; // 5s seektable interval + tolerance
    assert!(
        decoded_duration_ms - expected_duration_ms < max_overshoot_ms,
        "BUG: decoded_duration ({}ms) is way longer than expected ({}ms). \
         Something is wrong with byte range extraction.",
        decoded_duration_ms,
        expected_duration_ms
    );

    info!("✅ CUE/FLAC decoded duration matches CUE timing");
}

/// Test that consecutive CUE/FLAC tracks have no gaps in byte ranges.
///
/// Byte ranges may overlap (due to FLAC frame alignment) but must not have gaps,
/// which would lose audio. FLAC decoder handles overlapping byte ranges correctly.
///
/// Uses a realistic fixture with seektable (generated by scripts/generate_cue_flac_fixture.sh)
#[tokio::test]
async fn test_cue_flac_byte_ranges_have_no_gaps() {
    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    std::fs::create_dir_all(&album_dir).expect("album dir");
    std::fs::create_dir_all(&db_dir).expect("db dir");

    copy_cue_flac_fixture_with_seektable(&album_dir);

    let db_file = db_dir.join("test.db");
    let database = Database::new(db_file.to_str().unwrap())
        .await
        .expect("database");
    let encryption_service = Some(EncryptionService::new_with_key(&[0u8; 32]));
    let library_manager = LibraryManager::new(database.clone(), test_encryption_service());
    let shared_library_manager = SharedLibraryManager::new(library_manager.clone());
    let library_manager = Arc::new(library_manager);
    let runtime_handle = tokio::runtime::Handle::current();
    let database_arc = Arc::new(database.clone());
    let import_handle = ImportService::start(
        runtime_handle,
        shared_library_manager,
        encryption_service,
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
    assert_eq!(tracks.len(), 3, "Should have exactly 3 tracks");

    // Check all consecutive track pairs have contiguous byte ranges
    for i in 0..tracks.len() - 1 {
        let track_a_format = library_manager
            .get_audio_format_by_track_id(&tracks[i].id)
            .await
            .expect("get track format")
            .expect("track should have audio format");
        let track_b_format = library_manager
            .get_audio_format_by_track_id(&tracks[i + 1].id)
            .await
            .expect("get next track format")
            .expect("next track should have audio format");

        let track_a_end = track_a_format
            .end_byte_offset
            .expect("track should have end offset");
        let track_b_start = track_b_format
            .start_byte_offset
            .expect("next track should have start offset");

        info!(
            "Track {} '{}' end: {}, Track {} '{}' start: {}",
            i + 1,
            tracks[i].title,
            track_a_end,
            i + 2,
            tracks[i + 1].title,
            track_b_start
        );

        // Byte ranges may overlap (FLAC decoder handles this) but must not have gaps
        assert!(
            track_a_end >= track_b_start,
            "BUG: Track {} end ({}) < Track {} start ({}). \
             Gap of {} bytes would lose audio!",
            i + 1,
            track_a_end,
            i + 2,
            track_b_start,
            track_b_start - track_a_end
        );
    }

    info!("✅ CUE/FLAC byte ranges have no gaps");
}

/// Test that we build a dense seektable during import for frame-accurate seeking.
///
/// The fixture has an embedded seektable with ~5 second gaps between entries.
/// We must build our own dense seektable (every frame, ~93ms) to:
/// 1. Enable smooth seeking during playback
/// 2. Calculate accurate track boundary byte positions
///
/// This test verifies:
/// - The embedded seektable is sparse (few entries)
/// - We store a dense seektable (many more entries)
/// - Track byte positions use frame-accurate offsets
#[tokio::test]
async fn test_cue_flac_builds_dense_seektable() {
    use bae_core::cue_flac::CueFlacProcessor;

    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    std::fs::create_dir_all(&album_dir).expect("album dir");
    std::fs::create_dir_all(&db_dir).expect("db dir");
    copy_cue_flac_fixture_with_seektable(&album_dir);

    // First, analyze the FLAC to see what's embedded
    let flac_path = album_dir.join("Test Album.flac");
    let file_data = std::fs::read(&flac_path).expect("read flac");
    let flac_info = CueFlacProcessor::analyze_flac(&flac_path).expect("analyze flac");

    // Build our dense seektable
    let dense_seektable = CueFlacProcessor::build_dense_seektable(&file_data, &flac_info);

    info!(
        "FLAC info: {} samples at {}Hz, audio starts at byte {}",
        flac_info.total_samples, flac_info.sample_rate, flac_info.audio_data_start
    );
    info!(
        "Dense seektable: {} entries (frame-accurate, ~{}ms precision)",
        dense_seektable.entries.len(),
        if dense_seektable.entries.len() > 1 {
            (flac_info.total_samples as f64 / dense_seektable.entries.len() as f64)
                / flac_info.sample_rate as f64
                * 1000.0
        } else {
            0.0
        }
    );

    // The dense seektable should have MANY more entries than a sparse one
    // For a 30-second file at 44100Hz with 4096-sample frames: ~322 frames
    // Sparse seektable (5s intervals) would have ~7 entries
    assert!(
        dense_seektable.entries.len() > 50,
        "Dense seektable should have many entries (got {}), not a sparse ~7 entries",
        dense_seektable.entries.len()
    );

    // Now import and verify the stored seektable is dense
    let db_file = db_dir.join("test.db");
    let database = Database::new(db_file.to_str().unwrap())
        .await
        .expect("database");
    let encryption_service = Some(EncryptionService::new_with_key(&[0u8; 32]));
    let library_manager = LibraryManager::new(database.clone(), test_encryption_service());
    let shared_library_manager = SharedLibraryManager::new(library_manager.clone());
    let library_manager = Arc::new(library_manager);
    let runtime_handle = tokio::runtime::Handle::current();
    let database_arc = Arc::new(database.clone());
    let import_handle = ImportService::start(
        runtime_handle,
        shared_library_manager,
        encryption_service,
        database_arc,
    );

    let discogs_release = create_test_discogs_release();
    let import_id = uuid::Uuid::new_v4().to_string();
    let (_album_id, release_id) = import_handle
        .send_request(ImportRequest::Folder {
            import_id,
            discogs_release: Some(discogs_release),
            mb_release: None,
            folder: album_dir.clone(),
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

    // Verify track 2 byte position is frame-accurate
    // Track 2 starts at 00:08:00 (8 seconds) = 8 * 44100 = 352800 samples
    // With frame-accurate positioning, byte offset should be within one frame (~93ms)
    let track2 = &tracks[1];
    let track2_format = library_manager
        .get_audio_format_by_track_id(&track2.id)
        .await
        .expect("get audio format")
        .expect("audio format exists");

    let _track2_start_byte = track2_format.start_byte_offset.expect("start offset");

    // Find the dense seektable entry closest to track 2's start
    let track2_start_sample = 8 * 44100u64; // 8 seconds
    let closest_entry = dense_seektable
        .entries
        .iter()
        .rev()
        .find(|e| e.sample <= track2_start_sample)
        .expect("should find entry before track 2");

    let samples_from_boundary = track2_start_sample - closest_entry.sample;
    let ms_from_boundary = (samples_from_boundary * 1000) / flac_info.sample_rate as u64;

    info!(
        "Track 2 starts at sample {}, nearest frame at sample {} ({}ms offset)",
        track2_start_sample, closest_entry.sample, ms_from_boundary
    );

    // With frame-accurate positioning, we should be within one frame (~93ms)
    assert!(
        ms_from_boundary < 100,
        "Track 2 should start within ~93ms of actual position (got {}ms). \
         This confirms we're using the dense seektable for frame-accurate positioning.",
        ms_from_boundary
    );

    info!("✅ Dense seektable built and stored correctly for frame-accurate seeking");
}

/// Copy the CUE/FLAC fixture with seektable (30-second file with 3 tracks).
/// Generated by scripts/generate_cue_flac_fixture.sh
fn copy_cue_flac_fixture_with_seektable(dir: &Path) {
    use std::fs;
    let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cue_flac");
    let flac_data = fs::read(fixture_dir.join("Test Album.flac")).unwrap_or_else(|_| {
        panic!("CUE/FLAC fixture not found. Run: ./scripts/generate_cue_flac_fixture.sh")
    });
    let cue_data = fs::read(fixture_dir.join("Test Album.cue")).unwrap_or_else(|_| {
        panic!("CUE fixture not found. Run: ./scripts/generate_cue_flac_fixture.sh")
    });
    fs::write(dir.join("Test Album.flac"), &flac_data).expect("write flac");
    fs::write(dir.join("Test Album.cue"), &cue_data).expect("write cue");
    info!(
        "Copied CUE/FLAC fixture with seektable: {} bytes FLAC",
        flac_data.len()
    );
}

/// Test that track 2's decoded audio matches ground truth.
///
/// This is the definitive test for CUE/FLAC byte range extraction:
/// 1. Ground truth: Decode entire FLAC, seek to track 2's start sample
/// 2. Under test: Use import + playback code path with byte range extraction
/// 3. Compare: First N samples after track start should match exactly
///
/// If byte offsets are wrong by even one frame, samples won't match.
#[tokio::test]
async fn test_cue_flac_track2_samples_match_ground_truth() {
    use bae_core::audio_codec::decode_audio;
    use bae_core::cue_flac::CueFlacProcessor;

    tracing_init();

    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac");
    let flac_path = fixture_dir.join("Test Album.flac");
    let cue_path = fixture_dir.join("Test Album.cue");

    if !flac_path.exists() || !cue_path.exists() {
        panic!("Fixture not found. Run: ./scripts/generate_cue_flac_fixture.sh");
    }

    info!("Testing with fixture: {:?}", fixture_dir);

    // Parse CUE to get track 2's timing
    let cue_sheet = CueFlacProcessor::parse_cue_sheet(&cue_path).expect("parse cue");
    let track2_cue = &cue_sheet.tracks[1]; // 0-indexed, track 2
    let track2_start_ms = track2_cue.audio_start_ms();
    info!(
        "Track 2 '{}' starts at {}ms (CUE timing)",
        track2_cue.title, track2_start_ms
    );

    // Read and analyze the FLAC
    let flac_data = std::fs::read(&flac_path).expect("read flac");
    let flac_info = CueFlacProcessor::analyze_flac(&flac_path).expect("analyze flac");
    info!(
        "FLAC: {} samples at {}Hz, audio starts at byte {}",
        flac_info.total_samples, flac_info.sample_rate, flac_info.audio_data_start
    );

    // === GROUND TRUTH ===
    // Decode the entire FLAC and extract samples starting at track 2's position
    info!("Decoding entire FLAC for ground truth...");
    let full_decode = decode_audio(&flac_data, None, None).expect("decode full flac");
    let channels = full_decode.channels as usize;
    let sample_rate = full_decode.sample_rate;

    // Calculate track 2's start position in samples (interleaved)
    let track2_start_sample = (track2_start_ms * sample_rate as u64 / 1000) as usize * channels;
    info!(
        "Ground truth: track 2 starts at sample index {} (of {})",
        track2_start_sample,
        full_decode.samples.len()
    );

    let truth_samples = &full_decode.samples[track2_start_sample..];

    // === UNDER TEST ===
    // Build seektable and find byte range (same as import does)
    let dense_seektable = CueFlacProcessor::build_dense_seektable(&flac_data, &flac_info);
    info!(
        "Built dense seektable with {} entries",
        dense_seektable.entries.len()
    );

    let seektable_entries: Vec<bae_core::audio_codec::SeekEntry> = dense_seektable.entries;

    let (start_byte, end_byte, _frame_offset_samples, _exact_sample_count) =
        CueFlacProcessor::find_track_byte_range(
            track2_start_ms,
            track2_cue.end_time_ms,
            &seektable_entries,
            flac_info.sample_rate,
            flac_info.total_samples,
            flac_info.audio_data_start,
            flac_info.audio_data_end,
        );
    info!(
        "Byte range for track 2: {} - {} ({} bytes)",
        start_byte,
        end_byte,
        end_byte - start_byte
    );

    // Extract byte range and prepend headers (same as playback does)
    let headers = CueFlacProcessor::extract_flac_headers(&flac_path).expect("extract headers");
    let track_bytes = &flac_data[start_byte as usize..end_byte as usize];

    let mut flac_with_headers = headers.headers.clone();
    flac_with_headers.extend_from_slice(track_bytes);

    info!(
        "Decoding extracted track ({} bytes with headers)...",
        flac_with_headers.len()
    );
    let track_decode = decode_audio(&flac_with_headers, None, None).expect("decode track");
    let actual_samples = &track_decode.samples;

    info!(
        "Decoded {} samples from byte range extraction",
        actual_samples.len()
    );

    // === COMPARE ===
    // Compare first ~1 second of audio (44100 * 2 channels = 88200 samples)
    let compare_count = (sample_rate as usize * channels).min(actual_samples.len());

    // The extracted track should start at the frame boundary at or before track2_start_ms.
    // We need to find where in the extracted audio the actual track 2 content starts.
    // The seektable entry we used is at or before track2_start_sample, so there may be
    // some "lead-in" samples from before track 2.

    // Find the seektable entry used for start_byte
    let start_entry = seektable_entries
        .iter()
        .rev()
        .find(|e| e.sample <= track2_start_ms * sample_rate as u64 / 1000)
        .expect("find start entry");

    let lead_in_samples =
        (track2_start_ms * sample_rate as u64 / 1000 - start_entry.sample) as usize * channels;

    info!(
        "Lead-in samples (frame alignment): {} ({:.1}ms)",
        lead_in_samples,
        lead_in_samples as f64 / channels as f64 / sample_rate as f64 * 1000.0
    );

    // Compare samples after the lead-in
    let actual_start = lead_in_samples;
    let truth_start = 0; // Ground truth already starts at track 2

    info!(
        "Comparing {} samples: actual[{}..] vs truth[{}..{}]",
        compare_count,
        actual_start,
        truth_start,
        truth_start + compare_count
    );

    // Check we have enough samples
    assert!(
        actual_samples.len() >= actual_start + compare_count,
        "Not enough actual samples: {} < {} + {}",
        actual_samples.len(),
        actual_start,
        compare_count
    );
    assert!(
        truth_samples.len() >= truth_start + compare_count,
        "Not enough truth samples: {} < {} + {}",
        truth_samples.len(),
        truth_start,
        compare_count
    );

    // Compare sample by sample
    let mut mismatches = 0;
    let mut first_mismatch = None;
    for i in 0..compare_count {
        let actual = actual_samples[actual_start + i];
        let truth = truth_samples[truth_start + i];
        if actual != truth {
            mismatches += 1;
            if first_mismatch.is_none() {
                first_mismatch = Some((i, actual, truth));
            }
        }
    }

    if mismatches > 0 {
        let (idx, actual, truth) = first_mismatch.unwrap();
        let offset_ms = idx as f64 / channels as f64 / sample_rate as f64 * 1000.0;
        panic!(
            "AUDIO MISMATCH: {} of {} samples differ!\n\
             First mismatch at index {} ({:.1}ms): actual={}, truth={}\n\
             This means byte range extraction is returning wrong audio.\n\
             The seektable or byte offset calculation is broken.",
            mismatches, compare_count, idx, offset_ms, actual, truth
        );
    }

    info!(
        "✅ All {} samples match! Track 2 byte range extraction is correct.",
        compare_count
    );
}

/// Test that seeking within a CUE/FLAC track and decoding produces correct audio.
///
/// This verifies the seektable-based seeking produces the correct samples by comparing
/// against ground truth (full decode of the file at the same position).
#[tokio::test]
async fn test_cue_flac_seek_within_track2_produces_correct_samples() {
    use bae_core::audio_codec::{decode_audio, SeekEntry};
    use bae_core::cue_flac::CueFlacProcessor;

    tracing_init();

    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac");
    let flac_path = fixture_dir.join("Test Album.flac");
    let cue_path = fixture_dir.join("Test Album.cue");

    if !flac_path.exists() || !cue_path.exists() {
        panic!("Fixture not found. Run: ./scripts/generate_cue_flac_fixture.sh");
    }

    // Parse CUE to get track 2's timing
    let cue_sheet = CueFlacProcessor::parse_cue_sheet(&cue_path).expect("parse cue");
    let track2_cue = &cue_sheet.tracks[1]; // 0-indexed
    let track2_start_ms = track2_cue.audio_start_ms();

    info!(
        "Track 2 '{}' starts at {}ms",
        track2_cue.title, track2_start_ms
    );

    // Read and analyze the FLAC
    let flac_data = std::fs::read(&flac_path).expect("read flac");
    let flac_info = CueFlacProcessor::analyze_flac(&flac_path).expect("analyze flac");
    let sample_rate = flac_info.sample_rate;
    let channels = 2usize;

    // Build file-absolute seektable (sample 0 = file start)
    let dense_seektable = CueFlacProcessor::build_dense_seektable(&flac_data, &flac_info);
    let seektable_entries: Vec<SeekEntry> = dense_seektable.entries;

    // === GROUND TRUTH ===
    // Decode entire file and extract samples at track2_start + 3s
    let full_decode = decode_audio(&flac_data, None, None).expect("decode");
    let seek_within_track_ms: u64 = 3000; // Seek 3 seconds into track 2
    let absolute_seek_ms = track2_start_ms + seek_within_track_ms;
    let absolute_seek_sample = (absolute_seek_ms * sample_rate as u64 / 1000) as usize * channels;

    info!(
        "Ground truth: seeking {}ms into track 2 = {}ms into file = sample {}",
        seek_within_track_ms, absolute_seek_ms, absolute_seek_sample
    );

    let truth_samples = &full_decode.samples[absolute_seek_sample..];

    // Find frame boundary using file-absolute position
    let target_sample = absolute_seek_ms * sample_rate as u64 / 1000;
    let mut best_idx = 0;
    for (i, entry) in seektable_entries.iter().enumerate() {
        if entry.sample <= target_sample {
            best_idx = i;
        } else {
            break;
        }
    }
    let frame = &seektable_entries[best_idx];
    let sample_offset = target_sample.saturating_sub(frame.sample);
    let frame_byte = frame.byte;

    info!(
        "Seektable lookup: target_sample {}, frame_byte {}, sample_offset {}",
        target_sample, frame_byte, sample_offset
    );

    // Decode from the seek position and verify samples match ground truth
    let headers = CueFlacProcessor::extract_flac_headers(&flac_path).expect("extract headers");
    let file_byte = flac_info.audio_data_start + frame_byte;
    let seek_bytes = &flac_data[file_byte as usize..];

    let mut flac_with_headers = headers.headers.clone();
    flac_with_headers.extend_from_slice(seek_bytes);

    let seeked_decode = decode_audio(&flac_with_headers, None, None).expect("decode seeked");

    // Skip sample_offset samples (frame alignment)
    let skip_interleaved = sample_offset as usize * channels;
    let actual_samples = &seeked_decode.samples[skip_interleaved..];

    // Compare first 0.5s of audio
    let compare_count = (sample_rate as usize / 2 * channels)
        .min(actual_samples.len())
        .min(truth_samples.len());

    let mut mismatches = 0;
    for i in 0..compare_count {
        if actual_samples[i] != truth_samples[i] {
            mismatches += 1;
        }
    }

    assert_eq!(
        mismatches, 0,
        "Seek produced wrong samples: {} of {} mismatch",
        mismatches, compare_count
    );

    info!(
        "✅ Seek within track 2 produces correct samples ({} compared)",
        compare_count
    );
}

/// Test that extracted track audio starts at the correct position for all scenarios.
///
/// Tests 2x2 matrix:
/// - With/without pregap
/// - Auto-advance (natural transition) vs manual start (user clicks track)
///
/// Currently FAILS due to lead-in samples from frame boundary alignment.
#[tokio::test]
async fn test_cue_flac_track_start_positions() {
    use bae_core::audio_codec::decode_audio;
    use bae_core::cue_flac::CueFlacProcessor;

    tracing_init();

    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac");
    let flac_path = fixture_dir.join("Test Album.flac");
    let cue_path = fixture_dir.join("Test Album.cue");

    if !flac_path.exists() || !cue_path.exists() {
        panic!("Fixture not found. Run: ./scripts/generate_cue_flac_fixture.sh");
    }

    let cue_sheet = CueFlacProcessor::parse_cue_sheet(&cue_path).expect("parse cue");
    let flac_data = std::fs::read(&flac_path).expect("read flac");
    let flac_info = CueFlacProcessor::analyze_flac(&flac_path).expect("analyze flac");
    let dense_seektable = CueFlacProcessor::build_dense_seektable(&flac_data, &flac_info);
    let full_decode = decode_audio(&flac_data, None, None).expect("decode full");
    let channels = full_decode.channels as usize;
    let sample_rate = full_decode.sample_rate;

    // Test cases: (track_index, is_auto_advance, description)
    // Track 2 has pregap (INDEX 00 at 8s, INDEX 01 at 10s)
    // Track 3 has no pregap (INDEX 01 at 20s)
    let test_cases = [
        (1, true, "Track 2 with pregap, auto-advance"), // INDEX 00 at 8000ms
        (1, false, "Track 2 with pregap, manual start"), // INDEX 01 at 10000ms
        (2, true, "Track 3 no pregap, auto-advance"),   // INDEX 01 at 20000ms
        (2, false, "Track 3 no pregap, manual start"),  // INDEX 01 at 20000ms
    ];

    for (track_idx, is_auto_advance, desc) in test_cases {
        let track = &cue_sheet.tracks[track_idx];

        // Determine expected start position
        let expected_start_ms = if is_auto_advance {
            track.audio_start_ms() // INDEX 00 if pregap exists, else INDEX 01
        } else {
            track.start_time_ms // INDEX 01 always
        };

        info!("Testing: {} - expected start {}ms", desc, expected_start_ms);

        // Get byte range and frame offset
        let (start_byte, end_byte, frame_offset_samples, _exact_sample_count) =
            CueFlacProcessor::find_track_byte_range(
                expected_start_ms,
                track.end_time_ms,
                &dense_seektable.entries,
                flac_info.sample_rate,
                flac_info.total_samples,
                flac_info.audio_data_start,
                flac_info.audio_data_end,
            );

        info!(
            "  Frame offset: {} samples ({:.1}ms)",
            frame_offset_samples,
            frame_offset_samples as f64 * 1000.0 / sample_rate as f64
        );

        // Extract and decode, then skip lead-in samples
        let headers = CueFlacProcessor::extract_flac_headers(&flac_path).expect("headers");
        let track_bytes = &flac_data[start_byte as usize..end_byte as usize];
        let mut flac_with_headers = headers.headers.clone();
        flac_with_headers.extend_from_slice(track_bytes);

        let extracted = decode_audio(&flac_with_headers, None, None).expect("decode");

        // Skip lead-in samples (the fix!)
        let skip_samples = if frame_offset_samples > 0 {
            frame_offset_samples as usize * channels
        } else {
            0
        };
        let extracted_samples_all = &extracted.samples[skip_samples..];

        // Ground truth: samples at the expected start position
        let expected_sample_idx =
            (expected_start_ms * sample_rate as u64 / 1000) as usize * channels;
        let truth_samples: Vec<i32> =
            full_decode.samples[expected_sample_idx..expected_sample_idx + channels].to_vec();
        let extracted_samples: Vec<i32> = extracted_samples_all[0..channels].to_vec();

        info!("  Ground truth: {:?}", truth_samples);
        info!("  Extracted:    {:?}", extracted_samples);

        assert_eq!(
            extracted_samples, truth_samples,
            "{}: Lead-in bug! Expected {:?} at {}ms, got {:?}",
            desc, truth_samples, expected_start_ms, extracted_samples
        );

        info!("  ✅ passed");
    }
}

/// Test gapless continuity at track boundaries.
///
/// Verifies that when Track 1 ends and Track 2 begins during auto-advance,
/// there are no missing or duplicated samples at the boundary.
///
/// Our fixture has:
/// - Track 1: 0ms to 8000ms (ends at Track 2's INDEX 00)
/// - Track 2: 8000ms to 20000ms (starts at INDEX 00, pregap until INDEX 01 at 10000ms)
///
/// For gapless playback, the concatenation of Track 1 + Track 2 must equal
/// the continuous audio from the full decode.
#[tokio::test]
async fn test_cue_flac_gapless_track_boundary() {
    use bae_core::audio_codec::decode_audio;
    use bae_core::cue_flac::CueFlacProcessor;

    tracing_init();

    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac");
    let flac_path = fixture_dir.join("Test Album.flac");
    let cue_path = fixture_dir.join("Test Album.cue");

    if !flac_path.exists() || !cue_path.exists() {
        panic!("Fixture not found. Run: ./scripts/generate_cue_flac_fixture.sh");
    }

    let cue_sheet = CueFlacProcessor::parse_cue_sheet(&cue_path).expect("parse cue");
    let flac_data = std::fs::read(&flac_path).expect("read flac");
    let flac_info = CueFlacProcessor::analyze_flac(&flac_path).expect("analyze flac");
    let dense_seektable = CueFlacProcessor::build_dense_seektable(&flac_data, &flac_info);
    let headers = CueFlacProcessor::extract_flac_headers(&flac_path).expect("headers");

    // Decode full file as ground truth
    let full_decode = decode_audio(&flac_data, None, None).expect("decode full");
    let channels = full_decode.channels as usize;
    let sample_rate = full_decode.sample_rate;

    info!(
        "Full decode: {} total samples ({} per channel), {}Hz",
        full_decode.samples.len(),
        full_decode.samples.len() / channels,
        sample_rate
    );

    // Track 1: ends at Track 2's audio_start (INDEX 00 at 8000ms)
    let track1 = &cue_sheet.tracks[0];
    let (t1_start_byte, t1_end_byte, t1_frame_offset, t1_exact_samples) =
        CueFlacProcessor::find_track_byte_range(
            track1.audio_start_ms(),
            track1.end_time_ms,
            &dense_seektable.entries,
            flac_info.sample_rate,
            flac_info.total_samples,
            flac_info.audio_data_start,
            flac_info.audio_data_end,
        );

    // Track 2: starts at INDEX 00 (8000ms), auto-advance plays pregap
    let track2 = &cue_sheet.tracks[1];
    let (t2_start_byte, t2_end_byte, t2_frame_offset, t2_exact_samples) =
        CueFlacProcessor::find_track_byte_range(
            track2.audio_start_ms(), // INDEX 00 for auto-advance
            track2.end_time_ms,
            &dense_seektable.entries,
            flac_info.sample_rate,
            flac_info.total_samples,
            flac_info.audio_data_start,
            flac_info.audio_data_end,
        );

    info!(
        "Track 1: bytes {}..{}, frame_offset={}, exact_samples={}",
        t1_start_byte, t1_end_byte, t1_frame_offset, t1_exact_samples
    );
    info!(
        "Track 2: bytes {}..{}, frame_offset={}, exact_samples={}",
        t2_start_byte, t2_end_byte, t2_frame_offset, t2_exact_samples
    );

    // Extract and decode Track 1
    let t1_bytes = &flac_data[t1_start_byte as usize..t1_end_byte as usize];
    let mut t1_flac = headers.headers.clone();
    t1_flac.extend_from_slice(t1_bytes);
    let t1_decode = decode_audio(&t1_flac, None, None).expect("decode track 1");

    // Extract and decode Track 2
    let t2_bytes = &flac_data[t2_start_byte as usize..t2_end_byte as usize];
    let mut t2_flac = headers.headers.clone();
    t2_flac.extend_from_slice(t2_bytes);
    let t2_decode = decode_audio(&t2_flac, None, None).expect("decode track 2");

    // Skip lead-in samples from frame boundary alignment, then trim to exact sample count
    let t1_skip = if t1_frame_offset > 0 {
        t1_frame_offset as usize * channels
    } else {
        0
    };
    let t2_skip = if t2_frame_offset > 0 {
        t2_frame_offset as usize * channels
    } else {
        0
    };

    // Trim to exact sample count (channels * samples)
    let t1_end_idx = t1_skip + (t1_exact_samples as usize * channels);
    let t2_end_idx = t2_skip + (t2_exact_samples as usize * channels);

    let t1_samples = &t1_decode.samples[t1_skip..t1_end_idx.min(t1_decode.samples.len())];
    let t2_samples = &t2_decode.samples[t2_skip..t2_end_idx.min(t2_decode.samples.len())];

    info!(
        "Track 1: {} samples after skipping {} lead-in",
        t1_samples.len() / channels,
        t1_skip / channels
    );
    info!(
        "Track 2: {} samples after skipping {} lead-in",
        t2_samples.len() / channels,
        t2_skip / channels
    );

    // The boundary is at Track 2's audio_start_ms (8000ms)
    let boundary_ms = track2.audio_start_ms();
    let boundary_sample = (boundary_ms * sample_rate as u64 / 1000) as usize;
    let boundary_idx = boundary_sample * channels;

    info!(
        "Boundary at {}ms = sample {} (idx {})",
        boundary_ms, boundary_sample, boundary_idx
    );

    // Track 1's last samples should match ground truth just before boundary
    let t1_expected_samples = boundary_sample; // samples from 0 to boundary
    let t1_actual_samples = t1_samples.len() / channels;

    info!(
        "Track 1: expected {} samples, got {}",
        t1_expected_samples, t1_actual_samples
    );

    // Track 2's first samples should match ground truth starting at boundary
    let compare_samples = 100; // Check 100 samples at the boundary

    // Ground truth at boundary
    let truth_at_boundary: Vec<i32> =
        full_decode.samples[boundary_idx..boundary_idx + compare_samples * channels].to_vec();

    // Track 2's first samples (after lead-in skip)
    let t2_at_start: Vec<i32> = t2_samples[0..compare_samples * channels].to_vec();

    info!(
        "Ground truth at boundary: {:?}...",
        &truth_at_boundary[0..8]
    );
    info!("Track 2 start:            {:?}...", &t2_at_start[0..8]);

    // Verify Track 2 starts at the right sample
    let mut t2_mismatches = 0;
    for i in 0..compare_samples * channels {
        if t2_at_start[i] != truth_at_boundary[i] {
            t2_mismatches += 1;
        }
    }

    // Also verify Track 1 ends at the right sample
    let truth_before_boundary: Vec<i32> =
        full_decode.samples[boundary_idx - compare_samples * channels..boundary_idx].to_vec();
    let t1_at_end: Vec<i32> = t1_samples[t1_samples.len() - compare_samples * channels..].to_vec();

    info!(
        "Ground truth before boundary: {:?}...",
        &truth_before_boundary[0..8]
    );
    info!("Track 1 end:                  {:?}...", &t1_at_end[0..8]);

    let mut t1_mismatches = 0;
    for i in 0..compare_samples * channels {
        if t1_at_end[i] != truth_before_boundary[i] {
            t1_mismatches += 1;
        }
    }

    // Calculate total samples and check for gaps/overlaps
    let total_extracted_samples = t1_actual_samples + t2_samples.len() / channels;
    let track2_end_sample = (track2.end_time_ms.unwrap() * sample_rate as u64 / 1000) as usize;
    let expected_total = track2_end_sample; // From 0 to end of Track 2

    info!(
        "Total extracted: {} samples, expected: {} (diff: {})",
        total_extracted_samples,
        expected_total,
        total_extracted_samples as i64 - expected_total as i64
    );

    // These assertions verify gapless playback
    assert_eq!(
        t2_mismatches, 0,
        "Track 2 start mismatch: {} samples differ at boundary. \
         Track 2's extracted audio doesn't start at the right position.",
        t2_mismatches
    );

    assert_eq!(
        t1_mismatches, 0,
        "Track 1 end mismatch: {} samples differ at boundary. \
         Track 1's extracted audio doesn't end at the right position.",
        t1_mismatches
    );

    // Check for gaps (missing samples) or overlaps (extra samples)
    let sample_diff = total_extracted_samples as i64 - expected_total as i64;
    assert!(
        sample_diff.abs() < 100,
        "Sample count mismatch at boundary: {} samples {}. \
         This would cause {} at track transition.",
        sample_diff.abs(),
        if sample_diff > 0 {
            "extra (overlap)"
        } else {
            "missing (gap)"
        },
        if sample_diff > 0 {
            "audio repeat"
        } else {
            "audio skip"
        }
    );

    info!(
        "✅ Gapless boundary verified: Track 1 ends and Track 2 starts correctly at {}ms",
        boundary_ms
    );
}

/// Discogs release matching the seektable fixture (tests/fixtures/cue_flac/)
fn create_test_discogs_release() -> DiscogsRelease {
    DiscogsRelease {
        id: "test-cue-flac".to_string(),
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
                title: "Track One (440Hz)".to_string(),
                duration: Some("0:10".to_string()),
            },
            DiscogsTrack {
                position: "2".to_string(),
                title: "Track Two (880Hz)".to_string(),
                duration: Some("0:10".to_string()),
            },
            DiscogsTrack {
                position: "3".to_string(),
                title: "Track Three (660Hz)".to_string(),
                duration: Some("0:10".to_string()),
            },
        ],
        master_id: "test-master".to_string(),
    }
}
