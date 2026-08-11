#![cfg(feature = "test-utils")]
// Fixture row ids. coven validates every synced row's primary key as a
// canonical v4 UUID (`RowIdentity::IndependentUuid`), which is what bae's
// real ids are, so these fixtures carry UUIDs too. Each constant is named
// for the moniker it replaced, so assertions still read by name.
const RELEASE_THAT_WAS_DELETED: &str = "763072b0-643f-4469-8ac7-799c4550a769"; // was "release-that-was-deleted"

use bae_core::db::Database;
use bae_core::discogs::models::{DiscogsArtist, DiscogsRelease, DiscogsTrack};
use bae_core::import::{IdentityChoice, ImportCommand, MetadataRef, MetadataSource, StorageMode};
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
    samples_as_f32, seed_discogs_test_release, test_config_and_keys, tracing_init,
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
/// anchor and return the most recent adjusted `position_ms` seen. With a
/// real-time capture sink wall time tracks playback time, so this reads "where
/// the position bar sits `settle` into audible playback" — the signal that
/// distinguishes a skipped pregap (position climbs from 0 immediately) from a
/// played pregap (position pinned at 0 for the pregap's length, then climbs).
/// Panics if no position update ever arrives.
async fn position_after(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    settle: Duration,
) -> u64 {
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
/// itself the measurement (a seek-target value, a pregap that must stay pinned
/// at 0 across its length, a periodic persist that must have time to fire).
async fn wait_for_position_advance(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
) -> Option<u64> {
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
                return Some(position_ms);
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    None
}

/// Drain progress events up to the Playing state, returning whether Playing
/// arrived and the entries from the most recent `QueueUpdated` seen (each
/// carrying a per-instance id). `play` rebuilds the queue with fresh ids, so a
/// mutation must target those — captured here, after play settles. Shared by
/// `PlaybackTestFixture::wait_for_playing_capturing_queue` and any fixture
/// with only a raw progress receiver.
async fn wait_for_playing_capturing_queue_on(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    timeout_duration: Duration,
) -> (bool, Vec<bae_core::playback::QueueEntry>) {
    let deadline = Instant::now() + timeout_duration;
    let mut entries = Vec::new();
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(100), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::QueueUpdated(projection))) => {
                // The mutation targets entries in either lane, so flatten the
                // two lanes into one play-order list for id lookup.
                entries = projection.manual;
                if let Some(ctx) = projection.context {
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
) -> Option<u64> {
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
    let (capture_output, capture_stream_rx) = bae_core::playback::RealtimeCaptureAudioOutput::new();
    let handle = library_manager.start_playback_service_with_output(
        runtime_handle,
        100,
        restore_playback,
        Box::new(capture_output),
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
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let runtime_handle = tokio::runtime::Handle::current();
    let library_manager = LibraryManager::new(
        (*database_arc).clone(),
        config_handle,
        key_service,
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
            folder: album_dir.clone(),
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
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
        let (config_handle, key_service) = test_config_and_keys(&library_dir);
        let runtime_handle = tokio::runtime::Handle::current();
        let library_manager = LibraryManager::new(
            database,
            config_handle,
            key_service,
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
            bae_core::diagnostics::Diagnostics::noop(),
            runtime_handle.clone(),
            bae_core::import::cover_art::RemoteImageCache::for_test(),
        );

        // A real-time capture sink stands in for the audio device: no hardware
        // required, and it paces the decoder to wall-clock like a real device so
        // position/seek/auto-advance timing matches production.
        let (capture_output, capture_stream_rx) =
            bae_core::playback::RealtimeCaptureAudioOutput::new();
        let playback_handle = library_manager.start_playback_service_with_output(
            runtime_handle,
            100,
            true,
            Box::new(capture_output),
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
    /// arrived and the entries from the most recent QueueUpdated seen (each
    /// carrying a per-instance id). `play` rebuilds the queue with fresh ids, so
    /// a mutation must target those — captured here, after play settles.
    async fn wait_for_playing_capturing_queue(
        &mut self,
        timeout_duration: Duration,
    ) -> (bool, Vec<bae_core::playback::QueueEntry>) {
        wait_for_playing_capturing_queue_on(&mut self.progress_rx, timeout_duration).await
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
                    if position_ms > floor_ms =>
                {
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
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "2".to_string(),
                title: "Test Track 2".to_string(),
                duration: Some("0:10".to_string()),
                artists: vec![],
                extraartists: None,
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "3".to_string(),
                title: "Test Track 3".to_string(),
                duration: Some("0:10".to_string()),
                artists: vec![],
                extraartists: None,
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
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "2".to_string(),
                title: "Track Two (White Noise)".to_string(),
                duration: Some("0:10".to_string()),
                artists: vec![],
                extraartists: None,
            },
            DiscogsTrack {
                type_: "track".to_string(),
                position: "3".to_string(),
                title: "Track Three (Brown Noise)".to_string(),
                duration: Some("0:10".to_string()),
                artists: vec![],
                extraartists: None,
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
        let (capture_output, capture_stream_rx) = bae_core::playback::CaptureAudioOutput::new();
        Self::with_capture_output(Box::new(capture_output), capture_stream_rx).await
    }

    /// Real-time-paced capture: the drain sleeps each buffer's wall-clock
    /// duration, so the decoder fills the ring and parks instead of racing
    /// whole tracks ahead. Required for tests that play, then issue a command
    /// (seek, pause) that must land on the track under test.
    async fn with_realtime_capture() -> Result<Self, Box<dyn std::error::Error>> {
        let (capture_output, capture_stream_rx) =
            bae_core::playback::RealtimeCaptureAudioOutput::new();
        Self::with_capture_output(Box::new(capture_output), capture_stream_rx).await
    }

    async fn with_capture_output(
        capture_output: Box<dyn bae_core::playback::AudioOutput>,
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
        let (config_handle, key_service) = test_config_and_keys(&library_dir);
        let library_manager = LibraryManager::new(
            (*database_arc).clone(),
            config_handle,
            key_service,
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
                identity_choice: IdentityChoice::Exact {
                    release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
                },
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

        let playback_handle = library_manager.start_playback_service_with_output(
            runtime_handle,
            100,
            true,
            capture_output,
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
        let (capture_output, capture_stream_rx) =
            bae_core::playback::RealtimeCaptureAudioOutput::new();
        let playback_handle = setup.library_manager.start_playback_service_with_output(
            setup.runtime_handle,
            100,
            true,
            Box::new(capture_output),
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
async fn enabling_setting_mid_track_pauses_at_the_imminent_boundary() {
    // The setting starts OFF: A2 preloads B1 and stages it gapless into the
    // live audio chain immediately after A2 starts. Turning the setting on
    // while A2 is still playing must still catch that already-staged
    // boundary — the natural way anyone would try the feature.
    let mut fixture = SidePauseTestFixture::new("Vinyl", ["A1", "A2", "B1"], false)
        .await
        .expect("side-pause fixture");
    let side_a_track_id = fixture.track_ids[1].clone();

    fixture.play_track_and_wait(1, &side_a_track_id).await;

    fixture.set_pause_between_sides_mid_track(true);

    fixture.seek_to_auto_advance();

    fixture
        .wait_for_side_pause("A", SIDE_PAUSE_VINYL_MESSAGE_KEY)
        .await;
}

#[tokio::test]
async fn disabling_setting_mid_track_keeps_playing_across_the_boundary() {
    // Regression guard for the direction that already worked: the setting
    // starts ON, so B1 is held (not staged) rather than gaplessly chained.
    // Turning the setting off mid-track must let it play straight through —
    // the drain-time gate re-reads the config before the boundary fires.
    let mut fixture = SidePauseTestFixture::new("Vinyl", ["A1", "A2", "B1"], true)
        .await
        .expect("side-pause fixture");
    let side_a_track_id = fixture.track_ids[1].clone();
    let next_side_track_id = fixture.track_ids[2].clone();

    fixture.play_track_and_wait(1, &side_a_track_id).await;

    fixture.set_pause_between_sides_mid_track(false);

    fixture.seek_to_auto_advance();

    fixture
        .wait_for_playing_track(
            &next_side_track_id,
            Duration::from_secs(10),
            "disabling the setting mid-track should keep playing across the boundary",
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

#[derive(Clone, Copy)]
enum SkipDirection {
    Next,
    Previous,
}

impl SkipDirection {
    fn label(self) -> &'static str {
        match self {
            SkipDirection::Next => "Next",
            SkipDirection::Previous => "Previous",
        }
    }
}

/// Next and Previous preserve the current play/pause state: pressing either
/// while paused lands on the adjacent track still paused; while playing, still
/// playing. (Fresh `play` always starts playing — that's the deliberate
/// exception, pinned by `test_fresh_play_always_starts_playing`.)
async fn assert_skip_preserves_play_state(direction: SkipDirection, start_paused: bool) {
    let mut fixture = PlaybackTestFixture::new().await;
    let first_track_id = fixture.track_ids[0].clone();
    let second_track_id = fixture.track_ids[1].clone();

    // Previous needs a track behind the cursor, so start on the second track;
    // Next starts on the first. The target is the adjacent track either way.
    let (start_track_id, target_track_id) = match direction {
        SkipDirection::Next => (first_track_id, second_track_id),
        SkipDirection::Previous => (second_track_id, first_track_id),
    };

    fixture.playback_handle.play(start_track_id.clone());
    fixture
        .wait_for_state(
            |s| {
                matches!(s, PlaybackState::Playing { track_info, .. }
                    if track_info.track_id == start_track_id)
            },
            Duration::from_secs(5),
        )
        .await
        .expect("the starting track should play");

    if start_paused {
        fixture.playback_handle.pause();
        fixture
            .wait_for_state(
                |s| matches!(s, PlaybackState::Paused { .. }),
                Duration::from_secs(2),
            )
            .await
            .expect("playback should pause");
    }

    // Press promptly (well inside Previous's 3s window) so Previous steps back a
    // track rather than restarting the current one.
    match direction {
        SkipDirection::Next => fixture.playback_handle.next(),
        SkipDirection::Previous => fixture.playback_handle.previous(),
    }

    let landed = fixture
        .wait_for_state(
            |s| {
                let (track_info, is_paused) = match s {
                    PlaybackState::Playing { track_info, .. } => (track_info, false),
                    PlaybackState::Paused { track_info, .. } => (track_info, true),
                    _ => return false,
                };
                track_info.track_id == target_track_id && is_paused == start_paused
            },
            Duration::from_secs(5),
        )
        .await;
    assert!(
        landed.is_some(),
        "{} while {} should land on the adjacent track in the same play/pause state",
        direction.label(),
        if start_paused { "paused" } else { "playing" },
    );
}

#[tokio::test]
async fn next_while_paused_stays_paused() {
    assert_skip_preserves_play_state(SkipDirection::Next, true).await;
}

#[tokio::test]
async fn next_while_playing_stays_playing() {
    assert_skip_preserves_play_state(SkipDirection::Next, false).await;
}

#[tokio::test]
async fn previous_while_paused_stays_paused() {
    assert_skip_preserves_play_state(SkipDirection::Previous, true).await;
}

#[tokio::test]
async fn previous_while_playing_stays_playing() {
    assert_skip_preserves_play_state(SkipDirection::Previous, false).await;
}

/// Pause and seek interact correctly in both orderings, which exercise different
/// code paths. Seek issued WHILE paused (the is_playing-not-set-after-seek-while-
/// paused regression: seek clears is_playing, and if only Pause is re-sent it
/// stays false so audio never resumes): the seek lands without auto-playing, and
/// once resumed the position advances — audio actually flows. Seek issued while
/// PLAYING, then paused: the position is preserved across the pause and resume.
#[tokio::test]
async fn pause_and_seek_interact_in_both_orderings() {
    // Ordering 1 — seek while paused, then resume: position advances.
    let mut fixture = PlaybackTestFixture::new().await;
    let track_id = fixture.track_ids[0].clone();
    fixture.playback_handle.play(track_id.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await
        .expect("should start playing");

    fixture.playback_handle.pause();
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Paused { .. }),
            Duration::from_secs(2),
        )
        .await
        .expect("should be paused");

    fixture.playback_handle.seek(Duration::from_secs(2));
    let seeked_ms = fixture
        .wait_for_seeked(Duration::from_secs(5))
        .await
        .expect("Seeked event after seeking while paused");
    assert!(
        seeked_ms >= 1900,
        "seek should land near 2s, got {seeked_ms}ms"
    );

    // A seek while paused must not auto-play.
    assert!(
        fixture
            .wait_for_state(
                |s| matches!(s, PlaybackState::Playing { .. }),
                Duration::from_millis(200),
            )
            .await
            .is_none(),
        "should stay paused after seek, not auto-play"
    );

    fixture.playback_handle.resume();
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(2),
        )
        .await
        .expect("should resume playing");
    let advanced_ms = fixture
        .wait_for_position_past(seeked_ms, Duration::from_secs(5))
        .await
        .unwrap_or_else(|| {
            panic!(
                "position should advance past the seek target after resume; seeked {seeked_ms}ms"
            )
        });
    assert!(advanced_ms > seeked_ms);

    // Ordering 2 — seek while playing, then pause and resume: position maintained.
    let mut fixture = PlaybackTestFixture::new().await;
    let track_id = fixture.track_ids[0].clone();
    fixture.playback_handle.play(track_id.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await
        .expect("should start playing");

    let seek_target = Duration::from_secs(2);
    fixture.playback_handle.seek(seek_target);
    let seeked_ms = fixture
        .wait_for_seeked(Duration::from_secs(2))
        .await
        .expect("Seeked event");
    assert!(
        Duration::from_millis(seeked_ms).abs_diff(seek_target) < Duration::from_secs(1),
        "seek should land near 2s, got {seeked_ms}ms"
    );

    fixture.playback_handle.pause();
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Paused { .. }),
            Duration::from_secs(2),
        )
        .await
        .expect("should be paused");
    fixture.playback_handle.resume();
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(2),
        )
        .await
        .expect("should resume playing");
    let maintained_ms = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await
        .expect("position update after resume");
    assert!(
        Duration::from_millis(maintained_ms).abs_diff(seek_target) < Duration::from_secs(1),
        "position should be maintained across pause/resume, got {maintained_ms}ms"
    );
}

#[tokio::test]
async fn test_fresh_play_always_starts_playing() {
    // Fresh play should always start playing, even if previously paused

    let mut fixture = PlaybackTestFixture::new().await;

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
    let mut fixture = PlaybackTestFixture::new().await;

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

    // Position must climb past the seek target — the signal that audio is
    // actually playing, not just that the seek landed.
    let final_position_ms = fixture
        .wait_for_position_past(seeked_position_ms, Duration::from_secs(5))
        .await
        .unwrap_or_else(|| {
            panic!(
                "position should advance past the seek target while playing; seeked to {seeked_position_ms}ms"
            )
        });
    assert!(final_position_ms > seeked_position_ms);
}

/// The true gapless handoff: a track played to its natural end with the next
/// track staged crosses the boundary inside the running stream
/// (boundary_rx → handle_track_crossed → advance_to_preloaded), never rebuilding
/// via the TrackCompleted → AutoAdvance path. The signature is that the
/// finishing track emits its DecodeStats (reported by the boundary handler) but
/// not TrackCompleted, and the incoming track's position resets to 0.
#[tokio::test]
async fn gapless_boundary_hands_off_without_rebuild() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    let second = fixture.track_ids[1].clone();

    fixture.playback_handle.play(first.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
            Duration::from_secs(5),
        )
        .await
        .expect("the first track should play");

    // Seek partway to bring the natural end sooner while leaving ample runway:
    // the staged next is re-staged well before the decoder reaches EOF. (Seeking
    // right up against the end instead is the not-ready-fallback case below.)
    fixture.playback_handle.seek(Duration::from_secs(3));
    fixture
        .wait_for_seeked(Duration::from_secs(5))
        .await
        .expect("the seek should land");

    let outcome = observe_boundary(
        &mut fixture.progress_rx,
        &first,
        &second,
        Duration::from_secs(8),
    )
    .await;
    assert!(
        outcome.reached_incoming,
        "playback should cross into the second track at the first track's end"
    );
    assert!(
        outcome.decode_stats_for_finishing,
        "the gapless boundary handler reports the finishing track's decode stats"
    );
    assert!(
        !outcome.completed_for_finishing,
        "a gapless handoff must not emit TrackCompleted for the finishing track — \
         that event only fires on the stream-rebuild path"
    );
    assert!(
        !outcome.loading_for_incoming,
        "a gapless handoff must not emit a Loading state for the incoming track — \
         the staged next plays straight through, only the stream-rebuild path shows Loading"
    );
    assert_eq!(
        outcome.decode_errors, 0,
        "the crossing must decode cleanly, got {} decode errors",
        outcome.decode_errors
    );

    let position_ms =
        wait_for_track_position(&mut fixture.progress_rx, &second, Duration::from_secs(2))
            .await
            .expect("a position update for the incoming track");
    assert!(
        position_ms < 1500,
        "the incoming track's position resets near 0 at the boundary, got {position_ms}ms"
    );
}

/// The boundary handler re-preloads the *following* track, so a chain of natural
/// ends crosses gaplessly the whole way down. Play track 0 → track 1 (first
/// crossing, which re-preloads track 2) → track 2: the second crossing is also
/// gapless, which it can only be if track 2 was staged during the first.
#[tokio::test]
async fn gapless_boundary_repreloads_following_track() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    let second = fixture.track_ids[1].clone();
    let third = fixture.track_ids[2].clone();

    fixture.playback_handle.play(first.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
            Duration::from_secs(5),
        )
        .await
        .expect("the first track should play");

    fixture.playback_handle.seek(Duration::from_secs(3));
    fixture
        .wait_for_seeked(Duration::from_secs(5))
        .await
        .expect("the first seek should land");
    let first_cross = observe_boundary(
        &mut fixture.progress_rx,
        &first,
        &second,
        Duration::from_secs(8),
    )
    .await;
    assert!(
        first_cross.reached_incoming,
        "playback should cross into the second track"
    );

    // Shorten the second track too, then let it end. If the third track was
    // re-preloaded and staged during the first crossing, this crosses gaplessly.
    fixture.playback_handle.seek(Duration::from_secs(3));
    fixture
        .wait_for_seeked(Duration::from_secs(5))
        .await
        .expect("the second seek should land");
    let second_cross = observe_boundary(
        &mut fixture.progress_rx,
        &second,
        &third,
        Duration::from_secs(8),
    )
    .await;
    assert!(
        second_cross.reached_incoming,
        "the following track was re-preloaded, so the second boundary also crosses into it"
    );
    assert!(
        second_cross.decode_stats_for_finishing,
        "the second boundary reports the finishing track's decode stats"
    );
    assert!(
        !second_cross.completed_for_finishing,
        "the re-preloaded following track crosses gaplessly — no rebuild"
    );
}

/// The rebuild advance: seeking right up against the end leaves the post-seek
/// decoder with ~no samples, so it hits EOF and completes rather than crossing
/// the staged boundary — driving TrackCompleted → AutoAdvance →
/// advance_and_play_preloaded, which recovers the preloaded next and plays it
/// (advance.rs ~407, the has_preloaded_next branch). This is the non-gapless
/// counterpart to the two handoff tests above: the next track still plays from
/// its start via the preloaded decoder.
///
/// (The sibling not-ready fallback at ~413 — play_track when the staged source
/// was already consumed — is a defensive race between a boundary crossing and a
/// concurrent Next/AutoAdvance; the serial command loop keeps those from
/// interleaving, so it isn't deterministically reachable from a black-box test.)
#[tokio::test]
async fn boundary_advances_to_next_track_after_late_seek() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    let second = fixture.track_ids[1].clone();

    fixture.playback_handle.play(first.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
            Duration::from_secs(5),
        )
        .await
        .expect("the first track should play");

    // Tracks run ~5s; seek to 4.8s leaves ~0.2s before the end.
    fixture.playback_handle.seek(Duration::from_millis(4800));
    fixture
        .wait_for_seeked(Duration::from_secs(5))
        .await
        .expect("the seek should land");

    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == second),
            Duration::from_secs(10),
        )
        .await
        .expect("playback should advance to the second track at the end");
    let position_ms =
        wait_for_track_position(&mut fixture.progress_rx, &second, Duration::from_secs(3))
            .await
            .expect("a position update for the second track");
    assert!(
        position_ms < 1500,
        "the next track starts from ~0, got {position_ms}ms"
    );
}

/// A seek rebuilds the stream, but the staged gapless next must survive it
/// (seek.rs take_next → re-stage_next): the subsequent natural end still crosses
/// gaplessly. Seek twice to stress the take-out/re-stage across an
/// already-re-staged source, then let the track end and assert the boundary is
/// gapless (finishing track emits DecodeStats but not TrackCompleted).
#[tokio::test]
async fn seek_preserves_staged_next_for_a_gapless_advance() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    let second = fixture.track_ids[1].clone();

    fixture.playback_handle.play(first.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
            Duration::from_secs(5),
        )
        .await
        .expect("the first track should play");

    // Two seeks: each takes the staged next out of the old stream and re-stages
    // it into the rebuilt one. Both land with runway to spare before the end.
    fixture.playback_handle.seek(Duration::from_secs(1));
    fixture
        .wait_for_seeked(Duration::from_secs(5))
        .await
        .expect("the first seek should land");
    fixture.playback_handle.seek(Duration::from_secs(3));
    fixture
        .wait_for_seeked(Duration::from_secs(5))
        .await
        .expect("the second seek should land");

    let outcome = observe_boundary(
        &mut fixture.progress_rx,
        &first,
        &second,
        Duration::from_secs(8),
    )
    .await;
    assert!(
        outcome.reached_incoming,
        "the staged next should survive the seeks and play at the boundary"
    );
    assert!(
        !outcome.completed_for_finishing,
        "the advance after seeking must stay gapless — the staged next was preserved, not rebuilt"
    );
    assert!(
        outcome.decode_stats_for_finishing,
        "the gapless boundary handler still reports the finishing track's decode stats"
    );
}

/// A same-format seek swaps the persistent output's source in place
/// (`PlaybackSource::replace`) — it never rebuilds the device stream, so it has
/// no `create_stream` step that can fail. Dropping the capture sink's stream
/// receiver (which under the old per-seek-rebuild model made the seek's
/// `create_stream` error and stopped playback) now only fails the non-fatal,
/// logged capture-buffer rotation: the seek still lands and playback continues.
/// This pins that a dropped test observer can't tear playback down.
#[tokio::test]
async fn seek_with_dropped_capture_receiver_keeps_playing() {
    let lib = restore_test_library().await;
    let first = lib.track_ids[0].clone();

    // Build a capture-backed service directly so the test owns the stream
    // receiver and can drop it mid-session (real-time paced so the track doesn't
    // race to its end before we seek).
    let (capture_output, capture_stream_rx) = bae_core::playback::RealtimeCaptureAudioOutput::new();
    let mut capture_stream_rx = Some(capture_stream_rx);
    let handle = lib.library_manager.start_playback_service_with_output(
        lib.runtime_handle,
        100,
        true,
        Box::new(capture_output),
    );
    let mut progress_rx = handle.subscribe_progress();

    handle.play(first.clone());
    wait_for_state_on(
        &mut progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
        Duration::from_secs(5),
    )
    .await
    .expect("the track should start playing (first stream created)");

    // Drop the receiver: under the old per-seek rebuild this made create_stream
    // fail; the persistent output uses replace, so the seek must still land.
    capture_stream_rx.take();
    handle.seek(Duration::from_secs(2));

    // The seek lands (Seeked for the same track); no error, no stop.
    let mut saw_seeked = false;
    let mut saw_error = false;
    let mut saw_stopped = false;
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(200), progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::Seeked { track_id, .. })) if track_id == first => {
                saw_seeked = true;
                break;
            }
            Ok(Some(PlaybackProgress::PlaybackError { .. })) => saw_error = true,
            Ok(Some(PlaybackProgress::StateChanged {
                state: PlaybackState::Stopped,
            })) => {
                saw_stopped = true;
                break;
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    assert!(saw_seeked, "an in-place seek should land and emit Seeked");
    assert!(
        !saw_error,
        "an in-place seek must not surface a PlaybackError"
    );
    assert!(!saw_stopped, "an in-place seek must not stop playback");

    handle.stop();
}

#[tokio::test]
async fn test_previous_track_navigation() {
    let mut fixture = PlaybackTestFixture::new().await;
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
async fn test_same_position_seek_keeps_position_updates_flowing() {
    let mut fixture = PlaybackTestFixture::new().await;
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
    let current_pos_ms = fixture
        .wait_for_position_update(Duration::from_secs(2))
        .await
        .expect("a position update should arrive while playing");
    let same_position = Duration::from_millis(current_pos_ms + 50);
    fixture.playback_handle.seek(same_position);
    let seeked_position = fixture.wait_for_seeked(Duration::from_secs(2)).await;
    assert!(
        seeked_position.is_some(),
        "Should receive Seeked event when position difference < 100ms",
    );
    if let Some(seeked_position) = seeked_position {
        let diff = Duration::from_millis(seeked_position).abs_diff(same_position);
        assert!(
            diff < Duration::from_millis(100),
            "Seeked display should stay near the requested position, got {:?}",
            diff,
        );
    }
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
    let mut fixture = PlaybackTestFixture::new().await;
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

    // `play` refills the context lane from the track's release, minting fresh
    // entry ids, and leaves Up Next alone — so capture the entries *after* play
    // settles: drain up to the Playing state, keeping the latest QueueUpdated.
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

    let state = next_state.expect("Next should switch off track0 after the queue mutation");
    let playing_id = match &state {
        PlaybackState::Playing { track_info, .. } => track_info.track_id.clone(),
        PlaybackState::Paused { track_info, .. } => track_info.track_id.clone(),
        _ => unreachable!(),
    };
    assert_eq!(playing_id, expected);

    // The queue/context projection landing on `expected` isn't proof audio is
    // actually flowing on it — assert the position keeps climbing too.
    if matches!(state, PlaybackState::Playing { .. }) {
        assert_position_advances(&mut fixture.progress_rx).await;
    }
}

#[tokio::test]
async fn test_add_next_displaces_preloaded_track() {
    let mut fixture = PlaybackTestFixture::new().await;
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
    let mut fixture = PlaybackTestFixture::new().await;
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
    let mut fixture = PlaybackTestFixture::new().await;
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
    let mut fixture = PlaybackTestFixture::new().await;
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

// ============================================================================
// Service command coverage
// ============================================================================
// One focused test per PlaybackCommand / LibraryEvent branch that had no
// coverage, driven through the PlaybackHandle like the rest of the suite. Where
// a command's effect is a queue-shape change, the serial command loop lets a
// following queue_projection() read reflect it (the projection query is
// processed after the mutation it follows).

/// The release id of the currently-playing context, read off the projection.
async fn current_release_id(handle: &bae_core::playback::PlaybackHandle) -> String {
    let proj = handle.queue_projection().await.expect("queue projection");
    match proj.context.expect("a playing context").source {
        bae_core::playback::ContextSource::Release(id) => id,
        bae_core::playback::ContextSource::Releases(ids) => {
            panic!("expected a single-release context, got a multi-release one: {ids:?}")
        }
        bae_core::playback::ContextSource::Library => {
            panic!("expected a release context, got the whole-library context")
        }
    }
}

/// `SetShuffle` permutes and unpermutes the context lane the queue already
/// holds — nothing is re-fetched — and the track that is playing keeps playing
/// through both toggles.
#[tokio::test]
async fn set_shuffle_permutes_and_unpermutes_the_context_lane_in_place() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    let second = fixture.track_ids[1].clone();
    let third = fixture.track_ids[2].clone();
    fixture.playback_handle.play(first.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
            Duration::from_secs(5),
        )
        .await
        .expect("the release starts playing");

    // Shuffle on re-materializes the playing context into a shuffled order —
    // not merely a persisted seed.
    fixture.playback_handle.set_shuffle(true);
    let shuffled = fixture
        .playback_handle
        .queue_projection()
        .await
        .expect("queue projection");
    assert!(
        shuffled.context.as_ref().is_some_and(|c| c.shuffled),
        "SetShuffle(true) puts the lane in shuffled order"
    );

    // Shuffle off unpermutes the lane: the not-yet-played tail is the two tracks
    // after the current one, in release order.
    fixture.playback_handle.set_shuffle(false);
    let sequential = fixture
        .playback_handle
        .queue_projection()
        .await
        .expect("queue projection");
    let ctx = sequential.context.expect("still a playing context");
    assert!(
        !ctx.shuffled,
        "SetShuffle(false) puts the lane back in sequential order"
    );
    let upcoming: Vec<String> = ctx.upcoming.iter().map(|e| e.track_id.clone()).collect();
    assert_eq!(
        upcoming,
        vec![second, third],
        "sequential upcoming is source order after the current track"
    );

    // The lane projects correctly, but that alone doesn't prove the
    // currently-playing track's audio survived the churn.
    assert_position_advances(&mut fixture.progress_rx).await;
}

/// Shuffling changes what plays next, so the preloaded next track has to follow
/// the reshuffled queue front. The preload is not an optimization the queue can
/// disagree with: `Next` and the natural advance both play the preloaded track
/// in preference to asking the queue, while advancing the cursor past the row
/// the queue calls the front. A shuffle that emits its new order without
/// reconciling the preload therefore plays the pre-shuffle track over a lane
/// whose cursor has moved somewhere else — the speakers and the queue the UI
/// renders disagree about what is playing.
#[tokio::test]
async fn shuffling_makes_the_next_track_follow_the_reshuffled_queue_front() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    let sequential_next = fixture.track_ids[1].clone();
    fixture.playback_handle.play(first.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
            Duration::from_secs(5),
        )
        .await
        .expect("the release starts playing");
    // Playing the release preloaded its sequential next track; that is the stale
    // preload a shuffle has to displace.

    // Each SetShuffle mints its own seed, so a permutation of the two-track tail
    // leaves the front alone about half the time. Reshuffle until it moves —
    // only then is a stale preload distinguishable from a reconciled one — and
    // read the new front off the projection, the same way a UI learns it.
    let mut shuffled_next = None;
    for _ in 0..32 {
        fixture.playback_handle.set_shuffle(true);
        let front = fixture
            .playback_handle
            .queue_projection()
            .await
            .expect("queue projection")
            .context
            .expect("a playing context")
            .upcoming
            .first()
            .expect("the shuffled lane still has an upcoming track")
            .track_id
            .clone();
        if front != sequential_next {
            shuffled_next = Some(front);
            break;
        }
        fixture.playback_handle.set_shuffle(false);
    }
    let shuffled_next =
        shuffled_next.expect("a shuffle that moves a different track to the lane's front");

    fixture.playback_handle.next();

    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == shuffled_next),
            Duration::from_secs(10),
        )
        .await
        .expect(
            "the track after a shuffle is the lane's new front, not the one preloaded before it",
        );
}

#[tokio::test]
async fn play_library_shuffled_plays_a_shuffled_library_context() {
    let mut fixture = PlaybackTestFixture::new().await;
    fixture.playback_handle.play_library_shuffled();
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { .. }),
            Duration::from_secs(5),
        )
        .await
        .expect("a library track starts playing");
    let proj = fixture
        .playback_handle
        .queue_projection()
        .await
        .expect("queue projection");
    let ctx = proj.context.expect("a library context");
    assert!(
        matches!(ctx.source, bae_core::playback::ContextSource::Library),
        "PlayLibraryShuffled plays from the whole library"
    );
    assert!(ctx.shuffled, "the library context is shuffled");
}

#[tokio::test]
async fn play_library_shuffled_is_a_no_op_on_an_empty_library() {
    let (library_manager, runtime_handle, _temp) = empty_test_library().await;
    let (handle, _capture_rx) = start_capture_service(library_manager, runtime_handle);
    let mut progress = handle.subscribe_progress();
    handle.play_library_shuffled();
    let played = wait_for_state_on(
        &mut progress,
        |s| matches!(s, PlaybackState::Playing { .. }),
        Duration::from_secs(1),
    )
    .await;
    assert!(
        played.is_none(),
        "an empty library has nothing to shuffle-play"
    );
    handle.stop();
}

#[tokio::test]
async fn set_repeat_mode_applies_and_reports_each_mode() {
    let mut fixture = PlaybackTestFixture::new().await;
    for expected in [RepeatMode::Context, RepeatMode::Track, RepeatMode::Off] {
        fixture.playback_handle.set_repeat_mode(expected);
        let mode = wait_for_repeat_mode(&mut fixture.progress_rx, Duration::from_secs(3)).await;
        assert_eq!(mode, expected, "SetRepeatMode applies the requested mode");
    }
}

#[tokio::test]
async fn set_muted_round_trips_and_set_volume_clears_mute() {
    let mut fixture = PlaybackTestFixture::new().await;
    fixture.playback_handle.set_volume(0.5);

    fixture.playback_handle.set_muted(true);
    assert!(
        wait_for_mute(&mut fixture.progress_rx, Duration::from_secs(3)).await,
        "SetMuted(true) mutes"
    );

    // A repeated set to the state already held emits nothing; the next
    // MuteChanged is the unmute that follows. A toggle-shaped implementation
    // would instead emit an unmute here and re-mute below, so the waited event
    // would be `true`.
    fixture.playback_handle.set_muted(true);
    fixture.playback_handle.set_muted(false);
    assert!(
        !wait_for_mute(&mut fixture.progress_rx, Duration::from_secs(3)).await,
        "the repeated SetMuted(true) was a no-op; only SetMuted(false) emitted"
    );

    // Mute again, then a non-zero SetVolume clears the mute.
    fixture.playback_handle.set_muted(true);
    assert!(
        wait_for_mute(&mut fixture.progress_rx, Duration::from_secs(3)).await,
        "muted again"
    );
    fixture.playback_handle.set_volume(0.7);
    assert!(
        !wait_for_mute(&mut fixture.progress_rx, Duration::from_secs(3)).await,
        "SetVolume above zero clears the mute"
    );
}

#[tokio::test]
async fn skip_to_entry_jumps_to_that_queue_entry() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    let third = fixture.track_ids[2].clone();
    fixture.playback_handle.play(first.clone());
    let (played, entries) = fixture
        .wait_for_playing_capturing_queue(Duration::from_secs(5))
        .await;
    assert!(played, "the release starts playing");
    let target = entries
        .iter()
        .find(|e| e.track_id == third)
        .expect("the third track is queued in the context");
    fixture.playback_handle.skip_to_entry(target.id.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == third),
            Duration::from_secs(5),
        )
        .await
        .expect("SkipTo jumps to the targeted entry");

    // Landing on the right track's state isn't proof its audio is flowing.
    assert_position_advances(&mut fixture.progress_rx).await;
}

#[tokio::test]
async fn clear_up_next_empties_the_manual_lane_keeping_the_context() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    let second = fixture.track_ids[1].clone();
    fixture.playback_handle.play(first.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
            Duration::from_secs(5),
        )
        .await
        .expect("the release starts playing");
    fixture.playback_handle.add_to_queue(vec![second]);
    let before = fixture
        .playback_handle
        .queue_projection()
        .await
        .expect("queue projection");
    assert!(
        !before.manual.is_empty(),
        "the manual lane has the added track"
    );

    fixture.playback_handle.clear_up_next();
    let after = fixture
        .playback_handle
        .queue_projection()
        .await
        .expect("queue projection");
    assert!(
        after.manual.is_empty(),
        "ClearUpNext empties the manual lane"
    );
    assert!(
        after.context.is_some(),
        "ClearUpNext leaves the playing context intact"
    );
}

/// The counterpart: clearing "Playing From" drops the section the release
/// filled, but not the track coming out of the speakers — that keeps playing,
/// and Up Next keeps its rows.
#[tokio::test]
async fn clear_playing_from_drops_the_context_while_the_track_keeps_playing() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    let queued = fixture.track_ids[2].clone();
    fixture.playback_handle.play(first.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
            Duration::from_secs(5),
        )
        .await
        .expect("the release starts playing");
    fixture.playback_handle.add_to_queue(vec![queued.clone()]);

    fixture.playback_handle.clear_playing_from();

    let after = fixture
        .playback_handle
        .queue_projection()
        .await
        .expect("queue projection");
    assert!(
        after.context.is_none(),
        "the context section leaves the snapshot entirely"
    );
    let manual: Vec<String> = after.manual.iter().map(|e| e.track_id.clone()).collect();
    assert_eq!(manual, vec![queued], "Up Next is untouched");
    assert!(
        !after.has_previous,
        "the context's history went with it, so there is nothing to step back to"
    );

    // The section is gone from the queue; the audio is not.
    assert_position_advances(&mut fixture.progress_rx).await;
}

#[tokio::test]
async fn add_release_to_queue_appends_its_tracks_to_the_manual_lane() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    fixture.playback_handle.play(first.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
            Duration::from_secs(5),
        )
        .await
        .expect("the release starts playing");
    let release_id = current_release_id(&fixture.playback_handle).await;
    fixture.playback_handle.add_release_to_queue(release_id);
    let proj = fixture
        .playback_handle
        .queue_projection()
        .await
        .expect("queue projection");
    assert_eq!(
        proj.manual.len(),
        3,
        "the release's three tracks are appended to the manual lane"
    );
}

#[tokio::test]
async fn add_release_next_puts_its_tracks_at_the_front_of_the_manual_lane() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    let second = fixture.track_ids[1].clone();
    fixture.playback_handle.play(first.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
            Duration::from_secs(5),
        )
        .await
        .expect("the release starts playing");
    // A marker already in the manual lane: AddReleaseNext must land before it.
    fixture.playback_handle.add_to_queue(vec![second.clone()]);
    let release_id = current_release_id(&fixture.playback_handle).await;
    fixture.playback_handle.add_release_next(release_id);
    let proj = fixture
        .playback_handle
        .queue_projection()
        .await
        .expect("queue projection");
    assert_eq!(proj.manual.len(), 4, "three release tracks plus the marker");
    assert_eq!(
        proj.manual.last().unwrap().track_id,
        second,
        "AddReleaseNext inserts the release before the existing marker"
    );
}

#[tokio::test]
async fn remove_currently_playing_entry_stops_playback() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    let second = fixture.track_ids[1].clone();
    fixture.playback_handle.play(first.clone());
    let (played, entries) = fixture
        .wait_for_playing_capturing_queue(Duration::from_secs(5))
        .await;
    assert!(played, "the release starts playing");

    // Skip onto the second track so it's the current context entry, then remove
    // that very entry: the handler compares the removed entry's track to the
    // playing track and stops.
    let entry = entries
        .iter()
        .find(|e| e.track_id == second)
        .expect("the second track is queued in the context")
        .clone();
    fixture.playback_handle.skip_to_entry(entry.id.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == second),
            Duration::from_secs(5),
        )
        .await
        .expect("SkipTo makes the second track current");

    fixture.playback_handle.remove_entry(entry.id.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Stopped),
            Duration::from_secs(5),
        )
        .await
        .expect("removing the currently-playing entry stops playback");
}

#[tokio::test]
async fn play_release_clamps_an_out_of_range_start_index() {
    let mut fixture = PlaybackTestFixture::new().await;
    let first = fixture.track_ids[0].clone();
    let third = fixture.track_ids[2].clone();
    // Start on the third track so a clamp-to-first is observable.
    fixture.playback_handle.play(third.clone());
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == third),
            Duration::from_secs(5),
        )
        .await
        .expect("the third track plays");
    let release_id = current_release_id(&fixture.playback_handle).await;
    fixture
        .playback_handle
        .play_release(release_id, Some(99), false);
    fixture
        .wait_for_state(
            |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
            Duration::from_secs(5),
        )
        .await
        .expect("an out-of-range start index clamps to the first track");
}

/// LibraryEvent::TracksDeleted, both branches. Two releases share one library so
/// the current track (release A) and a preloaded next (release B) can be deleted
/// independently: deleting B clears the preloaded next and purges its queued
/// track while A keeps playing; deleting A deletes the current track and stops.
/// Deleting B also races the event against B's own byte fill, which finds the
/// release's file rows gone: either order has to leave A playing.
#[tokio::test]
async fn tracks_deleted_clears_a_preloaded_next_then_stops_on_the_current() {
    let lib = restore_test_library().await;
    let first = lib.track_ids[0].clone();
    let a_release_id = lib
        .library_manager
        .get_releases_for_album(&lib.library_manager.get_albums(&[]).await.unwrap()[0].id)
        .await
        .unwrap()[0]
        .id
        .clone();
    let (b_release_id, b_tracks, _b_source) = import_second_release(&lib).await;
    let b_first = b_tracks[0].clone();

    let (handle, _capture_rx) =
        start_capture_service(lib.library_manager.clone(), lib.runtime_handle.clone());
    let mut progress = handle.subscribe_progress();

    handle.play(first.clone());
    wait_for_state_on(
        &mut progress,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
        Duration::from_secs(5),
    )
    .await
    .expect("release A starts playing");

    // Queue B's first track as the preloaded next.
    handle.add_next(vec![b_first.clone()]);
    let queued = handle.queue_projection().await.expect("queue projection");
    assert!(
        queued.manual.iter().any(|e| e.track_id == b_first),
        "B's track is queued as the next up"
    );

    // Delete B: current (A) survives, the preloaded next (B) is cleared and its
    // queued entry purged — no stop.
    lib.library_manager
        .delete_release(&b_release_id)
        .await
        .expect("delete release B");
    let still_playing =
        wait_for_track_position(&mut progress, &first, Duration::from_secs(5)).await;
    assert!(
        still_playing.is_some(),
        "deleting only the preloaded-next release leaves the current track playing"
    );
    let after = handle.queue_projection().await.expect("queue projection");
    assert!(
        !after.manual.iter().any(|e| e.track_id == b_first),
        "the deleted release's queued track is purged"
    );

    // Delete A: the current track is deleted → playback stops.
    lib.library_manager
        .delete_release(&a_release_id)
        .await
        .expect("delete release A");
    wait_for_state_on(
        &mut progress,
        |s| matches!(s, PlaybackState::Stopped),
        Duration::from_secs(5),
    )
    .await
    .expect("deleting the current track's release stops playback");

    handle.stop();
}

// ============================================================================
// Pregap behavior tests
// ============================================================================
// These exercise CD-like pregap behavior against the CUE/FLAC fixture, whose
// track 2 carries a real 2s pregap (INDEX 00 at 8s, INDEX 01 at 10s):
// - Direct selection (play / next): skip the pregap, start at INDEX 01, so the
//   adjusted position climbs from 0 the moment audio flows.
// - Natural transition (auto-advance): play the pregap from INDEX 00, so the
//   adjusted position stays pinned at 0 across the pregap, then climbs.
// The distinguishing signal is *where the position sits partway in*: a skipped
// pregap is already climbing; a played pregap is still at 0. (A no-pregap FLAC
// can't tell these apart — both start at 0 — which is why these use the CUE
// fixture, not the plain FLAC one.)

#[tokio::test]
async fn test_direct_play_skips_pregap() {
    let mut fixture = CueFlacTestFixture::with_realtime_capture()
        .await
        .expect("set up CUE/FLAC realtime capture fixture");
    let pregapped_track_id = fixture.track_ids[1].clone();

    fixture.playback_handle.play(pregapped_track_id.clone());
    wait_for_state_on(
        &mut fixture.progress_rx,
        |s| {
            matches!(s, PlaybackState::Playing { track_info, .. }
                if track_info.track_id == pregapped_track_id)
        },
        Duration::from_secs(5),
    )
    .await
    .expect("the pregapped track should start playing");

    let position_ms = position_after(&mut fixture.progress_rx, Duration::from_millis(1200)).await;
    assert!(
        position_ms > 600,
        "direct play should skip the 2s pregap and let position climb from 0; \
         got {position_ms}ms ~1.2s in (a played pregap would keep it pinned at 0)",
    );
}

#[tokio::test]
async fn test_next_button_skips_pregap() {
    let mut fixture = CueFlacTestFixture::with_realtime_capture()
        .await
        .expect("set up CUE/FLAC realtime capture fixture");
    let first_track_id = fixture.track_ids[0].clone();
    let pregapped_track_id = fixture.track_ids[1].clone();

    fixture.playback_handle.play(first_track_id.clone());
    wait_for_state_on(
        &mut fixture.progress_rx,
        |s| matches!(s, PlaybackState::Playing { .. }),
        Duration::from_secs(5),
    )
    .await
    .expect("the first track should start playing");

    // Next is a direct selection: it skips the incoming track's pregap.
    fixture.playback_handle.next();
    wait_for_state_on(
        &mut fixture.progress_rx,
        |s| {
            matches!(s, PlaybackState::Playing { track_info, .. }
                if track_info.track_id == pregapped_track_id)
        },
        Duration::from_secs(5),
    )
    .await
    .expect("Next should switch to the pregapped track");

    let position_ms = position_after(&mut fixture.progress_rx, Duration::from_millis(1200)).await;
    assert!(
        position_ms > 600,
        "Next should skip the 2s pregap and let position climb from 0; \
         got {position_ms}ms ~1.2s in (a played pregap would keep it pinned at 0)",
    );
}

#[tokio::test]
async fn test_auto_advance_plays_pregap() {
    let mut fixture = CueFlacTestFixture::with_realtime_capture()
        .await
        .expect("set up CUE/FLAC realtime capture fixture");
    let first_track_id = fixture.track_ids[0].clone();
    let pregapped_track_id = fixture.track_ids[1].clone();

    fixture.playback_handle.play(first_track_id.clone());
    wait_for_state_on(
        &mut fixture.progress_rx,
        |s| {
            matches!(s, PlaybackState::Playing { track_info, .. }
                if track_info.track_id == first_track_id)
        },
        Duration::from_secs(5),
    )
    .await
    .expect("the first track should start playing");

    // Track 1 runs 0–8s; seek near its end so it completes and crosses into
    // track 2's pregap within a second or so.
    fixture.playback_handle.seek(Duration::from_secs(7));
    wait_for_state_on(
        &mut fixture.progress_rx,
        |s| {
            matches!(s, PlaybackState::Playing { track_info, .. }
                if track_info.track_id == pregapped_track_id)
        },
        Duration::from_secs(10),
    )
    .await
    .expect("playback should auto-advance into the pregapped track");

    // ~1s into track 2 the 2s pregap is still playing: adjusted position pinned at 0.
    let during_pregap = position_after(&mut fixture.progress_rx, Duration::from_millis(1000)).await;
    assert!(
        during_pregap < 600,
        "auto-advance should play the pregap: position stays pinned at 0 across it, \
         got {during_pregap}ms ~1s in (a skipped pregap would already be climbing)",
    );

    // Past the 2s pregap, INDEX 01 content plays and position climbs.
    let after_pregap = position_after(&mut fixture.progress_rx, Duration::from_millis(2500)).await;
    assert!(
        after_pregap > 600,
        "once the pregap passes, position should climb into the track; got {after_pregap}ms",
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
    let mut fixture = CueFlacTestFixture::with_realtime_capture()
        .await
        .expect("set up CUE/FLAC realtime capture fixture");

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
    let reference =
        decode_audio(buffer_from(&reference_data), None, None).expect("decode reference");
    let channels = reference.channels as usize;
    let sample_rate = reference.sample_rate;
    let reference_f32 = samples_as_f32(&reference);

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

    let mut fixture = CueFlacTestFixture::with_capture()
        .await
        .expect("set up CUE/FLAC capture fixture");

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
    let reference =
        decode_audio(buffer_from(&reference_data), None, None).expect("decode reference");
    let channels = reference.channels as usize;
    let sample_rate = reference.sample_rate;
    let reference_f32 = samples_as_f32(&reference);

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
        let (config_handle, key_service) = test_config_and_keys(&library_dir);
        let library_manager = LibraryManager::new(
            (*database_arc).clone(),
            config_handle,
            key_service,
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
                identity_choice: IdentityChoice::Exact {
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
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::new(
        (*database_arc).clone(),
        config_handle,
        key_service,
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
            identity_choice: IdentityChoice::Exact {
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
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::new(
        (*database_arc).clone(),
        config_handle,
        key_service,
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
            identity_choice: IdentityChoice::Exact {
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

async fn restore_test_library() -> RestoreTestLibrary {
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
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::new(
        (*database_arc).clone(),
        config_handle,
        key_service,
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
            identity_choice: IdentityChoice::Exact {
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
    let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
    assert!(!track_ids.is_empty(), "the test album imported some tracks");
    RestoreTestLibrary {
        library_manager,
        runtime_handle,
        track_ids,
        _temp_dir: temp_dir,
    }
}

/// A library with no imports, for the empty-library no-op paths. Returns the
/// manager, runtime handle, and temp dir (kept alive for the DB file).
async fn empty_test_library() -> (LibraryManager, tokio::runtime::Handle, TempDir) {
    tracing_init();
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let library_dir = StoreDir::new(temp_dir.path().to_path_buf());
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::new(
        database,
        config_handle,
        key_service,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        bae_core::import::cover_art::RemoteImageCache::for_test(),
    );
    (library_manager, tokio::runtime::Handle::current(), temp_dir)
}

/// Import a second, distinct-content release (the CUE/FLAC album) into `lib`'s
/// library so a test can have a current track in one release and a
/// preloaded/queued track in another — needed to delete one without the other.
/// Returns the new release's id and track ids; the source `TempDir` is returned
/// so the caller keeps it alive.
async fn import_second_release(lib: &RestoreTestLibrary) -> (String, Vec<String>, TempDir) {
    let source = TempDir::new().unwrap();
    generate_cue_flac_files(source.path());
    let release_key = seed_discogs_test_release(create_cue_flac_test_album());
    let import_handle =
        start_test_import(lib.runtime_handle.clone(), lib.library_manager.clone()).await;
    let import_id = uuid::Uuid::new_v4().to_string();
    import_handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "second".to_string(),
            folder: source.path().to_path_buf(),
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Local,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
        .unwrap();
    let mut progress_rx = import_handle.subscribe_import(import_id);
    let (release_id, _album_id) = wait_for_import_complete(&mut progress_rx).await;
    let tracks = lib
        .library_manager
        .get_tracks_for_release(&release_id)
        .await
        .unwrap()
        .iter()
        .map(|t| t.id.clone())
        .collect();
    (release_id, tracks, source)
}

/// A resume cache whose context release is gone (its `get_track_ids` is empty)
/// restores the manual lane only — the context drops, the surviving manual
/// tracks and current track stay. The restored queue re-persists with no
/// context, which is what we read back.
#[tokio::test]
async fn test_restore_drops_deleted_context_keeps_manual() {
    let lib = restore_test_library().await;
    let track_id = lib.track_ids[0].clone();

    // The context points at a release id that no longer exists, so its
    // `get_track_ids` returns empty (the deleted-release signal). The manual lane
    // and current track are a real, surviving track.
    let state = bae_core::db::DbPlaybackState {
        context: Some(bae_core::db::DbPlaybackContext {
            source: RELEASE_THAT_WAS_DELETED.to_string(),
            shuffled: false,
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

    let (handle, _capture_rx) =
        start_capture_service(lib.library_manager.clone(), lib.runtime_handle);
    let mut progress_rx = handle.subscribe_progress();

    // Restore committed once the surviving manual track surfaces as the
    // restored (Paused) current track.
    wait_for_state_on(
        &mut progress_rx,
        |s| matches!(s, PlaybackState::Paused { track_info, .. } if track_info.track_id == track_id),
        Duration::from_secs(20),
    )
    .await
    .expect("the surviving manual track should restore as current");

    // Force the restored queue to re-persist, then read it back: the dropped
    // context is gone and the manual track survived.
    handle.set_repeat_mode(bae_core::playback::RepeatMode::Track);
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut row = None;
    while Instant::now() < deadline {
        if let bae_core::db::LoadedPlaybackState::Present(loaded) =
            lib.library_manager.load_playback_state().await.unwrap()
        {
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
/// panics. No playback state surfaces because no current track restored.
#[tokio::test]
async fn test_restore_corrupt_row_starts_fresh() {
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

    let (handle, _capture_rx) = start_capture_service(lib.library_manager, lib.runtime_handle);
    let mut progress_rx = handle.subscribe_progress();

    // A discarded resume cache restores nothing: no playback state may surface.
    let restored = wait_for_state_on(&mut progress_rx, |_| true, Duration::from_millis(500)).await;
    assert!(
        restored.is_none(),
        "a discarded resume cache leaves nothing playing (fresh start), got {restored:?}"
    );
    let queue = handle.queue_projection().await.expect("queue projection");
    assert!(
        queue.manual.is_empty() && queue.context.is_none(),
        "a discarded resume cache starts with an empty queue"
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
    let mut fixture = PlaybackTestFixture::new().await;

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
    cloud: Arc<coven::InMemoryCloudHome>,
    track_ids: Vec<String>,
    _capture_stream_rx: CaptureStreamRx,
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
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await?;
        let library_dir = StoreDir::new(temp_dir.path().to_path_buf());
        let (config_handle, key_service) = test_config_and_keys(&library_dir);
        let library_manager = LibraryManager::new(
            database,
            config_handle,
            key_service,
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
            bae_core::diagnostics::Diagnostics::noop(),
            tokio::runtime::Handle::current(),
            bae_core::import::cover_art::RemoteImageCache::for_test(),
        );
        let master_key = [11u8; 32];
        let cloud = Arc::new(coven::InMemoryCloudHome::new());
        library_manager
            .connect_test_cloud_home(
                cloud.clone(),
                bae_core::sync::CloudCipher::Encrypted(coven::EncryptionService::from_key(
                    master_key,
                )),
            )
            .await?;

        let runtime_handle = tokio::runtime::Handle::current();
        let discogs_release = create_test_album();
        let release_id_key = seed_discogs_test_release(discogs_release);
        generate_test_flac_files(&album_dir);

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
                storage_mode: StorageMode::Remote,
                pin: false,
                identity_choice: IdentityChoice::Exact {
                    release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
                },
                user_edit: None,
            })
            .await
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
        let mut import_rx = import_handle.subscribe_import(import_id);
        let (release_id, _album_id) = wait_for_import_complete(&mut import_rx).await;

        // Run the upload so the encrypted blobs land in the cloud and the outbox
        // clears — after this the track resolves cloud-only (no local copy, no
        // pending upload).
        while matches!(
            library_manager.drain_uploads_for_test().await?,
            coven::DrainOutcome::Drained { uploaded, .. } if uploaded > 0
        ) {}

        // Delete the import originals so file resolution can't fall back to them.
        std::fs::remove_dir_all(&album_dir)?;

        let tracks = library_manager.get_tracks_for_release(&release_id).await?;
        let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
        assert!(!track_ids.is_empty(), "Should have imported tracks");

        let (playback_handle, capture_stream_rx) =
            start_capture_service(library_manager, runtime_handle);
        let progress_rx = playback_handle.subscribe_progress();
        Ok(Self {
            playback_handle,
            progress_rx,
            cloud,
            track_ids,
            _capture_stream_rx: capture_stream_rx,
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
    let mut fixture = CloudOnlyPlaybackFixture::new()
        .await
        .expect("set up cloud-only playback fixture");
    assert!(
        !fixture.track_ids.is_empty(),
        "fixture must import at least one playable track"
    );

    // Every release-file object leaves the cloud, so the background reader's
    // fetch 404s and it cancels the buffer and emits PlaybackError. Arming the
    // legacy range-read failure no longer covers this: coven reads an exact
    // object by its locator, on a path that hook does not sit on.
    let objects: Vec<String> = fixture
        .cloud
        .keys()
        .into_iter()
        .filter(|key| key.starts_with("release_files/"))
        .collect();
    assert!(
        !objects.is_empty(),
        "the fixture uploaded its release files"
    );
    for key in objects {
        fixture.cloud.remove(&key);
    }

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

/// One ~30 MiB noise FLAC shared by every multi-window test. Encoded once —
/// per-test encoding would dominate the suite's wall time. White noise
/// compresses poorly, so 3 minutes of it stays near raw PCM size.
static MULTI_WINDOW_FLAC: std::sync::LazyLock<Vec<u8>> = std::sync::LazyLock::new(|| {
    use rand::{Rng, SeedableRng};

    bae_core::audio_codec::init();

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
    let samples: Vec<i32> = (0..frames * 2)
        .map(|i| {
            if pregap_samples.contains(&i) {
                0
            } else {
                (rng.random::<i16>() as i32) << 16
            }
        })
        .collect();
    let flac = bae_core::audio_codec::encode_i32(
        bae_core::audio_codec::EncodeFormat::Flac {
            bits_per_sample: 16,
        },
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
fn generate_multi_window_cue_flac_files(dir: &std::path::Path) {
    std::fs::write(
        dir.join("Multi Window Album.flac"),
        MULTI_WINDOW_FLAC.as_slice(),
    )
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

fn create_multi_window_cue_album() -> DiscogsRelease {
    let track = |position: &str, title: &str| DiscogsTrack {
        type_: "track".to_string(),
        position: position.to_string(),
        title: title.to_string(),
        duration: Some("1:00".to_string()),
        artists: vec![],
        extraartists: None,
    };
    DiscogsRelease {
        id: "multi-window-cue-release".to_string(),
        title: "Multi Window Album".to_string(),
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
            track("1", "Multi Window One"),
            track("2", "Multi Window Two"),
            track("3", "Multi Window Three"),
        ],
        master_id: Some("multi-window-cue-master".to_string()),
    }
}

/// The multi-window CUE album, imported once (decode-verify + loudness over
/// ~30 MiB) into a template library directory that outlives every test. Each
/// `MultiWindowPlayback` clones the template's small DB/library files into
/// its own `TempDir` instead of re-importing, so porting many behaviors onto
/// this fixture doesn't multiply the import cost by the number of tests.
///
/// The audio file itself is never cloned: `local_blob_refs` stores an
/// absolute path, so a clone's DB rows point straight back at the template's
/// stable `album/` directory, which is held alive for the process's lifetime.
struct MultiWindowTemplate {
    dir: TempDir,
    track_ids: Vec<String>,
}

static MULTI_WINDOW_TEMPLATE: std::sync::LazyLock<MultiWindowTemplate> =
    std::sync::LazyLock::new(|| {
        // A dedicated runtime, not the calling test's: the import must finish
        // and every task/connection it spawned must be torn down (dropping
        // the runtime blocks until they are) before the template directory
        // is safe to copy — coven opens SQLite in WAL mode, so a copy taken
        // while a connection is still live could catch an unmerged -wal file.
        let rt = tokio::runtime::Runtime::new().expect("build the template import's runtime");
        let template = rt.block_on(async {
            let import_ids = SequentialIdProvider::new("multi-window-template");
            let setup = imported_release_setup(
                create_multi_window_cue_album(),
                "multi-window-template",
                import_ids.new_id(),
                generate_multi_window_cue_flac_files,
                |_| Ok(()),
            )
            .await
            .expect("import the multi-window template release");
            assert_eq!(setup.track_ids.len(), 3, "the CUE album imports 3 tracks");
            MultiWindowTemplate {
                dir: setup.temp_dir,
                track_ids: setup.track_ids,
            }
        });
        drop(rt);
        template
    });

/// Recursively copy `src` into `dst` (which must not yet exist).
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) {
    std::fs::create_dir_all(dst).expect("create cloned directory");
    for entry in std::fs::read_dir(src).expect("read directory to clone") {
        let entry = entry.expect("directory entry to clone");
        let dest = dst.join(entry.file_name());
        let file_type = entry.file_type().expect("entry file type");
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), &dest).expect("clone file");
        }
    }
}

/// SQLite's WAL-mode sidecar files: present only while a connection has
/// uncommitted WAL data, and reclaimed by SQLite itself around a connection's
/// close. A file matching one of these suffixes disappearing between
/// `read_dir` and `copy` is that reclaim, not a real race on the template's
/// stable content — its absence at clone time means "fully checkpointed,"
/// exactly the state a fresh open of the (always-present) main `.db` file
/// needs.
fn is_sqlite_wal_sidecar(name: &std::ffi::OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    name.ends_with("-wal") || name.ends_with("-shm") || name.ends_with("-journal")
}

/// Clone a template library's DB/library files into a fresh `TempDir`, skipping
/// the `album/` directory — the imported audio stays at the template's own
/// stable path, which the cloned DB's `local_blob_refs` rows (absolute paths)
/// already point at — and skipping SQLite's WAL sidecars, reclaimed around a
/// connection's close. Blocking (file I/O only); callers run it on a blocking
/// thread. Shared by every import-once template fixture so the WAL-race
/// handling lives in one place.
fn clone_template_library(template_dir: &std::path::Path) -> TempDir {
    let fresh = TempDir::new().expect("fresh cloned library dir");
    for entry in std::fs::read_dir(template_dir).expect("read template dir") {
        let entry = entry.expect("template dir entry");
        if entry.file_name() == "album" {
            continue;
        }
        let dest = fresh.path().join(entry.file_name());
        let file_type = entry.file_type().expect("template entry file type");
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dest);
        } else if let Err(e) = std::fs::copy(entry.path(), &dest) {
            if e.kind() == std::io::ErrorKind::NotFound && is_sqlite_wal_sidecar(&entry.file_name())
            {
                debug!(
                    "template clone: {:?} was reclaimed before copy (fully checkpointed); skipping",
                    entry.file_name()
                );
            } else {
                panic!("clone template file {:?}: {e}", entry.path());
            }
        }
    }
    fresh
}

fn clone_multi_window_library() -> (TempDir, Vec<String>) {
    let template = &*MULTI_WINDOW_TEMPLATE;
    let fresh = clone_template_library(template.dir.path());
    (fresh, template.track_ids.clone())
}

/// An imported multi-window CUE album with a real-time-paced playback service
/// over it, for tests that drive switches, advances, and boundaries against a
/// sparsely buffered file.
struct MultiWindowPlayback {
    playback_handle: bae_core::playback::PlaybackHandle,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_ids: Vec<String>,
    library_manager: LibraryManager,
    runtime_handle: tokio::runtime::Handle,
    _capture_stream_rx: CaptureStreamRx,
    _temp_dir: TempDir,
}

impl MultiWindowPlayback {
    /// Build against a fresh clone of the shared multi-window template. `name`
    /// only labels the clone in test failure output — the import itself runs
    /// once per process, in `MULTI_WINDOW_TEMPLATE`.
    async fn new(name: &str) -> Self {
        let (temp_dir, track_ids) = tokio::task::spawn_blocking(clone_multi_window_library)
            .await
            .expect("clone the multi-window template library");
        tracing::debug!("multi-window clone ready for {name}");

        let db_path = temp_dir.path().join("test.db");
        let database = Database::new_test(
            db_path.to_str().expect("db path is valid UTF-8"),
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await
        .expect("open the cloned multi-window database");
        let library_dir = StoreDir::new(temp_dir.path().to_path_buf());
        let (config_handle, key_service) = test_config_and_keys(&library_dir);
        let runtime_handle = tokio::runtime::Handle::current();
        let library_manager = LibraryManager::new(
            database,
            config_handle,
            key_service,
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
            bae_core::diagnostics::Diagnostics::noop(),
            runtime_handle.clone(),
            bae_core::import::cover_art::RemoteImageCache::for_test(),
        );

        let (playback_handle, capture_stream_rx) =
            start_capture_service(library_manager.clone(), runtime_handle.clone());
        let progress_rx = playback_handle.subscribe_progress();
        Self {
            playback_handle,
            progress_rx,
            track_ids,
            library_manager,
            runtime_handle,
            _capture_stream_rx: capture_stream_rx,
            _temp_dir: temp_dir,
        }
    }

    /// Play `track_id` and wait for its Playing state.
    async fn play_and_wait(&mut self, track_id: &str) {
        play_and_wait_on(&self.playback_handle, &mut self.progress_rx, track_id).await;
    }
}

/// Switching tracks within a single multi-window file must keep the shared
/// file buffer streaming. Playing the last track buffers the file head and a
/// window at that track's offset; the middle track's bytes are in neither, so
/// reaching Playing after the switch requires the buffer's fill task to still
/// be fetching on demand. A fill task killed by the switch leaves the decoder
/// waiting forever on bytes that never arrive: no Playing, position pinned at
/// the start.
#[tokio::test(flavor = "multi_thread")]
async fn switching_tracks_within_a_multi_window_file_keeps_streaming() {
    let mut playback = MultiWindowPlayback::new("multi-window-switch").await;

    let last_track = playback.track_ids[2].clone();
    playback.play_and_wait(&last_track).await;

    let middle_track = playback.track_ids[1].clone();
    playback.playback_handle.play(middle_track.clone());
    let playing = wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == middle_track),
        Duration::from_secs(20),
    )
    .await;
    assert!(
        playing.is_some(),
        "the middle track reaches Playing after the switch — its bytes must be fetched on demand"
    );
}

/// Natural track ends within a multi-window file cross gaplessly, twice in a
/// row. Each crossing proves the preload actually decoded the next track's
/// bytes — regions no earlier fetch buffered, so the shared buffer's fill task
/// must serve the preload decoder's demand while the current track plays. The
/// seeks that pull each boundary closer land in unbuffered regions too, so
/// mid-track sparse seeking is exercised on the way.
#[tokio::test(flavor = "multi_thread")]
async fn auto_advance_crosses_gaplessly_within_a_multi_window_file() {
    let mut playback = MultiWindowPlayback::new("multi-window-gapless").await;
    let [first, second, third] = [
        playback.track_ids[0].clone(),
        playback.track_ids[1].clone(),
        playback.track_ids[2].clone(),
    ];

    playback.play_and_wait(&first).await;

    // Track 1's raw timeline is 58 s (it ends at track 2's INDEX 00); seek near
    // the end so the boundary arrives in a few real-time seconds.
    playback.playback_handle.seek(Duration::from_secs(54));
    let outcome = observe_boundary(
        &mut playback.progress_rx,
        &first,
        &second,
        Duration::from_secs(25),
    )
    .await;
    assert!(
        outcome.reached_incoming,
        "playback crosses into the second track (through its pregap)"
    );
    assert!(
        outcome.decode_stats_for_finishing && !outcome.completed_for_finishing,
        "the first boundary is gapless: decode stats without TrackCompleted"
    );

    // The crossing events fire at the boundary whether or not audio flows, so
    // require the position to actually advance into track 2 before seeking --
    // a preload decoder starved at the pregap boundary pins it in place.
    wait_for_position_advance(&mut playback.progress_rx)
        .await
        .expect(
            "track 2's position must advance after the crossing -- its decoder must keep \
             producing samples past the pregap boundary",
        );

    // Track 2's raw timeline is 62 s (2 s pregap + 60 s).
    playback.playback_handle.seek(Duration::from_secs(58));
    let outcome = observe_boundary(
        &mut playback.progress_rx,
        &second,
        &third,
        Duration::from_secs(25),
    )
    .await;
    assert!(
        outcome.reached_incoming,
        "playback crosses into the third track"
    );
    assert!(
        outcome.decode_stats_for_finishing && !outcome.completed_for_finishing,
        "the second boundary is gapless: the crossing handler re-preloaded track 3"
    );

    let position = position_after(&mut playback.progress_rx, Duration::from_secs(1)).await;
    assert!(
        position > 0,
        "the third track's position advances — samples are flowing after two crossings"
    );
}

/// A manual Next into a track WITH a pregap can't use the preloaded stream
/// (it was decoded from INDEX 00; a manual skip starts at INDEX 01), so it
/// discards the preload and rebuilds through play_track. The rebuild re-reads
/// the same file — its shared buffer and fill task must survive the discard
/// for the skip to ever produce audio.
#[tokio::test(flavor = "multi_thread")]
async fn manual_next_into_a_pregap_track_rebuilds_and_keeps_streaming() {
    let mut playback = MultiWindowPlayback::new("multi-window-next-pregap").await;
    let [first, second] = [playback.track_ids[0].clone(), playback.track_ids[1].clone()];

    playback.play_and_wait(&first).await;

    playback.playback_handle.next();
    let states = collect_states_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == second),
        Duration::from_secs(20),
    )
    .await;
    assert!(
        matches!(&states.last(), Some(PlaybackState::Playing { track_info, .. }) if track_info.track_id == second),
        "the second track reaches Playing after a manual Next, got {states:?}"
    );
    assert!(
        states
            .iter()
            .any(|s| matches!(s, PlaybackState::Loading { track_id, .. } if *track_id == second)),
        "a pregap skip rebuilds through play_track, which surfaces a Loading arc"
    );

    let position = position_after(&mut playback.progress_rx, Duration::from_secs(1)).await;
    assert!(
        position > 0,
        "the rebuilt track's position advances — its bytes are fetched on demand"
    );
}

/// A manual Next into a track WITHOUT a pregap promotes the preloaded stream:
/// no rebuild, so no Loading arc — Playing lands directly. The promoted
/// decoder then streams bytes from an unbuffered region of the shared file,
/// which only works while the buffer's fill task is alive.
#[tokio::test(flavor = "multi_thread")]
async fn manual_next_into_a_clean_track_promotes_the_preload() {
    let mut playback = MultiWindowPlayback::new("multi-window-next-preload").await;
    let [second, third] = [playback.track_ids[1].clone(), playback.track_ids[2].clone()];

    playback.play_and_wait(&second).await;

    playback.playback_handle.next();
    let states = collect_states_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == third),
        Duration::from_secs(20),
    )
    .await;
    assert!(
        matches!(&states.last(), Some(PlaybackState::Playing { track_info, .. }) if track_info.track_id == third),
        "the third track reaches Playing after a manual Next, got {states:?}"
    );
    assert!(
        !states
            .iter()
            .any(|s| matches!(s, PlaybackState::Loading { .. })),
        "a pregap-free Next promotes the preloaded stream — no Loading arc, got {states:?}"
    );

    let position = position_after(&mut playback.progress_rx, Duration::from_secs(1)).await;
    assert!(
        position > 0,
        "the promoted track's position advances — its bytes are fetched on demand"
    );
}

/// Every seek test elsewhere in this suite runs on a single-window fixture
/// that is fully buffered the moment it opens — seeking has never been
/// exercised against a sparse, multi-range buffer. Playing the last track
/// buffers only the file head and this track's own region (the other
/// tracks' regions, including everywhere earlier in the track's own raw
/// timeline that a backward seek can reach, are untouched); seeking backward
/// and then forward within it runs the real seek machinery — decoder
/// rebuild, byte-seek, demand publication, window fetch — over holes in the
/// buffer, not a fully-populated one.
#[tokio::test(flavor = "multi_thread")]
async fn seek_within_the_last_track_lands_and_keeps_streaming_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-seek-sparse").await;
    let last_track = playback.track_ids[2].clone();
    playback.play_and_wait(&last_track).await;

    // Seek backward toward the start of the track's own raw timeline — into a
    // region the initial fetch (which lands at the track's start byte) may
    // already partly cover, but well behind wherever playback has since moved.
    let backward_target = Duration::from_secs(5);
    playback.playback_handle.seek(backward_target);
    let landed = wait_for_seeked_on(&mut playback.progress_rx, Duration::from_secs(20))
        .await
        .expect("a backward seek within the last track emits Seeked");
    assert!(
        Duration::from_millis(landed).abs_diff(backward_target) < Duration::from_secs(2),
        "backward seek should land near {backward_target:?}, got {landed}ms"
    );
    wait_for_position_advance(&mut playback.progress_rx)
        .await
        .expect("audio must keep flowing after the backward seek");

    // Seek forward, well past the backward target and into territory the fill
    // has not fetched — the fill's demand-driven window fetch must catch up.
    let forward_target = Duration::from_secs(40);
    playback.playback_handle.seek(forward_target);
    let landed = wait_for_seeked_on(&mut playback.progress_rx, Duration::from_secs(20))
        .await
        .expect("a forward seek within the last track emits Seeked");
    assert!(
        Duration::from_millis(landed).abs_diff(forward_target) < Duration::from_secs(2),
        "forward seek should land near {forward_target:?}, got {landed}ms"
    );
    wait_for_position_advance(&mut playback.progress_rx)
        .await
        .expect("audio must keep flowing after the forward seek");
}

/// A seek issued the instant Playing arrives — before the fill has had any
/// settle time to buffer ahead of the startup window — must still land and
/// produce audio rather than deadlock waiting on bytes the sparse buffer
/// hasn't fetched yet.
#[tokio::test(flavor = "multi_thread")]
async fn seek_immediately_after_playing_lands_and_plays_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-seek-immediate").await;
    let last_track = playback.track_ids[2].clone();

    playback.playback_handle.play(last_track.clone());
    let playing = wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == last_track),
        Duration::from_secs(20),
    )
    .await;
    assert!(playing.is_some(), "the last track reaches Playing");

    // Seek immediately — no settle time for the fill to get ahead.
    let target = Duration::from_secs(20);
    playback.playback_handle.seek(target);
    let landed = wait_for_seeked_on(&mut playback.progress_rx, Duration::from_secs(20))
        .await
        .expect("a seek issued right after Playing still emits Seeked");
    assert!(
        Duration::from_millis(landed).abs_diff(target) < Duration::from_secs(2),
        "should land near {target:?}, got {landed}ms"
    );
    wait_for_position_advance(&mut playback.progress_rx)
        .await
        .expect("audio must play after an immediate seek");
}

/// A seek into an unbuffered region surfaces a buffering state before it
/// confirms: the seek target shows as `Loading { resolved: Some(..) }` (the
/// metadata is already known — this is a seek, not a fresh play), and `Seeked`
/// follows only once the demanded window lands and the ready-watcher fires. A
/// fully-buffered file already has the window, so this arc only appears over a
/// sparse buffer.
#[tokio::test(flavor = "multi_thread")]
async fn seek_into_an_unbuffered_region_emits_resolved_loading_before_seeked() {
    let mut playback = MultiWindowPlayback::new("multi-window-seek-loading").await;
    let last_track = playback.track_ids[2].clone();
    playback.play_and_wait(&last_track).await;

    // Seek forward into territory the demand-driven fill has not fetched yet.
    playback.playback_handle.seek(Duration::from_secs(40));

    let deadline = Instant::now() + Duration::from_secs(20);
    let mut saw_loading = false;
    let mut saw_seeked = false;
    while Instant::now() < deadline && !saw_seeked {
        match timeout(Duration::from_millis(200), playback.progress_rx.recv()).await {
            Ok(Some(PlaybackProgress::StateChanged {
                state:
                    PlaybackState::Loading {
                        track_id,
                        resolved: Some(_),
                    },
            })) if track_id == last_track => {
                saw_loading = true;
            }
            Ok(Some(PlaybackProgress::Seeked { track_id, .. })) if track_id == last_track => {
                assert!(
                    saw_loading,
                    "a resolved Loading must arrive before Seeked on a seek into an unbuffered region"
                );
                saw_seeked = true;
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }
    assert!(saw_loading, "the seek must emit a resolved Loading state");
    assert!(
        saw_seeked,
        "the seek must emit Seeked once the window lands"
    );
}

// ============================================================================
// Port matrix: behaviors that involve more than decode, run against the real
// sparse multi-window fixture instead of only the fully-buffered single-window
// one. Each ports an existing single-window test's assertion; see that test's
// name in this section's doc comments for the source. The single-window
// originals stay — they are the fast decode-correctness/XLD-alignment
// coverage — this only adds the streaming-relevant coverage they never had.
// ============================================================================

/// Multi-window port of `seek_past_end_of_track_signals_rather_than_hanging`:
/// a seek well past the last track's end must still SIGNAL (`Seeked` or
/// `PlaybackError`), not freeze — over the sparse buffer, the target is past
/// everything the fill has fetched, so the end-of-stream resolution has to
/// happen without the byte-fetch machinery ever landing more data.
#[tokio::test(flavor = "multi_thread")]
async fn seek_past_end_of_track_signals_rather_than_hanging_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-seek-past-end").await;
    let last_track = playback.track_ids[2].clone();
    playback.play_and_wait(&last_track).await;

    // Track 3's raw timeline is 60s; 600s is far past the end.
    playback.playback_handle.seek(Duration::from_secs(600));

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut signaled = false;
    while Instant::now() < deadline {
        match timeout(Duration::from_millis(200), playback.progress_rx.recv()).await {
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
        "a seek past the end must signal (Seeked or PlaybackError), not freeze silently, \
         even when the target is well past everything the sparse buffer has fetched"
    );
}

/// Multi-window port of `seek_by_ratio_maps_to_a_proportional_position`:
/// `SeekByRatio`'s position math runs down through the same `seek()` this
/// suite already drives directly, but over the sparse buffer instead of a
/// fully-buffered file.
#[tokio::test(flavor = "multi_thread")]
async fn seek_by_ratio_maps_to_a_proportional_position_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-seek-ratio").await;
    let last_track = playback.track_ids[2].clone();
    playback.play_and_wait(&last_track).await;

    playback.playback_handle.seek_by_ratio(0.25);
    let quarter = wait_for_seeked_on(&mut playback.progress_rx, Duration::from_secs(20))
        .await
        .expect("Seeked for ratio 0.25");
    playback.playback_handle.seek_by_ratio(0.75);
    let three_quarter = wait_for_seeked_on(&mut playback.progress_rx, Duration::from_secs(20))
        .await
        .expect("Seeked for ratio 0.75");

    assert!(
        quarter > 1_000,
        "ratio 0.25 should land well past the start, got {quarter}ms"
    );
    assert!(
        three_quarter > quarter + 1_000,
        "ratio 0.75 should land clearly later than 0.25 (quarter={quarter}ms three_quarter={three_quarter}ms)"
    );
}

/// Multi-window port of `test_pause_seek_resume_advances_position`: seeking
/// while paused must not auto-play, and resuming afterward must produce
/// audio — over the sparse buffer, the resumed decoder has to fetch the seek
/// target's window on demand rather than reading it out of an
/// already-fully-buffered file.
#[tokio::test(flavor = "multi_thread")]
async fn pause_seek_resume_advances_position_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-pause-seek-resume").await;
    let last_track = playback.track_ids[2].clone();
    playback.play_and_wait(&last_track).await;

    playback.playback_handle.pause();
    wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Paused { .. }),
        Duration::from_secs(5),
    )
    .await
    .expect("playback should pause");

    let seek_target = Duration::from_secs(20);
    playback.playback_handle.seek(seek_target);
    let landed = wait_for_seeked_on(&mut playback.progress_rx, Duration::from_secs(20))
        .await
        .expect("Seeked after seeking while paused");
    assert!(
        Duration::from_millis(landed).abs_diff(seek_target) < Duration::from_secs(2),
        "should land near {seek_target:?}, got {landed}ms"
    );

    let auto_played = wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { .. }),
        Duration::from_millis(500),
    )
    .await;
    assert!(
        auto_played.is_none(),
        "should still be paused after seek, not auto-playing"
    );

    playback.playback_handle.resume();
    wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { .. }),
        Duration::from_secs(5),
    )
    .await
    .expect("resume should start playing");

    wait_for_position_advance(&mut playback.progress_rx)
        .await
        .expect("audio must advance after resuming a paused sparse-buffer seek");
}

/// Multi-window port of `test_direct_play_skips_pregap`: a direct selection
/// of the pregapped track (track 2) must skip its 2s pregap and let position
/// climb from the very start — over the sparse buffer, this is the decoder's
/// first read landing on a window the fill has to fetch fresh, not one
/// already sitting in a fully-buffered file.
#[tokio::test(flavor = "multi_thread")]
async fn direct_play_skips_pregap_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-direct-skip-pregap").await;
    let pregapped_track = playback.track_ids[1].clone();
    playback.play_and_wait(&pregapped_track).await;

    let position_ms = position_after(&mut playback.progress_rx, Duration::from_millis(1200)).await;
    assert!(
        position_ms > 600,
        "direct play should skip the 2s pregap and let position climb from 0; \
         got {position_ms}ms ~1.2s in (a played pregap would keep it pinned at 0)",
    );
}

/// Multi-window port of `test_auto_advance_plays_pregap`'s "position pinned
/// during the pregap" assertion — `auto_advance_crosses_gaplessly_within_a_multi_window_file`
/// already proves the crossing happens and that audio keeps flowing
/// afterward, but never checks that the pregap itself is actually played
/// (position pinned near 0) rather than silently skipped.
#[tokio::test(flavor = "multi_thread")]
async fn auto_advance_plays_pregap_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-auto-advance-pregap").await;
    let first = playback.track_ids[0].clone();
    let second = playback.track_ids[1].clone();
    playback.play_and_wait(&first).await;

    // Track 1 runs 0–58s; seek near the end so the crossing arrives soon.
    playback.playback_handle.seek(Duration::from_secs(56));
    wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == second),
        Duration::from_secs(25),
    )
    .await
    .expect("playback should auto-advance into the pregapped track");

    // ~1s into track 2 the 2s pregap is still playing: position pinned at 0.
    let during_pregap =
        position_after(&mut playback.progress_rx, Duration::from_millis(1000)).await;
    assert!(
        during_pregap < 600,
        "auto-advance should play the pregap: position stays pinned at 0 across it, \
         got {during_pregap}ms ~1s in (a skipped pregap would already be climbing)",
    );
}

/// Multi-window analog of `assert_skip_preserves_play_state`: Next/Previous
/// must land on the adjacent track in the same play/pause state — exercised
/// here against the sparse buffer's promotion/rebuild paths (the single-window
/// fixture's file is fully buffered before either test ever seeks).
async fn assert_skip_preserves_play_state_over_sparse_buffer(
    direction: SkipDirection,
    start_paused: bool,
) {
    let mut playback = MultiWindowPlayback::new("multi-window-skip-state").await;
    let first = playback.track_ids[0].clone();
    let second = playback.track_ids[1].clone();

    let (start_track_id, target_track_id) = match direction {
        SkipDirection::Next => (first, second),
        SkipDirection::Previous => (second, first),
    };

    playback.play_and_wait(&start_track_id).await;

    if start_paused {
        playback.playback_handle.pause();
        wait_for_state_on(
            &mut playback.progress_rx,
            |s| matches!(s, PlaybackState::Paused { .. }),
            Duration::from_secs(5),
        )
        .await
        .expect("playback should pause");
    }

    // Previous needs to land inside its 3s "step back" window; Next has no
    // such window. Pressed immediately after play_and_wait/pause either way.
    match direction {
        SkipDirection::Next => playback.playback_handle.next(),
        SkipDirection::Previous => playback.playback_handle.previous(),
    }

    let landed = wait_for_state_on(
        &mut playback.progress_rx,
        |s| {
            let (track_info, is_paused) = match s {
                PlaybackState::Playing { track_info, .. } => (track_info, false),
                PlaybackState::Paused { track_info, .. } => (track_info, true),
                _ => return false,
            };
            track_info.track_id == target_track_id && is_paused == start_paused
        },
        Duration::from_secs(25),
    )
    .await;
    assert!(
        landed.is_some(),
        "{} while {} should land on the adjacent track in the same play/pause state",
        direction.label(),
        if start_paused { "paused" } else { "playing" },
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn next_while_paused_stays_paused_over_sparse_buffer() {
    assert_skip_preserves_play_state_over_sparse_buffer(SkipDirection::Next, true).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn next_while_playing_stays_playing_over_sparse_buffer() {
    assert_skip_preserves_play_state_over_sparse_buffer(SkipDirection::Next, false).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn previous_while_paused_stays_paused_over_sparse_buffer() {
    assert_skip_preserves_play_state_over_sparse_buffer(SkipDirection::Previous, true).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn previous_while_playing_stays_playing_over_sparse_buffer() {
    assert_skip_preserves_play_state_over_sparse_buffer(SkipDirection::Previous, false).await;
}

/// Multi-window port of `test_previous_track_navigation`: pressed promptly,
/// Previous steps back a track; pressed later in the track, it restarts the
/// current one instead — both outcomes rebuild the decoder over the sparse
/// buffer rather than a fully-buffered file.
#[tokio::test(flavor = "multi_thread")]
async fn previous_navigation_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-previous-nav").await;
    let first = playback.track_ids[0].clone();
    let second = playback.track_ids[1].clone();
    playback.play_and_wait(&first).await;

    playback.playback_handle.next();
    wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == second),
        Duration::from_secs(25),
    )
    .await
    .expect("Next should switch to the second track");

    // Pressed promptly (well inside the 3s window): Previous steps back.
    playback.playback_handle.seek(Duration::from_secs(1));
    position_after(&mut playback.progress_rx, Duration::from_millis(500)).await;
    playback.playback_handle.previous();
    wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
        Duration::from_secs(25),
    )
    .await
    .expect("Previous called early in the track should go to the previous track");

    // Pressed late in the track: Previous restarts the current one.
    playback.playback_handle.seek(Duration::from_secs(10));
    position_after(&mut playback.progress_rx, Duration::from_millis(500)).await;
    playback.playback_handle.previous();
    wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == first),
        Duration::from_secs(25),
    )
    .await
    .expect("Previous called late in the track should restart it");
    let restart_position =
        position_after(&mut playback.progress_rx, Duration::from_millis(800)).await;
    assert!(
        restart_position < 3_000,
        "restart should reset position near 0, got {restart_position}ms",
    );
}

/// Multi-window port of `seek_preserves_staged_next_for_a_gapless_advance`:
/// a seek rebuilds the stream, but the staged gapless next must survive it —
/// exercised here where re-staging must also survive the sparse buffer's
/// on-demand fetch rather than reading an already-buffered preload.
#[tokio::test(flavor = "multi_thread")]
async fn seek_preserves_staged_next_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-seek-preserves-next").await;
    let first = playback.track_ids[0].clone();
    let second = playback.track_ids[1].clone();
    playback.play_and_wait(&first).await;

    // Seek twice to stress the take-out/re-stage across an already-re-staged
    // source, then let the boundary arrive.
    playback.playback_handle.seek(Duration::from_secs(30));
    wait_for_seeked_on(&mut playback.progress_rx, Duration::from_secs(20))
        .await
        .expect("first seek lands");
    playback.playback_handle.seek(Duration::from_secs(56));
    wait_for_seeked_on(&mut playback.progress_rx, Duration::from_secs(20))
        .await
        .expect("second seek lands");

    let outcome = observe_boundary(
        &mut playback.progress_rx,
        &first,
        &second,
        Duration::from_secs(25),
    )
    .await;
    assert!(
        outcome.reached_incoming,
        "playback crosses into the second track after two re-staging seeks"
    );
    assert!(
        outcome.decode_stats_for_finishing && !outcome.completed_for_finishing,
        "the boundary stays gapless: the staged next survived both seeks"
    );
}

/// Multi-window port of `test_restore_emits_seeked_at_saved_position` over
/// the shared-buffer resume path: persist mid-track, restart the service, and
/// confirm the restored track resumes and streams from the saved position
/// rather than starving on a stale/cancelled buffer.
#[tokio::test(flavor = "multi_thread")]
async fn restore_at_position_over_sparse_buffer_resumes_and_advances() {
    let mut playback = MultiWindowPlayback::new("multi-window-restore").await;
    let last_track = playback.track_ids[2].clone();
    playback.play_and_wait(&last_track).await;

    let target = Duration::from_secs(20);
    playback.playback_handle.seek(target);
    wait_for_seeked_on(&mut playback.progress_rx, Duration::from_secs(20))
        .await
        .expect("the seek before shutdown lands");

    // Persist and tear the service down. The second service below starts
    // completely cold — no shared file buffer cache, no preload — the same
    // as a real app relaunch reopening the same library.
    playback.playback_handle.shutdown().await;

    let (handle, _capture_rx) = start_capture_service(
        playback.library_manager.clone(),
        playback.runtime_handle.clone(),
    );
    let mut progress_rx = handle.subscribe_progress();

    wait_for_state_on(
        &mut progress_rx,
        |s| matches!(s, PlaybackState::Paused { track_info, .. } if track_info.track_id == last_track),
        Duration::from_secs(20),
    )
    .await
    .expect("restart should restore paused at the saved track");

    handle.resume();
    wait_for_state_on(
        &mut progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == last_track),
        Duration::from_secs(20),
    )
    .await
    .expect("resume after restore should start playing");

    wait_for_position_advance(&mut progress_rx)
        .await
        .expect("resumed playback must keep advancing from the restored position");
}

/// With the "Restore on launch" preference off, a service that starts over a
/// saved `playback_state` row must come up with nothing in playback: empty
/// queue, no current track, no restored state emission. The row itself stays —
/// it's the crash-safe resume point, and flipping the preference back on
/// restores it at the next launch.
#[tokio::test(flavor = "multi_thread")]
async fn restore_off_starts_with_nothing_in_playback_and_keeps_the_row() {
    let mut playback = MultiWindowPlayback::new("multi-window-restore-off").await;
    let track = playback.track_ids[0].clone();
    playback.play_and_wait(&track).await;

    // Persist mid-track and tear the service down, like an app quit.
    playback.playback_handle.shutdown().await;
    assert!(
        matches!(
            playback
                .library_manager
                .load_playback_state()
                .await
                .expect("read the resume row"),
            bae_core::db::LoadedPlaybackState::Present(_)
        ),
        "shutdown persists a resume row while a track is current"
    );

    // Relaunch with the preference off.
    let (handle, _capture_rx) = start_capture_service_with_restore(
        playback.library_manager.clone(),
        playback.runtime_handle.clone(),
        false,
    );
    let mut progress_rx = handle.subscribe_progress();

    // No restored Paused/Playing state may surface. The bounded window is
    // generous relative to how fast a restore emits (immediately at startup).
    let restored = wait_for_state_on(
        &mut progress_rx,
        |s| {
            matches!(
                s,
                PlaybackState::Paused { .. } | PlaybackState::Playing { .. }
            )
        },
        Duration::from_secs(3),
    )
    .await;
    assert!(
        restored.is_none(),
        "restore-off must not surface a restored playback state, got {restored:?}"
    );

    let queue = handle.queue_projection().await.expect("queue projection");
    assert!(
        queue.manual.is_empty() && queue.context.is_none(),
        "restore-off starts with an empty queue"
    );

    // The row survives the skipped restore, so turning the preference on
    // restores this session at the next launch.
    assert!(
        matches!(
            playback
                .library_manager
                .load_playback_state()
                .await
                .expect("re-read the resume row"),
            bae_core::db::LoadedPlaybackState::Present(_)
        ),
        "the resume row must survive a restore-off launch"
    );
}

/// Multi-window analog of `assert_preload_refreshed_after_queue_mutation`,
/// used by all four preload-displacement mutations (`add_next`,
/// `reorder_entry`, `insert_in_queue`, `remove_entry`): after the mutation
/// displaces the stale preload, Next plays the newly-correct track AND its
/// audio actually flows over the sparse buffer — a regression that keeps the
/// queue correct but silences the promoted track fails the audio-advances
/// assertion.
async fn assert_preload_refreshed_over_sparse_buffer<F>(
    playback: &mut MultiWindowPlayback,
    initial_queue: Vec<String>,
    track0: &str,
    expected: &str,
    mutate: F,
) where
    F: FnOnce(&bae_core::playback::PlaybackHandle, &[bae_core::playback::QueueEntry]),
{
    playback.playback_handle.add_to_queue(initial_queue);
    playback.playback_handle.play(track0.to_string());

    let (played, entries) =
        wait_for_playing_capturing_queue_on(&mut playback.progress_rx, Duration::from_secs(25))
            .await;
    assert!(played, "track0 should start playing");

    mutate(&playback.playback_handle, &entries);
    playback.playback_handle.next();

    let next_state = wait_for_state_on(
        &mut playback.progress_rx,
        |s| match s {
            PlaybackState::Playing { track_info, .. }
            | PlaybackState::Paused { track_info, .. } => track_info.track_id != track0,
            _ => false,
        },
        Duration::from_secs(25),
    )
    .await;
    let state = next_state.expect("Next should switch off track0 after the queue mutation");
    let playing_id = match &state {
        PlaybackState::Playing { track_info, .. } => track_info.track_id.clone(),
        PlaybackState::Paused { track_info, .. } => track_info.track_id.clone(),
        _ => unreachable!(),
    };
    assert_eq!(playing_id, expected);

    if matches!(state, PlaybackState::Playing { .. }) {
        assert_position_advances(&mut playback.progress_rx).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn add_next_displaces_preloaded_track_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-add-next-displace").await;
    let (track0, track1, track2) = (
        playback.track_ids[0].clone(),
        playback.track_ids[1].clone(),
        playback.track_ids[2].clone(),
    );
    let t2 = track2.clone();
    assert_preload_refreshed_over_sparse_buffer(
        &mut playback,
        vec![track1],
        &track0,
        &track2,
        move |h, _entries| h.add_next(vec![t2]),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn remove_entry_refreshes_preloaded_track_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-remove-entry-refresh").await;
    let (track0, track1, track2) = (
        playback.track_ids[0].clone(),
        playback.track_ids[1].clone(),
        playback.track_ids[2].clone(),
    );
    assert_preload_refreshed_over_sparse_buffer(
        &mut playback,
        vec![track1, track2.clone()],
        &track0,
        &track2,
        |h, entries| h.remove_entry(entries[0].id.clone()),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn reorder_entry_displaces_preloaded_track_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-reorder-displace").await;
    let (track0, track1, track2) = (
        playback.track_ids[0].clone(),
        playback.track_ids[1].clone(),
        playback.track_ids[2].clone(),
    );
    assert_preload_refreshed_over_sparse_buffer(
        &mut playback,
        vec![track1, track2.clone()],
        &track0,
        &track2,
        |h, entries| h.reorder_entry(entries[1].id.clone(), Some(entries[0].id.clone())),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn insert_in_queue_displaces_preloaded_track_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-insert-displace").await;
    let (track0, track1, track2) = (
        playback.track_ids[0].clone(),
        playback.track_ids[1].clone(),
        playback.track_ids[2].clone(),
    );
    let t2 = track2.clone();
    assert_preload_refreshed_over_sparse_buffer(
        &mut playback,
        vec![track1],
        &track0,
        &track2,
        move |h, _entries| h.insert_in_queue(vec![t2], 0),
    )
    .await;
}

/// Multi-window port of `skip_to_entry_jumps_to_that_queue_entry`: SkipTo
/// must land on the targeted entry and actually stream it, not just project
/// it as current.
#[tokio::test(flavor = "multi_thread")]
async fn skip_to_entry_jumps_to_that_queue_entry_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-skip-to-entry").await;
    let first = playback.track_ids[0].clone();
    let third = playback.track_ids[2].clone();
    playback.playback_handle.play(first.clone());
    let (played, entries) =
        wait_for_playing_capturing_queue_on(&mut playback.progress_rx, Duration::from_secs(25))
            .await;
    assert!(played, "the release starts playing");
    let target = entries
        .iter()
        .find(|e| e.track_id == third)
        .expect("the third track is queued in the context");
    playback.playback_handle.skip_to_entry(target.id.clone());
    wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == third),
        Duration::from_secs(25),
    )
    .await
    .expect("SkipTo jumps to the targeted entry");

    assert_position_advances(&mut playback.progress_rx).await;
}

/// Multi-window port of
/// `set_shuffle_permutes_and_unpermutes_the_context_lane_in_place`: rearranging
/// the lane must not disturb the currently-playing track's audio.
#[tokio::test(flavor = "multi_thread")]
async fn set_shuffle_rearranges_the_context_lane_over_sparse_buffer() {
    let mut playback = MultiWindowPlayback::new("multi-window-set-shuffle").await;
    let first = playback.track_ids[0].clone();
    playback.play_and_wait(&first).await;

    playback.playback_handle.set_shuffle(true);
    let shuffled = playback
        .playback_handle
        .queue_projection()
        .await
        .expect("queue projection");
    assert!(
        shuffled.context.as_ref().is_some_and(|c| c.shuffled),
        "SetShuffle(true) puts the lane in shuffled order"
    );

    playback.playback_handle.set_shuffle(false);
    let sequential = playback
        .playback_handle
        .queue_projection()
        .await
        .expect("queue projection");
    assert!(
        !sequential
            .context
            .expect("still a playing context")
            .shuffled,
        "SetShuffle(false) puts the lane back in sequential order"
    );

    assert_position_advances(&mut playback.progress_rx).await;
}

// ============================================================================
// Remote (cloud-path) variant: local files make the fill near-instant, so the
// tests above never exercise a real ranged read. This imports the same
// multi-window CUE album as remote-unpinned against an InMemoryCloudHome and
// deletes the local originals, so every byte the fill touches comes from an
// actual ranged cloud read — the fetch arbiter and window fetches over the real
// remote path, not just the local-disk fast path.
// ============================================================================

/// The at-rest key the remote fixture encrypts under. Fixed, so the cipher each
/// cloned test rebuilds decrypts the blob the template uploaded once.
const REMOTE_FIXTURE_MASTER_KEY: [u8; 32] = [17u8; 32];

/// The multi-window CUE album imported remote-unpinned and uploaded once, for
/// every remote test to clone — the import (decode-verify + loudness over a
/// ~30 MiB FLAC) and the encrypt-and-upload of that blob are the expensive
/// half of this fixture, and neither depends on which test runs next.
///
/// The template owns the `InMemoryCloudHome` holding the encrypted blob. Its
/// clones share one backing store ("separate devices reading the same cloud
/// bucket"), so handing each test a clone gives it the same bucket with no
/// re-upload.
///
/// Nothing but the SQLite store is cloned, because nothing but the SQLite store
/// exists: the import original is deleted after the upload and a remote-unpinned
/// blob keeps no local copy, so the audio lives *only* in the cloud home. A
/// cloned test therefore has nothing on disk to resolve against and every read it
/// makes must go through a real ranged cloud fetch — the property these tests
/// exist to exercise.
struct RemoteMultiWindowTemplate {
    dir: TempDir,
    cloud: coven::InMemoryCloudHome,
    track_ids: Vec<String>,
}

static REMOTE_MULTI_WINDOW_TEMPLATE: std::sync::LazyLock<RemoteMultiWindowTemplate> =
    std::sync::LazyLock::new(|| {
        // A dedicated runtime, not a calling test's: the import and upload must
        // finish and every task/connection they spawned must be torn down
        // (dropping the runtime blocks until they are) before the template
        // directory is safe to copy — coven opens SQLite in WAL mode, so a copy
        // taken while a connection is still live could catch an unmerged -wal file.
        let rt = tokio::runtime::Runtime::new()
            .expect("build the remote multi-window template import's runtime");
        let template = rt.block_on(async {
            build_remote_multi_window_template()
                .await
                .expect("import and upload the remote multi-window template release")
        });
        drop(rt);
        template
    });

async fn build_remote_multi_window_template(
) -> Result<RemoteMultiWindowTemplate, Box<dyn std::error::Error>> {
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
    let library_dir = StoreDir::new(temp_dir.path().to_path_buf());
    let (config_handle, key_service) = test_config_and_keys(&library_dir);
    let library_manager = LibraryManager::new(
        database,
        config_handle,
        key_service,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        bae_core::import::cover_art::RemoteImageCache::for_test(),
    );
    let cloud = coven::InMemoryCloudHome::new();
    library_manager
        .connect_test_cloud_home(Arc::new(cloud.clone()), remote_fixture_cipher())
        .await?;

    let runtime_handle = tokio::runtime::Handle::current();
    let release_id_key = seed_discogs_test_release(create_multi_window_cue_album());
    generate_multi_window_cue_flac_files(&album_dir);

    let import_ids = SequentialIdProvider::new("multi-window-remote-template");
    let import_handle = start_test_import(runtime_handle.clone(), library_manager.clone()).await;
    let import_id = import_ids.new_id();
    import_handle
        .send_command(ImportCommand {
            import_id: import_id.clone(),
            candidate_key: "multi-window-remote-template".to_string(),
            folder: album_dir.clone(),
            scope: bae_core::import::ReleaseFileScope::Recursive,
            selected_cover: None,
            storage_mode: StorageMode::Remote,
            pin: false,
            identity_choice: IdentityChoice::Exact {
                release_ref: MetadataRef::new(release_id_key, MetadataSource::Discogs),
            },
            user_edit: None,
        })
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
    let mut import_rx = import_handle.subscribe_import(import_id);
    let (release_id, _album_id) = wait_for_import_complete(&mut import_rx).await;

    // Run the upload so the encrypted blob lands in the cloud and the outbox
    // clears — after this the track resolves cloud-only.
    while matches!(
        library_manager.drain_uploads_for_test().await?,
        coven::DrainOutcome::Drained { uploaded, .. } if uploaded > 0
    ) {}

    // Delete the import original so file resolution can't fall back to it. With
    // the blob unpinned there is now no copy of the audio anywhere but the cloud
    // home, which is what forces every read through a ranged cloud fetch.
    std::fs::remove_dir_all(&album_dir)?;

    let tracks = library_manager.get_tracks_for_release(&release_id).await?;
    let track_ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
    assert_eq!(track_ids.len(), 3, "the CUE album imports 3 tracks");

    Ok(RemoteMultiWindowTemplate {
        dir: temp_dir,
        cloud,
        track_ids,
    })
}

fn remote_fixture_cipher() -> bae_core::sync::CloudCipher {
    bae_core::sync::CloudCipher::Encrypted(coven::EncryptionService::from_key(
        REMOTE_FIXTURE_MASTER_KEY,
    ))
}

/// Clone the remote template into a fresh `TempDir`, returning it with a handle
/// on the shared cloud bucket and the template's track ids. Blocking (file I/O,
/// and dereferencing the template runs its one-time import on its own runtime),
/// so callers run it on a blocking thread. Mirrors `clone_multi_window_library`.
fn clone_remote_multi_window_library() -> (TempDir, coven::InMemoryCloudHome, Vec<String>) {
    let template = &*REMOTE_MULTI_WINDOW_TEMPLATE;
    let fresh = clone_template_library(template.dir.path());
    (fresh, template.cloud.clone(), template.track_ids.clone())
}

/// Import and upload a remote multi-window library into a bucket nothing else
/// touches, then clone it the way `clone_remote_multi_window_library` clones the
/// shared one. Blocking (an import plus file I/O), so callers run it on a
/// blocking thread.
///
/// Its own bucket, rather than a clone of the shared template's, because that one
/// serves every remote test at once: read counters taken against it would include
/// whatever the others happened to be fetching.
///
/// The import runs on its own runtime, which is dropped before the directory is
/// read — the same teardown `REMOTE_MULTI_WINDOW_TEMPLATE` performs. The result
/// is still a *clone* rather than the template directory itself: coven's
/// store-open guard is process-wide and keyed by directory, and the template's
/// own handle still holds its entry, so opening that same path again is refused
/// ("store is already open"). A clone is a different path, which is exactly why
/// every other fixture here opens one.
fn build_private_remote_multi_window_library() -> (TempDir, coven::InMemoryCloudHome, Vec<String>) {
    let rt = tokio::runtime::Runtime::new()
        .expect("build the private remote multi-window import's runtime");
    let template = rt.block_on(async {
        build_remote_multi_window_template()
            .await
            .expect("import and upload a private remote multi-window release")
    });
    drop(rt);
    let fresh = clone_template_library(template.dir.path());
    (fresh, template.cloud.clone(), template.track_ids.clone())
}

/// The multi-window CUE album imported remote-unpinned against a
/// `InMemoryCloudHome`, with the local originals deleted so every read resolves
/// through an actual ranged cloud fetch.
struct RemoteMultiWindowPlayback {
    playback_handle: bae_core::playback::PlaybackHandle,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_ids: Vec<String>,
    /// The bucket this library reads through, for a test that counts what a read
    /// actually cost. Shared with every other clone of the same template, so a
    /// test asserting on its counters needs a template of its own — see
    /// `seek_over_remote_cloud_costs_chunks_not_the_whole_blob`.
    cloud: coven::InMemoryCloudHome,
    _capture_stream_rx: CaptureStreamRx,
    _temp_dir: TempDir,
}

impl RemoteMultiWindowPlayback {
    /// Build against a fresh clone of the shared remote template. `name` only
    /// labels the clone in test output — the import and upload run once per
    /// process, in `REMOTE_MULTI_WINDOW_TEMPLATE`.
    async fn new(name: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let (temp_dir, cloud, track_ids) =
            tokio::task::spawn_blocking(clone_remote_multi_window_library).await?;
        tracing::debug!("remote multi-window clone ready for {name}");
        Self::over(temp_dir, cloud, track_ids).await
    }

    /// Build a playback service over an already-cloned remote library and the
    /// bucket its blob lives in. Split out so a test that counts cloud reads can
    /// supply a template it owns alone, instead of the process-wide one every
    /// other remote test is reading concurrently.
    async fn over(
        temp_dir: TempDir,
        cloud: coven::InMemoryCloudHome,
        track_ids: Vec<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let db_path = temp_dir.path().join("test.db");
        let database = Database::new_test(
            db_path.to_str().expect("db path is valid UTF-8"),
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
        )
        .await?;
        let library_dir = StoreDir::new(temp_dir.path().to_path_buf());
        let (config_handle, key_service) = test_config_and_keys(&library_dir);
        let runtime_handle = tokio::runtime::Handle::current();
        let library_manager = LibraryManager::new(
            database,
            config_handle,
            key_service,
            std::sync::Arc::new(coven::SystemClock),
            std::sync::Arc::new(coven::UuidProvider),
            bae_core::diagnostics::Diagnostics::noop(),
            runtime_handle.clone(),
            bae_core::import::cover_art::RemoteImageCache::for_test(),
        );

        // Reconnect the template's bucket: the cloned rows name the blob, but the
        // live cloud home is an in-process object no directory copy can carry.
        library_manager
            .connect_test_cloud_home(Arc::new(cloud.clone()), remote_fixture_cipher())
            .await?;

        let (playback_handle, capture_stream_rx) =
            start_capture_service(library_manager, runtime_handle);
        let progress_rx = playback_handle.subscribe_progress();
        Ok(Self {
            playback_handle,
            progress_rx,
            track_ids,
            cloud,
            _capture_stream_rx: capture_stream_rx,
            _temp_dir: temp_dir,
        })
    }

    /// Play `track_id` and wait for its Playing state.
    async fn play_and_wait(&mut self, track_id: &str) {
        play_and_wait_on(&self.playback_handle, &mut self.progress_rx, track_id).await;
    }
}

/// Remote-cloud port of `switching_tracks_within_a_multi_window_file_keeps_streaming`:
/// switching to the middle track needs bytes fetched by an actual ranged
/// cloud read, not a local-disk read.
#[tokio::test(flavor = "multi_thread")]
async fn switching_tracks_within_a_multi_window_file_over_remote_cloud() {
    let mut playback = RemoteMultiWindowPlayback::new("multi-window-remote-switch")
        .await
        .expect("set up the remote multi-window fixture");

    let last_track = playback.track_ids[2].clone();
    playback.play_and_wait(&last_track).await;

    let middle_track = playback.track_ids[1].clone();
    playback.playback_handle.play(middle_track.clone());
    let playing = wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == middle_track),
        Duration::from_secs(30),
    )
    .await;
    assert!(
        playing.is_some(),
        "the middle track reaches Playing after the switch over a real ranged cloud read"
    );
}

/// Remote-cloud port of `auto_advance_crosses_gaplessly_within_a_multi_window_file`:
/// the gapless crossing's preload decodes bytes fetched over a real ranged
/// cloud read.
#[tokio::test(flavor = "multi_thread")]
async fn auto_advance_crosses_gaplessly_over_remote_cloud() {
    let mut playback = RemoteMultiWindowPlayback::new("multi-window-remote-gapless")
        .await
        .expect("set up the remote multi-window fixture");
    let first = playback.track_ids[0].clone();
    let second = playback.track_ids[1].clone();
    playback.play_and_wait(&first).await;

    playback.playback_handle.seek(Duration::from_secs(54));
    let outcome = observe_boundary(
        &mut playback.progress_rx,
        &first,
        &second,
        Duration::from_secs(30),
    )
    .await;
    assert!(
        outcome.reached_incoming,
        "playback crosses into the second track over a real ranged cloud read"
    );
    assert!(
        outcome.decode_stats_for_finishing && !outcome.completed_for_finishing,
        "the boundary is gapless: decode stats without TrackCompleted"
    );

    wait_for_position_advance(&mut playback.progress_rx)
        .await
        .expect("track 2's position must advance after the crossing over a real ranged cloud read");
}

/// Remote-cloud port of `manual_next_into_a_pregap_track_rebuilds_and_keeps_streaming`.
#[tokio::test(flavor = "multi_thread")]
async fn manual_next_into_a_pregap_track_over_remote_cloud() {
    let mut playback = RemoteMultiWindowPlayback::new("multi-window-remote-next-pregap")
        .await
        .expect("set up the remote multi-window fixture");
    let [first, second] = [playback.track_ids[0].clone(), playback.track_ids[1].clone()];
    playback.play_and_wait(&first).await;

    playback.playback_handle.next();
    wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == second),
        Duration::from_secs(30),
    )
    .await
    .expect("the second track reaches Playing after a manual Next over a real ranged cloud read");

    let position = position_after(&mut playback.progress_rx, Duration::from_secs(3)).await;
    assert!(
        position > 0,
        "the rebuilt track's position advances — its bytes are fetched over the real cloud path"
    );
}

/// Remote-cloud port of `manual_next_into_a_clean_track_promotes_the_preload`.
#[tokio::test(flavor = "multi_thread")]
async fn manual_next_into_a_clean_track_over_remote_cloud() {
    let mut playback = RemoteMultiWindowPlayback::new("multi-window-remote-next-preload")
        .await
        .expect("set up the remote multi-window fixture");
    let [second, third] = [playback.track_ids[1].clone(), playback.track_ids[2].clone()];
    playback.play_and_wait(&second).await;

    playback.playback_handle.next();
    wait_for_state_on(
        &mut playback.progress_rx,
        |s| matches!(s, PlaybackState::Playing { track_info, .. } if track_info.track_id == third),
        Duration::from_secs(30),
    )
    .await
    .expect("the third track reaches Playing after a manual Next over a real ranged cloud read");

    let position = position_after(&mut playback.progress_rx, Duration::from_secs(3)).await;
    assert!(
        position > 0,
        "the promoted track's position advances — its bytes are fetched over the real cloud path"
    );
}

/// Remote-cloud seek port: seeking within the last track over a real ranged
/// cloud read must still land near the target and keep streaming.
#[tokio::test(flavor = "multi_thread")]
async fn seek_within_the_last_track_over_remote_cloud() {
    let mut playback = RemoteMultiWindowPlayback::new("multi-window-remote-seek")
        .await
        .expect("set up the remote multi-window fixture");
    let last_track = playback.track_ids[2].clone();
    playback.play_and_wait(&last_track).await;

    let target = Duration::from_secs(20);
    playback.playback_handle.seek(target);
    let landed = wait_for_seeked_on(&mut playback.progress_rx, Duration::from_secs(30))
        .await
        .expect("a seek within the last track emits Seeked over a real ranged cloud read");
    assert!(
        Duration::from_millis(landed).abs_diff(target) < Duration::from_secs(2),
        "should land near {target:?}, got {landed}ms"
    );

    wait_for_position_advance(&mut playback.progress_rx)
        .await
        .expect("audio must keep flowing after the seek over a real ranged cloud read");
}

/// No single ranged read may exceed this. The fill asks for one 4 MiB window at a
/// time and coven serves the chunks covering it, so the largest read is a window
/// plus chunk alignment; the slack here is for that alignment, not for a second
/// window. This is the assertion that states the property — *a range costs its own
/// bytes* — and it holds however many windows the fill happens to pull.
const SEEK_LARGEST_RANGE_BOUND: u64 = 5 * 1024 * 1024;

/// Everything the seek's ranged reads move, together, must stay under this — half
/// the fixture's 31,922,637-byte blob. Loose on purpose: the fill keeps reading
/// ahead while the seek settles, so the total lands on a whole number of 4 MiB
/// windows (measured at one or two, 4,195,328 or 8,521,760 bytes, depending on
/// how far read-ahead got before the counter was read). The point it pins is only
/// that a seek never drags the file across, so it is bounded well above that
/// race and well below the blob.
const SEEK_RANGE_BYTES_BOUND: u64 = 16 * 1024 * 1024;

/// Coarse backstop on the seek itself. Serving a range used to mean downloading,
/// decrypting and hashing the whole blob — seconds per read, several reads to a
/// seek — so this is far above a chunked seek and far below the old cost.
const SEEK_COST_BACKSTOP: Duration = Duration::from_secs(15);

/// A seek over a remote cloud home costs the chunks its range touches, not the
/// whole blob. coven seals a blob per chunk and authenticates each chunk on its
/// own, so a ranged read fetches only the chunks covering it: seeking near the end
/// of the last track moves 4,195,328 bytes — the fill's one 4 MiB window and a
/// kilobyte of probe — out of a 31,922,637-byte blob.
///
/// Before chunked ranges, opening a remote stream downloaded, decrypted and hashed
/// the entire blob before it could serve a byte. The byte count is what pins the
/// shape here, because it is exact where a stopwatch is load-dependent, and it is
/// two-sided: see the assertions. The elapsed bound is only a coarse backstop.
///
/// This test owns its bucket. Every other remote test clones the shared template
/// and reads one `InMemoryCloudHome` between them, so counters taken there would
/// include their traffic.
#[tokio::test(flavor = "multi_thread")]
async fn seek_over_remote_cloud_costs_chunks_not_the_whole_blob() {
    let (dir, cloud, track_ids) =
        tokio::task::spawn_blocking(build_private_remote_multi_window_library)
            .await
            .expect("build a private remote multi-window library");
    let mut playback = RemoteMultiWindowPlayback::over(dir, cloud, track_ids)
        .await
        .expect("set up playback over the private remote fixture");

    let last_track = playback.track_ids[2].clone();
    playback.play_and_wait(&last_track).await;

    // Whatever starting the track cost is already paid; measure the seek alone.
    playback.cloud.clear_exact_range_reads();

    // Near the end of the last track (it runs 2:00–3:00 of the file, so this
    // lands ~10 s from the end). Deliberately not mid-track: with a track ceiling
    // set the fill reads ahead to the end of the *current track*, which for a
    // seek to 0:20 would pull the remaining ~40 s — read-ahead the seek did not
    // need, swamping the thing being measured. Landing near the end leaves only a
    // short tail to read ahead into, so what the counter sees is the seek's own
    // window.
    let target = Duration::from_secs(50);
    let started = Instant::now();
    playback.playback_handle.seek(target);
    let landed = wait_for_seeked_on(&mut playback.progress_rx, SEEK_COST_BACKSTOP)
        .await
        .expect("the seek must land within the backstop over a chunked ranged read");
    assert!(
        Duration::from_millis(landed).abs_diff(target) < Duration::from_secs(2),
        "the seek should land near {target:?}, got {landed}ms"
    );
    wait_for_position_advance(&mut playback.progress_rx)
        .await
        .expect("audio must flow from the completed chunked-range seek");
    let elapsed = started.elapsed();

    // Ranged reads are the measurement, not `exact_full_read_count`: that counter
    // is bucket-wide and the sync loop reads whole objects (commits, snapshots)
    // throughout, so it climbs by the hundreds here for reasons that have nothing
    // to do with playback.
    //
    // Three checks, catching different regressions. That any ranged read happened
    // at all is what says the blob was served by range: were the whole-object
    // download back, the audio would arrive as one full read and this list would
    // be empty. The largest single read is the property itself — a range costs its
    // own bytes — and is independent of how many windows read-ahead pulled. The
    // total is the coarse backstop that no seek drags the file across.
    let reads = playback.cloud.exact_range_reads();
    let range_bytes = playback.cloud.exact_range_read_bytes();
    let largest = reads.iter().map(|(start, end)| end - start).max();
    let largest = largest.expect(
        "the seek served no ranged read at all -- the blob was fetched whole, not by range",
    );
    assert!(
        largest <= SEEK_LARGEST_RANGE_BOUND,
        "the seek's largest single ranged read was {largest} bytes (bound \
         {SEEK_LARGEST_RANGE_BOUND}); a chunked range must cost the chunks it covers, not the \
         31,922,637-byte blob. All reads: {reads:?}",
    );
    assert!(
        range_bytes < SEEK_RANGE_BYTES_BOUND,
        "the seek fetched {range_bytes} bytes from the cloud across {} reads (bound \
         {SEEK_RANGE_BYTES_BOUND}); a seek must not drag the whole blob across",
        reads.len(),
    );
    assert!(
        elapsed < SEEK_COST_BACKSTOP,
        "the seek took {elapsed:?}, past the {SEEK_COST_BACKSTOP:?} backstop"
    );
}

/// A sparse buffer pre-filled with the whole byte slice, so a decode exercises
/// the window logic without waiting on a fill.
fn buffer_from(bytes: &[u8]) -> bae_core::playback::SharedSparseBuffer {
    let buffer = bae_core::playback::sparse_buffer::create_sparse_buffer(bytes.len() as u64);
    buffer.append_at(0, bytes);
    buffer
}
