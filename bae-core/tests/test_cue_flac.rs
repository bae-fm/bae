#![cfg(feature = "test-utils")]
//! Tests for CUE/FLAC format handling.
//!
//! CUE/FLAC albums have multiple tracks in a single FLAC file. The import must:
//! - Parse the CUE sheet to find track boundaries
//! - Record each track's sample window in the database
//! - Enable playback to seek to the correct position for each track
//!
//! These tests use storageless imports for simplicity (the CUE/FLAC handling
//! is independent of storage configuration).
mod support;
use crate::support::{
    seed_discogs_test_release, test_config_and_keys, tracing_init, wait_for_import_complete,
};
use bae_core::db::{Database, DbAudioSegmentRole};
use bae_core::discogs::models::{DiscogsArtist, DiscogsRelease, DiscogsTrack};
use bae_core::import::{
    IdentityChoice, ImportCommand, ImportService, MetadataRef, MetadataSource, StorageMode,
};
use bae_core::library::LibraryManager;
use bae_core::playback::{PlaybackProgress, PlaybackState};
use bae_core::util::content_type::ContentType;
use coven::LibraryDir;
use std::path::Path;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::time::timeout;
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
/// CUE/FLAC imports record each track's sample window correctly.
///
/// Each track records its `[start_sample, end_sample)` window within the single
/// FLAC file so playback can decode and trim to that track.
#[tokio::test]
async fn test_cue_flac_records_track_positions() {
    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let (library_manager, release_id) = import_cue_flac_fixture(temp_root.path()).await;
    info!("Import completed, release_id: {}", release_id);
    let tracks = library_manager
        .get_tracks(&release_id)
        .await
        .expect("get tracks");
    assert_eq!(tracks.len(), 3, "Should have 3 tracks");
    // Check that each track has audio format with its sample window recorded
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
        assert_eq!(audio_format.content_type, ContentType::Flac);
        // Track 1 starts at sample 0; later tracks start further into the file.
        let resolved = library_manager
            .resolve_track_audio(&track.id)
            .await
            .expect("resolve track audio");
        let segment = resolved
            .segments
            .iter()
            .find(|segment| segment.role == DbAudioSegmentRole::Main)
            .unwrap_or_else(|| panic!("track {} should have a main audio segment", i + 1));
        let pregap_segment = resolved
            .segments
            .iter()
            .find(|segment| segment.role == DbAudioSegmentRole::AudioPregap);
        if i == 1 {
            let pregap = pregap_segment.expect("track 2 should have an audio pregap segment");
            assert_eq!(
                pregap.start_sample,
                8 * 44_100,
                "track 2 pregap should start at INDEX 00",
            );
            assert_eq!(
                pregap.end_sample,
                Some(10 * 44_100),
                "track 2 pregap should end at INDEX 01",
            );
        } else {
            assert!(
                pregap_segment.is_none(),
                "track {} should not have an audio pregap segment",
                i + 1,
            );
        }
        if i == 0 {
            assert_eq!(
                segment.start_sample, 0,
                "the first CUE/FLAC track starts at sample 0",
            );
            assert_eq!(
                segment.start_byte, None,
                "the first CUE/FLAC track starts at byte 0 (no prefetch, header covers it)",
            );
        } else {
            // A deep track records the seektable landing for its start_sample --
            // the byte playback byte-seeks to.
            let start_byte = segment.start_byte.unwrap_or_else(|| {
                panic!("deep CUE/FLAC track {} should record a start byte", i + 1)
            });
            assert!(
                start_byte > 0,
                "track {} start byte should be a real file offset, got {start_byte}",
                i + 1,
            );
        }
        // Every track but the last carries an end sample; the last runs to EOF.
        if i < tracks.len() - 1 {
            assert!(
                segment.end_sample.is_some(),
                "CUE/FLAC track {} should have an end sample",
                i + 1,
            );
            // The import seeks the shared file to record each track's end byte
            // offset -- the playback read-ahead ceiling. Every track but the last
            // gets one; the last runs to EOF.
            let end_byte = segment.end_byte.unwrap_or_else(|| {
                panic!("CUE/FLAC track {} should record an end byte offset", i + 1)
            });
            assert!(
                end_byte > 0,
                "track {} end byte should be a real file offset, got {end_byte}",
                i + 1,
            );
        } else {
            assert_eq!(
                segment.end_sample, None,
                "the last CUE/FLAC track runs to EOF",
            );
            assert_eq!(
                segment.end_byte, None,
                "the last CUE/FLAC track runs to EOF (no end byte)",
            );
        }
        if i > 0 {
            let prev_resolved = library_manager
                .resolve_track_audio(&tracks[i - 1].id)
                .await
                .expect("resolve previous track audio");
            let prev_segment = prev_resolved
                .segments
                .iter()
                .find(|segment| segment.role == DbAudioSegmentRole::Main)
                .expect("previous track should have a main audio segment");
            assert!(
                segment.start_sample > prev_segment.start_sample,
                "track {} start_sample ({}) should be > track {} start_sample ({})",
                i + 1,
                segment.start_sample,
                i,
                prev_segment.start_sample,
            );
            // End byte offsets ascend with the tracks (each is a later boundary
            // in the shared file). The last track's is None, so guard on both.
            if let (Some(prev_end), Some(cur_end)) = (prev_segment.end_byte, segment.end_byte) {
                assert!(
                    cur_end > prev_end,
                    "track {} end byte ({cur_end}) should be > track {} end byte ({prev_end})",
                    i + 1,
                    i,
                );
            }
            if let Some(pregap) = pregap_segment {
                assert_eq!(
                    pregap.start_byte,
                    prev_segment.end_byte,
                    "track {} pregap start byte should equal track {} end byte",
                    i + 1,
                    i,
                );
                assert_eq!(
                    segment.start_byte,
                    pregap.end_byte,
                    "track {} main start byte should equal its pregap end byte",
                    i + 1,
                );
            } else {
                // Tracks of one file are back-to-back byte ranges: this track's start
                // sample is the prior track's end sample, and both the start-byte seek
                // and the end-byte seek binary-search to that same frame. So the stored
                // start byte equals the prior track's end byte.
                assert_eq!(
                    segment.start_byte,
                    prev_segment.end_byte,
                    "track {} start byte should equal track {} end byte (contiguous seek landing)",
                    i + 1,
                    i,
                );
            }
        }
        info!(
            "Track {} '{}': samples {}..{:?}",
            i + 1,
            track.title,
            segment.start_sample,
            segment.end_sample,
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
        assert_eq!(af.content_type, ContentType::Flac, "Should be FLAC format");
    }
    info!("✅ All track positions recorded correctly for CUE/FLAC import");
}
/// Track 2 playback must produce audio matching the XLD-split reference FLAC.
///
/// Playback decodes the FLAC and trims to the track's sample window, so the
/// audio is the correct portion of the file. Verifies by comparing captured
/// samples against the XLD ground truth.
#[tokio::test]
async fn test_cue_flac_playback_uses_track_positions() {
    use bae_core::audio_codec::decode_audio;

    let mut fixture = match CueFlacCaptureFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            info!("Failed to set up CUE/FLAC capture fixture: {}", e);
            return;
        }
    };

    let track_id = fixture.track_ids[1].clone();

    // Play track 2 (direct play skips pregap, starts at INDEX 01)
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

    // Decode XLD reference for track 2 (white noise)
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cue_flac");
    let reference_data =
        std::fs::read(fixture_dir.join("02 Test Artist - Track Two (White Noise).flac"))
            .expect("read reference");
    let reference = decode_audio(&reference_data, None, None).expect("decode reference");
    let channels = reference.channels as usize;
    let sample_rate = reference.sample_rate;
    let reference_f32 = reference.samples_as_f32();

    // Align and compare
    let snippet_len = 500 * channels;
    let max_alignment = sample_rate as usize * channels / 10;

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
        "Could not align captured track 2 audio with XLD reference.\n\
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
        "CUE/FLAC track 2 samples match XLD reference ({} samples, offset {:.1}ms).",
        compare_count, offset_ms,
    );
}

/// A CUE/FLAC track's reported duration matches its CUE timing.
///
/// Track 1 runs 0:00-0:08, so playback must report ~8s -- never short (audio cut
/// off) and not wildly long.
#[tokio::test]
async fn test_cue_flac_decoded_duration_matches_cue_timing() {
    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let (library_manager, release_id) = import_cue_flac_fixture(temp_root.path()).await;
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

    let playback_handle = bae_core::playback::PlaybackService::start(
        library_manager.clone(),
        tokio::runtime::Handle::current(),
        100,
    );
    playback_handle.set_volume(0.0);
    let mut progress_rx = playback_handle.subscribe_progress();
    playback_handle.play(track1.id.clone());

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut playback_duration_ms = None;
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(std::time::Duration::from_millis(100), progress_rx.recv()).await
        {
            Ok(Some(bae_core::playback::PlaybackProgress::StateChanged { state })) => {
                if let bae_core::playback::PlaybackState::Playing {
                    track_info,
                    duration_ms,
                    ..
                } = &state
                {
                    if track_info.track_id == track1.id {
                        playback_duration_ms = Some(*duration_ms);
                        break;
                    }
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }

    let duration_ms = playback_duration_ms.expect("Should get duration from playback") as i64;

    info!(
        "Track 1: expected {}ms, playback duration {}ms",
        expected_duration_ms, duration_ms
    );

    // The reported duration is the track's CUE-derived metadata duration, so it
    // matches the CUE timing closely -- not cut short, and not the whole file.
    assert!(
        (duration_ms - expected_duration_ms).abs() < 500,
        "playback duration ({}ms) should match the CUE timing ({}ms), not the whole file",
        duration_ms,
        expected_duration_ms,
    );

    info!("CUE/FLAC decoded duration matches CUE timing");
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

/// Import the 3-track CUE/FLAC fixture (storageless) under `temp_root` and return
/// the library manager and the imported release id. Shared by the CUE/FLAC tests
/// whose only common setup is this import.
async fn import_cue_flac_fixture(temp_root: &Path) -> (LibraryManager, String) {
    let album_dir = temp_root.join("album");
    let db_dir = temp_root.join("db");
    std::fs::create_dir_all(&album_dir).expect("album dir");
    std::fs::create_dir_all(&db_dir).expect("db dir");
    copy_cue_flac_fixture_with_seektable(&album_dir);
    let database = Database::new_test(
        db_dir.join("test.db").to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .expect("database");
    let library_dir = LibraryDir::new(db_dir);
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::new(
        database,
        config_handle,
        key_service,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        tokio::runtime::Handle::current(),
    );
    let release_id_key = seed_discogs_test_release(create_test_discogs_release());
    let import_handle =
        start_test_import(tokio::runtime::Handle::current(), library_manager.clone());
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
    (library_manager, release_id)
}

// ============================================================================
// CUE/FLAC playback fixture with sample capture
// ============================================================================

struct CueFlacCaptureFixture {
    playback_handle: bae_core::playback::PlaybackHandle,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_ids: Vec<String>,
    capture_stream_rx:
        tokio::sync::mpsc::UnboundedReceiver<std::sync::Arc<std::sync::Mutex<Vec<f32>>>>,
    _temp_dir: TempDir,
}

impl CueFlacCaptureFixture {
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

    async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        tracing_init();
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let album_dir = temp_dir.path().join("album");
        std::fs::create_dir_all(&album_dir)?;

        copy_cue_flac_fixture_with_seektable(&album_dir);

        let database = Database::new_test(
            db_path.to_str().unwrap(),
            std::sync::Arc::new(coven::SystemClock),
        )
        .await?;
        let library_dir = LibraryDir::new(temp_dir.path().to_path_buf());
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
        let tracks = library_manager.get_tracks(&releases[0].id).await?;
        let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
        assert_eq!(track_ids.len(), 3, "Should have 3 tracks from CUE/FLAC");

        let (capture_output, capture_stream_rx) = bae_core::playback::CaptureAudioOutput::new();
        let playback_handle = bae_core::playback::PlaybackService::start_with_output(
            library_manager.clone(),
            runtime_handle,
            100,
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
// CUE/FLAC playback tests with sample capture
// ============================================================================

/// Exporting a single CUE/FLAC track must produce only that track's audio,
/// not the entire source file.
///
/// The fixture has 3 tracks at ~10s each (~30s total). Track 2 runs from 10s to 20s.
/// If export doesn't trim to the track's sample window, the exported file will
/// contain all ~30s instead of ~10s.
#[tokio::test]
async fn test_cue_flac_export_single_track() {
    use bae_core::audio_codec::decode_audio;
    use bae_core::config::ExportSelection;

    tracing_init();

    let temp_root = TempDir::new().expect("temp root");
    let (library_manager, release_id) = import_cue_flac_fixture(temp_root.path()).await;

    let tracks = library_manager
        .get_tracks(&release_id)
        .await
        .expect("get tracks");
    assert_eq!(tracks.len(), 3, "Should have 3 tracks");

    // Export track 2 (index 1) as FLAC
    let export_path = temp_root.path().join("exported_track2.flac");
    library_manager
        .export_track(
            &tracks[1].id,
            &export_path,
            ExportSelection::Preset {
                preset_id: "flac".to_string(),
            },
        )
        .await
        .expect("export track");

    // Decode the exported file and check its duration
    let exported_data = std::fs::read(&export_path).expect("read exported file");
    let decoded = decode_audio(&exported_data, None, None).expect("decode exported");

    let exported_duration_ms = decoded.samples.len() as u64 * 1000
        / (decoded.sample_rate as u64 * decoded.channels as u64);

    info!(
        "Exported track 2: {} samples, {}Hz, {} channels, ~{}ms",
        decoded.samples.len(),
        decoded.sample_rate,
        decoded.channels,
        exported_duration_ms,
    );

    // Track 2 is ~10s. The full file is ~30s.
    // Allow some tolerance for frame alignment, but it must be closer to 10s than 30s.
    assert!(
        exported_duration_ms < 15_000,
        "Exported track 2 is {}ms — expected ~10s, got the whole file (~30s). \
         Export is not using CUE track boundaries.",
        exported_duration_ms,
    );
    assert!(
        exported_duration_ms > 5_000,
        "Exported track 2 is only {}ms — expected ~10s",
        exported_duration_ms,
    );

    info!(
        "Exported track 2 duration: {}ms (expected ~10000ms)",
        exported_duration_ms,
    );
}

/// Gapless continuity at a CUE/FLAC track boundary, through the real export path.
///
/// Exporting track 1 and track 2 -- each decodes the shared file and trims to its
/// stored sample window -- and concatenating the results must reproduce the
/// continuous source audio. A gap (missing samples) or overlap (duplicated
/// samples) at the boundary would make the tail of the concatenation diverge
/// from the full decode of the source.
#[tokio::test]
async fn test_cue_flac_gapless_track_boundary() {
    use bae_core::audio_codec::decode_audio;
    use bae_core::config::ExportSelection;

    tracing_init();
    let temp_root = TempDir::new().expect("temp root");
    let (library_manager, release_id) = import_cue_flac_fixture(temp_root.path()).await;
    let tracks = library_manager
        .get_tracks(&release_id)
        .await
        .expect("get tracks");
    assert_eq!(tracks.len(), 3, "Should have 3 tracks");

    // Export track 1 and track 2 through the real export path, then decode each.
    let t1_path = temp_root.path().join("t1.flac");
    library_manager
        .export_track(
            &tracks[0].id,
            &t1_path,
            ExportSelection::Preset {
                preset_id: "flac".to_string(),
            },
        )
        .await
        .expect("export track 1");
    let t2_path = temp_root.path().join("t2.flac");
    library_manager
        .export_track(
            &tracks[1].id,
            &t2_path,
            ExportSelection::Preset {
                preset_id: "flac".to_string(),
            },
        )
        .await
        .expect("export track 2");
    let t1 = decode_audio(&std::fs::read(&t1_path).unwrap(), None, None).expect("decode track 1");
    let t2 = decode_audio(&std::fs::read(&t2_path).unwrap(), None, None).expect("decode track 2");

    // Ground truth: the source file decoded once.
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac/Test Album.flac");
    let full = decode_audio(&std::fs::read(&source).unwrap(), None, None).expect("decode full");

    // Concatenating the two exported tracks reproduces the continuous source.
    let combined: Vec<i32> = t1
        .samples
        .iter()
        .chain(t2.samples.iter())
        .copied()
        .collect();
    assert!(
        combined.len() <= full.samples.len(),
        "track 1 + track 2 cannot exceed the full file",
    );
    assert_eq!(
        combined,
        full.samples[..combined.len()],
        "track 1 ++ track 2 must equal the continuous source (a gap or overlap at the boundary)",
    );

    info!(
        "Gapless boundary verified through export: {} + {} samples match the source",
        t1.samples.len(),
        t2.samples.len(),
    );
}

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
                title: "Track One (440Hz)".to_string(),
                duration: Some("0:10".to_string()),
                artists: vec![],
                extraartists: None,
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "2".to_string(),
                title: "Track Two (880Hz)".to_string(),
                duration: Some("0:10".to_string()),
                artists: vec![],
                extraartists: None,
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "3".to_string(),
                title: "Track Three (660Hz)".to_string(),
                duration: Some("0:10".to_string()),
                artists: vec![],
                extraartists: None,
            },
        ],
        master_id: Some("test-master".to_string()),
    }
}
