use crate::test_playback_behavior::fixtures::*;
use crate::test_playback_behavior::support::*;

/// Test fixture for high sample rate (96kHz) FLAC playback.
/// This catches bugs where the playback pipeline assumes 44.1kHz.
struct HighSampleRateTestFixture {
    playback_handle: bae_playback::PlaybackHandle,
    progress_rx: bae_playback::PlaybackProgressFeed,
    track_id: String,
    library: TestLibrary,
    _capture_stream_rx: bae_playback::test_capture::CaptureStreamRx,
    _temp_dir: TempDir,
}

impl PlaybackFixture for HighSampleRateTestFixture {
    async fn close(self) {
        self.playback_handle.shutdown().await;
        self.library.close().await.expect("close the test library");
    }
}

impl HighSampleRateTestFixture {
    async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let fakes = bae_catalogs::fake_catalogs::FakeCatalogs::offline();
        let (library, album_dir, temp_dir) =
            bae_library::test_libraries::setup_test_library_with_album_dir(fakes.providers()).await;

        let fixture_path = bae_import::test_fixtures::flac_dir().join("96khz_test.flac");
        let test_path = album_dir.join("01 96kHz Track.flac");
        std::fs::write(&test_path, bae_import::test_fixtures::read_fixture(&fixture_path))
            .expect("copy the 96 kHz FLAC fixture");

        // Create release with one track
        let discogs_release = DiscogsRelease {
            artists: vec![bae_catalogs::test_releases::discogs_artist(
                "test-artist-1",
                "Test Artist",
            )],
            master_id: Some("test-master-96khz".to_string()),
            ..bae_catalogs::test_releases::discogs_test_release(
                "high-sample-rate-test",
                "96kHz Test Album",
                &[("96kHz Track", "0:03")],
            )
        };
        let release_id_key = fakes.seed_discogs_test_release(discogs_release);

        library.import_folder(album_dir, release_id_key).await;

        let albums =
            bae_views::LibraryViews::new(library.database().clone()).get_albums(&[]).await?;
        let releases = library.get_releases_for_album(&albums[0].id).await?;
        let tracks = library.get_tracks_for_release(&releases[0].id).await?;
        let track_id = tracks[0].id.clone();

        // Verify the audio format was correctly detected as 96kHz
        let audio_format = library
            .get_audio_format_by_track_id(&track_id)
            .await?
            .expect("Audio format should be detected for 96kHz track");
        assert_eq!(
            audio_format.sample_rate.get(),
            96000,
            "Import should detect 96kHz sample rate, got {}",
            audio_format.sample_rate
        );

        let (playback_handle, capture_stream_rx) = start_capture_service(&library).await;
        let progress_rx = playback_handle.subscribe_progress();

        Ok(Self {
            playback_handle,
            progress_rx,
            track_id,
            library,
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
    let mut fixture =
        HighSampleRateTestFixture::new().await.expect("set up high sample rate fixture");

    // Play the 96kHz track (3 seconds duration)
    fixture.playback_handle.play(fixture.track_id.clone());

    // Wait for track to complete, keeping the last position seen before it.
    let mut final_position_ms: Option<i64> = None;
    let track_completed =
        bae_util::test_channels::next_matching(&mut fixture.progress_rx, HANG_GUARD, |event| {
            match event {
                PlaybackProgress::PositionUpdate { position_ms, .. } => {
                    final_position_ms = Some(position_ms);
                    None
                }
                PlaybackProgress::TrackCompleted { .. } => Some(()),
                _ => None,
            }
        })
        .await;

    assert!(track_completed.is_some(), "Track should complete");

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
    with_fixture(PlaybackTestFixture::new(), async |fixture| {
        let track_id = fixture.track_ids[0].clone();

        // Play the track (5 seconds at 44.1kHz mono = 220500 total samples)
        fixture.playback_handle.play(track_id.clone());

        // Wait for playback to start
        let playing_state =
            fixture.wait_for_state(|s| matches!(s, PlaybackState::Playing { .. })).await;
        assert!(playing_state.is_some(), "Should start playing");

        // Seek to 2.5 seconds
        let seek_position = Duration::from_millis(2500);
        fixture.playback_handle.seek(TrackTime::from_duration(seek_position));

        // Wait for seek to complete
        let seeked = fixture.wait_for_seeked().await;
        assert!(seeked.is_some(), "Should receive Seeked event");

        // Wait for track to complete and get decode stats
        let decode_stats =
            bae_util::test_channels::next_matching(&mut fixture.progress_rx, HANG_GUARD, |event| {
                match event {
                    PlaybackProgress::DecodeStats {
                        error_count,
                        samples_decoded,
                        track_id: stats_track_id,
                    } if stats_track_id == track_id => Some((error_count, samples_decoded)),
                    _ => None,
                }
            })
            .await;

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
            samples_decoded >= expected_min
                && samples_decoded <= expected_min + max_frame_overshoot,
            "After seeking to 2.5s in a 5s track, expected {}..{} samples but got {}.",
            expected_min,
            expected_min + max_frame_overshoot,
            samples_decoded,
        );
    })
    .await;
}

/// CUE/FLAC seek must not play past the track's end boundary.
///
/// After seeking to 5s in track 2, the captured audio must match the reference
/// from 5s onward AND must stop at track 2's end (not bleed into track 3).
/// The captured sample count should correspond to roughly the remaining duration.
#[tokio::test]
async fn test_cue_flac_seek_respects_track_end_boundary() {
    use bae_audio::decode_audio;

    // Real-time capture: at full speed track 2's decoder finishes the whole
    // track before the seek restarts it at 5s, so its sample count overshoots
    // the boundary (flaky under load). Real-time keeps the decoder on the seek.
    with_fixture(
        async {
            CueFlacTestFixture::new(bae_playback::test_capture::TestAudioDevice::RealtimeCapture)
                .await
                .expect("set up CUE/FLAC realtime capture fixture")
        },
        async |fixture| {
            let track_id = fixture.track_ids[1].clone();

            fixture.playback_handle.play(track_id.clone());
            // Drain the play stream; the seek below mints a fresh one. Track 2 then
            // advances gaplessly into track 3 within that same seek stream, so we assert
            // on the decoder's own sample count rather than the captured buffer length.
            let _play_stream = fixture.next_capture_stream().await;

            // Wait for playback to start
            let started = bae_util::test_channels::next_matching(
                &mut fixture.progress_rx,
                HANG_GUARD,
                |event| {
                    matches!(
                        event,
                        PlaybackProgress::StateChanged { state: PlaybackState::Playing { .. } }
                    )
                    .then_some(())
                },
            )
            .await;
            assert!(started.is_some(), "Playback should start");

            // Seek to 5s into track 2
            let seek_position_ms: u64 = 5000;
            fixture
                .playback_handle
                .seek(TrackTime::from_duration(Duration::from_millis(seek_position_ms)));
            let mut captured = fixture.next_capture_stream().await;

            bae_playback::test_capture::wait_for_seek(&mut fixture.progress_rx, &track_id).await;

            // Wait for the track's decode stats. Under gapless playback the seeked track
            // advances into the next track within one persistent stream, so it reports
            // its decode stats at the boundary rather than via TrackCompleted.
            let decoded_samples = bae_util::test_channels::next_matching(
                &mut fixture.progress_rx,
                HANG_GUARD,
                |event| match event {
                    PlaybackProgress::DecodeStats {
                        track_id: ref tid, samples_decoded, ..
                    } if *tid == track_id => Some(samples_decoded),
                    _ => None,
                },
            )
            .await
            .expect("Track 2 should report decode stats");

            let captured_snapshot: Vec<f32> = captured.samples();

            // Decode reference and compare at the seek position
            let fixture_dir = bae_audio::test_fixtures::cue_flac_dir();
            let reference_data =
                std::fs::read(fixture_dir.join("02 Test Artist - Track Two (White Noise).flac"))
                    .expect("read reference");
            let reference = decode_audio(
                bae_audio::test_samples::filled_sparse_buffer(&reference_data),
                None,
                None,
            )
            .expect("decode reference");
            let channels = reference.channels as usize;
            let sample_rate = reference.sample_rate;
            let reference_f32 = samples_as_f32(&reference);

            // Track 2 runs 10s from INDEX 01 (10s to 20s in the file). After seeking to
            // 5s into it, 5s remain. The decoder must stop at track 2's end boundary
            // (stop_at), decoding ~5s — not bleeding into track 3. We assert on the decoder's own
            // sample count rather than the captured buffer length, since under gapless
            // playback the buffer continues into track 3 via a separate decoder.
            let track_remaining_ms: u64 = 10_000 - seek_position_ms; // 5s
            let max_allowed_ms = track_remaining_ms + 2000; // 5s + 2s tolerance for frame alignment
            let max_allowed_samples = max_allowed_ms * sample_rate as u64 / 1000 * channels as u64;

            // If bleeding into track 3, we'd see ~15s of decoded audio. Our limit is ~7s.
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
                let snippet_len =
                    (200 * channels).min(captured_snapshot.len()).min(reference_from_seek.len());
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
        },
    )
    .await;
}

/// The three-track FLAC album imported into a fresh library with no playback
/// service started — the arrange for the tests below, which each decide when
/// their own service comes up — beside the library's store. The `TempDir` owns
/// the library's files.
async fn imported_library_without_playback() -> (TestLibrary, TempDir) {
    let fakes = bae_catalogs::fake_catalogs::FakeCatalogs::offline();
    let (library, album_dir, temp_dir) =
        bae_library::test_libraries::setup_test_library_with_album_dir(fakes.providers()).await;
    let _ = generate_test_flac_files(&album_dir);
    let release_id_key = fakes.seed_discogs_test_release(create_test_album());
    library.import_folder(album_dir, release_id_key).await;
    (library, temp_dir)
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
    bae_util::test_logs::install();

    // Build a library + import tracks, but do NOT start a playback service.
    let (library, _temp_dir) = imported_library_without_playback().await;
    let releases = library
        .get_releases_for_album(
            &bae_views::LibraryViews::new(library.database().clone())
                .get_albums(&[])
                .await
                .unwrap()[0]
                .id,
        )
        .await
        .unwrap();
    let tracks = library.get_tracks_for_release(&releases[0].id).await.unwrap();
    let track_id = tracks[0].id.clone();

    // Write a snapshot before the service starts, so no one can consume it
    // before our test service starts.
    library.write_resume_point(&releases[0].id, track_id.clone(), Some(0)).await.unwrap();

    // Start the playback service — restore() runs on the audio thread before
    // run() and calls emit_position_display at its tail.
    let (handle, _capture_rx) = start_capture_service(&library).await;
    let mut progress_rx = handle.subscribe_progress();

    let landed = wait_for_seeked_on(&mut progress_rx).await;
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
    bae_util::test_logs::install();

    let (library, _temp_dir) = imported_library_without_playback().await;
    let release_id = library
        .get_releases_for_album(
            &bae_views::LibraryViews::new(library.database().clone())
                .get_albums(&[])
                .await
                .unwrap()[0]
                .id,
        )
        .await
        .unwrap()[0]
        .id
        .clone();
    let first_track = library.get_tracks_for_release(&release_id).await.unwrap()[0].id.clone();

    let (handle, _capture_rx) = start_capture_service(&library).await;

    // Playing a release persists the row: current is the first track, and the
    // queue plays from the release.
    let played =
        handle.play_release(release_id.clone(), None, false).await.expect("the play is answered");
    assert_eq!(played, bae_playback::PlayOutcome::Playing);
    let mut state = bae_playback::tables::subscribe_playback_state(library.database());
    let row = bae_library::followed::value_matching(&mut state, Option::is_some)
        .await
        .expect("playing a release persists the playback_state row");
    assert_eq!(row.current.map(|current| current.track_id).as_deref(), Some(first_track.as_str()));
    let context = handle
        .queue_value()
        .await
        .expect("queue value")
        .context
        .expect("playing a release fills the context lane");
    assert_eq!(context.source, bae_playback::ContextSource::Release(release_id.clone()));

    // Stopping clears it, so a restart wouldn't re-cue a finished session.
    handle.stop();
    bae_library::followed::value_matching(&mut state, Option::is_none).await;
}

/// The `playback_state` row must advance while a track plays, with no
/// Shutdown/SaveState sent — that's the crash-safe resume point `start`'s doc
/// promises. Without periodic persistence the row holds whatever position the
/// last discrete event (track load, pause, seek) captured, usually 0.
#[tokio::test]
async fn test_position_persists_periodically_while_playing() {
    with_fixture(PlaybackTestFixture::new(), async |fixture| {
        let track_id = fixture.track_ids[0].clone();

        fixture.playback_handle.play(track_id.clone());
        fixture
            .wait_for_state(|s| matches!(s, PlaybackState::Playing { .. }))
            .await
            .expect("the track should start playing");

        // No Shutdown, no SaveState — only the ordinary play-progress ticks. The
        // track's load persisted position 0, so a later position is a periodic
        // write.
        let first_persisted_ms = persisted_position_past(fixture, 0).await;

        // The stored position keeps climbing — proof this is a periodic write, not
        // a one-shot at track start.
        persisted_position_past(fixture, first_persisted_ms).await;
    })
    .await;
}

/// The `playback_state` row's position once it passes `floor_ms`. Panics if
/// it never does.
async fn persisted_position_past(fixture: &PlaybackTestFixture, floor_ms: u64) -> u64 {
    let mut state = bae_playback::tables::subscribe_playback_state(fixture.library.database());
    let past = |row: &Option<bae_playback::tables::PlaybackResumeRow>| {
        row.as_ref().and_then(|row| row.position_ms).filter(|ms| *ms > floor_ms)
    };
    let row = bae_library::followed::value_matching(&mut state, |row| past(row).is_some()).await;
    past(&row).expect("the matched row's position is past the floor")
}
