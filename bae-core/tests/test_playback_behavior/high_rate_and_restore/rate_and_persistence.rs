/// Test fixture for high sample rate (96kHz) FLAC playback.
/// This catches bugs where the playback pipeline assumes 44.1kHz.
struct HighSampleRateTestFixture {
    playback_handle: bae_core::playback::PlaybackHandle,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_id: String,
    _capture_stream_rx: CaptureStreamRx,
    _temp_dir: TempDir,
}

impl HighSampleRateTestFixture {
    async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        tracing_init();
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let album_dir = temp_dir.path().join("album");
        std::fs::create_dir_all(&album_dir)?;

        let database = Database::new_test(
            db_path.to_str().unwrap(),
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await?;
        let database_arc = Arc::new(database.clone());
        let library_dir = StoreDir::new(temp_dir.path().to_path_buf());
        let config_handle = test_config(&library_dir);
        let library_manager = LibraryManager::new(
            (*database_arc).clone(),
            config_handle,
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
            bae_core::diagnostics::Diagnostics::noop(),
            tokio::runtime::Handle::current(),
            bae_core::import::cover_art::RemoteImageCache::for_test(),
        );
        let runtime_handle = tokio::runtime::Handle::current();

        // Copy 96kHz fixture
        let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("flac")
            .join("96khz_test.flac");
        let test_path = album_dir.join("01 96kHz Track.flac");
        std::fs::copy(&fixture_path, &test_path).unwrap_or_else(|_| {
            panic!(
                "96kHz FLAC fixture not found: {}\n\
                 Run: ./scripts/generate_high_sample_rate_flac.sh",
                fixture_path.display()
            );
        });

        // Create release with one track
        let discogs_release = DiscogsRelease {
            id: "high-sample-rate-test".to_string(),
            title: "96kHz Test Album".to_string(),
            year: Some(2024),
            format: vec![],
            country: Some("US".to_string()),
            label: vec!["Test Label".to_string()],
            cover_image: None,
            thumb: None,
            catno: None,
            artists: vec![DiscogsArtist {
                name: "Test Artist".to_string(),
                id: "test-artist-1".to_string(),
            }],
            extraartists: Some(vec![]),
            tracklist: vec![DiscogsTrack {
                type_: "track".to_string(),
                position: "1".to_string(),
                title: "96kHz Track".to_string(),
                duration: Some("0:03".to_string()),
                artists: vec![],
                extraartists: None,
            }],
            master_id: Some("test-master-96khz".to_string()),
        };
        let release_id_key = seed_discogs_test_release(discogs_release);

        let import_handle =
            start_test_import(runtime_handle.clone(), library_manager.clone()).await;

        let import_id = uuid::Uuid::new_v4().to_string();
        import_handle
            .send_command(ImportCommand {
                import_id: import_id.clone(),
                candidate_key: "test".to_string(),
                folder: album_dir.clone(),
                scope: bae_core::import::ReleaseFileScope::Recursive,
                selected_cover: None,
                storage_mode: StorageMode::Local,
                pin: false,
                identity_choice: IdentityChoice::Release {
                    release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
                },
                user_edit: None,
            })
            .await
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
        let mut progress_rx = import_handle.subscribe_import(import_id);
        try_wait_for_import_complete(&mut progress_rx)
            .await
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

        let albums = library_manager.get_albums(&[]).await?;
        let releases = library_manager
            .get_releases_for_album(&albums[0].id)
            .await?;
        let tracks = library_manager
            .get_tracks_for_release(&releases[0].id)
            .await?;
        let track_id = tracks[0].id.clone();

        // Verify the audio format was correctly detected as 96kHz
        let audio_format = library_manager
            .get_audio_format_by_track_id(&track_id)
            .await?
            .expect("Audio format should be detected for 96kHz track");
        assert_eq!(
            audio_format.sample_rate, 96000,
            "Import should detect 96kHz sample rate, got {}",
            audio_format.sample_rate
        );

        let (playback_handle, capture_stream_rx) =
            start_capture_service(library_manager.clone(), runtime_handle);
        let progress_rx = playback_handle.subscribe_progress();

        Ok(Self {
            playback_handle,
            progress_rx,
            track_id,
            _capture_stream_rx: capture_stream_rx,
            _temp_dir: temp_dir,
        })
    }
}

/// Test that high sample rate (96kHz) FLAC files report correct position/duration.
///
/// Bug: `create_track_stream_pair(44100, 2)` is hardcoded, ignoring the actual
/// sample rate from the audio file. This causes position calculation to be wrong:
/// - 96kHz track produces 96000 samples/sec
/// - Position calculates as `samples / 44100` instead of `samples / 96000`
/// - A 3-second track appears to be ~6.5 seconds long
///
/// This test verifies that a 3-second 96kHz track completes with position ~3s,
/// not ~6.5s.
#[tokio::test]
async fn test_high_sample_rate_position_calculation() {
    let mut fixture = HighSampleRateTestFixture::new()
        .await
        .expect("set up high sample rate fixture");

    // Play the 96kHz track (3 seconds duration)
    fixture.playback_handle.play(fixture.track_id.clone());

    // Wait for track to complete and capture final position
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut final_position_ms: Option<u64> = None;
    let mut track_completed = false;

    while Instant::now() < deadline {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::PositionUpdate { position_ms, .. })) => {
                final_position_ms = Some(position_ms);
            }
            Ok(Some(PlaybackProgress::TrackCompleted { .. })) => {
                track_completed = true;
                // Wait a bit for any final position update
                tokio::time::sleep(Duration::from_millis(100)).await;
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }

    assert!(track_completed, "Track should complete");

    let position_ms = final_position_ms.expect("Should have received position updates");
    debug!("Final position at track completion: {}ms", position_ms);

    // The track is 3 seconds at 96kHz. With the bug (44.1kHz assumed), position
    // would show ~6.5 seconds (3 * 96000 / 44100 = 6.53).
    // With correct sample rate, position should be ~3 seconds.
    let position_secs = position_ms as f64 / 1000.0;

    assert!(
        position_secs < 5.0,
        "96kHz track position calculation is wrong: final position {:.2}s exceeds 5s. \
         Expected ~3s for a 3-second track. This indicates the streaming source is using \
         hardcoded 44.1kHz sample rate instead of the track's 96kHz. \
         (Position = samples / wrong_rate = frames / 44100 instead of frames / 96000)",
        position_secs
    );

    assert!(
        position_secs >= 2.5,
        "96kHz track position too low: {:.2}s (expected ~3s)",
        position_secs
    );
}

// ============================================================================
// Sample offset tests (frame-accurate seeking)
// ============================================================================

/// Test that seeking via seek_to lands near the correct position.
///
/// The seek implementation uses avformat_seek_file which lands on the nearest
/// keyframe AT or BEFORE the target. For FLAC, this means the decoder starts
/// at the frame boundary before the target and decodes from there.
///
/// This test:
/// 1. Plays a track and seeks to 2.5 seconds
/// 2. Checks samples_decoded after completion
/// 3. Expected: approximately 2.5s worth of samples, plus up to one FLAC frame
///    (typically 4096 samples at 44.1kHz) from the keyframe alignment
#[tokio::test]
async fn test_seek_lands_near_target_position() {
    let mut fixture = PlaybackTestFixture::new().await;

    let track_id = fixture.track_ids[0].clone();

    // Play the track (5 seconds at 44.1kHz mono = 220500 total samples)
    fixture.playback_handle.play(track_id.clone());

    // Wait for playback to start
    let playing_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;
    assert!(playing_state.is_some(), "Should start playing");

    // Seek to 2.5 seconds
    let seek_position = Duration::from_millis(2500);
    fixture.playback_handle.seek(seek_position);

    // Wait for seek to complete
    let seeked = fixture.wait_for_seeked(Duration::from_secs(3)).await;
    assert!(seeked.is_some(), "Should receive Seeked event");

    // Wait for track to complete and get decode stats
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut decode_stats: Option<(u32, u64)> = None;

    while Instant::now() < deadline {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::DecodeStats {
                error_count,
                samples_decoded,
                track_id: stats_track_id,
            })) => {
                if stats_track_id == track_id {
                    decode_stats = Some((error_count, samples_decoded));
                    break;
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }

    let (_error_count, samples_decoded) =
        decode_stats.expect("Should receive DecodeStats after track completes");

    // Track is 5 seconds at 44.1kHz mono = 220500 total samples
    // After seeking to 2.5s, remaining = 2.5s = 110250 samples
    //
    // avformat_seek_file lands on the keyframe at or before the target,
    // so decoded output may include up to one FLAC frame of extra samples
    // (typically 4096 samples). The exact amount depends on frame alignment.
    let expected_min: u64 = 110250; // Exact 2.5s at 44.1kHz mono
    let max_frame_overshoot: u64 = 4608; // Max FLAC blocksize at 44.1kHz

    assert!(
        samples_decoded >= expected_min && samples_decoded <= expected_min + max_frame_overshoot,
        "After seeking to 2.5s in a 5s track, expected {}..{} samples but got {}.",
        expected_min,
        expected_min + max_frame_overshoot,
        samples_decoded,
    );
}

/// CUE/FLAC seek must not play past the track's end boundary.
///
/// After seeking to 5s in track 2, the captured audio must match the reference
/// from 5s onward AND must stop at track 2's end (not bleed into track 3).
/// The captured sample count should correspond to roughly the remaining duration.
#[tokio::test]
async fn test_cue_flac_seek_respects_track_end_boundary() {
    use bae_core::audio_codec::decode_audio;

    // Real-time capture: at full speed track 2's decoder finishes the whole
    // track before the seek restarts it at 5s, so its sample count overshoots
    // the boundary (flaky under load). Real-time keeps the decoder on the seek.
    let mut fixture = CueFlacTestFixture::with_realtime_capture()
        .await
        .expect("set up CUE/FLAC realtime capture fixture");

    let track_id = fixture.track_ids[1].clone();

    fixture.playback_handle.play(track_id.clone());
    // Drain the play stream; the seek below mints a fresh one. Track 2 then
    // advances gaplessly into track 3 within that same seek stream, so we assert
    // on the decoder's own sample count rather than the captured buffer length.
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

    // Seek to 5s into track 2
    let seek_position_ms: u64 = 5000;
    fixture
        .playback_handle
        .seek(Duration::from_millis(seek_position_ms));
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

    // Wait for the track's decode stats. Under gapless playback the seeked track
    // advances into the next track within one persistent stream, so it reports
    // its decode stats at the boundary rather than via TrackCompleted.
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut decoded_samples: Option<u64> = None;
    while Instant::now() < deadline && decoded_samples.is_none() {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::DecodeStats {
                track_id: ref tid,
                samples_decoded,
                ..
            })) => {
                if *tid == track_id {
                    decoded_samples = Some(samples_decoded);
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    let decoded_samples = decoded_samples.expect("Track 2 should report decode stats");

    let captured_snapshot: Vec<f32> = captured.lock().unwrap().clone();

    // Decode reference and compare at the seek position
    let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cue_flac");
    let reference_data =
        std::fs::read(fixture_dir.join("02 Test Artist - Track Two (White Noise).flac"))
            .expect("read reference");
    let reference =
        decode_audio(buffer_from(&reference_data), None, None).expect("decode reference");
    let channels = reference.channels as usize;
    let sample_rate = reference.sample_rate;
    let reference_f32 = samples_as_f32(&reference);

    // Track 2 is ~12s total (INDEX 00 at 8s to 20s in file). After seeking to 5s,
    // ~7s remain. The decoder must stop at track 2's end boundary (stop_at),
    // decoding ~7s — not bleeding into track 3. We assert on the decoder's own
    // sample count rather than the captured buffer length, since under gapless
    // playback the buffer continues into track 3 via a separate decoder.
    let track_remaining_ms: u64 = 12000 - seek_position_ms; // ~7s
    let max_allowed_ms = track_remaining_ms + 2000; // 7s + 2s tolerance for frame alignment
    let max_allowed_samples = max_allowed_ms * sample_rate as u64 / 1000 * channels as u64;

    // If bleeding into track 3, we'd see ~17s of decoded audio. Our limit is ~9s.
    assert!(
        decoded_samples <= max_allowed_samples,
        "Track 2's decoder ran past its end boundary: decoded {} samples ({:.1}s), \
         max expected {} ({:.1}s). The decoder must stop at the track's end sample.",
        decoded_samples,
        decoded_samples as f64 / channels as f64 / sample_rate as f64,
        max_allowed_samples,
        max_allowed_ms as f64 / 1000.0,
    );

    // Verify the captured audio is from the right region using SAD alignment
    let skip_samples = (seek_position_ms * sample_rate as u64 / 1000) as usize * channels;
    if skip_samples < reference_f32.len() {
        let reference_from_seek = &reference_f32[skip_samples..];
        let snippet_len = (200 * channels)
            .min(captured_snapshot.len())
            .min(reference_from_seek.len());
        let mut sad: f64 = 0.0;
        for i in 0..snippet_len {
            sad += (captured_snapshot[i] as f64 - reference_from_seek[i] as f64).abs();
        }
        let avg_diff = sad / snippet_len as f64;

        // The streaming decoder's f32 output can differ from the i32->f32 reference
        // due to different FFmpeg code paths. Allow a generous tolerance for the
        // secondary alignment check; the primary assertion is the sample count.
        assert!(
            avg_diff < 0.5,
            "Post-seek audio doesn't match reference at {}ms (avg diff {:.4})",
            seek_position_ms,
            avg_diff,
        );
    }

    debug!(
        "Track ended correctly: {} captured samples ({:.1}s), max allowed {} ({:.1}s).",
        captured_snapshot.len(),
        captured_snapshot.len() as f64 / channels as f64 / sample_rate as f64,
        max_allowed_samples,
        max_allowed_ms as f64 / 1000.0,
    );
}

/// Restoring playback state on service start must emit the restored position
/// as a `Seeked` progress event, since the restored track comes up Paused and
/// no position ticks will fire to put a position on screen otherwise.
///
/// This test specifically uses `position_ms = 0` so the internal `seek()`
/// call (which has its own `emit_position_display`) is skipped, forcing the
/// test to rely entirely on the tail emission in `restore()`.
///
/// We do NOT reuse `PlaybackTestFixture` here because its playback service is
/// spawned on a background thread that may not finish initializing before the
/// test writes the snapshot, causing the fixture's service to consume the
/// snapshot instead of the test's new service.
#[tokio::test]
async fn test_restore_emits_seeked_at_saved_position() {
    tracing_init();

    // Build a library + import tracks, but do NOT start a playback service.
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let album_dir = temp_dir.path().join("album");
    std::fs::create_dir_all(&album_dir).unwrap();
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let database_arc = Arc::new(database.clone());
    let library_dir = StoreDir::new(temp_dir.path().to_path_buf());
    let config_handle = test_config(&library_dir);
    let library_manager = LibraryManager::new(
        (*database_arc).clone(),
        config_handle,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        bae_core::import::cover_art::RemoteImageCache::for_test(),
    );
    let runtime_handle = tokio::runtime::Handle::current();
    let _ = generate_test_flac_files(&album_dir);
    let discogs_release = create_test_album();
    let release_id_key = seed_discogs_test_release(discogs_release);
    let import_handle = start_test_import(runtime_handle.clone(), library_manager.clone()).await;
    let import_id = uuid::Uuid::new_v4().to_string();
    import_handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Release {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
        .unwrap();
    let mut progress_rx = import_handle.subscribe_import(import_id);
    let _ = wait_for_import_complete(&mut progress_rx).await;
    let releases = library_manager
        .get_releases_for_album(&library_manager.get_albums(&[]).await.unwrap()[0].id)
        .await
        .unwrap();
    let tracks = library_manager
        .get_tracks_for_release(&releases[0].id)
        .await
        .unwrap();
    let track_id = tracks[0].id.clone();

    // Write a snapshot before the service starts, so no one can consume it
    // before our test service starts.
    let state = bae_core::db::DbPlaybackState {
        context: None,
        manual: "[]".to_string(),
        repeat: "off".to_string(),
        current_track_id: Some(track_id.clone()),
        position_ms: Some(0),
        volume: 0.8,
        is_muted: false,
    };
    library_manager.save_playback_state(&state).await.unwrap();

    // Start the playback service — restore() runs on the audio thread before
    // run() and calls emit_position_display at its tail.
    let (handle, _capture_rx) = start_capture_service(library_manager, runtime_handle);
    let mut progress_rx = handle.subscribe_progress();

    let landed = wait_for_seeked_on(&mut progress_rx, Duration::from_secs(20)).await;
    assert_eq!(
        landed,
        Some(0),
        "restore() must emit a Seeked at the restored position — the tail \
         emit is how a progress display learns the resume point (the restored \
         track comes up Paused, so no position ticks fire)"
    );

    handle.stop();
}

/// The persist-on-change wiring is load-bearing: playing a release writes the
/// device-local `playback_state` row, and stopping clears it — so a restart
/// resumes a live session but never re-cues a finished one.
#[tokio::test]
async fn test_play_persists_then_stop_clears_playback_state() {
    tracing_init();

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let album_dir = temp_dir.path().join("album");
    std::fs::create_dir_all(&album_dir).unwrap();
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let database_arc = Arc::new(database.clone());
    let library_dir = StoreDir::new(temp_dir.path().to_path_buf());
    let config_handle = test_config(&library_dir);
    let library_manager = LibraryManager::new(
        (*database_arc).clone(),
        config_handle,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        bae_core::import::cover_art::RemoteImageCache::for_test(),
    );
    let runtime_handle = tokio::runtime::Handle::current();
    let _ = generate_test_flac_files(&album_dir);
    let discogs_release = create_test_album();
    let release_id_key = seed_discogs_test_release(discogs_release);
    let import_handle = start_test_import(runtime_handle.clone(), library_manager.clone()).await;
    let import_id = uuid::Uuid::new_v4().to_string();
    import_handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "test".to_string(),
            folder: album_dir,
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Release {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
        .unwrap();
    let mut progress_rx = import_handle.subscribe_import(import_id);
    let _ = wait_for_import_complete(&mut progress_rx).await;
    let release_id = library_manager
        .get_releases_for_album(&library_manager.get_albums(&[]).await.unwrap()[0].id)
        .await
        .unwrap()[0]
        .id
        .clone();
    let first_track = library_manager
        .get_tracks_for_release(&release_id)
        .await
        .unwrap()[0]
        .id
        .clone();

    let (handle, _capture_rx) = start_capture_service(library_manager.clone(), runtime_handle);

    // Playing a release persists the row: source is the release, current is the
    // first track.
    handle.play_release(release_id.clone(), None, false);
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut persisted = None;
    while Instant::now() < deadline {
        if let bae_core::db::LoadedPlaybackState::Present(row) =
            library_manager.load_playback_state().await.unwrap()
        {
            persisted = Some(row);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let row = persisted.expect("playing a release should persist the playback_state row");
    let context = row.context.expect("playing a release persists its context");
    assert_eq!(context.source, release_id);
    assert_eq!(row.current_track_id.as_deref(), Some(first_track.as_str()));

    // Stopping clears it, so a restart wouldn't re-cue a finished session.
    handle.stop();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut cleared = false;
    while Instant::now() < deadline {
        if matches!(
            library_manager.load_playback_state().await.unwrap(),
            bae_core::db::LoadedPlaybackState::Absent
        ) {
            cleared = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        cleared,
        "stopping playback should clear the playback_state row"
    );
}

/// The `playback_state` row must advance while a track plays, with no
/// Shutdown/SaveState sent — that's the crash-safe resume point `start`'s doc
/// promises. Without periodic persistence the row holds whatever position the
/// last discrete event (track load, pause, seek) captured, usually 0.
#[tokio::test]
async fn test_position_persists_periodically_while_playing() {
    let mut fixture = PlaybackTestFixture::new().await;
    let track_id = fixture.track_ids[0].clone();

    fixture.playback_handle.play(track_id.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await
        .expect("the track should start playing");

    // Let playback advance past the periodic-persist threshold. No Shutdown,
    // no SaveState — only the ordinary play-progress ticks.
    let live_position_ms =
        position_after(&mut fixture.progress_rx, Duration::from_millis(1500)).await;
    assert!(
        live_position_ms > 0,
        "sanity: playback should have advanced, got {live_position_ms}ms"
    );

    let bae_core::db::LoadedPlaybackState::Present(row) = fixture
        .library_manager
        .load_playback_state()
        .await
        .expect("load_playback_state should succeed")
    else {
        panic!(
            "a playback_state row should be persisted while playing, \
             with no Shutdown/SaveState sent"
        );
    };
    let first_persisted_ms = row
        .position_ms
        .expect("position_ms must be recorded while playing");
    assert!(
        first_persisted_ms > 0,
        "persisted position should be > 0 after ~1.5s of playback, got {first_persisted_ms}ms"
    );

    // Advance further and confirm the stored position keeps climbing — proof
    // this is a periodic write, not a one-shot at track start.
    let _ = position_after(&mut fixture.progress_rx, Duration::from_millis(1500)).await;
    let bae_core::db::LoadedPlaybackState::Present(row) = fixture
        .library_manager
        .load_playback_state()
        .await
        .expect("load_playback_state should succeed")
    else {
        panic!("the playback_state row should still be present");
    };
    let second_persisted_ms = row.position_ms.expect("position_ms must still be recorded");
    assert!(
        second_persisted_ms > first_persisted_ms,
        "persisted position must keep advancing while playing: \
         {first_persisted_ms}ms then {second_persisted_ms}ms"
    );
}

/// An imported test library with no playback service running, so a test can
/// write a `playback_state` row and then start its own service to exercise
/// restore without racing a fixture's service for the row.
struct RestoreTestLibrary {
    library_manager: LibraryManager,
    runtime_handle: tokio::runtime::Handle,
    track_ids: Vec<String>,
    _temp_dir: TempDir,
}
