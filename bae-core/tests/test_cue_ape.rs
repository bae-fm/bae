#![cfg(feature = "test-utils")]
//! Tests for CUE/APE format handling.
//!
//! CUE/APE albums have multiple tracks in a single APE file. The import must:
//! - Parse the CUE sheet to find track boundaries
//! - Record per-track timing (track_start_ms / track_end_ms)
//! - Store correct per-track durations (NOT the full file duration)
//! - Enable playback of individual tracks via full-file decode with seek/stop
mod support;
use crate::support::{
    samples_as_f32, seed_discogs_test_release, test_config_and_keys, tracing_init,
    wait_for_import_complete,
};
use bae_core::db::Database;
use bae_core::discogs::models::{DiscogsArtist, DiscogsRelease, DiscogsTrack};
use bae_core::import::{
    IdentityChoice, ImportCommand, ImportService, MetadataRef, MetadataSource, StorageMode,
};
use bae_core::library::LibraryManager;
use bae_core::playback::{PlaybackProgress, PlaybackState};
use bae_core::sync::CloudCipher;
use bae_core::util::content_type::ContentType;
use coven::{EncryptionService, InMemoryCloudHome, StoreDir};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::time::{sleep, timeout};
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

fn copy_cue_ape_fixture(dir: &Path) {
    use std::fs;
    let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cue_ape");
    let ape_data = fs::read(fixture_dir.join("Test Album.ape"))
        .unwrap_or_else(|_| panic!("CUE/APE fixture not found at {:?}", fixture_dir));
    let cue_data = fs::read(fixture_dir.join("Test Album.cue"))
        .unwrap_or_else(|_| panic!("CUE/APE fixture not found at {:?}", fixture_dir));
    fs::write(dir.join("Test Album.ape"), &ape_data).expect("write ape");
    fs::write(dir.join("Test Album.cue"), &cue_data).expect("write cue");
    info!("Copied CUE/APE fixture: {} bytes APE", ape_data.len());
}

fn create_test_discogs_release() -> DiscogsRelease {
    DiscogsRelease {
        id: "test-cue-ape".to_string(),
        title: "Test Album".to_string(),
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
                duration: Some("0:30".to_string()),
                artists: vec![],
                extraartists: None,
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "2".to_string(),
                title: "Track Two".to_string(),
                duration: Some("0:30".to_string()),
                artists: vec![],
                extraartists: None,
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "3".to_string(),
                title: "Track Three".to_string(),
                duration: Some("0:30".to_string()),
                artists: vec![],
                extraartists: None,
            },
        ],
        master_id: Some("test-master".to_string()),
    }
}

/// Ground truth durations derived from CUE INDEX 01 timings.
/// CUE frames are 1/75th of a second.
///
/// Track boundaries (INDEX 01):
///   1: 00:00:00 (0ms)
///   2: 00:30:00 (30000ms)
///   3: 01:00:00 (60000ms)
/// Full file: 90000ms
const EXPECTED_DURATIONS_MS: [i64; 3] = [
    30000, // Track 1: 0 -> 30000
    30000, // Track 2: 30000 -> 60000
    30000, // Track 3: 60000 -> 90000
];

const FULL_FILE_DURATION_MS: i64 = 90000;

/// Regression: CUE+APE imports must store per-track durations, not the full file duration.
///
/// Bug: `extract_and_store_durations` only entered the CUE duration path for .flac files.
/// For CUE+APE, all 8 tracks got the full APE file duration (37:29).
#[tokio::test]
async fn test_cue_ape_records_correct_durations() {
    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    std::fs::create_dir_all(&album_dir).expect("album dir");
    std::fs::create_dir_all(&db_dir).expect("db dir");
    copy_cue_ape_fixture(&album_dir);

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
            folder: album_dir,
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

    let tracks = library_manager
        .get_tracks_for_release(&release_id)
        .await
        .expect("get tracks");
    assert_eq!(tracks.len(), 3, "Should have 3 tracks");

    // Core assertion: each track must have its own duration, NOT the full file duration
    for (i, track) in tracks.iter().enumerate() {
        let duration_ms = track.duration_ms.unwrap_or_else(|| {
            panic!(
                "Track {} '{}' should have duration_ms set",
                i + 1,
                track.title
            )
        });

        // Must NOT be the full file duration
        assert_ne!(
            duration_ms,
            FULL_FILE_DURATION_MS,
            "BUG: Track {} '{}' has full file duration ({}ms). \
             CUE+APE duration extraction is not using CUE sheet timings.",
            i + 1,
            track.title,
            FULL_FILE_DURATION_MS,
        );

        let expected = EXPECTED_DURATIONS_MS[i];
        // Allow 1 second tolerance for the last track (FFmpeg probe vs CUE timing)
        let tolerance = if i == 2 { 1000 } else { 50 };
        assert!(
            (duration_ms - expected).abs() < tolerance,
            "Track {} '{}': expected ~{}ms, got {}ms (diff {}ms)",
            i + 1,
            track.title,
            expected,
            duration_ms,
            (duration_ms - expected).abs(),
        );

        info!(
            "Track {} '{}': duration={}ms (expected ~{}ms)",
            i + 1,
            track.title,
            duration_ms,
            expected,
        );
    }
}

/// Track 2's decoded audio via full-file decode + sample skip must match XLD reference.
///
/// This tests the decoder level: decode the full APE file and extract track 2's
/// samples by skipping the first 30s and stopping at 60s (same approach as the
/// playback service uses). Compare against the XLD-split reference FLAC.
#[tokio::test]
async fn test_cue_ape_track2_samples_match_xld_reference() {
    use bae_core::audio_codec::decode_audio;

    tracing_init();

    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cue_ape");

    let ape_path = fixture_dir.join("Test Album.ape");
    let ref_path = fixture_dir.join("02 Test Artist - Track Two.flac");

    let ape_data = std::fs::read(&ape_path).expect("read ape");

    // Decode the full APE file (no seeking -- APE frames are stateful)
    let full_decoded = decode_audio(&ape_data, None, None).expect("decode full APE");
    let channels = full_decoded.channels as usize;
    let sample_rate = full_decoded.sample_rate;

    // Extract track 2: skip first 30s, take next 30s
    let skip_samples = 30_000u64 * sample_rate as u64 / 1000 * channels as u64;
    let end_samples = 60_000u64 * sample_rate as u64 / 1000 * channels as u64;
    let track2_samples = &full_decoded.samples[skip_samples as usize..end_samples as usize];

    info!(
        "Extracted {} track 2 samples from full decode ({}Hz, {}ch)",
        track2_samples.len(),
        sample_rate,
        channels,
    );

    // Decode XLD reference
    let ref_data = std::fs::read(&ref_path).expect("read reference");
    let reference = decode_audio(&ref_data, None, None).expect("decode reference");

    assert_eq!(reference.sample_rate, sample_rate, "sample rate mismatch");
    assert_eq!(
        reference.channels, full_decoded.channels,
        "channel count mismatch"
    );

    // Compare first 1 second
    let compare_count = (sample_rate as usize * channels)
        .min(track2_samples.len())
        .min(reference.samples.len());

    let mut mismatches = 0;
    let mut first_mismatch = None;
    for (i, (&actual, &truth)) in track2_samples
        .iter()
        .zip(reference.samples.iter())
        .enumerate()
        .take(compare_count)
    {
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
            "AUDIO MISMATCH: {} of {} samples differ vs XLD reference!\n\
             First at index {} ({:.1}ms): actual={}, reference={}",
            mismatches, compare_count, idx, offset_ms, actual, truth
        );
    }

    info!("All {} samples match XLD reference.", compare_count);
}

/// CUE+APE imports must record track timing (track_start_ms / track_end_ms)
/// for full-file decode with seek/stop.
#[tokio::test]
async fn test_cue_ape_records_track_timing() {
    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let db_dir = temp_root.path().join("db");
    std::fs::create_dir_all(&album_dir).expect("album dir");
    std::fs::create_dir_all(&db_dir).expect("db dir");
    copy_cue_ape_fixture(&album_dir);

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
            folder: album_dir,
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
    let tracks = library_manager
        .get_tracks_for_release(&release_id)
        .await
        .expect("get tracks");
    assert_eq!(tracks.len(), 3);

    // Expected CUE timings in samples at 44100Hz:
    // CUE frames: 0, 30*75=2250, 60*75=4500
    // Samples: cue_frames * 588
    let expected_start_sample = [0u64, 2250 * 588, 4500 * 588];
    let expected_end_sample: [Option<u64>; 3] = [Some(2250 * 588), Some(4500 * 588), None];

    for (i, track) in tracks.iter().enumerate() {
        let af = library_manager
            .get_audio_format_by_track_id(&track.id)
            .await
            .expect("get audio format")
            .unwrap_or_else(|| panic!("Track {} should have audio format", i + 1));

        assert_eq!(
            af.content_type,
            ContentType::Ape,
            "Track {} should be APE",
            i + 1
        );

        let resolved = library_manager
            .resolve_track_audio(&track.id)
            .await
            .expect("resolve track audio");
        assert_eq!(resolved.segments.len(), 1);
        let segment = &resolved.segments[0];

        // A CUE+APE track is a sample window of the shared file.
        assert_eq!(
            segment.start_sample,
            expected_start_sample[i],
            "Track {} start_sample",
            i + 1,
        );
        assert_eq!(
            segment.end_sample,
            expected_end_sample[i],
            "Track {} end_sample",
            i + 1,
        );

        info!(
            "Track {} '{}': start_sample={}, end_sample={:?}",
            i + 1,
            track.title,
            segment.start_sample,
            segment.end_sample,
        );
    }
}

// ============================================================================
// Playback fixture — imports the CUE+APE album and sets up playback
// ============================================================================

struct CueApeTestFixture {
    playback_handle: bae_core::playback::PlaybackHandle,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_ids: Vec<String>,
    capture_stream_rx:
        tokio::sync::mpsc::UnboundedReceiver<std::sync::Arc<std::sync::Mutex<Vec<f32>>>>,
    _temp_dir: TempDir,
}

impl CueApeTestFixture {
    /// Awaits the next capture buffer minted by `create_stream`. Buffers are
    /// yielded in creation order; tests that exercise auto-advance, seek, or
    /// next call this once per stream they want to inspect.
    async fn next_capture_stream(&mut self) -> std::sync::Arc<std::sync::Mutex<Vec<f32>>> {
        match timeout(Duration::from_secs(5), self.capture_stream_rx.recv()).await {
            Ok(Some(buf)) => buf,
            Ok(None) => panic!("capture stream channel closed before a stream was created"),
            Err(_) => panic!("no capture stream created within 5s"),
        }
    }

    async fn with_capture() -> Result<Self, Box<dyn std::error::Error>> {
        tracing_init();
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let album_dir = temp_dir.path().join("album");
        std::fs::create_dir_all(&album_dir)?;

        copy_cue_ape_fixture(&album_dir);

        let database = Database::new_test(
            db_path.to_str().unwrap(),
            std::sync::Arc::new(coven::SystemClock),
        )
        .await?;
        let library_dir = StoreDir::new(temp_dir.path().to_path_buf());
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
        let import_handle = start_test_import(runtime_handle.clone(), library_manager.clone());
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
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
        let mut progress_rx = import_handle.subscribe_import(import_id);
        let (_release_id, _album_id) = wait_for_import_complete(&mut progress_rx).await;

        let albums = library_manager.get_albums(&[]).await?;
        assert!(!albums.is_empty(), "Should have imported album");
        let releases = library_manager
            .get_releases_for_album(&albums[0].id)
            .await?;
        assert!(!releases.is_empty(), "Should have imported release");
        let tracks = library_manager
            .get_tracks_for_release(&releases[0].id)
            .await?;
        let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
        assert_eq!(track_ids.len(), 3, "Should have 3 tracks from CUE/APE");

        let (capture_output, capture_stream_rx) = bae_core::playback::CaptureAudioOutput::new();
        let playback_handle = bae_core::playback::PlaybackService::start_with_output(
            library_manager.clone(),
            runtime_handle,
            100,
            true,
            Box::new(capture_output),
        );
        let progress_rx = playback_handle.subscribe_progress();

        Ok(Self {
            playback_handle,
            progress_rx,
            track_ids,
            capture_stream_rx,
            _temp_dir: temp_dir,
        })
    }
}

// ============================================================================
// Playback tests
// ============================================================================

/// Track 2 playback must produce audio matching the XLD-split reference FLAC.
///
/// Regression: without the duration fix, all tracks report the full file duration.
/// This test goes further — it captures the actual audio samples and compares them
/// against the ground truth reference.
#[tokio::test]
async fn test_cue_ape_playback_uses_track_positions() {
    use bae_core::audio_codec::decode_audio;

    let mut fixture = CueApeTestFixture::with_capture()
        .await
        .unwrap_or_else(|e| panic!("failed to set up CUE/APE playback fixture: {e}"));

    let track_id = fixture.track_ids[1].clone();

    // Play track 2
    fixture.playback_handle.play(track_id.clone());
    let captured = fixture.next_capture_stream().await;

    // Wait for playback to start
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut started = false;
    while Instant::now() < deadline && !started {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => {
                if let PlaybackState::Playing { track_info, .. } = &state {
                    if track_info.track_id == track_id {
                        started = true;
                    }
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    assert!(started, "Track 2 should start playing");

    // Decode XLD reference
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cue_ape");
    let reference_data =
        std::fs::read(fixture_dir.join("02 Test Artist - Track Two.flac")).expect("read reference");
    let reference = decode_audio(&reference_data, None, None).expect("decode reference");
    let channels = reference.channels as usize;
    let sample_rate = reference.sample_rate;
    let reference_f32 = samples_as_f32(&reference);

    // Align and compare
    let snippet_len = 500 * channels;
    // APE frames are ~73728 samples (~1.7s). FFmpeg's seek lands at a frame
    // boundary, so the captured audio may start up to one frame before the
    // track's CUE start time. Search within 2s for alignment.
    let max_alignment = sample_rate as usize * channels * 2;

    let needed = max_alignment + snippet_len;
    let captured_snapshot =
        bae_core::playback::wait_for_samples(&captured, needed + 1, Duration::from_secs(60)).await;
    assert!(
        captured_snapshot.len() > needed,
        "Not enough captured samples after 60s: {} (needed {})",
        captured_snapshot.len(),
        needed,
    );

    let mut best_max_diff: f32 = f32::MAX;
    let mut best_offset: usize = 0;
    for offset in 0..max_alignment.min(captured_snapshot.len().saturating_sub(snippet_len)) {
        let mut max_diff: f32 = 0.0;
        for i in 0..snippet_len {
            let diff = (captured_snapshot[offset + i] - reference_f32[i]).abs();
            max_diff = max_diff.max(diff);
            if max_diff > best_max_diff {
                break;
            }
        }
        if max_diff < best_max_diff {
            best_max_diff = max_diff;
            best_offset = offset;
        }
    }

    let offset_ms = best_offset as f64 / channels as f64 / sample_rate as f64 * 1000.0;

    assert!(
        best_max_diff < 0.01,
        "Could not align captured track 2 audio with XLD reference within 100ms.\n\
         Best offset {:.1}ms, max sample diff {:.6}",
        offset_ms,
        best_max_diff,
    );

    let compare_count = (sample_rate as usize * channels)
        .min(captured_snapshot.len() - best_offset)
        .min(reference_f32.len());

    for i in 0..compare_count {
        let diff = (captured_snapshot[best_offset + i] - reference_f32[i]).abs();
        assert!(
            diff < 0.01,
            "AUDIO MISMATCH at index {} ({:.1}ms): captured={:.6}, reference={:.6}",
            i,
            i as f64 / channels as f64 / sample_rate as f64 * 1000.0,
            captured_snapshot[best_offset + i],
            reference_f32[i],
        );
    }

    info!(
        "Track 2 samples match XLD reference ({} samples, offset {:.1}ms).",
        compare_count, offset_ms,
    );
}

/// Track 1 playback must produce audio matching the XLD-split reference FLAC.
///
/// If decode errors occur, the captured samples won't match the reference.
#[tokio::test]
async fn test_cue_ape_playback_no_decode_errors() {
    use bae_core::audio_codec::decode_audio;

    let mut fixture = CueApeTestFixture::with_capture()
        .await
        .unwrap_or_else(|e| panic!("failed to set up CUE/APE playback fixture: {e}"));

    let track_id = fixture.track_ids[0].clone();

    fixture.playback_handle.play(track_id.clone());
    let captured = fixture.next_capture_stream().await;

    // Wait for playback to start
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut started = false;
    while Instant::now() < deadline && !started {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => {
                if let PlaybackState::Playing { track_info, .. } = &state {
                    if track_info.track_id == track_id {
                        started = true;
                    }
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    assert!(started, "Track 1 should start playing");

    // Decode XLD reference
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cue_ape");
    let reference_data =
        std::fs::read(fixture_dir.join("01 Test Artist - Track One.flac")).expect("read reference");
    let reference = decode_audio(&reference_data, None, None).expect("decode reference");
    let channels = reference.channels as usize;
    let sample_rate = reference.sample_rate;
    let reference_f32 = samples_as_f32(&reference);

    // Align and compare
    let snippet_len = 500 * channels;
    // APE frames are ~73728 samples (~1.7s). FFmpeg's seek lands at a frame
    // boundary, so the captured audio may start up to one frame before the
    // track's CUE start time. Search within 2s for alignment.
    let max_alignment = sample_rate as usize * channels * 2;

    let needed = max_alignment + snippet_len;
    let captured_snapshot =
        bae_core::playback::wait_for_samples(&captured, needed + 1, Duration::from_secs(60)).await;
    assert!(
        captured_snapshot.len() > needed,
        "Not enough captured samples after 60s: {} (needed {})",
        captured_snapshot.len(),
        needed,
    );

    let mut best_max_diff: f32 = f32::MAX;
    let mut best_offset: usize = 0;
    for offset in 0..max_alignment.min(captured_snapshot.len().saturating_sub(snippet_len)) {
        let mut max_diff: f32 = 0.0;
        for i in 0..snippet_len {
            let diff = (captured_snapshot[offset + i] - reference_f32[i]).abs();
            max_diff = max_diff.max(diff);
            if max_diff > best_max_diff {
                break;
            }
        }
        if max_diff < best_max_diff {
            best_max_diff = max_diff;
            best_offset = offset;
        }
    }

    let offset_ms = best_offset as f64 / channels as f64 / sample_rate as f64 * 1000.0;

    assert!(
        best_max_diff < 0.01,
        "Could not align captured track 1 audio with XLD reference.\n\
         Best offset {:.1}ms, max sample diff {:.6}",
        offset_ms,
        best_max_diff,
    );

    let compare_count = (sample_rate as usize * channels)
        .min(captured_snapshot.len() - best_offset)
        .min(reference_f32.len());

    for i in 0..compare_count {
        let diff = (captured_snapshot[best_offset + i] - reference_f32[i]).abs();
        assert!(
            diff < 0.01,
            "AUDIO MISMATCH at index {} ({:.1}ms): captured={:.6}, reference={:.6}",
            i,
            i as f64 / channels as f64 / sample_rate as f64 * 1000.0,
            captured_snapshot[best_offset + i],
            reference_f32[i],
        );
    }

    info!(
        "Track 1 samples match XLD reference ({} samples, offset {:.1}ms).",
        compare_count, offset_ms,
    );
}

/// Seeking to 27s in track 2 must produce audio matching the reference from that point.
///
/// Verifies seek lands at the correct audio position by comparing captured samples
/// against the XLD reference starting at the 27s mark.
#[tokio::test]
async fn test_cue_ape_seek() {
    use bae_core::audio_codec::decode_audio;

    let mut fixture = CueApeTestFixture::with_capture()
        .await
        .unwrap_or_else(|e| panic!("failed to set up CUE/APE playback fixture: {e}"));

    let track_id = fixture.track_ids[1].clone();

    fixture.playback_handle.play(track_id.clone());
    // Drain the play stream; the seek below mints a fresh one.
    let _play_stream = fixture.next_capture_stream().await;

    // Wait for playback to start
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut started = false;
    while Instant::now() < deadline && !started {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => {
                if matches!(state, PlaybackState::Playing { .. }) {
                    started = true;
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    assert!(started, "Playback should start");

    // Seek to 27s — capture the new stream and wait for new audio post-seek
    fixture.playback_handle.seek(Duration::from_secs(27));
    let captured = fixture.next_capture_stream().await;

    // Wait for seek confirmation
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut seeked = false;
    while Instant::now() < deadline && !seeked {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::Seeked {
                track_id: ref sid, ..
            })) => {
                if *sid == track_id {
                    seeked = true;
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    assert!(seeked, "Should receive Seeked event");

    // Snapshot captured samples after seek has had time to produce audio.
    // The seek allocated its own buffer, so the captured samples start at the
    // seek position (not the original play position).
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cue_ape");
    let reference_data =
        std::fs::read(fixture_dir.join("02 Test Artist - Track Two.flac")).expect("read reference");
    let reference = decode_audio(&reference_data, None, None).expect("decode reference");
    let channels = reference.channels as usize;
    let sample_rate = reference.sample_rate;
    let reference_f32 = samples_as_f32(&reference);

    let needed_for_seek = sample_rate as usize * channels;
    let captured_snapshot =
        bae_core::playback::wait_for_samples(&captured, needed_for_seek, Duration::from_secs(60))
            .await;

    assert!(
        captured_snapshot.len() >= needed_for_seek,
        "Not enough captured samples after seek (60s): {} (needed {})",
        captured_snapshot.len(),
        needed_for_seek,
    );

    // The streaming AVIO decoder produces f32 samples through a different FFmpeg code path
    // than decode_audio (which produces i32). After seeking, this conversion difference is
    // larger because the decoder restarts at a frame boundary. Use a looser tolerance (0.02)
    // for the alignment phase and verify the seek landed in the correct region of the track.
    //
    // APE frames are ~73728 samples (~1.67s at 44100Hz). The seek may land at any frame
    // boundary within ~2s of the target.
    let seek_ms: u64 = 27000;
    let seek_sample = (seek_ms * sample_rate as u64 / 1000) as usize * channels;
    let snippet_len = 200 * channels; // Smaller snippet for faster search

    // Search the reference within a 3s window around the seek target
    let search_start = seek_sample.saturating_sub(sample_rate as usize * channels * 2);
    let search_end = seek_sample.min(reference_f32.len().saturating_sub(snippet_len));
    let step = 100 * channels; // ~2.3ms steps

    // Use SAD (sum of absolute differences) for alignment instead of max-diff.
    // This is more robust against individual sample conversion noise.
    let mut best_sad: f64 = f64::MAX;
    let mut best_ref_offset: usize = 0;

    let mut ref_offset = search_end;
    while ref_offset > search_start {
        let mut sad: f64 = 0.0;
        for i in 0..snippet_len {
            sad += (captured_snapshot[i] as f64 - reference_f32[ref_offset + i] as f64).abs();
            if sad > best_sad {
                break;
            }
        }
        if sad < best_sad {
            best_sad = sad;
            best_ref_offset = ref_offset;
        }
        if ref_offset < step {
            break;
        }
        ref_offset -= step;
    }

    let ref_time_ms = best_ref_offset as f64 / channels as f64 / sample_rate as f64 * 1000.0;
    let avg_diff = best_sad / snippet_len as f64;

    // Verify the seek landed reasonably close to the target (within 2s for APE frame alignment)
    let drift_ms = (seek_ms as f64 - ref_time_ms).abs();

    assert!(
        drift_ms < 2000.0,
        "Seek landed too far from target: wanted {:.0}ms, got {:.0}ms ({:.0}ms drift)",
        seek_ms,
        ref_time_ms,
        drift_ms,
    );

    // Verify the audio is roughly correct by checking average difference
    // The streaming decoder's f32 output has ~0.01-0.15 per-sample noise vs i32->f32,
    // but the average should be much lower for correct audio.
    assert!(
        avg_diff < 0.1,
        "Post-seek audio average difference too high ({:.4}), audio may be from wrong position.\n\
         Best ref offset {:.1}ms",
        avg_diff,
        ref_time_ms,
    );

    info!(
        "Post-seek audio aligned at {:.1}ms in reference (drift {:.1}ms, avg_diff {:.4}).",
        ref_time_ms, drift_ms, avg_diff,
    );
}

/// A seek must surface a buffering state, then confirm. The UI shows the seek
/// target bar immediately as `Loading{resolved: Some(..)}`, and `Seeked` follows
/// once the demanded window lands and the ready-watcher fires — the same
/// Loading→Playing arc the play path uses. The old seek emitted no Loading and
/// gated `Seeked` behind a fixed wait, so it never produced the Loading event.
#[tokio::test]
async fn test_cue_ape_seek_emits_loading_then_seeked() {
    let mut fixture = CueApeTestFixture::with_capture()
        .await
        .unwrap_or_else(|e| panic!("failed to set up CUE/APE playback fixture: {e}"));

    let track_id = fixture.track_ids[1].clone();
    fixture.playback_handle.play(track_id.clone());
    let _play_stream = fixture.next_capture_stream().await;

    // Wait for playback to start.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut started = false;
    while Instant::now() < deadline && !started {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged {
                state: PlaybackState::Playing { .. },
            })) => started = true,
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    assert!(started, "Playback should start");

    // Seek deep into the track.
    fixture.playback_handle.seek(Duration::from_secs(27));
    let _seek_stream = fixture.next_capture_stream().await;

    // The seek must emit a resolved Loading for this track BEFORE Seeked.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut saw_loading = false;
    let mut saw_seeked = false;
    while Instant::now() < deadline && !saw_seeked {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged {
                state:
                    PlaybackState::Loading {
                        track_id: ref lid,
                        resolved: Some(_),
                    },
            })) if *lid == track_id => {
                saw_loading = true;
            }
            Ok(Some(PlaybackProgress::Seeked {
                track_id: ref sid, ..
            })) if *sid == track_id => {
                assert!(
                    saw_loading,
                    "Loading{{resolved}} must arrive before Seeked on a seek"
                );
                saw_seeked = true;
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    assert!(saw_loading, "seek must emit a resolved Loading state");
    assert!(saw_seeked, "seek must emit Seeked once the window lands");
}

/// Gapless survives a seek: after seeking near the end of track 1 (queued
/// track 2), the auto-advance into track 2 must NOT emit a Loading — the staged
/// next track is re-staged into the rebuilt stream, so the boundary advance
/// plays through without a rebuild.
#[tokio::test]
async fn test_cue_ape_gapless_preserved_across_seek() {
    let mut fixture = CueApeTestFixture::with_capture()
        .await
        .unwrap_or_else(|e| panic!("failed to set up CUE/APE playback fixture: {e}"));

    let track1_id = fixture.track_ids[0].clone();
    let track2_id = fixture.track_ids[1].clone();

    fixture.playback_handle.play(track1_id.clone());
    let _track1_play = fixture.next_capture_stream().await;
    fixture
        .playback_handle
        .add_to_queue(vec![track2_id.clone()]);

    // Wait for track 1 to start.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut started = false;
    while Instant::now() < deadline && !started {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => {
                if let PlaybackState::Playing { track_info, .. } = &state {
                    if track_info.track_id == track1_id {
                        started = true;
                    }
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    assert!(started, "Track 1 should start playing");

    // Seek near the end of track 1 so it completes quickly and auto-advances.
    fixture.playback_handle.seek(Duration::from_secs(28));
    let _seek_stream = fixture.next_capture_stream().await;

    // Wait for track 2 via auto-advance; assert no Loading for track 2 appears
    // (a gapless advance plays straight through without rebuilding the stream).
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut track2_started = false;
    while Instant::now() < deadline {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => match state {
                PlaybackState::Loading {
                    track_id: ref lid,
                    resolved: Some(_),
                } if *lid == track2_id => {
                    panic!("gapless advance into track 2 must not emit Loading");
                }
                PlaybackState::Playing { track_info, .. } if track_info.track_id == track2_id => {
                    track2_started = true;
                    break;
                }
                _ => continue,
            },
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    assert!(
        track2_started,
        "Track 2 should auto-advance gaplessly after the seek"
    );
}

/// Auto-advance must not replay audio from before the track boundary.
///
/// The preload path must trim the decoder to the track's start sample
/// (`start_at_sample`). APE frames are large (~73728 samples) and a track starts
/// mid-frame, so without the trim the decoder emits the previous track's tail --
/// heard as a 1-6 second "skip back" on transition.
///
/// This test captures raw audio samples via CaptureAudioOutput, plays through
/// the auto-advance path, then compares the captured track 2 samples against
/// the XLD-split reference FLAC (exact ground truth).
#[tokio::test]
async fn test_cue_ape_auto_advance_no_replay() {
    use bae_core::audio_codec::decode_audio;

    let mut fixture = CueApeTestFixture::with_capture()
        .await
        .unwrap_or_else(|e| panic!("failed to set up CUE/APE playback fixture: {e}"));

    let track1_id = fixture.track_ids[0].clone();
    let track2_id = fixture.track_ids[1].clone();

    // Play track 1 and queue track 2 so auto-advance uses the preload path
    fixture.playback_handle.play(track1_id.clone());
    // Drain track 1's play stream. With gapless playback the auto-advance does
    // NOT mint a new stream — track 2 plays through the seek stream below, so
    // that single buffer holds track 1's tail followed by track 2.
    let _track1_play = fixture.next_capture_stream().await;
    fixture
        .playback_handle
        .add_to_queue(vec![track2_id.clone()]);

    // Wait for track 1 to start
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut started = false;
    while Instant::now() < deadline && !started {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => {
                if let PlaybackState::Playing { track_info, .. } = &state {
                    if track_info.track_id == track1_id {
                        started = true;
                    }
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    assert!(started, "Track 1 should start playing");

    // Seek to 28s into the 30s track so it completes quickly, then gaplessly
    // auto-advances into track 2 within this same stream. This buffer therefore
    // holds track 1's 28s→30s tail followed by track 2.
    fixture.playback_handle.seek(Duration::from_secs(28));
    let captured = fixture.next_capture_stream().await;

    // Wait for track 2 to start via auto-advance, then capture samples.
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut track2_started = false;
    while Instant::now() < deadline {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => {
                if let PlaybackState::Playing { track_info, .. } = &state {
                    if track_info.track_id == track2_id {
                        track2_started = true;
                        break;
                    }
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    assert!(
        track2_started,
        "Track 2 should auto-advance after track 1 completes"
    );

    // === GROUND TRUTH: XLD-split reference FLAC ===
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cue_ape");
    let reference_data =
        std::fs::read(fixture_dir.join("02 Test Artist - Track Two.flac")).expect("read reference");
    let reference = decode_audio(&reference_data, None, None).expect("decode reference");
    let channels = reference.channels as usize;
    let sample_rate = reference.sample_rate;

    // Convert reference i32 to f32
    let reference_f32 = samples_as_f32(&reference);

    // === COMPARE ===
    // The capture is one gapless stream: track 1's 28s→30s tail, then track 2.
    // Find where track 2 aligns with the reference, then verify per-sample match
    // with f32 tolerance. If the start-sample trim is broken, track 2's audio
    // would be shifted and the alignment max-diff would exceed tolerance.
    let snippet_len = 500 * channels;
    // ~2s of track 1's tail precedes track 2, plus APE frame alignment (~1.7s
    // frames, FFmpeg seeks to a frame boundary). Search within 5s so the
    // alignment locks onto track 2's start past track 1's tail.
    let max_alignment = sample_rate as usize * channels * 5;

    let needed = max_alignment + snippet_len;
    let captured_snapshot =
        bae_core::playback::wait_for_samples(&captured, needed + 1, Duration::from_secs(60)).await;
    assert!(
        captured_snapshot.len() > needed,
        "Not enough captured samples after 60s: {} (needed {})",
        captured_snapshot.len(),
        needed,
    );

    // Alignment search using per-sample max-diff instead of SAD.
    // At the correct offset, every sample should be within f32 tolerance.
    // At the wrong offset, samples will have large differences.
    let mut best_max_diff: f32 = f32::MAX;
    let mut best_offset: usize = 0;
    for offset in 0..max_alignment.min(captured_snapshot.len().saturating_sub(snippet_len)) {
        let mut max_diff: f32 = 0.0;
        for i in 0..snippet_len {
            let diff = (captured_snapshot[offset + i] - reference_f32[i]).abs();
            max_diff = max_diff.max(diff);
            if max_diff > best_max_diff {
                break;
            }
        }
        if max_diff < best_max_diff {
            best_max_diff = max_diff;
            best_offset = offset;
        }
    }

    let offset_ms = best_offset as f64 / channels as f64 / sample_rate as f64 * 1000.0;
    info!(
        "Alignment: offset={} ({:.1}ms), max_diff={:.6}",
        best_offset, offset_ms, best_max_diff
    );

    // The streaming AVIO decoder's SwrContext f32 output can differ slightly from
    // decode_audio's i32→f32 path for the same audio data. This is an FFmpeg internal
    // difference, not a bug. The decoder-level test (test_cue_ape_track2_samples_match_xld_reference)
    // verifies exact sample correctness. Here we use a tolerance that catches the real
    // bug (wrong track audio, diffs > 0.1) while allowing f32 conversion noise.
    let tolerance = 0.01;

    assert!(
        best_max_diff < tolerance,
        "BUG: Could not align captured audio with XLD reference within 100ms.\n\
         Best offset {:.1}ms, max sample diff {:.6} (tolerance {:.6}).\n\
         the start-sample trim is likely not being applied during preload.",
        offset_ms,
        best_max_diff,
        tolerance,
    );

    // Compare 1 second at the aligned offset
    let compare_count = (sample_rate as usize * channels)
        .min(captured_snapshot.len() - best_offset)
        .min(reference_f32.len());

    let mut mismatches = 0;
    let mut max_diff: f32 = 0.0;
    let mut first_mismatch = None;
    for i in 0..compare_count {
        let actual = captured_snapshot[best_offset + i];
        let truth = reference_f32[i];
        let diff = (actual - truth).abs();
        if diff > tolerance {
            mismatches += 1;
            max_diff = max_diff.max(diff);
            if first_mismatch.is_none() {
                first_mismatch = Some((i, actual, truth));
            }
        }
    }

    if mismatches > 0 {
        let (idx, actual, truth) = first_mismatch.unwrap();
        let mismatch_ms = idx as f64 / channels as f64 / sample_rate as f64 * 1000.0;
        panic!(
            "AUDIO MISMATCH: {} of {} samples differ vs XLD reference!\n\
             First at index {} ({:.1}ms): actual={:.6}, reference={:.6}, max_diff={:.6}",
            mismatches, compare_count, idx, mismatch_ms, actual, truth, max_diff
        );
    }

    info!(
        "All {} samples match XLD reference after auto-advance (offset {:.1}ms).",
        compare_count, offset_ms,
    );
}

/// Next button must advance to track 2 and produce audio matching the XLD reference.
///
/// Tests the manual Next path (not auto-advance). After pressing Next, the captured
/// audio from track 2 must match the XLD-split reference.
#[tokio::test]
async fn test_cue_ape_next_track() {
    use bae_core::audio_codec::decode_audio;

    let mut fixture = CueApeTestFixture::with_capture()
        .await
        .unwrap_or_else(|e| panic!("failed to set up CUE/APE playback fixture: {e}"));

    let track1_id = fixture.track_ids[0].clone();
    let track2_id = fixture.track_ids[1].clone();

    fixture.playback_handle.play(track1_id.clone());
    // Drain track 1's play stream; the Next below mints a fresh one for track 2.
    let _track1_play = fixture.next_capture_stream().await;

    // Wait for track 1 to start
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut started = false;
    while Instant::now() < deadline && !started {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => {
                if let PlaybackState::Playing { track_info, .. } = &state {
                    if track_info.track_id == track1_id {
                        started = true;
                    }
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    assert!(started, "Track 1 should start playing");

    // Press Next — track 2's stream replaces track 1's.
    fixture.playback_handle.next();
    let captured = fixture.next_capture_stream().await;

    // Wait for track 2 to start
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut track2_started = false;
    while Instant::now() < deadline {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => {
                if let PlaybackState::Playing { track_info, .. } = &state {
                    if track_info.track_id == track2_id {
                        track2_started = true;
                        break;
                    }
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    assert!(track2_started, "Track 2 should start after Next");

    // Decode XLD reference
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cue_ape");
    let reference_data =
        std::fs::read(fixture_dir.join("02 Test Artist - Track Two.flac")).expect("read reference");
    let reference = decode_audio(&reference_data, None, None).expect("decode reference");
    let channels = reference.channels as usize;
    let sample_rate = reference.sample_rate;
    let reference_f32 = samples_as_f32(&reference);

    // Align and compare
    let snippet_len = 500 * channels;
    // APE frames are ~73728 samples (~1.7s). FFmpeg's seek lands at a frame
    // boundary, so the captured audio may start up to one frame before the
    // track's CUE start time. Search within 2s for alignment.
    let max_alignment = sample_rate as usize * channels * 2;

    let track2_sample_count = timeout(Duration::from_secs(10), async {
        loop {
            match fixture.progress_rx.recv().await {
                Some(PlaybackProgress::DecodeStats {
                    track_id,
                    samples_decoded,
                    ..
                }) if track_id == track2_id => break samples_decoded as usize,
                Some(_) => continue,
                None => panic!("Track 2 should emit decode stats"),
            }
        }
    })
    .await
    .expect("Track 2 should emit decode stats within 10s");
    assert!(
        track2_sample_count > snippet_len,
        "Track 2 should produce enough samples to compare: {} (needed more than {})",
        track2_sample_count,
        snippet_len,
    );
    assert!(
        reference_f32.len() >= track2_sample_count,
        "Track 2 XLD reference should cover decoded track: {} (needed {})",
        reference_f32.len(),
        track2_sample_count,
    );

    let captured_snapshot = bae_core::playback::wait_for_samples(
        &captured,
        track2_sample_count,
        Duration::from_secs(60),
    )
    .await;
    assert!(
        captured_snapshot.len() >= track2_sample_count,
        "Not enough captured samples after Next (60s): {} (needed {})",
        captured_snapshot.len(),
        track2_sample_count,
    );

    let track2_end = track2_sample_count;
    let alignment_limit = max_alignment.min(track2_end - snippet_len);

    let mut best_max_diff: f32 = f32::MAX;
    let mut best_offset: usize = 0;
    for offset in 0..=alignment_limit {
        let mut max_diff: f32 = 0.0;
        for i in 0..snippet_len {
            let diff = (captured_snapshot[offset + i] - reference_f32[i]).abs();
            max_diff = max_diff.max(diff);
            if max_diff > best_max_diff {
                break;
            }
        }
        if max_diff < best_max_diff {
            best_max_diff = max_diff;
            best_offset = offset;
        }
    }

    let offset_ms = best_offset as f64 / channels as f64 / sample_rate as f64 * 1000.0;

    assert!(
        best_max_diff < 0.01,
        "Could not align track 2 audio after Next with XLD reference.\n\
         Best offset {:.1}ms, max sample diff {:.6}",
        offset_ms,
        best_max_diff,
    );

    let compare_count = track2_end - best_offset;

    for i in 0..compare_count {
        let diff = (captured_snapshot[best_offset + i] - reference_f32[i]).abs();
        assert!(
            diff < 0.01,
            "AUDIO MISMATCH after Next at index {} ({:.1}ms)",
            i,
            i as f64 / channels as f64 / sample_rate as f64 * 1000.0,
        );
    }

    info!(
        "Track 2 after Next matches XLD reference ({} samples, offset {:.1}ms).",
        compare_count, offset_ms,
    );
}

/// Import a two-disc CUE/APE release under the given storage mode and assert
/// each track's main audio segment resolves to its own disc's APE file
/// (identified by the relative path `CD{N}/CDImage.ape`).
///
/// This exercises the bare-filename collision regression in every code path:
/// bytes-never-copied (Local), bytes-uploaded-and-pinned (Remote + pin),
/// and bytes-uploaded-cloud-only (Remote, no pin). Pin is an orthogonal coven
/// cache choice, so it rides alongside the `StorageMode` as its own argument.
async fn assert_multi_disc_cue_ape_per_disc_mapping(storage_mode: StorageMode, pin: bool) {
    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let album_dir = temp_root.path().join("album");
    let cd1_dir = album_dir.join("CD1");
    let cd2_dir = album_dir.join("CD2");
    let db_dir = temp_root.path().join("db");
    std::fs::create_dir_all(&cd1_dir).expect("cd1 dir");
    std::fs::create_dir_all(&cd2_dir).expect("cd2 dir");
    std::fs::create_dir_all(&db_dir).expect("db dir");

    // Reuse the 90s / 3-track CUE+APE fixture, once per disc, renamed to
    // `CDImage.*` so both discs share the same bare filename (a common
    // multi-disc rip layout).
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cue_ape");
    let ape_bytes = std::fs::read(fixture_dir.join("Test Album.ape")).expect("read ape fixture");
    let cue_body = "PERFORMER \"Test Artist\"\n\
                    TITLE \"Disc Title\"\n\
                    FILE \"CDImage.ape\" WAVE\n  \
                    TRACK 01 AUDIO\n    TITLE \"Track One\"\n    INDEX 01 00:00:00\n  \
                    TRACK 02 AUDIO\n    TITLE \"Track Two\"\n    INDEX 01 00:30:00\n  \
                    TRACK 03 AUDIO\n    TITLE \"Track Three\"\n    INDEX 01 01:00:00\n";
    for dir in [&cd1_dir, &cd2_dir] {
        std::fs::write(dir.join("CDImage.ape"), &ape_bytes).expect("write ape");
        std::fs::write(dir.join("CDImage.cue"), cue_body).expect("write cue");
    }

    let library_dir = StoreDir::new(db_dir.clone());
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::open(
        config_handle,
        key_service,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        tokio::runtime::Handle::current(),
        None,
    )
    .expect("open library manager");
    if storage_mode == StorageMode::Remote {
        library_manager
            .connect_test_cloud_home(
                Arc::new(InMemoryCloudHome::new()),
                CloudCipher::Encrypted(EncryptionService::from_key([7u8; 32])),
            )
            .await
            .expect("connect in-memory cloud home");
        assert!(
            library_manager.is_sync_ready(),
            "remote import test cloud should start sync"
        );
    }

    // Discogs multi-disc tracklist: positions "1-1".."1-3", "2-1".."2-3".
    // `parse_side_from_position` maps these to side=1 and side=2.
    let discogs_release = DiscogsRelease {
        id: "test-multi-disc-cue-ape".to_string(),
        title: "Multi-Disc Album".to_string(),
        year: Some(2024),
        format: vec![],
        country: None,
        label: vec![],
        cover_image: None,
        thumb: None,
        catno: None,
        artists: vec![DiscogsArtist {
            id: "discogs-artist-1".to_string(),
            name: "Artist Name".to_string(),
        }],
        extraartists: Some(vec![]),
        tracklist: (1..=2)
            .flat_map(|disc| {
                (1..=3).map(move |n| DiscogsTrack {
                    type_: "track".to_string(),
                    position: format!("{}-{}", disc, n),
                    title: format!("Disc {} Track {}", disc, n),
                    duration: Some("0:30".to_string()),
                    artists: vec![],
                    extraartists: None,
                })
            })
            .collect(),
        master_id: None,
    };
    let release_id_key = seed_discogs_test_release(discogs_release);
    let import_handle =
        start_test_import(tokio::runtime::Handle::current(), library_manager.clone());

    let import_id = uuid::Uuid::new_v4().to_string();
    import_handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            selected_cover: None,
            storage_mode,
            pin,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .expect("send command");

    let mut progress_rx = import_handle.subscribe_import(import_id);
    let (release_id, _) =
        wait_for_multi_disc_cue_ape_import_ready(&library_manager, storage_mode, &mut progress_rx)
            .await;

    let tracks = library_manager
        .get_tracks_for_release(&release_id)
        .await
        .expect("get tracks");
    assert_eq!(tracks.len(), 6, "should have 6 tracks (2 discs × 3 tracks)");

    // `original_filename` in release_files must be the full relative path
    // (`CD1/CDImage.ape`), not the bare filename — otherwise the two discs'
    // APE rows would collide on the same `"CDImage.ape"` string.
    let files = library_manager
        .get_files_for_release(&release_id)
        .await
        .expect("get files");
    let mut ape_filenames: Vec<String> = files
        .iter()
        .filter(|f| f.original_filename.ends_with(".ape"))
        .map(|f| f.original_filename.clone())
        .collect();
    ape_filenames.sort();
    assert_eq!(
        ape_filenames,
        vec!["CD1/CDImage.ape".to_string(), "CD2/CDImage.ape".to_string()],
        "release_files must store relative paths for both discs, not bare filenames"
    );

    // Each track's main audio segment must point to its own disc's APE.
    let filename_by_id: std::collections::HashMap<String, String> = files
        .iter()
        .map(|f| (f.id.clone(), f.original_filename.clone()))
        .collect();

    for track in &tracks {
        let resolved = library_manager
            .resolve_track_audio(&track.id)
            .await
            .expect("resolve track audio");
        let segment = resolved
            .segments
            .iter()
            .find(|segment| segment.role == bae_core::db::DbAudioSegmentRole::Main)
            .unwrap_or_else(|| panic!("track {} has no main audio segment", track.title));
        let filename = filename_by_id
            .get(&segment.file_id)
            .unwrap_or_else(|| panic!("file_id {} not in release_files", segment.file_id));

        let expected = match track.side {
            1 => "CD1/CDImage.ape",
            2 => "CD2/CDImage.ape",
            other => panic!("unexpected side {other} for track {}", track.title),
        };
        assert_eq!(
            filename, expected,
            "track '{}' (side {}) should point at {expected}, got {filename}",
            track.title, track.side,
        );
    }
}

async fn wait_for_multi_disc_cue_ape_import_ready(
    library_manager: &LibraryManager,
    storage_mode: StorageMode,
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<bae_core::import::ImportProgress>,
) -> (String, String) {
    match storage_mode {
        StorageMode::Local => wait_for_import_complete(progress_rx).await,
        StorageMode::Remote => {
            let (release_id, album_id) = wait_for_remote_upload_queued(progress_rx).await;
            timeout(Duration::from_secs(10), async {
                loop {
                    library_manager
                        .drain_uploads_for_test()
                        .await
                        .expect("drain queued remote import uploads");
                    let release = library_manager
                        .get_release_by_id(&release_id)
                        .await
                        .expect("find imported release")
                        .expect("imported release exists");
                    let pending = library_manager
                        .count_pending_uploads_for_release(&release_id)
                        .await
                        .expect("count pending release uploads");
                    if release.remote && pending == 0 {
                        break;
                    }
                    sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("remote multi-disc CUE/APE import uploads all files");
            let files = library_manager
                .get_files_for_release(&release_id)
                .await
                .expect("get release files");
            assert_eq!(
                files.len(),
                4,
                "remote multi-disc CUE/APE import keeps both audio files and both CUE sheets"
            );
            (release_id, album_id)
        }
    }
}

async fn wait_for_remote_upload_queued(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<bae_core::import::ImportProgress>,
) -> (String, String) {
    while let Some(progress) = progress_rx.recv().await {
        match progress {
            bae_core::import::ImportProgress::RemoteUploadQueued { id, album_id, .. } => {
                return (id, album_id)
            }
            bae_core::import::ImportProgress::Failed { error, .. } => {
                panic!("Import failed: {}", error);
            }
            _ => {}
        }
    }
    panic!("Progress channel closed without remote upload being queued");
}

/// Regression: multi-disc CUE/APE imports must link each track to its OWN disc's
/// audio file, not collapse every track onto the last disc's file.
///
/// Bug: `file_ids` was keyed by bare filename (`CDImage.ape`), so CD1's entry
/// was overwritten by CD2's. Every `audio_formats.file_id` ended up pointing at
/// CD2's APE, so playing any disc 1 track decoded disc 2's audio.
///
/// The same structural bug lived in all three byte-placement strategies, so we
/// cover each explicitly to prevent a regression on any one path.
#[tokio::test]
async fn test_multi_disc_cue_ape_local() {
    assert_multi_disc_cue_ape_per_disc_mapping(StorageMode::Local, false).await;
}

#[tokio::test]
async fn test_multi_disc_cue_ape_remote_pin() {
    assert_multi_disc_cue_ape_per_disc_mapping(StorageMode::Remote, true).await;
}

#[tokio::test]
async fn test_multi_disc_cue_ape_remote_unpin() {
    assert_multi_disc_cue_ape_per_disc_mapping(StorageMode::Remote, false).await;
}
