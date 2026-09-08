/// Playing a track emits Loading without metadata first (before the DB lookup),
/// then Loading carrying the target's metadata (after prepare), then Playing
/// once the decoder buffer is ready. The middle Loading lets the UI switch the
/// now-playing bar to the target while audio fills; emitting Playing only at
/// ready means the position bar never freezes against a not-yet-started stream.
/// The sequence asserts `resolved: Some` on the second Loading — the metadata
/// the UI swaps to before audio is flowing.
#[tokio::test]
async fn play_emits_bare_loading_then_loading_with_metadata_then_playing() {
    let mut fixture = PlaybackTestFixture::new().await;
    assert!(
        !fixture.track_ids.is_empty(),
        "fixture must import at least one playable track"
    );
    let track_id = fixture.track_ids[0].clone();

    fixture.playback_handle.play(track_id.clone());
    let states = fixture
        .collect_states_until(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;

    let loading: Vec<&PlaybackState> = states
        .iter()
        .filter(|s| matches!(s, PlaybackState::Loading { .. }))
        .collect();
    assert!(
        loading.len() >= 2,
        "expected a bare Loading then a Loading with metadata, got {states:?}"
    );

    match loading[0] {
        PlaybackState::Loading {
            track_id: id,
            resolved,
        } => {
            assert_eq!(id, &track_id);
            assert!(
                resolved.is_none(),
                "first Loading is emitted before prepare resolves metadata"
            );
        }
        other => panic!("expected Loading, got {other:?}"),
    }

    let resolved_loading = loading
        .iter()
        .find_map(|s| match s {
            PlaybackState::Loading {
                track_id: id,
                resolved: Some(info),
            } => Some((id, info)),
            _ => None,
        })
        .expect("a Loading carrying resolved metadata must be emitted");
    assert_eq!(resolved_loading.0, &track_id);
    assert_eq!(resolved_loading.1.track_info.track_id, track_id);

    let playing = states
        .last()
        .expect("at least one state should be collected");
    assert!(
        matches!(playing, PlaybackState::Playing { track_info, .. } if track_info.track_id == track_id),
        "the terminal state must be Playing for the requested track, got {playing:?}"
    );
}

/// A seek well past the end of a track exercises the real end-of-stream
/// handling in PlaybackService::seek() (which does no bounds check). Whatever it
/// resolves to — clamp-via-EOF and a Seeked, or a surfaced error — it must
/// SIGNAL: never leave the UI with no Seeked and no PlaybackError. This drives
/// the real seek(), replacing the former test-only validate_seek_position
/// reconstruction.
#[tokio::test]
async fn seek_past_end_of_track_signals_rather_than_hanging() {
    let mut fixture = PlaybackTestFixture::new().await;
    fixture.playback_handle.play(fixture.track_ids[0].clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await
        .expect("track should start playing");

    // Fixture tracks are ~10s; 600s is far past the end.
    fixture.playback_handle.seek(Duration::from_secs(600));

    // seek()'s decoder-ready timeout is 5s; allow margin past it.
    let signaled =
        support::next_matching(&mut fixture.progress_rx, Duration::from_secs(8), |event| {
            matches!(
                event,
                PlaybackProgress::Seeked { .. } | PlaybackProgress::PlaybackError { .. }
            )
            .then_some(())
        })
        .await;
    assert!(
        signaled.is_some(),
        "a seek past the end must signal (Seeked or PlaybackError), not freeze silently"
    );
}

/// SeekByRatio runs the real handler (position = pregap + ratio·(duration −
/// pregap)) down through seek(). On this no-pregap track the ratio maps straight
/// onto the duration: half-way lands well past the start and clearly before the
/// full-length seek. (The pregap-offset case — ratio 0.0 landing at the post-
/// pregap start — needs a CUE fixture with a known pregap; left to test_cue_*.)
#[tokio::test]
async fn seek_by_ratio_maps_to_a_proportional_position() {
    let mut fixture = PlaybackTestFixture::new().await;
    fixture.playback_handle.play(fixture.track_ids[0].clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await
        .expect("track should start playing");

    fixture.playback_handle.seek_by_ratio(0.5);
    let mid = fixture
        .wait_for_seeked(Duration::from_secs(5))
        .await
        .expect("Seeked for ratio 0.5");
    fixture.playback_handle.seek_by_ratio(1.0);
    let end = fixture
        .wait_for_seeked(Duration::from_secs(5))
        .await
        .expect("Seeked for ratio 1.0");

    assert!(
        mid > 1_000,
        "ratio 0.5 should land well past the start, got {mid}ms"
    );
    assert!(
        end > mid + 1_000,
        "ratio 1.0 should land clearly later than 0.5 (mid={mid}ms end={end}ms)"
    );
}

/// Create a test album with 2 short tracks
fn create_test_album() -> DiscogsRelease {
    DiscogsRelease {
        artists: vec![support::discogs_artist("test-artist-1", "Test Artist")],
        master_id: Some("test-master-123".to_string()),
        ..support::discogs_test_release(
            "test-playback-123",
            "Playback Test Album",
            &[
                ("Test Track 1", "0:10"),
                ("Test Track 2", "0:10"),
                ("Test Track 3", "0:10"),
            ],
        )
    }
}
/// Copy pre-generated FLAC fixtures to test directory
/// Fixtures should be generated using scripts/generate_test_flac.sh
fn generate_test_flac_files(dir: &std::path::Path) -> Vec<Vec<u8>> {
    use std::fs;
    let fixture_dir = bae_test_support::fixture_dir!("flac");
    let fixture_files = vec![
        "01 Test Track 1.flac",
        "02 Test Track 2.flac",
        "03 Test Track 3.flac",
    ];
    let mut file_data = Vec::new();
    for fixture_name in fixture_files {
        let fixture_path = fixture_dir.join(fixture_name);
        let test_path = dir.join(fixture_name);
        let data = fs::read(&fixture_path).unwrap_or_else(|_| {
            panic!(
                "FLAC fixture not found: {}\n\
                     Run: ./scripts/generate_test_flac.sh",
                fixture_path.display(),
            );
        });
        fs::write(&test_path, &data).expect("Failed to copy FLAC fixture");
        file_data.push(data);
    }
    file_data
}
/// Copy pre-generated CUE/FLAC fixtures to test directory
/// Fixtures should be generated using scripts/generate_cue_flac_fixture.sh
fn generate_cue_flac_files(dir: &std::path::Path) {
    use std::fs;
    let fixture_dir = bae_test_support::fixture_dir!("cue_flac");

    // Copy FLAC file
    let flac_src = fixture_dir.join("Test Album.flac");
    let flac_dst = dir.join("Test Album.flac");
    let flac_data = fs::read(&flac_src).unwrap_or_else(|_| {
        panic!(
            "CUE/FLAC fixture not found: {}\n\
             Run: ./scripts/generate_cue_flac_fixture.sh",
            flac_src.display(),
        );
    });
    fs::write(&flac_dst, &flac_data).expect("Failed to copy FLAC fixture");

    // Copy CUE file
    let cue_src = fixture_dir.join("Test Album.cue");
    let cue_dst = dir.join("Test Album.cue");
    let cue_data = fs::read(&cue_src).unwrap_or_else(|_| {
        panic!(
            "CUE fixture not found: {}\n\
             Run: ./scripts/generate_cue_flac_fixture.sh",
            cue_src.display(),
        );
    });
    fs::write(&cue_dst, &cue_data).expect("Failed to copy CUE fixture");
}

/// Create a test album matching the CUE/FLAC fixture (3 tracks)
fn create_cue_flac_test_album() -> DiscogsRelease {
    DiscogsRelease {
        country: Some("Test Country".to_string()),
        artists: vec![support::discogs_artist("test-artist-1", "Test Artist")],
        master_id: Some("test-master-cue-flac".to_string()),
        ..support::discogs_test_release(
            "cue-flac-test-release",
            "Test Album",
            &[
                ("Track One (Silence)", "0:10"),
                ("Track Two (White Noise)", "0:10"),
                ("Track Three (Brown Noise)", "0:10"),
            ],
        )
    }
}

/// Test fixture for CUE/FLAC playback (single FLAC with CUE sheet)
/// Test fixture for CUE/FLAC playback (single FLAC with CUE sheet). Full-speed
/// capture pulls as fast as the decoder fills — fast, but a track can fully
/// decode and gaplessly advance before a follow-up command lands, so seek and
/// pause tests ask for `TestAudioDevice::RealtimeCapture` instead.
struct CueFlacTestFixture {
    playback_handle: bae_core::playback::PlaybackHandle,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_ids: Vec<String>,
    capture_stream_rx: support::CaptureStreamRx,
    _temp_dir: TempDir,
}

impl CueFlacTestFixture {
    async fn new(device: support::TestAudioDevice) -> Result<Self, Box<dyn std::error::Error>> {
        // Import without storage (local CUE/FLAC playback).
        let (library_manager, imported) = imported_release_setup(
            create_cue_flac_test_album(),
            "test",
            uuid::Uuid::new_v4().to_string(),
            generate_cue_flac_files,
            |_| Ok(()),
        )
        .await?;
        assert_eq!(
            imported.track_ids.len(),
            3,
            "Should have 3 tracks from CUE/FLAC"
        );
        let (playback_handle, capture_stream_rx) =
            support::start_capture_playback(&library_manager, device);
        let progress_rx = playback_handle.subscribe_progress();
        Ok(Self {
            playback_handle,
            progress_rx,
            track_ids: imported.track_ids,
            capture_stream_rx,
            _temp_dir: imported.temp_dir,
        })
    }

    /// Awaits the next capture buffer minted by `create_stream`. Buffers are
    /// yielded in creation order; tests that exercise auto-advance, seek, or
    /// next call this once per stream they want to inspect.
    async fn next_capture_stream(&mut self) -> Arc<std::sync::Mutex<Vec<f32>>> {
        support::next_capture_stream(&mut self.capture_stream_rx).await
    }
}

/// The side-pause album playing through a capture sink, plus the library
/// manager the side-pause setting is written through.
struct SidePauseTestFixture {
    playback_handle: bae_core::playback::PlaybackHandle,
    library_manager: LibraryManager,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_ids: Vec<String>,
    release_id: String,
    capture_stream_rx: support::CaptureStreamRx,
    _temp_dir: TempDir,
}

impl SidePauseTestFixture {
    async fn new(
        format: &str,
        positions: [&str; 3],
        pause_between_sides: bool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let import_ids = SequentialIdProvider::new("side-pause-import");
        let import_id = import_ids.new_id();
        let (library_manager, imported) = imported_release_setup(
            create_side_pause_test_album(format, positions),
            "side-pause",
            import_id,
            |album_dir| {
                let _track_data = generate_test_flac_files(album_dir);
            },
            |library_manager| {
                library_manager.set_pause_between_sides(pause_between_sides)?;
                Ok(())
            },
        )
        .await?;
        assert_eq!(
            imported.track_ids.len(),
            3,
            "side-pause fixture imports 3 tracks"
        );

        // Real-time-paced capture, not full-speed: every test here plays a track
        // and then issues commands (the side-pause toggle, the seek) that must
        // land *before* the track's boundary is crossed. An unpaced drain empties
        // the remaining audio in milliseconds, so those commands would be racing
        // the decoder rather than arriving during playback. Pacing the sink to
        // wall-clock bounds how fast the boundary can arrive, and a loaded machine
        // can only slow that sink down, never speed it up.
        let (playback_handle, capture_stream_rx) = support::start_capture_playback(
            &library_manager,
            support::TestAudioDevice::RealtimeCapture,
        );
        let progress_rx = playback_handle.subscribe_progress();
        Ok(Self {
            playback_handle,
            library_manager,
            progress_rx,
            track_ids: imported.track_ids,
            release_id: imported.release_id,
            capture_stream_rx,
            _temp_dir: imported.temp_dir,
        })
    }

    /// Toggle `pause_between_sides` mid-track through the same effective path
    /// production uses (`AppServices::set_pause_between_sides`, not reachable
    /// directly from this fixture since it drives `PlaybackService` without an
    /// `AppServices`): write the config, then — turning it on — notify the
    /// playback service to re-evaluate its already-staged preload.
    fn set_pause_between_sides_mid_track(&self, enabled: bool) {
        self.library_manager
            .set_pause_between_sides(enabled)
            .expect("set_pause_between_sides");
        if enabled {
            self.playback_handle.reevaluate_side_pause_staging();
        }
    }

    async fn wait_for_state<F>(
        &mut self,
        predicate: F,
        timeout_duration: Duration,
    ) -> Option<PlaybackState>
    where
        F: Fn(&PlaybackState) -> bool,
    {
        wait_for_state_on(&mut self.progress_rx, predicate, timeout_duration).await
    }

    async fn next_capture_stream(&mut self) -> Arc<std::sync::Mutex<Vec<f32>>> {
        support::next_capture_stream(&mut self.capture_stream_rx).await
    }

    fn play_release_from(&self, start_track_index: usize) {
        self.playback_handle
            .play_release(self.release_id.clone(), Some(start_track_index), false);
    }

    /// Seek to 200 ms before the end of a 5 s fixture track, so the boundary
    /// arrives after a short run of real-time audio rather than a whole track's
    /// worth. Issued only after everything that must be in effect at the boundary
    /// (the side-pause setting, the staging re-evaluation) has been dispatched:
    /// those commands share the service's FIFO command channel with this seek, so
    /// they are processed before it.
    fn seek_to_auto_advance(&self) {
        self.playback_handle
            .seek(Duration::from_secs(4) + Duration::from_millis(800));
    }

    async fn wait_for_playing_track(
        &mut self,
        track_id: &str,
        timeout_duration: Duration,
        message: &str,
    ) {
        self.wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == track_id),
            timeout_duration,
        )
        .await
        .expect(message);
    }

    async fn play_track_and_wait(&mut self, start_track_index: usize, track_id: &str) {
        self.play_release_from(start_track_index);
        self.wait_for_playing_track(
            track_id,
            Duration::from_secs(5),
            "track before side boundary should start",
        )
        .await;
    }

    async fn wait_for_side_pause(
        &mut self,
        expected_side_label: &str,
        expected_message_key: &str,
    ) -> PlaybackState {
        self.wait_for_state(
            |s| {
                matches!(
                    s,
                    PlaybackState::Paused {
                        reason: PlaybackPauseReason::SideEnded(prompt),
                        ..
                    } if prompt.side_label == expected_side_label
                        && prompt.message_key == expected_message_key
                )
            },
            Duration::from_secs(10),
        )
        .await
        .expect("side boundary should pause")
    }

    async fn play_to_side_pause(
        &mut self,
        start_track_index: usize,
        track_id: &str,
        expected_side_label: &str,
        expected_message_key: &str,
    ) -> PlaybackState {
        self.play_track_and_wait(start_track_index, track_id).await;
        self.seek_to_auto_advance();
        self.wait_for_side_pause(expected_side_label, expected_message_key)
            .await
    }
}

fn create_side_pause_test_album(format: &str, positions: [&str; 3]) -> DiscogsRelease {
    let mut release = create_test_album();
    release.id = format!("side-pause-{format}-{}", positions.join("_"));
    release.title = format!("{format} Side Pause Fixture");
    release.format = vec![format.to_string()];
    for (track, position) in release.tracklist.iter_mut().zip(positions) {
        track.position = position.to_string();
    }
    release
}

// ============================================================================
// Pause state preservation tests
// ============================================================================
// These tests verify that Next/Previous preserve pause state while fresh Play
// and AutoAdvance always start playing.

include!("side_and_navigation.rs");
include!("cd_boundaries.rs");
include!("queue_and_pregap.rs");
include!("high_rate_and_restore.rs");
include!("local_sparse_buffer.rs");
include!("remote_sparse_buffer.rs");
