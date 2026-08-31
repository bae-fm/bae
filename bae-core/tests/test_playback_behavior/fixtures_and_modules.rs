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
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut signaled = false;
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(200), fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::Seeked { .. }))
            | Ok(Some(PlaybackProgress::PlaybackError { .. })) => {
                signaled = true;
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    assert!(
        signaled,
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
        id: "test-playback-123".to_string(),
        title: "Playback Test Album".to_string(),
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
        tracklist: vec![
            DiscogsTrack {
                type_: "track".to_string(),
                position: "1".to_string(),
                title: "Test Track 1".to_string(),
                duration: Some("0:10".to_string()),
                artists: vec![],
                extraartists: None,
                sub_tracks: vec![],
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "2".to_string(),
                title: "Test Track 2".to_string(),
                duration: Some("0:10".to_string()),
                artists: vec![],
                extraartists: None,
                sub_tracks: vec![],
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "3".to_string(),
                title: "Test Track 3".to_string(),
                duration: Some("0:10".to_string()),
                artists: vec![],
                extraartists: None,
                sub_tracks: vec![],
            },
        ],
        master_id: Some("test-master-123".to_string()),
    }
}
/// Copy pre-generated FLAC fixtures to test directory
/// Fixtures should be generated using scripts/generate_test_flac.sh
fn generate_test_flac_files(dir: &std::path::Path) -> Vec<Vec<u8>> {
    use std::fs;
    let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("flac");
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
    let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cue_flac");

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
        id: "cue-flac-test-release".to_string(),
        title: "Test Album".to_string(),
        year: Some(2024),
        format: vec![],
        country: Some("Test Country".to_string()),
        label: vec!["Test Label".to_string()],
        cover_image: None,
        thumb: None,
        catno: None,
        artists: vec![DiscogsArtist {
            name: "Test Artist".to_string(),
            id: "test-artist-1".to_string(),
        }],
        extraartists: Some(vec![]),
        tracklist: vec![
            DiscogsTrack {
                type_: "track".to_string(),
                position: "1".to_string(),
                title: "Track One (Silence)".to_string(),
                duration: Some("0:10".to_string()),
                artists: vec![],
                extraartists: None,
                sub_tracks: vec![],
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "2".to_string(),
                title: "Track Two (White Noise)".to_string(),
                duration: Some("0:10".to_string()),
                artists: vec![],
                extraartists: None,
                sub_tracks: vec![],
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "3".to_string(),
                title: "Track Three (Brown Noise)".to_string(),
                duration: Some("0:10".to_string()),
                artists: vec![],
                extraartists: None,
                sub_tracks: vec![],
            },
        ],
        master_id: Some("test-master-cue-flac".to_string()),
    }
}

/// Test fixture for CUE/FLAC playback (single FLAC with CUE sheet)
struct CueFlacTestFixture {
    playback_handle: bae_core::playback::PlaybackHandle,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_ids: Vec<String>,
    capture_stream_rx: tokio::sync::mpsc::UnboundedReceiver<Arc<std::sync::Mutex<Vec<f32>>>>,
    _temp_dir: TempDir,
}

async fn next_capture_stream_from(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<Arc<std::sync::Mutex<Vec<f32>>>>,
) -> Arc<std::sync::Mutex<Vec<f32>>> {
    match timeout(Duration::from_secs(5), rx.recv()).await {
        Ok(Some(buf)) => buf,
        Ok(None) => panic!("capture stream channel closed before a stream was created"),
        Err(_) => panic!("no capture stream created within 5s"),
    }
}

impl CueFlacTestFixture {
    /// Awaits the next capture buffer minted by `create_stream`. Buffers are
    /// yielded in creation order; tests that exercise auto-advance, seek, or
    /// next call this once per stream they want to inspect.
    async fn next_capture_stream(&mut self) -> Arc<std::sync::Mutex<Vec<f32>>> {
        next_capture_stream_from(&mut self.capture_stream_rx).await
    }

    /// Full-speed capture: the drain pulls as fast as the decoder fills. Fast,
    /// but a track can fully decode and gaplessly advance before a follow-up
    /// command lands — use `with_realtime_capture` for seek/pause tests.
    async fn with_capture() -> Result<Self, Box<dyn std::error::Error>> {
        let (capture_device, capture_stream_rx) = bae_core::playback::CaptureAudioDevice::new();
        Self::with_capture_device(Box::new(capture_device), capture_stream_rx).await
    }

    /// Real-time-paced capture: the drain sleeps each buffer's wall-clock
    /// duration, so the decoder fills the ring and parks instead of racing
    /// whole tracks ahead. Required for tests that play, then issue a command
    /// (seek, pause) that must land on the track under test.
    async fn with_realtime_capture() -> Result<Self, Box<dyn std::error::Error>> {
        let (capture_device, capture_stream_rx) =
            bae_core::playback::RealtimeCaptureAudioDevice::new();
        Self::with_capture_device(Box::new(capture_device), capture_stream_rx).await
    }

    async fn with_capture_device(
        capture_device: Box<dyn bae_core::playback::AudioOutputDevice>,
        capture_stream_rx: tokio::sync::mpsc::UnboundedReceiver<Arc<std::sync::Mutex<Vec<f32>>>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
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

        // Use CUE/FLAC fixtures
        let discogs_release = create_cue_flac_test_album();
        let release_id_key = seed_discogs_test_release(discogs_release);
        generate_cue_flac_files(&album_dir);

        let import_handle =
            start_test_import(runtime_handle.clone(), library_manager.clone()).await;

        let import_id = uuid::Uuid::new_v4().to_string();

        // Import without storage (local CUE/FLAC playback)
        import_handle
            .send_command(ImportCommand {
                import_id: import_id.clone(),
                candidate_key: "test".to_string(),
                folder: album_dir.clone(),
                scope: bae_core::import::ReleaseFileScope::Recursive,
                selected_cover: None,
                storage_mode: StorageMode::Local,
                pin: false,
                metadata_provenance: Some(MetadataProvenance::ExternalRelease {
                    source: MetadataSource::Discogs,
                release_id: release_id_key,
                }),
                user_edit: None,
            })
            .await
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
        assert_eq!(track_ids.len(), 3, "Should have 3 tracks from CUE/FLAC");

        let playback_handle = library_manager.start_playback_service_with_audio_device(
            runtime_handle,
            100,
            true,
            capture_device,
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

struct SidePauseTestFixture {
    playback_handle: bae_core::playback::PlaybackHandle,
    library_manager: LibraryManager,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_ids: Vec<String>,
    release_id: String,
    capture_stream_rx: tokio::sync::mpsc::UnboundedReceiver<Arc<std::sync::Mutex<Vec<f32>>>>,
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
        let setup = imported_release_setup(
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
            setup.track_ids.len(),
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
        let (capture_device, capture_stream_rx) =
            bae_core::playback::RealtimeCaptureAudioDevice::new();
        let playback_handle = setup
            .library_manager
            .start_playback_service_with_audio_device(
                setup.runtime_handle,
                100,
                true,
                Box::new(capture_device),
            );
        let progress_rx = playback_handle.subscribe_progress();

        Ok(Self {
            playback_handle,
            library_manager: setup.library_manager,
            progress_rx,
            track_ids: setup.track_ids,
            release_id: setup.release_id,
            capture_stream_rx,
            _temp_dir: setup.temp_dir,
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
        next_capture_stream_from(&mut self.capture_stream_rx).await
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
        expected_side_letter: &str,
        expected_message_key: &str,
    ) -> PlaybackState {
        self.wait_for_state(
            |s| {
                matches!(
                    s,
                    PlaybackState::Paused {
                        reason: PlaybackPauseReason::SideEnded(prompt),
                        ..
                    } if prompt.side_letter == expected_side_letter
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
        expected_side_letter: &str,
        expected_message_key: &str,
    ) -> PlaybackState {
        self.play_track_and_wait(start_track_index, track_id).await;
        self.seek_to_auto_advance();
        self.wait_for_side_pause(expected_side_letter, expected_message_key)
            .await
    }
}

fn create_side_pause_test_album(format: &str, positions: [&str; 3]) -> DiscogsRelease {
    let mut release = create_test_album();
    release.id = format!("side-pause-{format}");
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
include!("queue_and_pregap.rs");
include!("high_rate_and_restore.rs");
include!("local_sparse_buffer.rs");
include!("remote_sparse_buffer.rs");
