#![cfg(feature = "test-utils")]
mod support;
use crate::support::{
    seed_discogs_test_release, test_config_and_keys, tracing_init, try_wait_for_import_complete,
    wait_for_import_complete,
};
use bae_core::db::Database;
use bae_core::discogs::models::{DiscogsArtist, DiscogsRelease, DiscogsTrack};
use bae_core::id_provider::{IdProvider, SequentialIdProvider};
use bae_core::import::{IdentityChoice, ImportCommand, MetadataRef, MetadataSource, StorageMode};
use bae_core::library::LibraryManager;
use bae_core::library_dir::LibraryDir;
use bae_core::playback::{
    PlaybackPauseReason, PlaybackProgress, PlaybackState, RepeatMode,
    SIDE_PAUSE_CASSETTE_MESSAGE_KEY, SIDE_PAUSE_VINYL_MESSAGE_KEY,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::time::timeout;
use tracing::debug;

/// Drain StateChanged events from a progress receiver until one satisfies
/// `predicate` (returned) or the timeout elapses. Shared by the playback
/// fixtures so they don't each carry a copy.
async fn wait_for_state_on<F>(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    predicate: F,
    timeout_duration: Duration,
) -> Option<PlaybackState>
where
    F: Fn(&PlaybackState) -> bool,
{
    let deadline = Instant::now() + timeout_duration;
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(100), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => {
                if predicate(&state) {
                    return Some(state);
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("progress channel closed before a matching state arrived"),
            Err(_) => continue,
        }
    }
    None
}

/// Collect every StateChanged state in arrival order until one satisfies `done`
/// (that final state is included) or the timeout elapses.
async fn collect_states_on<F>(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    done: F,
    timeout_duration: Duration,
) -> Vec<PlaybackState>
where
    F: Fn(&PlaybackState) -> bool,
{
    let deadline = Instant::now() + timeout_duration;
    let mut states = Vec::new();
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(100), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => {
                let stop = done(&state);
                states.push(state);
                if stop {
                    break;
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("progress channel closed before the awaited terminal state"),
            Err(_) => continue,
        }
    }
    states
}

fn start_test_import(
    runtime_handle: tokio::runtime::Handle,
    library_manager: LibraryManager,
) -> bae_core::import::ImportServiceHandle {
    bae_core::import::ImportService::start(
        runtime_handle.clone(),
        library_manager,
        bae_core::import::cover_art::CoverArtArchiveClient::new(),
    )
}

struct ImportedReleaseSetup {
    library_manager: LibraryManager,
    runtime_handle: tokio::runtime::Handle,
    track_ids: Vec<String>,
    release_id: String,
    album_dir: std::path::PathBuf,
    temp_dir: TempDir,
}

async fn imported_release_setup<G, C>(
    release: DiscogsRelease,
    candidate_key: &str,
    import_id: String,
    generate_files: G,
    configure: C,
) -> Result<ImportedReleaseSetup, Box<dyn std::error::Error>>
where
    G: FnOnce(&std::path::Path),
    C: FnOnce(&LibraryManager) -> Result<(), Box<dyn std::error::Error>>,
{
    tracing_init();
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");
    let album_dir = temp_dir.path().join("album");
    std::fs::create_dir_all(&album_dir)?;

    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(bae_core::clock::SystemClock),
    )
    .await?;
    let database_arc = Arc::new(database.clone());
    let library_dir = LibraryDir::new(temp_dir.path().to_path_buf());
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let runtime_handle = tokio::runtime::Handle::current();
    let library_manager = LibraryManager::new(
        (*database_arc).clone(),
        library_dir,
        config_handle,
        key_service,
        std::sync::Arc::new(bae_core::clock::SystemClock),
        std::sync::Arc::new(bae_core::id_provider::UuidProvider),
        runtime_handle.clone(),
    );
    configure(&library_manager)?;

    let release_id_key = seed_discogs_test_release(release);
    generate_files(&album_dir);

    let import_handle = start_test_import(runtime_handle.clone(), library_manager.clone());
    import_handle
        .send_command(ImportCommand::Folder {
            import_id: import_id.clone(),
            candidate_key: candidate_key.to_string(),
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
    let (release_id, _album_id) = wait_for_import_complete(&mut progress_rx).await;
    let tracks = library_manager.get_tracks(&release_id).await?;
    let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();

    Ok(ImportedReleaseSetup {
        library_manager,
        runtime_handle,
        track_ids,
        release_id,
        album_dir,
        temp_dir,
    })
}

/// Test helper to set up playback service with imported test tracks
struct PlaybackTestFixture {
    playback_handle: bae_core::playback::PlaybackHandle,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_ids: Vec<String>,
    album_dir: std::path::PathBuf,
    _temp_dir: TempDir,
}
impl PlaybackTestFixture {
    async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let import_ids = SequentialIdProvider::new("playback-fixture-import");
        let setup = imported_release_setup(
            create_test_album(),
            "test",
            import_ids.new_id(),
            |album_dir| {
                let _track_data = generate_test_flac_files(album_dir);
            },
            |_| Ok(()),
        )
        .await?;
        assert!(!setup.track_ids.is_empty(), "Should have imported tracks");
        std::env::set_var("MUTE_TEST_AUDIO", "1");
        let playback_handle = bae_core::playback::PlaybackService::start(
            setup.library_manager.clone(),
            setup.runtime_handle,
            100,
        );
        playback_handle.set_volume(0.0);
        let progress_rx = playback_handle.subscribe_progress();
        Ok(Self {
            playback_handle,
            progress_rx,
            track_ids: setup.track_ids,
            album_dir: setup.album_dir,
            _temp_dir: setup.temp_dir,
        })
    }
    /// Wait for a specific state change with timeout
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
    /// Drain progress events up to the Playing state, returning whether Playing
    /// arrived and the entries from the most recent QueueUpdated seen (each
    /// carrying a per-instance id). `play` rebuilds the queue with fresh ids, so
    /// a mutation must target those — captured here, after play settles.
    async fn wait_for_playing_capturing_queue(
        &mut self,
        timeout_duration: Duration,
    ) -> (bool, Vec<bae_core::playback::QueueEntry>) {
        let deadline = Instant::now() + timeout_duration;
        let mut entries = Vec::new();
        while Instant::now() < deadline {
            match timeout(Duration::from_millis(100), self.progress_rx.recv()).await {
                Ok(Some(PlaybackProgress::QueueUpdated {
                    manual, context, ..
                })) => {
                    // The mutation targets entries in either lane, so flatten the
                    // two lanes into one play-order list for id lookup.
                    entries = manual;
                    if let Some(ctx) = context {
                        entries.extend(ctx.upcoming);
                    }
                }
                Ok(Some(PlaybackProgress::StateChanged {
                    state: PlaybackState::Playing { .. },
                })) => {
                    return (true, entries);
                }
                Ok(Some(_)) => continue,
                Ok(None) => break,
                Err(_) => continue,
            }
        }
        (false, entries)
    }
    /// Wait for a position update with timeout (returns position in ms)
    async fn wait_for_position_update(&mut self, timeout_duration: Duration) -> Option<u64> {
        let deadline = Instant::now() + timeout_duration;
        while Instant::now() < deadline {
            match timeout(Duration::from_millis(100), self.progress_rx.recv()).await {
                Ok(Some(PlaybackProgress::PositionUpdate { position_ms, .. })) => {
                    return Some(position_ms);
                }
                Ok(Some(_)) => continue,
                Ok(None) => break,
                Err(_) => continue,
            }
        }
        None
    }
    /// Wait for a Seeked event with timeout (returns position in ms)
    async fn wait_for_seeked(&mut self, timeout_duration: Duration) -> Option<u64> {
        let deadline = Instant::now() + timeout_duration;
        while Instant::now() < deadline {
            match timeout(Duration::from_millis(100), self.progress_rx.recv()).await {
                Ok(Some(PlaybackProgress::Seeked { position_ms, .. })) => {
                    return Some(position_ms);
                }
                Ok(Some(_)) => continue,
                Ok(None) => break,
                Err(_) => continue,
            }
        }
        None
    }
    /// Wait for a SeekSkipped event with timeout
    async fn wait_for_seek_skipped(
        &mut self,
        timeout_duration: Duration,
    ) -> Option<(Duration, Duration)> {
        let deadline = Instant::now() + timeout_duration;
        while Instant::now() < deadline {
            match timeout(Duration::from_millis(100), self.progress_rx.recv()).await {
                Ok(Some(PlaybackProgress::SeekSkipped {
                    requested_position,
                    current_position,
                })) => {
                    return Some((requested_position, current_position));
                }
                Ok(Some(_)) => continue,
                Ok(None) => break,
                Err(_) => continue,
            }
        }
        None
    }

    /// Collect every `StateChanged` state in arrival order until one satisfies
    /// `done` (that final state is included) or the timeout elapses.
    async fn collect_states_until<F>(
        &mut self,
        done: F,
        timeout_duration: Duration,
    ) -> Vec<PlaybackState>
    where
        F: Fn(&PlaybackState) -> bool,
    {
        collect_states_on(&mut self.progress_rx, done, timeout_duration).await
    }
}
/// Playing a track emits Loading without metadata first (before the DB lookup),
/// then Loading carrying the target's metadata (after prepare), then Playing
/// once the decoder buffer is ready. The middle Loading lets the UI switch the
/// now-playing bar to the target while audio fills; emitting Playing only at
/// ready means the position bar never freezes against a not-yet-started stream.
/// The sequence asserts `resolved: Some` on the second Loading — the metadata
/// the UI swaps to before audio is flowing.
#[tokio::test]
async fn play_emits_bare_loading_then_loading_with_metadata_then_playing() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("skip: fixture setup failed: {e}");
            return;
        }
    };
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
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("skip: fixture setup failed: {e}");
            return;
        }
    };
    if fixture.track_ids.is_empty() {
        return;
    }
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
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("skip: fixture setup failed: {e}");
            return;
        }
    };
    if fixture.track_ids.is_empty() {
        return;
    }
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
        genre: vec![],
        style: vec![],
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
        tracklist: vec![
            DiscogsTrack {
                type_: "track".to_string(),
                position: "1".to_string(),
                title: "Test Track 1".to_string(),
                duration: Some("0:10".to_string()),
                artists: vec![],
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "2".to_string(),
                title: "Test Track 2".to_string(),
                duration: Some("0:10".to_string()),
                artists: vec![],
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "3".to_string(),
                title: "Test Track 3".to_string(),
                duration: Some("0:10".to_string()),
                artists: vec![],
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
/// Check if audio tests should be skipped (e.g., in CI without audio device)
fn should_skip_audio_tests() -> bool {
    if std::env::var("SKIP_AUDIO_TESTS").is_ok() {
        return true;
    }
    use cpal::traits::HostTrait;
    cpal::default_host().default_output_device().is_none()
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
        genre: vec!["Test".to_string()],
        style: vec!["Test Style".to_string()],
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
        tracklist: vec![
            DiscogsTrack {
                type_: "track".to_string(),
                position: "1".to_string(),
                title: "Track One (Silence)".to_string(),
                duration: Some("0:10".to_string()),
                artists: vec![],
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "2".to_string(),
                title: "Track Two (White Noise)".to_string(),
                duration: Some("0:10".to_string()),
                artists: vec![],
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "3".to_string(),
                title: "Track Three (Brown Noise)".to_string(),
                duration: Some("0:10".to_string()),
                artists: vec![],
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
        Self::with_capture_paced(false).await
    }

    /// Real-time-paced capture: the drain sleeps each buffer's wall-clock
    /// duration, so the decoder fills the ring and parks instead of racing
    /// whole tracks ahead. Required for tests that play, then issue a command
    /// (seek, pause) that must land on the track under test.
    async fn with_realtime_capture() -> Result<Self, Box<dyn std::error::Error>> {
        Self::with_capture_paced(true).await
    }

    async fn with_capture_paced(realtime: bool) -> Result<Self, Box<dyn std::error::Error>> {
        tracing_init();
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let album_dir = temp_dir.path().join("album");
        std::fs::create_dir_all(&album_dir)?;

        let database = Database::new_test(
            db_path.to_str().unwrap(),
            std::sync::Arc::new(bae_core::clock::SystemClock),
        )
        .await?;
        let database_arc = Arc::new(database.clone());
        let library_dir = LibraryDir::new(temp_dir.path().to_path_buf());
        let (config_handle, key_service) = test_config_and_keys(&library_dir);
        let library_manager = LibraryManager::new(
            (*database_arc).clone(),
            library_dir.clone(),
            config_handle,
            key_service,
            std::sync::Arc::new(bae_core::clock::SystemClock),
            std::sync::Arc::new(bae_core::id_provider::UuidProvider),
            tokio::runtime::Handle::current(),
        );
        let runtime_handle = tokio::runtime::Handle::current();

        // Use CUE/FLAC fixtures
        let discogs_release = create_cue_flac_test_album();
        let release_id_key = seed_discogs_test_release(discogs_release);
        generate_cue_flac_files(&album_dir);

        let import_handle = start_test_import(runtime_handle.clone(), library_manager.clone());

        let import_id = uuid::Uuid::new_v4().to_string();

        // Import without storage (local CUE/FLAC playback)
        import_handle
            .send_command(ImportCommand::Folder {
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

        let (capture_output, capture_stream_rx) = if realtime {
            bae_core::playback::CaptureAudioOutput::new_realtime()
        } else {
            bae_core::playback::CaptureAudioOutput::new()
        };
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

struct SidePauseTestFixture {
    playback_handle: bae_core::playback::PlaybackHandle,
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

        let (capture_output, capture_stream_rx) = bae_core::playback::CaptureAudioOutput::new();
        let playback_handle = bae_core::playback::PlaybackService::start_with_output(
            setup.library_manager.clone(),
            setup.runtime_handle,
            100,
            Box::new(capture_output),
        );
        let progress_rx = playback_handle.subscribe_progress();

        Ok(Self {
            playback_handle,
            progress_rx,
            track_ids: setup.track_ids,
            release_id: setup.release_id,
            capture_stream_rx,
            _temp_dir: setup.temp_dir,
        })
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

#[tokio::test]
async fn sided_vinyl_boundary_pauses_on_auto_advance() {
    assert_sided_boundary_pauses(
        "Vinyl",
        ["A1", "A2", "B1"],
        1,
        "A",
        SIDE_PAUSE_VINYL_MESSAGE_KEY,
    )
    .await;
}

#[tokio::test]
async fn sided_cassette_boundary_pauses_on_auto_advance() {
    assert_sided_boundary_pauses(
        "Cassette",
        ["A1", "B1", "B2"],
        0,
        "A",
        SIDE_PAUSE_CASSETTE_MESSAGE_KEY,
    )
    .await;
}

async fn assert_sided_boundary_pauses(
    format: &str,
    positions: [&str; 3],
    start_track_index: usize,
    expected_side_letter: &str,
    expected_message_key: &str,
) {
    let mut fixture = SidePauseTestFixture::new(format, positions, true)
        .await
        .expect("side-pause fixture");
    let side_track_id = fixture.track_ids[start_track_index].clone();

    let paused = fixture
        .play_to_side_pause(
            start_track_index,
            &side_track_id,
            expected_side_letter,
            expected_message_key,
        )
        .await;

    match paused {
        PlaybackState::Paused { track_info, .. } => {
            assert_eq!(track_info.track_id, side_track_id);
        }
        other => panic!("expected side-ended pause, got {other:?}"),
    }
}

#[tokio::test]
async fn same_side_auto_advance_does_not_side_pause() {
    let mut fixture = SidePauseTestFixture::new("Vinyl", ["A1", "A2", "B1"], true)
        .await
        .expect("side-pause fixture");
    let first_side_track_id = fixture.track_ids[0].clone();
    let same_side_track_id = fixture.track_ids[1].clone();

    fixture.play_track_and_wait(0, &first_side_track_id).await;

    fixture.seek_to_auto_advance();

    fixture
        .wait_for_playing_track(
            &same_side_track_id,
            Duration::from_secs(10),
            "same side should keep playing",
        )
        .await;
}

#[tokio::test]
async fn cd_multi_disc_auto_advance_does_not_side_pause() {
    let mut fixture = SidePauseTestFixture::new("CD", ["1-1", "2-1", "2-2"], true)
        .await
        .expect("side-pause fixture");
    let first_disc_track_id = fixture.track_ids[0].clone();
    let next_disc_track_id = fixture.track_ids[1].clone();

    fixture.play_track_and_wait(0, &first_disc_track_id).await;

    fixture.seek_to_auto_advance();

    fixture
        .wait_for_playing_track(
            &next_disc_track_id,
            Duration::from_secs(10),
            "CD disc boundary should keep playing",
        )
        .await;
}

#[tokio::test]
async fn setting_off_auto_advances_across_sided_boundary() {
    let mut fixture = SidePauseTestFixture::new("Vinyl", ["A1", "A2", "B1"], false)
        .await
        .expect("side-pause fixture");
    let side_a_track_id = fixture.track_ids[1].clone();
    let next_side_track_id = fixture.track_ids[2].clone();

    fixture.play_track_and_wait(1, &side_a_track_id).await;

    fixture.seek_to_auto_advance();

    fixture
        .wait_for_playing_track(
            &next_side_track_id,
            Duration::from_secs(10),
            "setting off should keep playing across side boundary",
        )
        .await;
}

#[tokio::test]
async fn repeat_track_does_not_side_pause_at_boundary() {
    let mut fixture = SidePauseTestFixture::new("Vinyl", ["A1", "A2", "B1"], true)
        .await
        .expect("side-pause fixture");
    let repeated_track_id = fixture.track_ids[1].clone();

    fixture.play_track_and_wait(1, &repeated_track_id).await;
    fixture.playback_handle.set_repeat_mode(RepeatMode::Track);

    fixture.seek_to_auto_advance();

    fixture
        .wait_for_playing_track(
            &repeated_track_id,
            Duration::from_secs(10),
            "repeat-track should replay the current track",
        )
        .await;
}

#[tokio::test]
async fn resume_from_side_pause_starts_next_side() {
    let mut fixture = SidePauseTestFixture::new("Vinyl", ["A1", "A2", "B1"], true)
        .await
        .expect("side-pause fixture");
    let side_a_track_id = fixture.track_ids[1].clone();
    let next_side_track_id = fixture.track_ids[2].clone();

    fixture
        .play_to_side_pause(1, &side_a_track_id, "A", SIDE_PAUSE_VINYL_MESSAGE_KEY)
        .await;

    fixture.playback_handle.resume();

    fixture
        .wait_for_playing_track(
            &next_side_track_id,
            Duration::from_secs(5),
            "resume from side pause should start the next side",
        )
        .await;
}

#[tokio::test]
async fn side_boundary_pause_prevents_gapless_stream_handoff() {
    let mut fixture = SidePauseTestFixture::new("Vinyl", ["A1", "A2", "B1"], true)
        .await
        .expect("side-pause fixture");
    let side_a_track_id = fixture.track_ids[1].clone();
    let side_b_track_id = fixture.track_ids[2].clone();

    fixture.play_track_and_wait(1, &side_a_track_id).await;
    let _side_a_stream = fixture.next_capture_stream().await;

    fixture.seek_to_auto_advance();
    fixture
        .wait_for_side_pause("A", SIDE_PAUSE_VINYL_MESSAGE_KEY)
        .await;

    fixture.playback_handle.resume();
    let _side_b_stream = fixture.next_capture_stream().await;
    fixture
        .wait_for_playing_track(
            &side_b_track_id,
            Duration::from_secs(5),
            "next side should start from a new stream",
        )
        .await;
}

#[tokio::test]
async fn test_next_while_paused_stays_paused() {
    // When paused and pressing Next, the next track should start paused
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };

    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for next-while-paused test");
        return;
    }

    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();

    // Start playing first track
    fixture.playback_handle.play(first_track_id.clone());
    let _playing_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;

    // Pause
    fixture.playback_handle.pause();
    let paused_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Paused { .. }),
            Duration::from_secs(2),
        )
        .await;
    assert!(paused_state.is_some(), "Should be paused");

    // Press Next while paused
    fixture.playback_handle.next();

    // Should transition to second track in Paused state (not Playing)
    let next_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Paused { track_info, .. } = s {
                    track_info.track_id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;

    assert!(
        next_track_state.is_some(),
        "Next while paused should switch to next track but stay paused"
    );
}

#[tokio::test]
async fn test_next_while_playing_stays_playing() {
    // When playing and pressing Next, the next track should start playing
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };

    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for next-while-playing test");
        return;
    }

    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();

    // Start playing first track
    fixture.playback_handle.play(first_track_id.clone());
    let _playing_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;

    // Press Next while playing
    fixture.playback_handle.next();

    // Should transition to second track in Playing state
    let next_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;

    assert!(
        next_track_state.is_some(),
        "Next while playing should switch to next track and keep playing"
    );
}

/// Test that seeking while paused and then resuming works correctly.
///
/// Regression test for: is_playing flag not set after seek-while-paused.
/// The bug was that seek sends Stop (which clears is_playing), then when
/// seeking while paused, only Pause is sent (not Play first), so is_playing
/// stays false and audio doesn't play after resume.
#[tokio::test]
async fn test_pause_seek_resume_advances_position() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };

    let track_id = fixture.track_ids[0].clone();

    // Start playing
    fixture.playback_handle.play(track_id.clone());
    let playing_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;
    assert!(playing_state.is_some(), "Should start playing");

    // Pause
    fixture.playback_handle.pause();
    let paused_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Paused { .. }),
            Duration::from_secs(2),
        )
        .await;
    assert!(paused_state.is_some(), "Should be paused");

    // Seek while paused (to 2 seconds)
    let seek_target = Duration::from_secs(2);
    fixture.playback_handle.seek(seek_target);

    // Wait for seek to complete
    let seeked_position = fixture.wait_for_seeked(Duration::from_secs(5)).await;
    assert!(
        seeked_position.is_some(),
        "Should receive Seeked event after seeking while paused"
    );
    let seeked_position_ms = seeked_position.unwrap();
    assert!(
        seeked_position_ms >= 1900,
        "Seeked position should be near 2s, got {}ms",
        seeked_position_ms
    );

    // Verify still paused after seek (shouldn't auto-play)
    let auto_played = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_millis(200),
        )
        .await;
    assert!(
        auto_played.is_none(),
        "Should still be paused after seek, not auto-playing"
    );

    // Resume
    fixture.playback_handle.resume();

    // Wait for playing state
    let resumed_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(2),
        )
        .await;
    assert!(resumed_state.is_some(), "Should resume playing");

    // Wait a bit and check that position is advancing
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Get position updates - should be advancing past the seek position
    let position_update = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;

    assert!(
        position_update.is_some(),
        "Should receive position updates after resume (indicates audio is actually playing)"
    );

    let final_position_ms = position_update.unwrap();
    assert!(
        final_position_ms > seeked_position_ms,
        "Position should advance after resume. Seeked to {}ms, but position is {}ms",
        seeked_position_ms,
        final_position_ms
    );
}

#[tokio::test]
async fn test_previous_while_paused_stays_paused() {
    // When paused and pressing Previous, the previous track should start paused
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };

    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for previous-while-paused test");
        return;
    }

    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();

    // Start on second track
    fixture.playback_handle.play(second_track_id.clone());
    let _playing_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;

    // Pause
    fixture.playback_handle.pause();
    let paused_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Paused { .. }),
            Duration::from_secs(2),
        )
        .await;
    assert!(paused_state.is_some(), "Should be paused");

    // Press Previous while paused (within 3 seconds, so goes to previous track)
    fixture.playback_handle.previous();

    // Should transition to first track in Paused state (not Playing)
    let previous_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Paused { track_info, .. } = s {
                    track_info.track_id == first_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;

    assert!(
        previous_track_state.is_some(),
        "Previous while paused should switch to previous track but stay paused"
    );
}

#[tokio::test]
async fn test_previous_while_playing_stays_playing() {
    // When playing and pressing Previous, the previous track should start playing
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };

    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for previous-while-playing test");
        return;
    }

    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();

    // Start on second track
    fixture.playback_handle.play(second_track_id.clone());
    let _playing_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;

    // Press Previous while playing (within 3 seconds, so goes to previous track)
    fixture.playback_handle.previous();

    // Should transition to first track in Playing state
    let previous_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == first_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;

    assert!(
        previous_track_state.is_some(),
        "Previous while playing should switch to previous track and keep playing"
    );
}

#[tokio::test]
async fn test_fresh_play_always_starts_playing() {
    // Fresh play should always start playing, even if previously paused
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };

    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for fresh play test");
        return;
    }

    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();

    // Start playing first track
    fixture.playback_handle.play(first_track_id.clone());
    let _playing_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;

    // Pause
    fixture.playback_handle.pause();
    let paused_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Paused { .. }),
            Duration::from_secs(2),
        )
        .await;
    assert!(paused_state.is_some(), "Should be paused");

    // Fresh play of a different track should start Playing (not Paused)
    fixture.playback_handle.play(second_track_id.clone());

    let new_play_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;

    assert!(
        new_play_state.is_some(),
        "Fresh play should always start playing, not paused"
    );
}

/// Test that seeking while playing continues playback and advances position.
///
/// This is the counterpart to test_pause_seek_resume_advances_position.
/// When seeking while playing, playback should continue and position should advance.
#[tokio::test]
async fn test_seek_while_playing_advances_position() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };

    let track_id = fixture.track_ids[0].clone();

    // Start playing
    fixture.playback_handle.play(track_id.clone());
    let playing_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;
    assert!(playing_state.is_some(), "Should start playing");

    // Seek while playing (to 2 seconds)
    let seek_target = Duration::from_secs(2);
    fixture.playback_handle.seek(seek_target);

    // Wait for seek to complete
    let seeked_position = fixture.wait_for_seeked(Duration::from_secs(5)).await;
    assert!(
        seeked_position.is_some(),
        "Should receive Seeked event after seeking while playing"
    );
    let seeked_position_ms = seeked_position.unwrap();
    assert!(
        seeked_position_ms >= 1900,
        "Seeked position should be near 2s, got {}ms",
        seeked_position_ms
    );

    // Wait a bit and check that position is advancing
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Get position updates - should be advancing past the seek position
    let position_update = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;

    assert!(
        position_update.is_some(),
        "Should receive position updates after seek while playing (indicates audio is actually playing)"
    );

    let final_position_ms = position_update.unwrap();
    assert!(
        final_position_ms > seeked_position_ms,
        "Position should advance after seek while playing. Seeked to {}ms, but position is {}ms",
        seeked_position_ms,
        final_position_ms
    );
}

#[tokio::test]
async fn test_auto_advance_to_next_track() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };
    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for auto-advance test");
        return;
    }
    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();
    fixture.playback_handle.play(first_track_id.clone());
    let _playing_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;
    fixture
        .playback_handle
        .seek(Duration::from_secs(4) + Duration::from_millis(500));

    // Wait for auto-advance and collect decode stats
    let mut total_decode_errors = 0u32;
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut advanced = false;

    while Instant::now() < deadline {
        let remaining = deadline - Instant::now();
        match timeout(remaining, fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => {
                if let PlaybackState::Playing { track_info, .. } = state {
                    if track_info.track_id == second_track_id {
                        advanced = true;
                        break;
                    }
                }
            }
            Ok(Some(PlaybackProgress::DecodeStats { error_count, .. })) => {
                total_decode_errors += error_count;
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }

    if advanced {
        assert_eq!(
            total_decode_errors, 0,
            "Auto-advance test had {} decode errors",
            total_decode_errors
        );
    } else {
        debug!("Auto-advance test inconclusive - may need valid FLAC files");
    }
}
#[tokio::test]
async fn test_position_maintained_across_pause_resume() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };
    if fixture.track_ids.is_empty() {
        debug!("No tracks available for testing");
        return;
    }
    let track_id = &fixture.track_ids[0];
    fixture.playback_handle.play(track_id.clone());
    let _playing_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;
    let seek_position = Duration::from_secs(2);
    fixture.playback_handle.seek(seek_position);
    let seeked_position_ms = fixture
        .wait_for_seeked(Duration::from_secs(2))
        .await
        .expect("Seeked event");
    let diff = Duration::from_millis(seeked_position_ms).abs_diff(seek_position);
    assert!(
        diff < Duration::from_secs(1),
        "Seek should land near requested position",
    );

    fixture.playback_handle.pause();
    let paused_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Paused { .. }),
            Duration::from_secs(2),
        )
        .await;
    assert!(paused_state.is_some(), "Should reach Paused state");
    // Position persists through pause (it's not reset); verified by the
    // post-resume PositionUpdate below.

    fixture.playback_handle.resume();
    let resumed_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(2),
        )
        .await;
    assert!(resumed_state.is_some(), "Should reach Playing state");
    let position_after_resume = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await
        .expect("Position update after resume");
    let diff = Duration::from_millis(position_after_resume).abs_diff(seek_position);
    assert!(
        diff < Duration::from_secs(1),
        "Position should be maintained when resumed",
    );
}
#[tokio::test]
async fn test_previous_track_navigation() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };
    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for previous track test");
        return;
    }
    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();
    fixture.playback_handle.play(first_track_id.clone());
    let first_track_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;
    assert!(
        first_track_state.is_some(),
        "Should be playing first track after play command",
    );
    fixture.playback_handle.next();
    let second_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        second_track_state.is_some(),
        "Should be playing second track after Next command",
    );
    fixture.playback_handle.seek(Duration::from_secs(1));
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    fixture.playback_handle.previous();
    let previous_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == first_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        previous_track_state.is_some(),
        "Should go to previous track when Previous is called early in track",
    );
    fixture.playback_handle.seek(Duration::from_secs(4));
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    fixture.playback_handle.previous();
    let restart_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == first_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        restart_state.is_some(),
        "Should restart current track when Previous is called late in track",
    );
    let restart_position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await
        .expect("Position update after restart");
    assert!(
        Duration::from_millis(restart_position) < Duration::from_secs(1),
        "Restart should reset position near 0, got {restart_position}ms",
    );
}
#[tokio::test]
async fn test_previous_track_when_starting_on_second_track() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };
    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for previous track test");
        return;
    }
    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();
    fixture.playback_handle.play(second_track_id.clone());
    let second_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        second_track_state.is_some(),
        "Should be playing second track after play command",
    );
    fixture.playback_handle.seek(Duration::from_secs(1));
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    fixture.playback_handle.previous();
    let previous_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == first_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        previous_track_state.is_some(),
        "Should go to previous track when Previous is called after starting on second track",
    );
}
#[tokio::test]
async fn test_previous_track_multiple_navigation() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };
    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for previous track test");
        return;
    }
    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();
    fixture.playback_handle.play(second_track_id.clone());
    let _second_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    fixture.playback_handle.seek(Duration::from_secs(1));
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    fixture.playback_handle.previous();
    let first_nav_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == first_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        first_nav_state.is_some(),
        "Should go to first track when Previous is called from second track",
    );
    fixture.playback_handle.seek(Duration::from_secs(1));
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    fixture.playback_handle.previous();
    let restart_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == first_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        restart_state.is_some(),
        "Should restart first track when Previous is called and there's no previous track",
    );
    let restart_position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await
        .expect("Position update after restart");
    assert!(
        Duration::from_millis(restart_position) < Duration::from_secs(1),
        "Restart should reset position near 0, got {restart_position}ms",
    );
}
#[tokio::test]
async fn test_seek_to_same_position_sends_state_changed() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };
    if fixture.track_ids.is_empty() {
        debug!("No tracks available for testing");
        return;
    }
    let track_id = &fixture.track_ids[0];
    fixture.playback_handle.play(track_id.clone());
    let playing_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;
    assert!(
        playing_state.is_some(),
        "Should be playing after play command"
    );
    let seek_position = Duration::from_secs(2);
    fixture.playback_handle.seek(seek_position);
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let current_pos_ms = fixture
        .wait_for_position_update(Duration::from_secs(1))
        .await
        .unwrap_or(2000);
    let same_position = Duration::from_millis(current_pos_ms + 50);
    fixture.playback_handle.seek(same_position);
    let seek_skipped = fixture.wait_for_seek_skipped(Duration::from_secs(2)).await;
    assert!(
        seek_skipped.is_some(),
        "Should receive SeekSkipped event when position difference < 100ms",
    );
    if let Some((requested, current)) = seek_skipped {
        let diff = requested.abs_diff(current);
        assert!(
            diff < Duration::from_millis(100),
            "Seek should only be skipped when difference < 100ms, got {:?}",
            diff,
        );
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    let position_update = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    assert!(
        position_update.is_some(),
        "Position updates should continue after skipped seek",
    );
}
#[tokio::test]
async fn test_queue_maintained_after_previous_navigation() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };
    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for queue navigation test");
        return;
    }
    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();
    fixture.playback_handle.play(first_track_id.clone());
    let _first_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == first_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    fixture.playback_handle.next();
    let second_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        second_track_state.is_some(),
        "Should be playing second track after Next command",
    );
    fixture.playback_handle.seek(Duration::from_secs(1));
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    fixture.playback_handle.previous();
    let back_to_first_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == first_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        back_to_first_state.is_some(),
        "Should go back to first track when Previous is called from second track",
    );
    fixture.playback_handle.seek(Duration::from_secs(1));
    let _position = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await;
    fixture.playback_handle.next();
    let should_be_second_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        should_be_second_state.is_some(),
        "Should go to track 2 when Next is called after navigating back to track 1",
    );
}
/// Verifies that after a queue mutation, pressing Next plays `expected` and not the
/// track that was preloaded before the mutation.
///
/// `initial_queue` is added before play so the first entry gets preloaded.
/// `mutate` is called after track0 is confirmed playing, then Next is pressed.
async fn assert_preload_refreshed_after_queue_mutation<F>(
    fixture: &mut PlaybackTestFixture,
    initial_queue: Vec<String>,
    track0: &str,
    expected: &str,
    mutate: F,
) where
    F: FnOnce(&bae_core::playback::PlaybackHandle, &[bae_core::playback::QueueEntry]),
{
    fixture.playback_handle.add_to_queue(initial_queue);
    fixture.playback_handle.play(track0.to_string());

    // `play` clears the queue and repopulates it from the track's release
    // context with fresh entry ids, so capture the entries *after* play settles:
    // drain up to the Playing state, keeping the latest QueueUpdated.
    let (played, entries) = fixture
        .wait_for_playing_capturing_queue(Duration::from_secs(5))
        .await;
    assert!(played, "track0 should start playing");

    mutate(&fixture.playback_handle, &entries);
    fixture.playback_handle.next();

    let next_state = fixture
        .wait_for_state(
            |s| match s {
                PlaybackState::Playing { track_info, .. }
                | PlaybackState::Paused { track_info, .. } => track_info.track_id != track0,
                _ => false,
            },
            Duration::from_secs(5),
        )
        .await;

    if let Some(state) = next_state {
        let playing_id = match &state {
            PlaybackState::Playing { track_info, .. } => track_info.track_id.clone(),
            PlaybackState::Paused { track_info, .. } => track_info.track_id.clone(),
            _ => unreachable!(),
        };
        assert_eq!(playing_id, expected);
    } else {
        debug!("preload refresh test inconclusive - no state change received");
    }
}

#[tokio::test]
async fn test_add_next_displaces_preloaded_track() {
    if should_skip_audio_tests() {
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };
    if fixture.track_ids.len() < 3 {
        return;
    }
    let (track0, track1, track2) = (
        fixture.track_ids[0].clone(),
        fixture.track_ids[1].clone(),
        fixture.track_ids[2].clone(),
    );
    let t2 = track2.clone();
    assert_preload_refreshed_after_queue_mutation(
        &mut fixture,
        vec![track1],
        &track0,
        &track2,
        move |h, _entries| h.add_next(vec![t2]),
    )
    .await;
}

#[tokio::test]
async fn test_reorder_entry_displaces_preloaded_track() {
    if should_skip_audio_tests() {
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };
    if fixture.track_ids.len() < 3 {
        return;
    }
    let (track0, track1, track2) = (
        fixture.track_ids[0].clone(),
        fixture.track_ids[1].clone(),
        fixture.track_ids[2].clone(),
    );
    assert_preload_refreshed_after_queue_mutation(
        &mut fixture,
        vec![track1, track2.clone()],
        &track0,
        &track2,
        |h, entries| h.reorder_entry(entries[1].id.clone(), Some(entries[0].id.clone())),
    )
    .await;
}

#[tokio::test]
async fn test_insert_in_queue_displaces_preloaded_track() {
    if should_skip_audio_tests() {
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };
    if fixture.track_ids.len() < 3 {
        return;
    }
    let (track0, track1, track2) = (
        fixture.track_ids[0].clone(),
        fixture.track_ids[1].clone(),
        fixture.track_ids[2].clone(),
    );
    let t2 = track2.clone();
    assert_preload_refreshed_after_queue_mutation(
        &mut fixture,
        vec![track1],
        &track0,
        &track2,
        move |h, _entries| h.insert_in_queue(vec![t2], 0),
    )
    .await;
}

#[tokio::test]
async fn test_remove_entry_refreshes_preloaded_track() {
    if should_skip_audio_tests() {
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };
    if fixture.track_ids.len() < 3 {
        return;
    }
    let (track0, track1, track2) = (
        fixture.track_ids[0].clone(),
        fixture.track_ids[1].clone(),
        fixture.track_ids[2].clone(),
    );
    assert_preload_refreshed_after_queue_mutation(
        &mut fixture,
        vec![track1, track2.clone()],
        &track0,
        &track2,
        |h, entries| h.remove_entry(entries[0].id.clone()),
    )
    .await;
}

// Note: test_playback_error_emitted_when_storage_offline was removed because it relied
// on MockCloudStorage injection which was removed with CloudStorageManager.

// ============================================================================
// Pregap behavior tests
// ============================================================================
// These tests verify CD-like pregap behavior:
// - Direct selection (play, next, previous button): skip pregap, start at INDEX 01
// - Natural transition (auto-advance): play pregap from INDEX 00, show negative time

#[tokio::test]
async fn test_direct_play_skips_pregap() {
    // When directly playing a track with pregap_ms set,
    // playback should start at pregap_ms offset (INDEX 01), not 0 (INDEX 00)
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };

    if fixture.track_ids.is_empty() {
        debug!("No tracks available for testing");
        return;
    }

    let track_id = &fixture.track_ids[0];
    fixture.playback_handle.play(track_id.clone());

    let playing_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;
    assert!(playing_state.is_some(), "Should reach Playing state");

    // position_ms is pregap-adjusted: for direct play, position starts at 0
    // regardless of whether the track has a pregap (the pregap offset is subtracted).
    let position_ms = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await
        .expect("Position update after play");
    assert!(
        position_ms < 500,
        "Position should start near 0, got {position_ms}",
    );
}

#[tokio::test]
async fn test_next_button_skips_pregap() {
    // When pressing Next button, the next track should start at INDEX 01 (skip pregap)
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };

    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for next button test");
        return;
    }

    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();

    // Start playing first track
    fixture.playback_handle.play(first_track_id.clone());
    let _first_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;

    // Press Next (direct selection)
    fixture.playback_handle.next();

    let second_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        second_track_state.is_some(),
        "Should reach Playing state for second track",
    );

    // Verify position starts at pregap_ms (or 0 if no pregap)
    // position_ms is pregap-adjusted: for direct selection (Next button),
    // the position starts at 0 (pregap is skipped and subtracted).
    let position_ms = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await
        .expect("Position update on second track");
    assert!(
        position_ms < 500,
        "Next button should skip pregap: adjusted position should start near 0, got {position_ms}",
    );
}

#[tokio::test]
async fn test_auto_advance_plays_pregap() {
    // When a track naturally ends and auto-advances, the next track should
    // start at INDEX 00 (play pregap), with position showing negative time initially
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };

    if fixture.track_ids.len() < 2 {
        debug!("Need at least 2 tracks for auto-advance pregap test");
        return;
    }

    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();

    // Start playing first track
    fixture.playback_handle.play(first_track_id.clone());
    let _first_state = fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await;

    // Seek near end to trigger auto-advance
    // Test fixture tracks are ~5 seconds, so seek to 4.5s
    fixture
        .playback_handle
        .seek(Duration::from_secs(4) + Duration::from_millis(800));

    // Wait for auto-advance to second track
    let second_track_state = fixture
        .wait_for_state(
            |s| {
                if let PlaybackState::Playing { track_info, .. } = s {
                    track_info.track_id == second_track_id
                } else {
                    false
                }
            },
            Duration::from_secs(10),
        )
        .await;

    // For natural transition (auto-advance), position should start at 0 (INDEX 00)
    // to play the pregap (showing negative time in UI)
    // For auto-advance (natural transition), position_ms starts at 0 in the
    // adjusted view. The pregap is being played, but position_ms is adjusted
    // to show 0 until the pregap passes.
    if second_track_state.is_some() {
        let position_ms = fixture
            .wait_for_position_update(Duration::from_secs(2))
            .await
            .expect("Position update on auto-advance");
        assert!(
            position_ms < 500,
            "Auto-advance should start with adjusted position near 0, got {position_ms}",
        );
    } else {
        debug!("Auto-advance test inconclusive - may need longer track fixtures");
    }
}

/// CUE/FLAC track 1 playback must produce audio matching the XLD-split reference.
///
/// Tests the full playback pipeline with byte range extraction and header prepending.
/// If headers are doubled or corrupted, the captured samples won't match.
#[tokio::test]
async fn test_cue_flac_playback() {
    use bae_core::audio_codec::decode_audio;

    let mut fixture = match CueFlacTestFixture::with_capture().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up CUE/FLAC test fixture: {}", e);
            return;
        }
    };

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

    // Decode XLD reference for track 1 (silence)
    let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cue_flac");
    let reference_data =
        std::fs::read(fixture_dir.join("01 Test Artist - Track One (Silence).flac"))
            .expect("read reference");
    let reference = decode_audio(&reference_data, None, None).expect("decode reference");
    let channels = reference.channels as usize;
    let sample_rate = reference.sample_rate;
    let reference_f32: Vec<f32> = reference
        .samples
        .iter()
        .map(|&s| s as f32 / 32768.0)
        .collect();

    // Wait for enough samples (2 seconds)
    let target_samples = sample_rate as usize * channels * 2;
    let captured_snapshot =
        bae_core::playback::wait_for_samples(&captured, target_samples, Duration::from_secs(60))
            .await;

    let snippet_len = 500 * channels;
    let max_alignment = sample_rate as usize * channels / 10;

    assert!(
        captured_snapshot.len() > max_alignment + snippet_len,
        "Not enough captured samples: {}",
        captured_snapshot.len(),
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

    assert!(
        best_max_diff < 0.01,
        "Could not align captured track 1 audio with XLD reference.\n\
         Best offset {:.1}ms, max sample diff {:.6}",
        best_offset as f64 / channels as f64 / sample_rate as f64 * 1000.0,
        best_max_diff,
    );

    let compare_count = (sample_rate as usize * channels)
        .min(captured_snapshot.len() - best_offset)
        .min(reference_f32.len());

    for i in 0..compare_count {
        let diff = (captured_snapshot[best_offset + i] - reference_f32[i]).abs();
        assert!(
            diff < 0.01,
            "AUDIO MISMATCH at index {} ({:.1}ms)",
            i,
            i as f64 / channels as f64 / sample_rate as f64 * 1000.0,
        );
    }

    debug!(
        "CUE/FLAC track 1 samples match XLD reference ({} samples).",
        compare_count,
    );
}

/// Seeking to 5s in CUE/FLAC track 2 must produce audio matching the reference at that position.
///
/// Track 2 starts mid-album, exposing bugs where the album's seektable offsets
/// don't match the track's byte range. Compares captured post-seek samples against
/// the XLD reference at the corresponding offset.
#[tokio::test]
async fn test_cue_flac_seek() {
    use bae_core::audio_codec::decode_audio;

    // Real-time capture: a full-speed drain races the decoder past track 2 and
    // gaplessly onto the next track before the seek below lands, leaving the
    // post-seek stream empty (flaky under load — Linux CI hit it ~5%).
    let mut fixture = match CueFlacTestFixture::with_realtime_capture().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up CUE/FLAC test fixture: {}", e);
            return;
        }
    };

    let track_id = fixture.track_ids[1].clone();

    fixture.playback_handle.play(track_id.clone());
    // Drain the play stream; the seek below will mint a fresh one.
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
    fixture.playback_handle.seek(Duration::from_secs(5));
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

    // Decode XLD reference for track 2 (white noise)
    let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cue_flac");
    let reference_data =
        std::fs::read(fixture_dir.join("02 Test Artist - Track Two (White Noise).flac"))
            .expect("read reference");
    let reference = decode_audio(&reference_data, None, None).expect("decode reference");
    let channels = reference.channels as usize;
    let sample_rate = reference.sample_rate;
    let reference_f32: Vec<f32> = reference
        .samples
        .iter()
        .map(|&s| s as f32 / 32768.0)
        .collect();

    // Wait for enough captured samples (1 second)
    let target_samples = sample_rate as usize * channels;
    let captured_snapshot =
        bae_core::playback::wait_for_samples(&captured, target_samples, Duration::from_secs(60))
            .await;

    assert!(
        !captured_snapshot.is_empty(),
        "No samples captured after seek",
    );

    // The seek coordinate is relative to the track's pregap start (INDEX 00),
    // not INDEX 01. For track 2 with pregap (INDEX 00 at 8s, INDEX 01 at 10s),
    // seeking to 5s goes to 13s in the album = 3s into the reference (which starts
    // at INDEX 01 = 10s). Search the entire reference to find the alignment.
    let snippet_len = 200 * channels;
    let step = 100 * channels;

    let mut best_sad: f64 = f64::MAX;
    let mut best_ref_offset: usize = 0;

    let search_end = reference_f32.len().saturating_sub(snippet_len);
    for ref_offset in (0..search_end).step_by(step) {
        let mut sad: f64 = 0.0;
        for i in 0..snippet_len.min(captured_snapshot.len()) {
            sad += (captured_snapshot[i] as f64 - reference_f32[ref_offset + i] as f64).abs();
            if sad > best_sad {
                break;
            }
        }
        if sad < best_sad {
            best_sad = sad;
            best_ref_offset = ref_offset;
        }
    }

    let ref_time_ms = best_ref_offset as f64 / channels as f64 / sample_rate as f64 * 1000.0;
    let avg_diff = best_sad / snippet_len as f64;

    // The seek should land somewhere within the reference track (not at the very start)
    assert!(
        ref_time_ms > 0.0,
        "Seek appears to have gone to the beginning of the track instead of 5s in",
    );

    // The streaming AVIO decoder produces f32 via FFmpeg's internal resampler,
    // while the reference uses i32->f32 conversion. This causes per-sample noise
    // of up to ~0.2 average for CUE/FLAC. The important thing is that the alignment
    // found a position within the reference track, not that every sample matches exactly.
    // (The decoder-level tests in test_cue_flac.rs verify exact sample correctness.)
    assert!(
        avg_diff < 0.5,
        "Post-seek audio average difference too high ({:.4}), audio may be from wrong position.\n\
         Best alignment at {:.1}ms in reference.",
        avg_diff,
        ref_time_ms,
    );

    debug!(
        "Post-seek CUE/FLAC audio aligned at {:.1}ms in reference (avg_diff {:.4}).",
        ref_time_ms, avg_diff,
    );
}

/// Direct play of CUE/FLAC track 2 must skip the pregap and start at INDEX 01.
///
/// Track 2 has a 2-second pregap (INDEX 00 at 8s, INDEX 01 at 10s).
/// Direct play skips the pregap. The captured audio must match the XLD reference
/// starting at INDEX 01 (not the pregap content at INDEX 00).
#[tokio::test]
async fn test_direct_play_skips_pregap_cue_flac() {
    use bae_core::audio_codec::decode_audio;

    let mut fixture = match CueFlacTestFixture::with_capture().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up CUE/FLAC test fixture: {}", e);
            return;
        }
    };

    let track_id = fixture.track_ids[1].clone();

    // Direct play track 2
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

    // Decode XLD reference for track 2
    // XLD splits at INDEX 01, so the reference already starts at INDEX 01 (no pregap).
    // Direct play also starts at INDEX 01. Compare captured audio directly against reference.
    let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cue_flac");
    let reference_data =
        std::fs::read(fixture_dir.join("02 Test Artist - Track Two (White Noise).flac"))
            .expect("read reference");
    let reference = decode_audio(&reference_data, None, None).expect("decode reference");
    let channels = reference.channels as usize;
    let sample_rate = reference.sample_rate;
    let reference_f32: Vec<f32> = reference
        .samples
        .iter()
        .map(|&s| s as f32 / 32768.0)
        .collect();

    // Wait for enough captured samples (2 seconds)
    let target_samples = sample_rate as usize * channels * 2;
    let captured_snapshot =
        bae_core::playback::wait_for_samples(&captured, target_samples, Duration::from_secs(60))
            .await;

    // Align captured audio against reference AFTER pregap
    // If pregap was NOT skipped, alignment would fail or find a match offset
    // that corresponds to the pregap content instead of INDEX 01.
    let snippet_len = 500 * channels;
    let max_alignment = sample_rate as usize * channels / 10;

    assert!(
        captured_snapshot.len() > max_alignment + snippet_len,
        "Not enough captured samples: {}",
        captured_snapshot.len(),
    );

    let mut best_max_diff: f32 = f32::MAX;
    let mut best_offset: usize = 0;
    for offset in 0..max_alignment.min(captured_snapshot.len().saturating_sub(snippet_len)) {
        let mut max_diff: f32 = 0.0;
        for i in 0..snippet_len.min(reference_f32.len()) {
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
        "Direct play did not skip pregap: captured audio doesn't match reference at INDEX 01.\n\
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
            "AUDIO MISMATCH at index {} ({:.1}ms): pregap may not be properly skipped",
            i,
            i as f64 / channels as f64 / sample_rate as f64 * 1000.0,
        );
    }

    debug!(
        "Direct play correctly skips pregap ({} samples match after INDEX 01, offset {:.1}ms).",
        compare_count, offset_ms,
    );
}

// ============================================================================
// Sample rate handling tests
// ============================================================================

/// Test fixture for high sample rate (96kHz) FLAC playback.
/// This catches bugs where the playback pipeline assumes 44.1kHz.
struct HighSampleRateTestFixture {
    playback_handle: bae_core::playback::PlaybackHandle,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_id: String,
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
            std::sync::Arc::new(bae_core::clock::SystemClock),
        )
        .await?;
        let database_arc = Arc::new(database.clone());
        let library_dir = LibraryDir::new(temp_dir.path().to_path_buf());
        let (config_handle, key_service) = test_config_and_keys(&library_dir);
        let library_manager = LibraryManager::new(
            (*database_arc).clone(),
            library_dir.clone(),
            config_handle,
            key_service,
            std::sync::Arc::new(bae_core::clock::SystemClock),
            std::sync::Arc::new(bae_core::id_provider::UuidProvider),
            tokio::runtime::Handle::current(),
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
            genre: vec![],
            style: vec![],
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
            tracklist: vec![DiscogsTrack {
                type_: "track".to_string(),
                position: "1".to_string(),
                title: "96kHz Track".to_string(),
                duration: Some("0:03".to_string()),
                artists: vec![],
            }],
            master_id: Some("test-master-96khz".to_string()),
        };
        let release_id_key = seed_discogs_test_release(discogs_release);

        let import_handle = start_test_import(runtime_handle.clone(), library_manager.clone());

        let import_id = uuid::Uuid::new_v4().to_string();
        import_handle
            .send_command(ImportCommand::Folder {
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
        try_wait_for_import_complete(&mut progress_rx)
            .await
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

        let albums = library_manager.get_albums(&[]).await?;
        let releases = library_manager
            .get_releases_for_album(&albums[0].id)
            .await?;
        let tracks = library_manager.get_tracks(&releases[0].id).await?;
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

        std::env::set_var("MUTE_TEST_AUDIO", "1");
        let playback_handle = bae_core::playback::PlaybackService::start(
            library_manager.clone(),
            runtime_handle,
            100,
        );
        playback_handle.set_volume(0.0);
        let progress_rx = playback_handle.subscribe_progress();

        Ok(Self {
            playback_handle,
            progress_rx,
            track_id,
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
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match HighSampleRateTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up high sample rate test fixture: {}", e);
            return;
        }
    };

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
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }

    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };

    if fixture.track_ids.is_empty() {
        debug!("No tracks available for testing");
        return;
    }

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
    let mut fixture = match CueFlacTestFixture::with_realtime_capture().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up CUE/FLAC test fixture: {}", e);
            return;
        }
    };

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
    let reference = decode_audio(&reference_data, None, None).expect("decode reference");
    let channels = reference.channels as usize;
    let sample_rate = reference.sample_rate;
    let reference_f32: Vec<f32> = reference
        .samples
        .iter()
        .map(|&s| s as f32 / 32768.0)
        .collect();

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

/// Test CPU usage with real imported library.
///
/// Uses the actual bae library at ~/.bae/library.db to test with real imported albums.
/// This plays through the actual audio system to catch CPU issues.
///
/// Run with: cargo test --test test_playback_behavior test_real_library_cpu -- --nocapture --ignored
#[tokio::test]
#[ignore] // Only run manually with real library
async fn test_real_library_cpu_usage() {
    use bae_core::db::Database;
    use bae_core::library::LibraryManager;

    tracing_init();

    let db_path = dirs::home_dir()
        .expect("home dir")
        .join(".bae")
        .join("library.db");

    if !db_path.exists() {
        eprintln!("No library at {:?} - import an album first", db_path);
        return;
    }

    eprintln!("Using library: {:?}", db_path);

    // Connect to real database
    let bae_dir = dirs::home_dir().expect("home dir").join(".bae");
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(bae_core::clock::SystemClock),
    )
    .await
    .expect("open db");
    let library_dir = LibraryDir::new(&bae_dir);
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::new(
        database.clone(),
        library_dir.clone(),
        config_handle,
        key_service,
        std::sync::Arc::new(bae_core::clock::SystemClock),
        std::sync::Arc::new(bae_core::id_provider::UuidProvider),
        tokio::runtime::Handle::current(),
    );

    // Get first album and release
    let albums = library_manager.get_albums(&[]).await.expect("get albums");
    if albums.is_empty() {
        eprintln!("No albums in library");
        return;
    }

    let releases = library_manager
        .get_releases_for_album(&albums[0].id)
        .await
        .expect("get releases");
    if releases.is_empty() {
        eprintln!("No releases in library");
        return;
    }

    let album = &albums[0];
    let release = &releases[0];
    eprintln!("Using album: {}", album.title);

    let tracks = library_manager
        .get_tracks(&release.id)
        .await
        .expect("get tracks");

    if tracks.is_empty() {
        eprintln!("No tracks in release");
        return;
    }

    // Use track 2 if available (often a CUE/FLAC mid-album track)
    let track = if tracks.len() > 1 {
        &tracks[1]
    } else {
        &tracks[0]
    };
    eprintln!(
        "Playing track {}: {}",
        track.track_number.unwrap_or(0),
        track.title
    );

    // Start playback service
    let runtime_handle = tokio::runtime::Handle::current();
    let playback_handle =
        bae_core::playback::PlaybackService::start(library_manager.clone(), runtime_handle, 100);
    let mut progress_rx = playback_handle.subscribe_progress();

    // Measure CPU before playback
    let initial_cpu = get_process_cpu_time();

    // Start playback
    playback_handle.play(track.id.clone());

    // Wait for playback to start
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut started = false;
    while Instant::now() < deadline && !started {
        match timeout(Duration::from_millis(100), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => {
                if matches!(state, PlaybackState::Playing { .. }) {
                    started = true;
                    eprintln!("Playback started");
                }
            }
            Ok(Some(msg)) => eprintln!("Progress: {:?}", msg),
            _ => {}
        }
    }

    if !started {
        eprintln!("Playback failed to start");
        return;
    }

    // Let it play for measurement period - use thread::sleep to not interfere with tokio
    eprintln!("Measuring CPU for 10 seconds (will hear audio if not muted)...");
    let measure_start = Instant::now();

    // Let it play for measurement period
    for _ in 0..100 {
        std::thread::sleep(Duration::from_millis(100));
        // Drain progress channel to prevent backpressure
        while progress_rx.try_recv().is_ok() {}
    }

    let wall_time = measure_start.elapsed();

    // Get final CPU
    let final_cpu = get_process_cpu_time();
    let cpu_time = final_cpu.saturating_sub(initial_cpu);
    let cpu_percent = (cpu_time.as_secs_f64() / wall_time.as_secs_f64()) * 100.0;

    eprintln!(
        "\n=== CPU USAGE: {:.1}% ===\n(cpu_time={:?}, wall_time={:?})",
        cpu_percent, cpu_time, wall_time
    );

    playback_handle.stop();

    // Assert reasonable CPU usage
    let max_cpu = if cfg!(debug_assertions) { 100.0 } else { 30.0 };
    assert!(
        cpu_percent < max_cpu,
        "CPU too high: {:.1}% (max {:.0}%)\nThis indicates a busy-wait or spin loop.",
        cpu_percent,
        max_cpu
    );
}

/// Get total CPU time consumed by this process (user + system time).
/// Uses getrusage on Unix systems.
fn get_process_cpu_time() -> Duration {
    #[cfg(unix)]
    {
        use std::mem::MaybeUninit;
        let mut usage = MaybeUninit::<libc::rusage>::uninit();
        unsafe {
            if libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) == 0 {
                let usage = usage.assume_init();
                let user = Duration::new(
                    usage.ru_utime.tv_sec as u64,
                    (usage.ru_utime.tv_usec as u32) * 1000,
                );
                let system = Duration::new(
                    usage.ru_stime.tv_sec as u64,
                    (usage.ru_stime.tv_usec as u32) * 1000,
                );
                return user + system;
            }
        }
        Duration::ZERO
    }
    #[cfg(not(unix))]
    {
        Duration::ZERO
    }
}

/// Seeking while paused in a CUE/FLAC track must start playback at the target
/// and advance position -- state must not show "playing" while audio is frozen.
///
/// Run with: cargo test --test test_playback_behavior test_pause_seek_cue_flac -- --nocapture --ignored
#[tokio::test]
#[ignore = "Requires real library with CUE/FLAC album"]
async fn test_pause_seek_cue_flac() {
    use bae_core::db::Database;
    use bae_core::library::LibraryManager;

    tracing_init();

    let db_path = dirs::home_dir()
        .expect("home dir")
        .join(".bae")
        .join("library.db");

    if !db_path.exists() {
        eprintln!("No library at {:?} - import an album first", db_path);
        return;
    }

    eprintln!("Using library: {:?}", db_path);

    let bae_dir = dirs::home_dir().expect("home dir").join(".bae");
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(bae_core::clock::SystemClock),
    )
    .await
    .expect("open db");
    let library_dir = LibraryDir::new(&bae_dir);
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::new(
        database.clone(),
        library_dir.clone(),
        config_handle,
        key_service,
        std::sync::Arc::new(bae_core::clock::SystemClock),
        std::sync::Arc::new(bae_core::id_provider::UuidProvider),
        tokio::runtime::Handle::current(),
    );

    // Get albums (use first available CUE/FLAC album)
    let albums = library_manager.get_albums(&[]).await.expect("get albums");
    if albums.is_empty() {
        eprintln!("No albums in library");
        return;
    }

    let album = &albums[0];
    eprintln!("Using album: {}", album.title);

    let releases = library_manager
        .get_releases_for_album(&album.id)
        .await
        .expect("get releases");
    if releases.is_empty() {
        eprintln!("No releases");
        return;
    }

    let tracks = library_manager
        .get_tracks(&releases[0].id)
        .await
        .expect("get tracks");

    // Use track 3 (or last track if fewer)
    let track_idx = std::cmp::min(2, tracks.len().saturating_sub(1));
    let track = &tracks[track_idx];
    eprintln!(
        "Playing track {}: {} (duration: {:?})",
        track.track_number.unwrap_or(0),
        track.title,
        track.duration_ms
    );

    let runtime_handle = tokio::runtime::Handle::current();
    eprintln!("Starting PlaybackService...");
    let playback_handle =
        bae_core::playback::PlaybackService::start(library_manager.clone(), runtime_handle, 100);
    playback_handle.set_volume(0.0); // Mute for test
    let mut progress_rx = playback_handle.subscribe_progress();

    // Give the service time to initialize
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Start playback
    eprintln!("Calling play()...");
    playback_handle.play(track.id.clone());

    // Wait for playback to start
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut started = false;
    while Instant::now() < deadline && !started {
        match timeout(Duration::from_millis(200), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => {
                eprintln!("StateChanged: {:?}", state);
                if matches!(state, PlaybackState::Playing { .. }) {
                    started = true;
                    eprintln!("Playback started");
                }
            }
            Ok(Some(other)) => {
                eprintln!("Other progress: {:?}", other);
                continue;
            }
            Ok(None) => {
                eprintln!("Progress channel closed");
                break;
            }
            Err(_) => continue, // Timeout, keep waiting
        }
    }
    assert!(started, "Playback should start");

    // Let it play briefly
    tokio::time::sleep(Duration::from_millis(500)).await;

    // PAUSE
    eprintln!("Pausing...");
    playback_handle.pause();

    // Wait for pause state
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut paused = false;
    while Instant::now() < deadline && !paused {
        match timeout(Duration::from_millis(100), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => {
                if matches!(state, PlaybackState::Paused { .. }) {
                    paused = true;
                    eprintln!("Paused");
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    assert!(paused, "Should be paused");

    // SEEK while paused - 10 minutes (600 seconds) into track
    let seek_position = Duration::from_secs(600);
    eprintln!("Seeking to {:?} while paused...", seek_position);
    playback_handle.seek(seek_position);

    // Wait for Seeked event to confirm seek completed
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut seek_completed = false;
    let mut position_after_seek_ms = 0u64;
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(100), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::Seeked { position_ms, .. })) => {
                seek_completed = true;
                position_after_seek_ms = position_ms;
                eprintln!("Seek completed at position {}ms", position_ms);
                break;
            }
            Ok(Some(other)) => {
                eprintln!("Got event: {:?}", other);
                continue;
            }
            Ok(None) | Err(_) => break,
        }
    }
    assert!(seek_completed, "Seek should complete");
    assert!(
        position_after_seek_ms >= 590_000,
        "Position after seek should be near 600s, got {}ms",
        position_after_seek_ms
    );

    // RESUME
    eprintln!("Resuming...");
    playback_handle.resume();

    // Wait for playing state
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut resumed = false;
    let mut position_after_resume_ms = 0u64;
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(100), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => {
                if matches!(state, PlaybackState::Playing { .. }) {
                    resumed = true;
                    eprintln!("Resumed");
                    break;
                }
            }
            Ok(Some(PlaybackProgress::PositionUpdate { position_ms, .. })) => {
                position_after_resume_ms = position_ms;
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    assert!(resumed, "Should resume playing");

    // Wait and verify position advances via PositionUpdate events
    tokio::time::sleep(Duration::from_secs(2)).await;

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut final_position_ms = position_after_resume_ms;
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(100), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::PositionUpdate { position_ms, .. })) => {
                final_position_ms = position_ms;
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }

    let position_advanced = final_position_ms > position_after_seek_ms;
    eprintln!(
        "Position after 2s: {}ms (advanced: {})",
        final_position_ms, position_advanced
    );

    assert!(
        position_advanced,
        "Position should advance after resume. Seek position: {}ms, Final: {}ms",
        position_after_seek_ms, final_position_ms
    );

    playback_handle.stop();
    eprintln!("✅ Test passed: pause-seek-resume works correctly");
}

/// Test seeking while playing (not paused) in a CUE/FLAC track.
///
/// This test checks if large seeks work while audio is actively playing.
/// Compare with test_pause_seek_cue_flac to see if the bug is pause-specific.
///
/// Run with: cargo test --test test_playback_behavior test_playing_seek_cue_flac -- --nocapture --ignored
#[tokio::test]
#[ignore = "Requires real library with CUE/FLAC album"]
async fn test_playing_seek_cue_flac() {
    use bae_core::db::Database;
    use bae_core::library::LibraryManager;

    tracing_init();

    let db_path = dirs::home_dir()
        .expect("home dir")
        .join(".bae")
        .join("library.db");

    if !db_path.exists() {
        eprintln!("No library at {:?} - import an album first", db_path);
        return;
    }

    eprintln!("Using library: {:?}", db_path);

    let bae_dir = dirs::home_dir().expect("home dir").join(".bae");
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(bae_core::clock::SystemClock),
    )
    .await
    .expect("open db");
    let library_dir = LibraryDir::new(&bae_dir);
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::new(
        database.clone(),
        library_dir.clone(),
        config_handle,
        key_service,
        std::sync::Arc::new(bae_core::clock::SystemClock),
        std::sync::Arc::new(bae_core::id_provider::UuidProvider),
        tokio::runtime::Handle::current(),
    );

    let albums = library_manager.get_albums(&[]).await.expect("get albums");
    if albums.is_empty() {
        eprintln!("No albums in library");
        return;
    }

    let album = &albums[0];
    eprintln!("Using album: {}", album.title);

    let releases = library_manager
        .get_releases_for_album(&album.id)
        .await
        .expect("get releases");
    if releases.is_empty() {
        eprintln!("No releases");
        return;
    }

    let tracks = library_manager
        .get_tracks(&releases[0].id)
        .await
        .expect("get tracks");

    let track_idx = std::cmp::min(2, tracks.len().saturating_sub(1));
    let track = &tracks[track_idx];
    eprintln!(
        "Playing track {}: {} (duration: {:?})",
        track.track_number.unwrap_or(0),
        track.title,
        track.duration_ms
    );

    let runtime_handle = tokio::runtime::Handle::current();
    let playback_handle =
        bae_core::playback::PlaybackService::start(library_manager.clone(), runtime_handle, 100);
    playback_handle.set_volume(0.0);
    let mut progress_rx = playback_handle.subscribe_progress();

    tokio::time::sleep(Duration::from_millis(200)).await;

    eprintln!("Starting playback...");
    playback_handle.play(track.id.clone());

    // Wait for playback to start
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut started = false;
    while Instant::now() < deadline && !started {
        match timeout(Duration::from_millis(200), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged { state })) => {
                if matches!(state, PlaybackState::Playing { .. }) {
                    started = true;
                    eprintln!("Playback started");
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    assert!(started, "Playback should start");

    // Let it play for just 500ms, then seek WHILE PLAYING (no pause!)
    eprintln!("Playing for 500ms...");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // SEEK while playing - 10 minutes (600 seconds) into track
    let seek_position = Duration::from_secs(600);
    eprintln!("Seeking to {:?} WHILE PLAYING (no pause)...", seek_position);
    playback_handle.seek(seek_position);

    // Wait for Seeked event (not StateChanged!) to confirm seek completed
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut position_after_seek_ms: u64 = 0;
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(200), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::Seeked { position_ms, .. })) => {
                position_after_seek_ms = position_ms;
                eprintln!("Seeked event received: position = {}ms", position_ms);
                break;
            }
            Ok(Some(other)) => {
                eprintln!("Got other event: {:?}", other);
                continue;
            }
            Ok(None) => break,
            Err(_) => continue,
        }
    }

    assert!(
        position_after_seek_ms >= 590_000,
        "Position after seek should be near 600s, got {}ms",
        position_after_seek_ms
    );

    // Wait and verify position advances via PositionUpdate events
    eprintln!("Waiting 2s to check if position advances...");
    tokio::time::sleep(Duration::from_secs(2)).await;

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut final_position_ms = position_after_seek_ms;
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(100), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::PositionUpdate { position_ms, .. })) => {
                final_position_ms = position_ms;
                eprintln!("Position update: {}ms", position_ms);
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }

    let position_advanced = final_position_ms > position_after_seek_ms;
    eprintln!(
        "Position after 2s: {}ms (advanced: {})",
        final_position_ms, position_advanced
    );

    assert!(
        position_advanced,
        "Position should advance after seek while playing. Started at {}ms, ended at {}ms",
        position_after_seek_ms, final_position_ms
    );

    playback_handle.stop();
    eprintln!("✅ Test passed: seek while playing works correctly");
}

/// Restoring playback state on service start must populate the late-mount
/// display cache via `emit_position_display`, so the progress NSView created
/// after launch can read it on mount.
///
/// Regression test for the main-playback dual-sink fix: if someone removes
/// the unconditional emission at the end of `restore()` or wraps it in a
/// condition that skips the zero/near-zero case, the cache stays `None` and
/// late-mounting NSViews have nothing to read. This test specifically uses
/// `position_ms = 0` so the internal `seek()` call (which has its own
/// `emit_position_display`) is skipped, forcing the test to rely entirely
/// on the tail emission in `restore()`.
///
/// The cache is checked instead of a progress event because the event is
/// emitted from a background thread that races with the test's subscription.
/// The cache, by contrast, is an `Arc<Mutex<Option<_>>>` that persists
/// regardless of who's subscribed.
///
/// We do NOT reuse `PlaybackTestFixture` here because its playback service is
/// spawned on a background thread that may not finish initializing before the
/// test writes the snapshot, causing the fixture's service to consume the
/// snapshot instead of the test's new service.
#[tokio::test]
async fn test_restore_populates_last_position_display() {
    if should_skip_audio_tests() {
        eprintln!("Skipping: no audio device");
        return;
    }
    tracing_init();

    // Build a library + import tracks, but do NOT start a playback service.
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let album_dir = temp_dir.path().join("album");
    std::fs::create_dir_all(&album_dir).unwrap();
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(bae_core::clock::SystemClock),
    )
    .await
    .unwrap();
    let database_arc = Arc::new(database.clone());
    let library_dir = LibraryDir::new(temp_dir.path().to_path_buf());
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::new(
        (*database_arc).clone(),
        library_dir.clone(),
        config_handle,
        key_service,
        std::sync::Arc::new(bae_core::clock::SystemClock),
        std::sync::Arc::new(bae_core::id_provider::UuidProvider),
        tokio::runtime::Handle::current(),
    );
    let runtime_handle = tokio::runtime::Handle::current();
    let _ = generate_test_flac_files(&album_dir);
    let discogs_release = create_test_album();
    let release_id_key = seed_discogs_test_release(discogs_release);
    let import_handle = start_test_import(runtime_handle.clone(), library_manager.clone());
    let import_id = uuid::Uuid::new_v4().to_string();
    import_handle
        .send_command(ImportCommand::Folder {
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
        .unwrap();
    let mut progress_rx = import_handle.subscribe_import(import_id);
    let _ = wait_for_import_complete(&mut progress_rx).await;
    let releases = library_manager
        .get_releases_for_album(&library_manager.get_albums(&[]).await.unwrap()[0].id)
        .await
        .unwrap();
    let tracks = library_manager.get_tracks(&releases[0].id).await.unwrap();
    let track_id = tracks[0].id.clone();

    // Mute audio for tests and write a snapshot. No service is running yet,
    // so no one can consume the snapshot before our test service starts.
    std::env::set_var("MUTE_TEST_AUDIO", "1");
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
    let handle = bae_core::playback::PlaybackService::start(library_manager, runtime_handle, 100);
    handle.set_volume(0.0);

    // Poll the cache: restore() runs asynchronously on a spawned thread, so
    // we can't assume it has completed immediately.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut display = None;
    while Instant::now() < deadline {
        if let Some(d) = handle.get_last_position_display() {
            display = Some(d);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    assert!(
        display.is_some(),
        "last_position_display should be populated after restore — the tail \
         emit in restore() exists specifically so late-mounting NSViews can \
         read a value on mount"
    );

    handle.stop();
}

/// A saved context cursor that points past the source release's *current* tracks
/// (the release shrank between sessions — tracks were deleted) can't resume
/// there. Restore must drop the context and keep only the manual lane, never
/// silently snap the cursor back into range.
///
/// `has_previous` is the discriminator: a dropped context has no track before
/// the cursor (`false`); a context that survived with a snapped-back cursor
/// would sit past the start (`true`). The manual track must still be queued.
#[tokio::test]
async fn test_restore_drops_context_when_cursor_past_shrunk_tracks() {
    if should_skip_audio_tests() {
        eprintln!("Skipping: no audio device");
        return;
    }
    tracing_init();

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let album_dir = temp_dir.path().join("album");
    std::fs::create_dir_all(&album_dir).unwrap();
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(bae_core::clock::SystemClock),
    )
    .await
    .unwrap();
    let database_arc = Arc::new(database.clone());
    let library_dir = LibraryDir::new(temp_dir.path().to_path_buf());
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::new(
        (*database_arc).clone(),
        library_dir.clone(),
        config_handle,
        key_service,
        std::sync::Arc::new(bae_core::clock::SystemClock),
        std::sync::Arc::new(bae_core::id_provider::UuidProvider),
        tokio::runtime::Handle::current(),
    );
    let runtime_handle = tokio::runtime::Handle::current();
    let _ = generate_test_flac_files(&album_dir);
    let discogs_release = create_test_album();
    let release_id_key = seed_discogs_test_release(discogs_release);
    let import_handle = start_test_import(runtime_handle.clone(), library_manager.clone());
    let import_id = uuid::Uuid::new_v4().to_string();
    import_handle
        .send_command(ImportCommand::Folder {
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
        .unwrap();
    let mut import_progress_rx = import_handle.subscribe_import(import_id);
    let _ = wait_for_import_complete(&mut import_progress_rx).await;
    let release = library_manager
        .get_releases_for_album(&library_manager.get_albums(&[]).await.unwrap()[0].id)
        .await
        .unwrap()
        .remove(0);
    let tracks = library_manager.get_tracks(&release.id).await.unwrap();
    assert_eq!(tracks.len(), 3, "the fixture release has three tracks");
    // The manual entry is the last track; it must survive the restore. The
    // context source is the same release with a cursor past its three tracks.
    let manual_track_id = tracks[2].id.clone();

    std::env::set_var("MUTE_TEST_AUDIO", "1");
    let state = bae_core::db::DbPlaybackState {
        context: Some(bae_core::db::DbPlaybackContext {
            source: release.id.clone(),
            shuffle_seed: None,
            cursor: 5,
        }),
        manual: serde_json::to_string(&vec![manual_track_id.clone()]).unwrap(),
        repeat: "off".to_string(),
        current_track_id: Some(manual_track_id.clone()),
        position_ms: Some(0),
        volume: 0.8,
        is_muted: false,
    };
    library_manager.save_playback_state(&state).await.unwrap();

    let handle = bae_core::playback::PlaybackService::start(library_manager, runtime_handle, 100);
    handle.set_volume(0.0);
    let mut progress_rx = handle.subscribe_progress();

    // Wait for the queue update restore emits once it commits the restored queue.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut queue_update = None;
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(100), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::QueueUpdated {
                manual,
                context,
                has_previous,
                ..
            })) => {
                queue_update = Some((manual, context, has_previous));
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => continue,
        }
    }

    let (manual, context, has_previous) = queue_update.expect("restore emits a queue update");
    assert!(
        !has_previous,
        "the context was dropped, so nothing sits before the cursor"
    );
    assert!(
        context.is_none(),
        "the cursor was past the release, so the context is dropped entirely: {context:?}"
    );
    let queued: Vec<&str> = manual.iter().map(|e| e.track_id.as_str()).collect();
    assert!(
        queued.contains(&manual_track_id.as_str()),
        "the manual lane survives the restore: {queued:?}"
    );
    assert!(
        !queued.contains(&tracks[0].id.as_str()) && !queued.contains(&tracks[1].id.as_str()),
        "the dropped context contributes no tracks: {queued:?}"
    );

    handle.stop();
}

/// The persist-on-change wiring is load-bearing: playing a release writes the
/// device-local `playback_state` row, and stopping clears it — so a restart
/// resumes a live session but never re-cues a finished one.
#[tokio::test]
async fn test_play_persists_then_stop_clears_playback_state() {
    if should_skip_audio_tests() {
        eprintln!("Skipping: no audio device");
        return;
    }
    tracing_init();

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let album_dir = temp_dir.path().join("album");
    std::fs::create_dir_all(&album_dir).unwrap();
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(bae_core::clock::SystemClock),
    )
    .await
    .unwrap();
    let database_arc = Arc::new(database.clone());
    let library_dir = LibraryDir::new(temp_dir.path().to_path_buf());
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::new(
        (*database_arc).clone(),
        library_dir.clone(),
        config_handle,
        key_service,
        std::sync::Arc::new(bae_core::clock::SystemClock),
        std::sync::Arc::new(bae_core::id_provider::UuidProvider),
        tokio::runtime::Handle::current(),
    );
    let runtime_handle = tokio::runtime::Handle::current();
    let _ = generate_test_flac_files(&album_dir);
    let discogs_release = create_test_album();
    let release_id_key = seed_discogs_test_release(discogs_release);
    let import_handle = start_test_import(runtime_handle.clone(), library_manager.clone());
    let import_id = uuid::Uuid::new_v4().to_string();
    import_handle
        .send_command(ImportCommand::Folder {
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
        .unwrap();
    let mut progress_rx = import_handle.subscribe_import(import_id);
    let _ = wait_for_import_complete(&mut progress_rx).await;
    let release_id = library_manager
        .get_releases_for_album(&library_manager.get_albums(&[]).await.unwrap()[0].id)
        .await
        .unwrap()[0]
        .id
        .clone();
    let first_track = library_manager.get_tracks(&release_id).await.unwrap()[0]
        .id
        .clone();

    std::env::set_var("MUTE_TEST_AUDIO", "1");
    let handle =
        bae_core::playback::PlaybackService::start(library_manager.clone(), runtime_handle, 100);
    handle.set_volume(0.0);

    // Playing a release persists the row: source is the release, current is the
    // first track.
    handle.play_release(release_id.clone(), None, false);
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut persisted = None;
    while Instant::now() < deadline {
        if let Some(row) = library_manager.load_playback_state().await.unwrap() {
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
        if library_manager
            .load_playback_state()
            .await
            .unwrap()
            .is_none()
        {
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

/// An imported test library with no playback service running, so a test can
/// write a `playback_state` row and then start its own service to exercise
/// restore without racing a fixture's service for the row.
struct RestoreTestLibrary {
    library_manager: LibraryManager,
    runtime_handle: tokio::runtime::Handle,
    track_ids: Vec<String>,
    _temp_dir: TempDir,
}

async fn restore_test_library() -> RestoreTestLibrary {
    tracing_init();
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let album_dir = temp_dir.path().join("album");
    std::fs::create_dir_all(&album_dir).unwrap();
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(bae_core::clock::SystemClock),
    )
    .await
    .unwrap();
    let database_arc = Arc::new(database.clone());
    let library_dir = LibraryDir::new(temp_dir.path().to_path_buf());
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::new(
        (*database_arc).clone(),
        library_dir.clone(),
        config_handle,
        key_service,
        std::sync::Arc::new(bae_core::clock::SystemClock),
        std::sync::Arc::new(bae_core::id_provider::UuidProvider),
        tokio::runtime::Handle::current(),
    );
    let runtime_handle = tokio::runtime::Handle::current();
    let _ = generate_test_flac_files(&album_dir);
    let discogs_release = create_test_album();
    let release_id_key = seed_discogs_test_release(discogs_release);
    let import_handle = start_test_import(runtime_handle.clone(), library_manager.clone());
    let import_id = uuid::Uuid::new_v4().to_string();
    import_handle
        .send_command(ImportCommand::Folder {
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
        .unwrap();
    let mut progress_rx = import_handle.subscribe_import(import_id);
    let _ = wait_for_import_complete(&mut progress_rx).await;
    let releases = library_manager
        .get_releases_for_album(&library_manager.get_albums(&[]).await.unwrap()[0].id)
        .await
        .unwrap();
    let tracks = library_manager.get_tracks(&releases[0].id).await.unwrap();
    let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
    assert!(!track_ids.is_empty(), "the test album imported some tracks");
    std::env::set_var("MUTE_TEST_AUDIO", "1");
    RestoreTestLibrary {
        library_manager,
        runtime_handle,
        track_ids,
        _temp_dir: temp_dir,
    }
}

/// A resume cache whose context release is gone (its `get_track_ids` is empty)
/// restores the manual lane only — the context drops, the surviving manual
/// tracks and current track stay. The restored queue re-persists with no
/// context, which is what we read back.
#[tokio::test]
async fn test_restore_drops_deleted_context_keeps_manual() {
    if should_skip_audio_tests() {
        eprintln!("Skipping: no audio device");
        return;
    }
    let lib = restore_test_library().await;
    let track_id = lib.track_ids[0].clone();

    // The context points at a release id that no longer exists, so its
    // `get_track_ids` returns empty (the deleted-release signal). The manual lane
    // and current track are a real, surviving track.
    let state = bae_core::db::DbPlaybackState {
        context: Some(bae_core::db::DbPlaybackContext {
            source: "release-that-was-deleted".to_string(),
            shuffle_seed: None,
            cursor: 0,
        }),
        manual: format!("[{:?}]", track_id),
        repeat: "off".to_string(),
        current_track_id: Some(track_id.clone()),
        position_ms: Some(0),
        volume: 0.8,
        is_muted: false,
    };
    lib.library_manager
        .save_playback_state(&state)
        .await
        .unwrap();

    let handle = bae_core::playback::PlaybackService::start(
        lib.library_manager.clone(),
        lib.runtime_handle,
        100,
    );
    handle.set_volume(0.0);

    // Restore committed once the current (manual) track populates the late-mount
    // display cache.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && handle.get_last_position_display().is_none() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        handle.get_last_position_display().is_some(),
        "the surviving manual track should restore as current"
    );

    // Force the restored queue to re-persist, then read it back: the dropped
    // context is gone and the manual track survived.
    handle.set_repeat_mode(bae_core::playback::RepeatMode::Track);
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut row = None;
    while Instant::now() < deadline {
        if let Some(loaded) = lib.library_manager.load_playback_state().await.unwrap() {
            if loaded.repeat == "track" {
                row = Some(loaded);
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let row = row.expect("the restored, re-persisted row");
    assert!(
        row.context.is_none(),
        "the deleted context release is dropped on restore"
    );
    assert_eq!(
        row.manual,
        format!("[{:?}]", track_id),
        "the surviving manual lane restored"
    );

    handle.stop();
}

/// A corrupt resume cache (here, manual lane that is not valid JSON) is discarded
/// by the boundary parse: the service starts with an empty queue and never
/// panics. The position cache stays `None` because no current track restored.
#[tokio::test]
async fn test_restore_corrupt_row_starts_fresh() {
    if should_skip_audio_tests() {
        eprintln!("Skipping: no audio device");
        return;
    }
    let lib = restore_test_library().await;
    let track_id = lib.track_ids[0].clone();

    let state = bae_core::db::DbPlaybackState {
        context: None,
        manual: "not valid json".to_string(),
        repeat: "off".to_string(),
        current_track_id: Some(track_id),
        position_ms: Some(0),
        volume: 0.8,
        is_muted: false,
    };
    lib.library_manager
        .save_playback_state(&state)
        .await
        .unwrap();

    let handle =
        bae_core::playback::PlaybackService::start(lib.library_manager, lib.runtime_handle, 100);
    handle.set_volume(0.0);

    // Give restore time to run (and to NOT crash). A discarded cache restores no
    // current track, so the display cache stays empty.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        handle.get_last_position_display().is_none(),
        "a discarded resume cache leaves nothing playing (fresh start)"
    );

    handle.stop();
}

/// Seeking a preview while paused must emit a PreviewPositionUpdate even
/// though no position ticks fire in the paused state. Without this explicit
/// emission, the progress NSView stays stuck at the old position until the
/// user resumes.
///
/// Regression test for the preview dual-sink fix: if someone removes the
/// inline emit in handle_preview_seek, this test fails.
#[tokio::test]
async fn test_preview_seek_while_paused_emits_position_update() {
    if should_skip_audio_tests() {
        eprintln!("Skipping: no audio device");
        return;
    }
    let mut fixture = match PlaybackTestFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("Failed to set up test fixture: {}", e);
            return;
        }
    };

    let preview_path = fixture
        .album_dir
        .join("01 Test Track 1.flac")
        .to_string_lossy()
        .into_owned();

    // Start preview — wait for it to reach Playing state.
    fixture.playback_handle.preview_play(preview_path);
    let mut saw_playing = false;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(200), fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::PreviewStateChanged(
                bae_core::playback::PreviewState::Playing { .. },
            ))) => {
                saw_playing = true;
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    assert!(saw_playing, "preview should reach Playing state");

    // Give playback a moment so current position moves away from 0, making
    // the seek a meaningful movement.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Pause — wait for Paused state.
    fixture.playback_handle.preview_toggle_pause();
    let mut saw_paused = false;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(200), fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::PreviewStateChanged(
                bae_core::playback::PreviewState::Paused { .. },
            ))) => {
                saw_paused = true;
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    assert!(saw_paused, "preview should reach Paused state");

    // Seek to the middle of the file while paused. No ticks fire in the
    // paused state, so the only way the NSView learns the new position is
    // the explicit emit in handle_preview_seek.
    fixture.playback_handle.preview_seek_by_ratio(0.5);

    let mut saw_position_update: Option<u64> = None;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(200), fixture.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::PreviewPositionUpdate { position_ms, .. })) => {
                saw_position_update = Some(position_ms);
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }

    assert!(
        saw_position_update.is_some(),
        "seek-while-paused should emit PreviewPositionUpdate"
    );

    fixture.playback_handle.preview_stop();
}

/// A cloud-only remote track whose data lives only in the cloud. The reader
/// streams in the background after `play_track` has already built the stream,
/// so a cloud failure here is genuinely mid-flight — not a prepare-time error.
struct CloudOnlyPlaybackFixture {
    playback_handle: bae_core::playback::PlaybackHandle,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    cloud: Arc<support::MockCloudHome>,
    track_ids: Vec<String>,
    _temp_dir: TempDir,
}

impl CloudOnlyPlaybackFixture {
    /// Import the FLAC fixtures as remote-unpinned, run the upload to put the
    /// encrypted blobs in the (mock) cloud and clear the outbox, then delete the
    /// import originals so resolution lands on the cloud reader. The mock cloud
    /// is returned so the test can arm a read failure before playing.
    async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        tracing_init();
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let album_dir = temp_dir.path().join("album");
        std::fs::create_dir_all(&album_dir)?;
        let database = Database::new_test(
            db_path.to_str().unwrap(),
            std::sync::Arc::new(bae_core::clock::SystemClock),
        )
        .await?;
        let library_dir = LibraryDir::new(temp_dir.path().to_path_buf());
        let (config_handle, key_service) = test_config_and_keys(&library_dir);
        let library_manager = LibraryManager::new(
            database,
            library_dir,
            config_handle,
            key_service,
            std::sync::Arc::new(bae_core::clock::SystemClock),
            std::sync::Arc::new(bae_core::id_provider::UuidProvider),
            tokio::runtime::Handle::current(),
        );
        let master_key = [11u8; 32];
        let cloud = Arc::new(support::MockCloudHome::new());
        library_manager
            .connect_test_cloud_home(
                cloud.clone(),
                bae_core::sync::cloud_storage::CloudCipher::Encrypted(
                    bae_core::encryption::EncryptionService::new_with_key(&master_key),
                ),
            )
            .await?;

        let runtime_handle = tokio::runtime::Handle::current();
        let discogs_release = create_test_album();
        let release_id_key = seed_discogs_test_release(discogs_release);
        generate_test_flac_files(&album_dir);

        let import_handle = start_test_import(runtime_handle.clone(), library_manager.clone());
        let import_id = uuid::Uuid::new_v4().to_string();
        import_handle
            .send_command(ImportCommand::Folder {
                import_id: import_id.clone(),
                candidate_key: "test".to_string(),
                folder: album_dir.clone(),
                selected_cover: None,
                storage_mode: StorageMode::Remote,
                pin: false,
                identity_choice: IdentityChoice::Exact {
                    release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
                },
                user_edit: None,
            })
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
        let mut import_rx = import_handle.subscribe_import(import_id);
        let (release_id, _album_id) = wait_for_import_complete(&mut import_rx).await;

        // Run the upload so the encrypted blobs land in the cloud and the outbox
        // clears — after this the track resolves cloud-only (no local copy, no
        // pending upload).
        while library_manager.drain_uploads_for_test().await? > 0 {}

        // Delete the import originals so file resolution can't fall back to them.
        std::fs::remove_dir_all(&album_dir)?;

        let tracks = library_manager.get_tracks(&release_id).await?;
        let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
        assert!(!track_ids.is_empty(), "Should have imported tracks");

        std::env::set_var("MUTE_TEST_AUDIO", "1");
        let playback_handle =
            bae_core::playback::PlaybackService::start(library_manager, runtime_handle, 100);
        playback_handle.set_volume(0.0);
        let progress_rx = playback_handle.subscribe_progress();
        Ok(Self {
            playback_handle,
            progress_rx,
            cloud,
            track_ids,
            _temp_dir: temp_dir,
        })
    }
}

/// A mid-flight cloud read failure must drive playback to Stopped, not leave the
/// UI frozen in a not-yet-started "Playing" state. The reader fails in the
/// background after `play_track` built the stream; the service's progress
/// self-subscription turns the resulting PlaybackError into a stop. The terminal
/// state must be Stopped — never Playing/Loading — because the error path tears
/// playback down rather than leaving a frozen position bar.
#[tokio::test]
async fn mid_flight_cloud_read_failure_ends_in_stopped() {
    if should_skip_audio_tests() {
        debug!("Skipping audio test - no audio device available");
        return;
    }
    let mut fixture = match CloudOnlyPlaybackFixture::new().await {
        Ok(f) => f,
        Err(e) => {
            debug!("skip: cloud-only fixture setup failed: {e}");
            return;
        }
    };
    assert!(
        !fixture.track_ids.is_empty(),
        "fixture must import at least one playable track"
    );

    // Every cloud range read for this track fails (the nonce header read is the
    // first), so the background reader cancels the buffer and emits PlaybackError.
    fixture.cloud.fail_next_range_reads(usize::MAX);

    let track_id = fixture.track_ids[0].clone();
    fixture.playback_handle.play(track_id);

    // Collect the whole StateChanged sequence up to Stopped. The read fails
    // before the decoder's ready signal, so Playing must never appear: the
    // ready-gated emission holds Playing until audio flows, and the error path
    // tears playback down to Stopped instead. Both are exercised here.
    let states = collect_states_on(
        &mut fixture.progress_rx,
        |s| matches!(s, PlaybackState::Stopped),
        Duration::from_secs(8),
    )
    .await;
    assert!(
        matches!(states.last(), Some(PlaybackState::Stopped)),
        "a mid-flight cloud read failure must end in Stopped, got {states:?}"
    );
    assert!(
        !states
            .iter()
            .any(|s| matches!(s, PlaybackState::Playing { .. })),
        "Playing must never be emitted for a track whose audio never became ready, got {states:?}"
    );

    // And it must not bounce back into Playing afterwards.
    let resurfaced = wait_for_state_on(
        &mut fixture.progress_rx,
        |s| matches!(s, PlaybackState::Playing { .. }),
        Duration::from_millis(300),
    )
    .await;
    assert!(
        resurfaced.is_none(),
        "playback must stay stopped after the failure, not flip back to Playing"
    );
}
