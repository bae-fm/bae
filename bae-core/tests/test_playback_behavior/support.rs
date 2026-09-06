// Fixture row ids. coven validates every synced row's primary key as a
// canonical v4 UUID (`RowIdentity::IndependentUuid`), which is what bae's
// real ids are, so these fixtures carry UUIDs too. Each constant is named
// for the moniker it replaced, so assertions still read by name.
const RELEASE_THAT_WAS_DELETED: &str = "763072b0-643f-4469-8ac7-799c4550a769"; // was "release-that-was-deleted"

use bae_core::db::Database;
use bae_core::discogs::models::{DiscogsArtist, DiscogsRelease, DiscogsTrack};
use bae_core::import::{ImportCommand, MetadataProvenance, MetadataSource, StorageMode};
use bae_core::library::LibraryManager;
use bae_core::playback::{
    PlaybackPauseReason, PlaybackProgress, PlaybackState, RepeatMode,
    SIDE_PAUSE_CASSETTE_MESSAGE_KEY, SIDE_PAUSE_VINYL_MESSAGE_KEY,
};
use bae_test_support as support;
use coven::StoreDir;
use coven::{IdProvider, SequentialIdProvider};
use std::sync::Arc;
use std::time::{Duration, Instant};
use support::start_test_import;
use support::{
    samples_as_f32, seed_discogs_test_release, test_config, tracing_init,
    try_wait_for_import_complete, wait_for_import_complete,
};
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

/// Await the first position update (generous deadline — a fresh load or a seek
/// rebuild only starts emitting once its ring fills, which can take seconds on
/// a loaded machine), then drain progress for `settle` wall time from that
/// anchor and return the most recent track-relative `position_ms` seen. With a
/// real-time capture sink wall time tracks playback time, so this reads "where
/// the position bar sits `settle` into audible playback" — the signal that
/// distinguishes a skipped pregap (position climbs from 0 immediately) from a
/// played pregap (position counts up from a negative value, then climbs).
/// Panics if no position update ever arrives.
async fn position_after(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    settle: Duration,
) -> i64 {
    let first_deadline = Instant::now() + Duration::from_secs(30);
    let mut latest = None;
    while latest.is_none() {
        if Instant::now() >= first_deadline {
            panic!("no position update arrived within 30s of requesting one");
        }
        match timeout(Duration::from_millis(100), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::PositionUpdate { position_ms, .. })) => {
                latest = Some(position_ms)
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("progress channel closed before a position update arrived"),
            Err(_) => continue,
        }
    }
    let deadline = Instant::now() + settle;
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(100), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::PositionUpdate { position_ms, .. })) => {
                latest = Some(position_ms)
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("progress channel closed while sampling position updates"),
            Err(_) => continue,
        }
    }
    latest.expect("anchored on a first position update above")
}

/// Wait until the playing position demonstrably advances: anchor on the first
/// position update, then return the first later `position_ms` strictly greater
/// than that anchor. Early-exit counterpart to `position_after` for the "audio
/// is flowing" assertions — once the position has moved there is nothing more
/// to learn by sampling a fixed window, so this returns the instant it sees the
/// advance instead of burning the whole settle window. Panics if no first
/// update ever arrives (a broken fixture); returns `None` if the position never
/// advances within the deadline (the failure the caller asserts on).
///
/// Use `position_after` instead — not this — when the elapsed real time is
/// itself the measurement (a seek-target value, a pregap countdown, a periodic
/// persist that must have time to fire).
async fn wait_for_position_advance(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
) -> Option<i64> {
    let first_deadline = Instant::now() + Duration::from_secs(30);
    let mut anchor = None;
    while anchor.is_none() {
        if Instant::now() >= first_deadline {
            panic!("no position update arrived within 30s of requesting one");
        }
        match timeout(Duration::from_millis(100), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::PositionUpdate { position_ms, .. })) => {
                anchor = Some(position_ms)
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("progress channel closed before a position update arrived"),
            Err(_) => continue,
        }
    }
    let anchor = anchor.expect("anchored on a first position update above");
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(100), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::PositionUpdate { position_ms, .. }))
                if position_ms > anchor =>
            {
                return Some(position_ms);
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("progress channel closed while waiting for the position to advance"),
            Err(_) => continue,
        }
    }
    None
}

/// How long a `play` may take to produce its first audio before the wait is
/// treated as a hang.
///
/// A backstop, not a budget. Reaching `Playing` means the decoder filled its ring
/// — over a sparse or cloud-backed file that involves real fetches, and how long
/// they take is the machine's business, not the test's. Nothing here asserts that
/// `Playing` arrives *quickly*, only that it arrives; a loaded machine is
/// therefore slower and never redder. What the deadline catches is a decoder that
/// will never produce audio (a fill task killed, a reader deadlocked on bytes
/// that never come), and no length of wait rescues that.
const PLAY_START_BACKSTOP: Duration = Duration::from_secs(30);

/// Play `track_id` and wait for it to reach `Playing`. Shared by the fixtures
/// that drive a track from a raw progress receiver.
async fn play_and_wait_on(
    handle: &bae_core::playback::PlaybackHandle,
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_id: &str,
) {
    handle.play(track_id.to_string());
    let playing = wait_for_state_on(
        progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == track_id),
        PLAY_START_BACKSTOP,
    )
    .await;
    assert!(
        playing.is_some(),
        "track {track_id} never reached Playing within {PLAY_START_BACKSTOP:?}: playback \
         produced no audio at all. That is a stalled fill or a deadlocked decoder — not a \
         slow machine, which would only have made this wait longer."
    );
}

/// Wait for the next `Seeked` event and return its adjusted `position_ms`, or
/// `None` on timeout. Shared by `PlaybackTestFixture::wait_for_seeked` and any
/// fixture that only has a raw progress receiver (no wrapper method).
async fn wait_for_seeked_on(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    timeout_duration: Duration,
) -> Option<u64> {
    let deadline = Instant::now() + timeout_duration;
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(100), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::Seeked { position_ms, .. })) => {
                return Some(
                    u64::try_from(position_ms)
                        .expect("a seek target cannot be inside the pregap countdown"),
                );
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    None
}

/// Drain progress events up to the Playing state, returning whether Playing
/// arrived and the entries from the current queue value (each carrying a
/// per-instance id). `play` rebuilds the queue with fresh ids, so a mutation
/// must target those — captured here, after play settles. Shared by
/// `PlaybackTestFixture::wait_for_playing_capturing_queue` and any fixture
/// with only a raw progress receiver.
async fn wait_for_playing_capturing_queue_on(
    playback_handle: &bae_core::playback::PlaybackHandle,
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    timeout_duration: Duration,
) -> (bool, Vec<bae_core::playback::QueueEntry>) {
    let deadline = Instant::now() + timeout_duration;
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(100), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged {
                state: PlaybackState::Playing { .. },
            })) => {
                let mut projection = playback_handle
                    .subscribe_queue_values()
                    .borrow()
                    .clone();
                let mut entries = projection.manual;
                if let Some(ctx) = projection.context.take() {
                    entries.extend(ctx.upcoming);
                }
                return (true, entries);
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    (false, Vec::new())
}

/// Assert that audio keeps flowing: the playing position advances past its
/// anchor (via `wait_for_position_advance`). A regression that leaves the
/// queue/context projection correct but silences the actual audio stream (an
/// unrefreshed preload decoder, a promotion that never rebuilds the stream)
/// would leave the position pinned rather than climbing, and this catches it
/// where a projection-only assertion would not.
async fn assert_position_advances(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
) {
    wait_for_position_advance(progress_rx)
        .await
        .expect("position must keep advancing while playing (the audio stream stalled)");
}

/// Return the first `PositionUpdate` seen for `track_id` (its adjusted
/// position_ms), or `None` on timeout. Distinct from `wait_for_position_update`
/// in that it ignores position ticks belonging to a different track — needed
/// right after a track boundary, when a stale tick for the finishing track can
/// still be in flight.
async fn wait_for_track_position(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_id: &str,
    timeout_duration: Duration,
) -> Option<i64> {
    let deadline = Instant::now() + timeout_duration;
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(100), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::PositionUpdate {
                position_ms,
                track_id: tid,
                ..
            })) if tid == track_id => return Some(position_ms),
            Ok(Some(_)) => continue,
            Ok(None) => return None,
            Err(_) => continue,
        }
    }
    None
}

/// Wait for the next `RepeatModeChanged` and return its mode, or panic on
/// timeout.
async fn wait_for_repeat_mode(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    timeout_duration: Duration,
) -> RepeatMode {
    let deadline = Instant::now() + timeout_duration;
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(100), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::RepeatModeChanged { mode })) => return mode,
            Ok(Some(_)) => continue,
            Ok(None) => panic!("progress channel closed before a repeat-mode change"),
            Err(_) => continue,
        }
    }
    panic!("no RepeatModeChanged arrived within the timeout");
}

/// Wait for the next `MuteChanged` and return its flag, or panic on timeout.
async fn wait_for_mute(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    timeout_duration: Duration,
) -> bool {
    let deadline = Instant::now() + timeout_duration;
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(100), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::MuteChanged { is_muted })) => return is_muted,
            Ok(Some(_)) => continue,
            Ok(None) => panic!("progress channel closed before a mute change"),
            Err(_) => continue,
        }
    }
    panic!("no MuteChanged arrived within the timeout");
}

/// What a track boundary looked like on the wire. A *gapless* handoff crosses
/// inside the running stream: `handle_track_crossed` reports the finishing
/// track's `DecodeStats` but no `TrackCompleted` (that fires only when nothing
/// is staged and the stream rebuilds), and the incoming track never surfaces a
/// `Loading` state (the rebuild path's UI arc). So `completed_for_finishing ==
/// false` with `decode_stats_for_finishing == true` and `loading_for_incoming ==
/// false` is the gapless signature.
struct BoundaryOutcome {
    decode_stats_for_finishing: bool,
    completed_for_finishing: bool,
    loading_for_incoming: bool,
    reached_incoming: bool,
    decode_errors: u32,
}

/// Drain progress until `incoming` reaches Playing (or timeout), recording
/// whether `finishing`'s DecodeStats and/or TrackCompleted, any Loading state for
/// `incoming`, and the total decode errors reported across the crossing — i.e.
/// whether the boundary was gapless or a rebuild, and whether it decoded cleanly.
async fn observe_boundary(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    finishing: &str,
    incoming: &str,
    timeout_duration: Duration,
) -> BoundaryOutcome {
    let deadline = Instant::now() + timeout_duration;
    let mut outcome = BoundaryOutcome {
        decode_stats_for_finishing: false,
        completed_for_finishing: false,
        loading_for_incoming: false,
        reached_incoming: false,
        decode_errors: 0,
    };
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(200), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::DecodeStats {
                track_id,
                error_count,
                ..
            })) => {
                outcome.decode_errors += error_count;
                if track_id == finishing {
                    outcome.decode_stats_for_finishing = true;
                }
            }
            Ok(Some(PlaybackProgress::TrackCompleted { track_id })) if track_id == finishing => {
                outcome.completed_for_finishing = true;
            }
            Ok(Some(PlaybackProgress::StateChanged {
                state: PlaybackState::Loading { track_id, .. },
            })) if track_id == incoming => {
                outcome.loading_for_incoming = true;
            }
            Ok(Some(PlaybackProgress::StateChanged {
                state: PlaybackState::Playing { track_info, .. },
            })) if track_info.track_id == incoming => {
                outcome.reached_incoming = true;
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    outcome
}

/// Capture-stream receiver kept alive so the sink's `create_stream` has a
/// receiver to hand each stream's buffer to (dropping it fails stream creation).
type CaptureStreamRx = tokio::sync::mpsc::UnboundedReceiver<Arc<std::sync::Mutex<Vec<f32>>>>;

/// Start a playback service backed by a real-time capture sink, so a test runs
/// with no audio device while the decoder is still paced to wall-clock like the
/// real device. The returned receiver must be held for the service's lifetime.
#[must_use]
fn start_capture_service(
    library_manager: LibraryManager,
    runtime_handle: tokio::runtime::Handle,
) -> (bae_core::playback::PlaybackHandle, CaptureStreamRx) {
    start_capture_service_with_restore(library_manager, runtime_handle, true)
}

/// `start_capture_service` with the platform's "Restore on launch" preference
/// explicit, for tests that cover the restore-off launch path.
#[must_use]
fn start_capture_service_with_restore(
    library_manager: LibraryManager,
    runtime_handle: tokio::runtime::Handle,
    restore_playback: bool,
) -> (bae_core::playback::PlaybackHandle, CaptureStreamRx) {
    let (capture_device, capture_stream_rx) =
        bae_core::playback::RealtimeCaptureAudioDevice::new();
    let handle = library_manager.start_playback_service_with_audio_device(
        runtime_handle,
        100,
        restore_playback,
        Box::new(capture_device),
    );
    (handle, capture_stream_rx)
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
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await?;
    let database_arc = Arc::new(database.clone());
    let library_dir = StoreDir::new(temp_dir.path().to_path_buf());
    let config_handle = test_config(&library_dir);
    let runtime_handle = tokio::runtime::Handle::current();
    let library_manager = LibraryManager::new(
        (*database_arc).clone(),
        config_handle,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        runtime_handle.clone(),
        bae_core::import::cover_art::RemoteImageCache::for_test(),
    );
    configure(&library_manager)?;

    let release_id_key = seed_discogs_test_release(release);
    generate_files(&album_dir);

    let import_handle = start_test_import(runtime_handle.clone(), library_manager.clone()).await;
    import_handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: candidate_key.to_string(),
            source: bae_core::import::release_candidate::CandidateSource::Folder { path: album_dir.clone(), scope: bae_core::import::ReleaseFileScope::Recursive },
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            metadata_provenance: Some(MetadataProvenance::ExternalRelease {
                source: MetadataSource::Discogs,
                release_id: release_id_key,
                partners: vec![],
            }),
            user_edit: None,
        })
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
    let mut progress_rx = import_handle.subscribe_import(import_id);
    let (release_id, _album_id) = wait_for_import_complete(&mut progress_rx).await;
    let tracks = library_manager.get_tracks_for_release(&release_id).await?;
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

/// The three-track local FLAC album every `PlaybackTestFixture` plays,
/// imported once (decode-verify + DB writes) into a template library directory
/// that outlives every test. Each `PlaybackTestFixture::new` clones the
/// template's small DB/library files into its own `TempDir` rather than
/// re-importing, so the import cost is paid once per process instead of once
/// per test.
///
/// The FLAC files themselves are never cloned: `local_blob_refs` stores an
/// absolute path, so a clone's DB rows — and `album_dir`, which one preview
/// test reads a real file path from — resolve straight back to the template's
/// stable `album/` directory, held alive for the process's lifetime.
struct PlaybackFixtureTemplate {
    dir: TempDir,
    album_dir: std::path::PathBuf,
    track_ids: Vec<String>,
}

static PLAYBACK_FIXTURE_TEMPLATE: std::sync::LazyLock<PlaybackFixtureTemplate> =
    std::sync::LazyLock::new(|| {
        // A dedicated runtime, not a calling test's: the import must finish and
        // every task/connection it spawned must be torn down (dropping the
        // runtime blocks until they are) before the template directory is safe
        // to copy — coven opens SQLite in WAL mode, so a copy taken while a
        // connection is still live could catch an unmerged -wal file.
        let rt =
            tokio::runtime::Runtime::new().expect("build the playback template import's runtime");
        let template = rt.block_on(async {
            let import_ids = SequentialIdProvider::new("playback-fixture-template");
            let setup = imported_release_setup(
                create_test_album(),
                "test",
                import_ids.new_id(),
                |album_dir| {
                    let _track_data = generate_test_flac_files(album_dir);
                },
                |_| Ok(()),
            )
            .await
            .expect("import the playback fixture template release");
            assert!(
                !setup.track_ids.is_empty(),
                "the playback fixture template should import tracks"
            );
            PlaybackFixtureTemplate {
                album_dir: setup.album_dir.clone(),
                dir: setup.temp_dir,
                track_ids: setup.track_ids,
            }
        });
        drop(rt);
        template
    });

/// Clone the playback fixture template into a fresh `TempDir`, returning it
/// with the template's stable `album/` path and track ids. Dereferences the
/// template (running its one-time import on init) so callers invoke this on a
/// blocking thread — the template's initializer builds and blocks on its own
/// runtime, which a test's async thread can't. Mirrors
/// `clone_multi_window_library`.
fn clone_playback_fixture_library() -> (TempDir, std::path::PathBuf, Vec<String>) {
    let template = &*PLAYBACK_FIXTURE_TEMPLATE;
    let fresh = clone_template_library(template.dir.path());
    (
        fresh,
        template.album_dir.clone(),
        template.track_ids.clone(),
    )
}

/// Test helper to set up playback service with imported test tracks
struct PlaybackTestFixture {
    playback_handle: bae_core::playback::PlaybackHandle,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_ids: Vec<String>,
    album_dir: std::path::PathBuf,
    /// Held so tests can read the device-local `playback_state` row directly,
    /// without going through Shutdown/SaveState.
    library_manager: LibraryManager,
    /// Held so the capture sink's `create_stream` can hand off each stream's
    /// buffer — dropping the receiver would fail stream creation. The fixture's
    /// tests don't inspect the samples, only playback state.
    _capture_stream_rx: tokio::sync::mpsc::UnboundedReceiver<Arc<std::sync::Mutex<Vec<f32>>>>,
    _temp_dir: TempDir,
}
impl PlaybackTestFixture {
    async fn new() -> Self {
        // Clone the shared template's DB/library into this test's own TempDir
        // rather than re-importing — the import runs once per process in
        // PLAYBACK_FIXTURE_TEMPLATE. The cloned DB and album_dir both resolve to
        // the template's stable album/ directory (absolute local_blob_refs).
        // The template is dereferenced inside spawn_blocking: its lazy
        // initializer builds and blocks on its own runtime, which a test's
        // async thread can't do (no runtime-within-a-runtime).
        let (temp_dir, album_dir, track_ids) =
            tokio::task::spawn_blocking(clone_playback_fixture_library)
                .await
                .expect("clone the playback fixture template library");

        let db_path = temp_dir.path().join("test.db");
        let database = Database::new_test(
            db_path.to_str().expect("db path is valid UTF-8"),
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .expect("open the cloned playback database");
        let library_dir = StoreDir::new(temp_dir.path().to_path_buf());
        let config_handle = test_config(&library_dir);
        let runtime_handle = tokio::runtime::Handle::current();
        let library_manager = LibraryManager::new(
            database,
            config_handle,
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
            bae_core::diagnostics::Diagnostics::noop(),
            runtime_handle.clone(),
            bae_core::import::cover_art::RemoteImageCache::for_test(),
        );

        // A real-time capture sink stands in for the audio device: no hardware
        // required, and it paces the decoder to wall-clock like a real device so
        // position/seek/auto-advance timing matches production.
        let (capture_device, capture_stream_rx) =
            bae_core::playback::RealtimeCaptureAudioDevice::new();
        let playback_handle = library_manager.start_playback_service_with_audio_device(
            runtime_handle,
            100,
            true,
            Box::new(capture_device),
        );
        let progress_rx = playback_handle.subscribe_progress();
        Self {
            playback_handle,
            progress_rx,
            track_ids,
            album_dir,
            library_manager,
            _capture_stream_rx: capture_stream_rx,
            _temp_dir: temp_dir,
        }
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
    /// arrived and the entries from the current queue value (each carrying a
    /// per-instance id). `play` rebuilds the queue with fresh ids, so a mutation
    /// must target those — captured here, after play settles.
    async fn wait_for_playing_capturing_queue(
        &mut self,
        timeout_duration: Duration,
    ) -> (bool, Vec<bae_core::playback::QueueEntry>) {
        wait_for_playing_capturing_queue_on(
            &self.playback_handle,
            &mut self.progress_rx,
            timeout_duration,
        )
        .await
    }
    /// Wait for a position update with timeout (returns position in ms)
    async fn wait_for_position_update(&mut self, timeout_duration: Duration) -> Option<u64> {
        let deadline = Instant::now() + timeout_duration;
        while Instant::now() < deadline {
            match timeout(Duration::from_millis(100), self.progress_rx.recv()).await {
                Ok(Some(PlaybackProgress::PositionUpdate { position_ms, .. })) => {
                    return Some(
                        u64::try_from(position_ms)
                            .expect("this helper expects playback at or after track start"),
                    );
                }
                Ok(Some(_)) => continue,
                Ok(None) => break,
                Err(_) => continue,
            }
        }
        None
    }
    /// Wait for the first position update whose `position_ms` is strictly past
    /// `floor_ms`, or `None` on timeout. Blocks on the real "position has moved
    /// beyond X" signal — the correct wait when a test resumes or seeks and then
    /// asserts the position climbed past the seek target (the first raw update
    /// can still report the seek anchor itself, so filtering on the floor is
    /// what proves audio actually advanced).
    async fn wait_for_position_past(
        &mut self,
        floor_ms: u64,
        timeout_duration: Duration,
    ) -> Option<u64> {
        let deadline = Instant::now() + timeout_duration;
        while Instant::now() < deadline {
            match timeout(Duration::from_millis(100), self.progress_rx.recv()).await {
                Ok(Some(PlaybackProgress::PositionUpdate { position_ms, .. }))
                    if position_ms
                        > i64::try_from(floor_ms).expect("position floor exceeds i64 range") =>
                {
                    return Some(
                        u64::try_from(position_ms)
                            .expect("position past a nonnegative floor is nonnegative"),
                    );
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
        wait_for_seeked_on(&mut self.progress_rx, timeout_duration).await
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
