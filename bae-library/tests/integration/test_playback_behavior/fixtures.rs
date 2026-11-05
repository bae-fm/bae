use super::support::*;

/// Playing a track emits a bare `Loading`, then `Loading` with the prepared
/// track so the UI can show it while audio fills, then `Playing`.
#[tokio::test]
pub(super) async fn play_emits_bare_loading_then_loading_with_metadata_then_playing() {
    with_fixture(PlaybackTestFixture::new(), async |fixture| {
        assert!(!fixture.track_ids.is_empty(), "fixture must import at least one playable track");
        let track_id = fixture.track_ids[0].clone();

        fixture.playback_handle.play(track_id.clone());
        let states =
            fixture.collect_states_until(|s| matches!(s, PlaybackState::Playing { .. })).await;

        let loading: Vec<&PlaybackState> =
            states.iter().filter(|s| matches!(s, PlaybackState::Loading { .. })).collect();
        assert!(
            loading.len() >= 2,
            "expected a bare Loading then a Loading with the prepared track, got {states:?}"
        );

        match loading[0] {
            PlaybackState::Loading { track: LoadingTrack::Unprepared { track_id: id } } => {
                assert_eq!(id, &track_id)
            }
            other => panic!("first Loading is emitted before the track is prepared, got {other:?}"),
        }

        let prepared_loading = loading
            .iter()
            .find_map(|s| match s {
                PlaybackState::Loading { track: LoadingTrack::Prepared(track) } => Some(track),
                _ => None,
            })
            .expect("a Loading carrying the prepared track must be emitted");
        assert_eq!(prepared_loading.track_id, track_id);

        let playing = states.last().expect("at least one state should be collected");
        assert!(
            matches!(playing, PlaybackState::Playing { track, .. } if track.track_id == track_id),
            "the terminal state must be Playing for the requested track, got {playing:?}"
        );
    })
    .await;
}

/// `seek` does no bounds check, so a seek past the end of a track must still
/// produce a `Seeked` or a `PlaybackError` rather than nothing.
#[tokio::test]
pub(super) async fn seek_past_end_of_track_signals_rather_than_hanging() {
    with_fixture(PlaybackTestFixture::new(), async |fixture| {
        fixture.playback_handle.play(fixture.track_ids[0].clone());
        fixture
            .wait_for_state(|s| matches!(s, PlaybackState::Playing { .. }))
            .await
            .expect("track should start playing");

        // Fixture tracks are 5 s long.
        fixture.playback_handle.seek(TrackTime::from_duration(Duration::from_secs(600)));

        let signaled =
            bae_util::test_channels::next_matching(&mut fixture.progress_rx, HANG_GUARD, |event| {
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
    })
    .await;
}

/// On a track with no pregap, `seek_by_ratio` maps the ratio straight onto the
/// duration, so 0.5 lands well past the start and clearly before 1.0.
#[tokio::test]
pub(super) async fn seek_by_ratio_maps_to_a_proportional_position() {
    with_fixture(PlaybackTestFixture::new(), async |fixture| {
        fixture.playback_handle.play(fixture.track_ids[0].clone());
        fixture
            .wait_for_state(|s| matches!(s, PlaybackState::Playing { .. }))
            .await
            .expect("track should start playing");

        fixture.playback_handle.seek_by_ratio(0.5);
        let mid = fixture.wait_for_seeked().await.expect("Seeked for ratio 0.5");
        fixture.playback_handle.seek_by_ratio(1.0);
        let end = fixture.wait_for_seeked().await.expect("Seeked for ratio 1.0");

        assert!(mid > 1_000, "ratio 0.5 should land well past the start, got {mid}ms");
        assert!(
            end > mid + 1_000,
            "ratio 1.0 should land clearly later than 0.5 (mid={mid}ms end={end}ms)"
        );
    })
    .await;
}

/// The three-track album matching the `flac` fixtures.
pub(super) fn create_test_album() -> DiscogsRelease {
    DiscogsRelease {
        artists: vec![bae_catalogs::test_releases::discogs_artist("test-artist-1", "Test Artist")],
        master_id: Some("test-master-123".to_string()),
        ..bae_catalogs::test_releases::discogs_test_release(
            "test-playback-123",
            "Playback Test Album",
            &[("Test Track 1", "0:10"), ("Test Track 2", "0:10"), ("Test Track 3", "0:10")],
        )
    }
}
/// Copy the `flac` fixtures into `dir` and return their bytes.
pub(super) fn generate_test_flac_files(dir: &std::path::Path) -> Vec<Vec<u8>> {
    use std::fs;
    let fixture_dir = bae_import::test_fixtures::flac_dir();
    let fixture_files =
        vec!["01 Test Track 1.flac", "02 Test Track 2.flac", "03 Test Track 3.flac"];
    let mut file_data = Vec::new();
    for fixture_name in fixture_files {
        let fixture_path = fixture_dir.join(fixture_name);
        let test_path = dir.join(fixture_name);
        let data = bae_import::test_fixtures::read_fixture(&fixture_path);
        fs::write(&test_path, &data).expect("Failed to copy FLAC fixture");
        file_data.push(data);
    }
    file_data
}
/// Copy the `cue_flac` fixture's FLAC and CUE files into `dir`.
pub(super) fn generate_cue_flac_files(dir: &std::path::Path) {
    use std::fs;
    let fixture_dir = bae_audio::test_fixtures::cue_flac_dir();

    let flac_src = fixture_dir.join("Test Album.flac");
    let flac_dst = dir.join("Test Album.flac");
    let flac_data = bae_import::test_fixtures::read_fixture(&flac_src);
    fs::write(&flac_dst, &flac_data).expect("Failed to copy FLAC fixture");

    let cue_src = fixture_dir.join("Test Album.cue");
    let cue_dst = dir.join("Test Album.cue");
    let cue_data = bae_import::test_fixtures::read_fixture(&cue_src);
    fs::write(&cue_dst, &cue_data).expect("Failed to copy CUE fixture");
}

/// The three-track album matching the `cue_flac` fixture.
pub(super) fn create_cue_flac_test_album() -> DiscogsRelease {
    DiscogsRelease {
        country: Some("Test Country".to_string()),
        artists: vec![bae_catalogs::test_releases::discogs_artist("test-artist-1", "Test Artist")],
        master_id: Some("test-master-cue-flac".to_string()),
        ..bae_catalogs::test_releases::discogs_test_release(
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

/// Playback of a single-file CUE/FLAC album: the `cue_flac` fixture, or one a
/// test writes through `import`. Tests that seek or pause need
/// `TestAudioDevice::RealtimeCapture`.
pub(super) struct CueFlacTestFixture {
    pub(super) playback_handle: bae_playback::PlaybackHandle,
    pub(super) progress_rx: bae_playback::PlaybackProgressFeed,
    pub(super) track_ids: Vec<String>,
    pub(super) library: TestLibrary,
    pub(super) capture_stream_rx: bae_playback::test_capture::CaptureStreamRx,
    pub(super) _temp_dir: TempDir,
}

impl PlaybackFixture for CueFlacTestFixture {
    async fn close(self) {
        self.playback_handle.shutdown().await;
        self.library.close().await.expect("close the test library");
    }
}

impl CueFlacTestFixture {
    pub(super) async fn new(
        device: bae_playback::test_capture::TestAudioDevice,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::import(device, create_cue_flac_test_album(), generate_cue_flac_files).await
    }

    /// Import a three-track CUE/FLAC album that `generate_files` writes, and
    /// play it through `device`.
    pub(super) async fn import(
        device: bae_playback::test_capture::TestAudioDevice,
        album: DiscogsRelease,
        generate_files: impl FnOnce(&std::path::Path),
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (library, imported) =
            imported_release_setup(album, "test", uuid::Uuid::new_v4().to_string(), generate_files)
                .await?;
        assert_eq!(imported.track_ids.len(), 3, "Should have 3 tracks from CUE/FLAC");
        let (playback_handle, capture_stream_rx) =
            bae_library::test_libraries::start_capture_playback(&library, device).await;
        let progress_rx = playback_handle.subscribe_progress();
        Ok(Self {
            playback_handle,
            progress_rx,
            track_ids: imported.track_ids,
            library,
            capture_stream_rx,
            _temp_dir: imported.temp_dir,
        })
    }

    /// The next transition's captured stream, in the order streams were created.
    pub(super) async fn next_capture_stream(&mut self) -> bae_playback::CapturedStream {
        bae_playback::test_capture::next_capture_stream(&mut self.capture_stream_rx).await
    }
}

/// The side-pause album playing through a capture sink, plus the library
/// manager the side-pause setting is written through.
pub(super) struct SidePauseTestFixture {
    pub(super) playback_handle: bae_playback::PlaybackHandle,
    pub(super) library: TestLibrary,
    pub(super) progress_rx: bae_playback::PlaybackProgressFeed,
    pub(super) track_ids: Vec<String>,
    pub(super) release_id: String,
    pub(super) capture_stream_rx: bae_playback::test_capture::CaptureStreamRx,
    /// The service's playback clock. Nothing moves it but the test, so a
    /// side-pause countdown runs out exactly when the test says.
    pub(super) clock: Arc<bae_playback::ManualPlaybackClock>,
    pub(super) _temp_dir: TempDir,
}

impl PlaybackFixture for SidePauseTestFixture {
    async fn close(self) {
        self.playback_handle.shutdown().await;
        self.library.close().await.expect("close the test library");
    }
}

impl SidePauseTestFixture {
    pub(super) async fn new(
        format: &str,
        positions: [&str; 3],
        pause_between_sides: bool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::with_settings(format, positions, pause_between_sides, SidePauseCountdown::Off).await
    }

    /// A vinyl A1/A2/B1 fixture that pauses between sides with `countdown`.
    pub(super) async fn with_countdown(
        countdown: SidePauseCountdown,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::with_settings("Vinyl", ["A1", "A2", "B1"], true, countdown).await
    }

    pub(super) async fn with_settings(
        format: &str,
        positions: [&str; 3],
        pause_between_sides: bool,
        countdown: SidePauseCountdown,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let import_ids = SequentialIds::new();
        let import_id = import_ids.new_id().to_string();
        let (library, imported) = imported_release_setup(
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
        library.set_pause_between_sides(pause_between_sides).await?;
        library.set_side_pause_countdown(countdown).await?;
        assert_eq!(imported.track_ids.len(), 3, "side-pause fixture imports 3 tracks");

        // Paced to wall-clock time so commands issued after play land before
        // the track's boundary.
        let clock = Arc::new(bae_playback::ManualPlaybackClock::new(side_pause_clock_start()));
        let (playback_handle, capture_stream_rx) =
            bae_library::test_libraries::start_capture_playback_with_clock(
                &library,
                bae_playback::test_capture::TestAudioDevice::RealtimeCapture,
                clock.clone(),
            )
            .await;
        let progress_rx = playback_handle.subscribe_progress();
        Ok(Self {
            playback_handle,
            library,
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
    pub(super) async fn set_pause_between_sides_mid_track(&self, enabled: bool) {
        self.library.set_pause_between_sides(enabled).await.expect("set_pause_between_sides");
        if enabled {
            self.playback_handle.reevaluate_side_pause_staging();
        }
    }

    pub(super) async fn wait_for_state<F>(&mut self, predicate: F) -> Option<PlaybackState>
    where
        F: Fn(&PlaybackState) -> bool,
    {
        wait_for_state_on(&mut self.progress_rx, predicate).await
    }

    /// See `settled_events_on`.
    pub(super) async fn settled_events(&mut self) -> Vec<PlaybackProgress> {
        settled_events_on(&self.playback_handle, &mut self.progress_rx).await
    }

    pub(super) async fn next_capture_stream(&mut self) -> bae_playback::CapturedStream {
        bae_playback::test_capture::next_capture_stream(&mut self.capture_stream_rx).await
    }

    pub(super) async fn play_release_from(&self, track_id: &str) {
        let played = self
            .playback_handle
            .play_release(self.release_id.clone(), Some(track_id.to_string()), false)
            .await
            .expect("the play is answered");
        assert_eq!(played, bae_playback::PlayOutcome::Playing);
    }

    /// Seek to 200 ms before the end of the 5 s track. Commands run in order, so
    /// anything that must apply at the boundary goes before this.
    pub(super) fn seek_to_auto_advance(&self) {
        self.playback_handle
            .seek(TrackTime::from_duration(Duration::from_secs(4) + Duration::from_millis(800)));
    }

    pub(super) async fn wait_for_playing_track(&mut self, track_id: &str, message: &str) {
        self.wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track, .. } if track.track_id == track_id),
        )
        .await
        .expect(message);
    }

    pub(super) async fn play_track_and_wait(&mut self, track_id: &str) {
        self.play_release_from(track_id).await;
        self.wait_for_playing_track(track_id, "track before side boundary should start").await;
    }

    pub(super) async fn wait_for_side_pause(
        &mut self,
        expected_side_label: &str,
        expected_boundary: PlaybackPauseBoundary,
    ) -> PlaybackState {
        self.wait_for_state(|s| {
            matches!(
                s,
                PlaybackState::Paused {
                    reason: PlaybackPauseReason::SideEnded(prompt),
                    ..
                } if prompt.side_label == expected_side_label
                    && prompt.boundary == expected_boundary
            )
        })
        .await
        .expect("side boundary should pause")
    }

    pub(super) async fn play_to_side_pause(
        &mut self,
        track_id: &str,
        expected_side_label: &str,
        expected_boundary: PlaybackPauseBoundary,
    ) -> PlaybackState {
        self.play_track_and_wait(track_id).await;
        self.seek_to_auto_advance();
        self.wait_for_side_pause(expected_side_label, expected_boundary).await
    }
}

/// Where a side-pause fixture's playback clock starts.
pub(super) fn side_pause_clock_start() -> chrono::DateTime<chrono::Utc> {
    "2026-01-01T00:00:00Z".parse().unwrap()
}

pub(super) fn create_side_pause_test_album(format: &str, positions: [&str; 3]) -> DiscogsRelease {
    let mut release = create_test_album();
    release.id = format!("side-pause-{format}-{}", positions.join("_"));
    release.title = format!("{format} Side Pause Fixture");
    // "2xCD" is two of a CD format entry; a bare name is one.
    let (qty, name) = format
        .split_once('x')
        .filter(|(qty, _)| qty.parse::<u32>().is_ok())
        .unwrap_or(("1", format));
    release.formats = vec![bae_catalogs::discogs::DiscogsFormat {
        name: name.to_string(),
        qty: qty.to_string(),
        descriptions: Vec::new(),
        text: None,
    }];
    for (track, position) in release.tracklist.iter_mut().zip(positions) {
        track.position = position.to_string();
    }
    release
}

/// A 3-track CUE album over one FLAC large enough that the streaming reader's
/// fetch windows (4 MiB each) cover only slices of it — playing one track
/// buffers the file head (container probe) and a window at that track's byte
/// offset, leaving the other tracks' regions unfetched.
///
/// The boundaries are deliberately mixed: track 2 has a 2-second pregap
/// (INDEX 00 at 0:58), so a manual Next into it takes the pregap-skip rebuild
/// path; track 3 has none, so a manual Next into it promotes the preloaded
/// stream. Raw in-track timelines by construction: track 1 spans 0:00–0:58
/// (ends at track 2's INDEX 00), track 2 spans 0:58–2:00 (62 s including its
/// pregap), track 3 spans 2:00–3:00.
pub(super) fn generate_multi_window_cue_flac_files(dir: &std::path::Path) {
    std::fs::write(dir.join("Multi Window Album.flac"), MULTI_WINDOW_FLAC.as_slice())
        .expect("write the multi-window FLAC");

    let cue = "\
PERFORMER \"Test Artist\"
TITLE \"Multi Window Album\"
FILE \"Multi Window Album.flac\" WAVE
  TRACK 01 AUDIO
    TITLE \"Multi Window One\"
    PERFORMER \"Test Artist\"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE \"Multi Window Two\"
    PERFORMER \"Test Artist\"
    INDEX 00 00:58:00
    INDEX 01 01:00:00
  TRACK 03 AUDIO
    TITLE \"Multi Window Three\"
    PERFORMER \"Test Artist\"
    INDEX 01 02:00:00
";
    std::fs::write(dir.join("Multi Window Album.cue"), cue).expect("write the multi-window CUE");
}

pub(super) fn create_multi_window_cue_album() -> DiscogsRelease {
    DiscogsRelease {
        artists: vec![bae_catalogs::test_releases::discogs_artist("test-artist-1", "Test Artist")],
        master_id: Some("multi-window-cue-master".to_string()),
        ..bae_catalogs::test_releases::discogs_test_release(
            "multi-window-cue-release",
            "Multi Window Album",
            &[
                ("Multi Window One", "1:00"),
                ("Multi Window Two", "1:00"),
                ("Multi Window Three", "1:00"),
            ],
        )
    }
}

/// An imported multi-window CUE album with a real-time-paced playback service
/// over it, for tests that drive switches, advances, and boundaries against a
/// sparsely buffered file.
pub(super) struct MultiWindowPlayback {
    pub(super) playback_handle: bae_playback::PlaybackHandle,
    pub(super) progress_rx: bae_playback::PlaybackProgressFeed,
    pub(super) track_ids: Vec<String>,
    pub(super) library: TestLibrary,
    pub(super) _capture_stream_rx: bae_playback::test_capture::CaptureStreamRx,
    pub(super) _temp_dir: TempDir,
}

impl MultiWindowPlayback {
    /// Import this test's release into its own store.
    pub(super) async fn new(name: &str) -> Self {
        let (library, imported) = imported_release_setup(
            create_multi_window_cue_album(),
            name,
            uuid::Uuid::new_v4().to_string(),
            generate_multi_window_cue_flac_files,
        )
        .await
        .expect("import the multi-window fixture");
        let temp_dir = imported.temp_dir;
        let track_ids = imported.track_ids;
        let (playback_handle, capture_stream_rx) = start_capture_service(&library).await;
        let progress_rx = playback_handle.subscribe_progress();
        Self {
            playback_handle,
            progress_rx,
            track_ids,
            library,
            _capture_stream_rx: capture_stream_rx,
            _temp_dir: temp_dir,
        }
    }

    /// Play `track_id` and wait for its Playing state.
    pub(super) async fn play_and_wait(&mut self, track_id: &str) {
        play_and_wait_on(&self.playback_handle, &mut self.progress_rx, track_id).await;
    }
}

impl PlaybackFixture for MultiWindowPlayback {
    async fn close(self) {
        self.playback_handle.shutdown().await;
        self.library.close().await.expect("close the test library");
    }
}

/// One ~30 MiB noise FLAC shared by every multi-window test. Encoded once —
/// per-test encoding would dominate the suite's wall time. White noise
/// compresses poorly, so 3 minutes of it stays near raw PCM size.
pub(super) static MULTI_WINDOW_FLAC: std::sync::LazyLock<Vec<u8>> =
    std::sync::LazyLock::new(|| {
        use rand::{Rng, SeedableRng};

        bae_audio::init();

        const SAMPLE_RATE: u32 = 44_100;
        const TRACK_SECONDS: u32 = 60;
        const TRACKS: u32 = 3;

        let frames = (SAMPLE_RATE * TRACK_SECONDS * TRACKS) as usize;
        // The pregap region (0:58-1:00, track 2's INDEX 00 gap) is silence, like a
        // real rip's pregap. Silence compresses to a few KB, so the pregap becomes
        // a tiny segment whose decode must read past its own end byte (raw-FLAC
        // frame parsing needs lookahead) -- the fill has to serve a reader at its
        // read-ahead ceiling or the preload decoder deadlocks.
        let pregap_samples = (SAMPLE_RATE * 58) as usize * 2..(SAMPLE_RATE * 60) as usize * 2;
        let mut rng = rand::rngs::StdRng::seed_from_u64(0x0bae);
        let samples: Vec<i32> =
            (0..frames * 2)
                .map(|i| {
                    if pregap_samples.contains(&i) {
                        0
                    } else {
                        (rng.random::<i16>() as i32) << 16
                    }
                })
                .collect();
        let flac = bae_audio::encode_i32(
            bae_audio::EncodeFormat::Flac { bits_per_sample: 16 },
            &samples,
            SAMPLE_RATE,
            2,
        )
        .expect("encode the multi-window noise FLAC");
        assert!(
            flac.len() > 24 * 1024 * 1024,
            "the fixture must span several 4 MiB fetch windows, got {} bytes",
            flac.len()
        );
        flac
    });

/// An imported test library with no playback service running, so a test can
/// write a `playback_state` row and then start its own service to exercise
/// restore without racing a fixture's service for the row.
pub(super) struct RestoreTestLibrary {
    /// The library, which a test's own playback and import services run
    /// over.
    pub(super) library: TestLibrary,
    /// The catalog fakes the library asks, which a test that imports again
    /// seeds.
    pub(super) fakes: bae_catalogs::fake_catalogs::FakeCatalogs,
    pub(super) track_ids: Vec<String>,
    pub(super) _temp_dir: TempDir,
}

pub(super) async fn restore_test_library() -> RestoreTestLibrary {
    let fakes = bae_catalogs::fake_catalogs::FakeCatalogs::offline();
    let (library, album_dir, temp_dir) =
        bae_library::test_libraries::setup_test_library_with_album_dir(
            fakes.providers(),
            bae_util::http::Http::for_test(),
        )
        .await;
    let _ = generate_test_flac_files(&album_dir);
    let release_id_key = fakes.seed_discogs_test_release(create_test_album());
    library.import_folder(album_dir, release_id_key).await;
    let releases = library
        .get_releases_for_album(
            &bae_views::LibraryViews::new(library.store().clone()).get_albums(&[]).await.unwrap()
                [0]
            .id,
        )
        .await
        .unwrap();
    let tracks = library.get_tracks_for_release(&releases[0].id).await.unwrap();
    let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
    assert!(!track_ids.is_empty(), "the test album imported some tracks");
    RestoreTestLibrary { library, fakes, track_ids, _temp_dir: temp_dir }
}

/// A library with no imports, for the empty-library no-op paths, beside the
/// temp dir that keeps its files.
pub(super) async fn empty_test_library() -> (TestLibrary, TempDir) {
    bae_util::test_logs::install();
    let temp_dir = TempDir::new().unwrap();
    let library = open_test_library(temp_dir.path()).await;
    (library, temp_dir)
}

/// Import a second, distinct-content release (the CUE/FLAC album) into `lib`'s
/// library so a test can have a current track in one release and a
/// preloaded/queued track in another — needed to delete one without the other.
/// Returns the new release's id and track ids; the source `TempDir` is returned
/// so the caller keeps it alive.
pub(super) async fn import_second_release(
    lib: &RestoreTestLibrary,
) -> (String, Vec<String>, TempDir) {
    let source = TempDir::new().unwrap();
    generate_cue_flac_files(source.path());
    let release_key = lib.fakes.seed_discogs_test_release(create_cue_flac_test_album());
    let import_handle = lib.library.start_discogs_import().await;
    let import_id = uuid::Uuid::new_v4().to_string();
    import_handle
        .send_command(ImportCommand {
            candidate_key: bae_import::CandidateKey::from(b"second".to_vec()),
            ..bae_import::test_imports::folder_import(
                &import_id,
                source.path().to_path_buf(),
                bae_import::test_imports::discogs_release(release_key),
            )
        })
        .await
        .unwrap();
    let mut progress_rx = import_handle.subscribe_import(import_id);
    let (release_id, _album_id) = wait_for_import_complete(&mut progress_rx).await;
    let tracks = lib
        .library
        .get_tracks_for_release(&release_id)
        .await
        .unwrap()
        .iter()
        .map(|t| t.id.clone())
        .collect();
    (release_id, tracks, source)
}
