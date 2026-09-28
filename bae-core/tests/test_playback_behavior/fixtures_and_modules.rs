/// Playing a track emits `Loading` without metadata, then `Loading` with the
/// track's metadata so the UI can show it while audio fills, then `Playing`.
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

/// `seek` does no bounds check, so a seek past the end of a track must still
/// produce a `Seeked` or a `PlaybackError` rather than nothing.
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

    // Fixture tracks are 5 s long.
    fixture.playback_handle.seek(Duration::from_secs(600));

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

/// On a track with no pregap, `seek_by_ratio` maps the ratio straight onto the
/// duration, so 0.5 lands well past the start and clearly before 1.0.
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

/// The three-track album matching the `flac` fixtures.
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
/// Copy the `flac` fixtures into `dir` and return their bytes.
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
        let data = bae_test_support::read_fixture(&fixture_path);
        fs::write(&test_path, &data).expect("Failed to copy FLAC fixture");
        file_data.push(data);
    }
    file_data
}
/// Copy the `cue_flac` fixture's FLAC and CUE files into `dir`.
fn generate_cue_flac_files(dir: &std::path::Path) {
    use std::fs;
    let fixture_dir = bae_test_support::fixture_dir!("cue_flac");

    let flac_src = fixture_dir.join("Test Album.flac");
    let flac_dst = dir.join("Test Album.flac");
    let flac_data = bae_test_support::read_fixture(&flac_src);
    fs::write(&flac_dst, &flac_data).expect("Failed to copy FLAC fixture");

    let cue_src = fixture_dir.join("Test Album.cue");
    let cue_dst = dir.join("Test Album.cue");
    let cue_data = bae_test_support::read_fixture(&cue_src);
    fs::write(&cue_dst, &cue_data).expect("Failed to copy CUE fixture");
}

/// The three-track album matching the `cue_flac` fixture.
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

/// Playback of the single-file CUE/FLAC album. Tests that seek or pause need
/// `TestAudioDevice::RealtimeCapture`.
struct CueFlacTestFixture {
    playback_handle: bae_core::playback::PlaybackHandle,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_ids: Vec<String>,
    capture_stream_rx: support::CaptureStreamRx,
    _temp_dir: TempDir,
}

impl CueFlacTestFixture {
    async fn new(device: support::TestAudioDevice) -> Result<Self, Box<dyn std::error::Error>> {
        let (library_manager, imported) = imported_release_setup(
            create_cue_flac_test_album(),
            "test",
            uuid::Uuid::new_v4().to_string(),
            generate_cue_flac_files,
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

    /// The next stream's capture buffer, in the order streams were created.
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
    /// The service's playback clock. Nothing moves it but the test, so a
    /// side-pause countdown runs out exactly when the test says.
    clock: Arc<bae_core::playback::ManualPlaybackClock>,
    _temp_dir: TempDir,
}

impl SidePauseTestFixture {
    async fn new(
        format: &str,
        positions: [&str; 3],
        pause_between_sides: bool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::with_settings(
            format,
            positions,
            pause_between_sides,
            SidePauseCountdown::Off,
        )
        .await
    }

    /// A vinyl A1/A2/B1 fixture that pauses between sides with `countdown`.
    async fn with_countdown(
        countdown: SidePauseCountdown,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::with_settings("Vinyl", ["A1", "A2", "B1"], true, countdown).await
    }

    async fn with_settings(
        format: &str,
        positions: [&str; 3],
        pause_between_sides: bool,
        countdown: SidePauseCountdown,
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
        )
        .await?;
        // Set before the playback service starts, so its first preload reads
        // them.
        library_manager
            .set_pause_between_sides(pause_between_sides)
            .await?;
        library_manager.set_side_pause_countdown(countdown).await?;
        assert_eq!(
            imported.track_ids.len(),
            3,
            "side-pause fixture imports 3 tracks"
        );

        // Paced to wall-clock time so commands issued after play land before
        // the track's boundary.
        let clock = Arc::new(bae_core::playback::ManualPlaybackClock::new(
            side_pause_clock_start(),
        ));
        let (playback_handle, capture_stream_rx) = support::start_capture_playback_with_clock(
            &library_manager,
            support::TestAudioDevice::RealtimeCapture,
            clock.clone(),
        );
        let progress_rx = playback_handle.subscribe_progress();
        Ok(Self {
            playback_handle,
            library_manager,
            progress_rx,
            track_ids: imported.track_ids,
            release_id: imported.release_id,
            capture_stream_rx,
            clock,
            _temp_dir: imported.temp_dir,
        })
    }

    /// Toggle `pause_between_sides` the way `AppServices::set_pause_between_sides`
    /// does; this fixture has no `AppServices`.
    async fn set_pause_between_sides_mid_track(&self, enabled: bool) {
        self.library_manager
            .set_pause_between_sides(enabled).await
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

    /// Seek to 200 ms before the end of the 5 s track. Commands run in order, so
    /// anything that must apply at the boundary goes before this.
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
        expected_boundary: PlaybackPauseBoundary,
    ) -> PlaybackState {
        self.wait_for_state(
            |s| {
                matches!(
                    s,
                    PlaybackState::Paused {
                        reason: PlaybackPauseReason::SideEnded(prompt),
                        ..
                    } if prompt.side_label == expected_side_label
                        && prompt.boundary == expected_boundary
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
        expected_boundary: PlaybackPauseBoundary,
    ) -> PlaybackState {
        self.play_track_and_wait(start_track_index, track_id).await;
        self.seek_to_auto_advance();
        self.wait_for_side_pause(expected_side_label, expected_boundary)
            .await
    }
}

/// Where a side-pause fixture's playback clock starts.
fn side_pause_clock_start() -> chrono::DateTime<chrono::Utc> {
    "2026-01-01T00:00:00Z".parse().unwrap()
}

fn create_side_pause_test_album(format: &str, positions: [&str; 3]) -> DiscogsRelease {
    let mut release = create_test_album();
    release.id = format!("side-pause-{format}-{}", positions.join("_"));
    release.title = format!("{format} Side Pause Fixture");
    // "2xCD" is two of a CD format entry; a bare name is one.
    let (qty, name) = format
        .split_once('x')
        .filter(|(qty, _)| qty.parse::<u32>().is_ok())
        .unwrap_or(("1", format));
    release.formats = vec![bae_core::discogs::DiscogsFormat {
        name: name.to_string(),
        qty: qty.to_string(),
        descriptions: Vec::new(),
    }];
    for (track, position) in release.tracklist.iter_mut().zip(positions) {
        track.position = position.to_string();
    }
    release
}

include!("side_and_navigation.rs");
include!("side_pause_countdown.rs");
include!("cd_boundaries.rs");
include!("queue_and_pregap.rs");
include!("high_rate_and_restore.rs");
include!("local_sparse_buffer.rs");
include!("remote_sparse_buffer.rs");
