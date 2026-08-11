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

include!("test_playback_behavior/side_and_navigation.rs");
include!("test_playback_behavior/queue_and_pregap.rs");
include!("test_playback_behavior/high_rate_and_restore.rs");
include!("test_playback_behavior/local_sparse_buffer.rs");
include!("test_playback_behavior/remote_sparse_buffer.rs");
