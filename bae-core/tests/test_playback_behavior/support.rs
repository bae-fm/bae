// coven requires every synced row's primary key to be a v4 UUID.
const RELEASE_THAT_WAS_DELETED: &str = "763072b0-643f-4469-8ac7-799c4550a769";

use bae_core::discogs::models::DiscogsRelease;
use bae_core::import::{ImportCommand, ImportDestination};
use bae_core::library::LibraryManager;
use bae_core::config::SidePauseCountdown;
use bae_core::playback::{
    LoadingTrack, PlaybackPauseBoundary, PlaybackPauseReason, PlaybackProgress,
    PlaybackSideCountdown,
    PlaybackState, RepeatMode,
};
use bae_test_support as support;
use coven::{IdProvider, SequentialIdProvider};
use std::sync::Arc;
use std::time::{Duration, Instant};
use support::start_test_import;
use support::{
    imported_release_setup, open_test_library, samples_as_f32, seed_discogs_test_release,
    tracing_init, wait_for_import_complete,
};
use tempfile::TempDir;
use tracing::debug;

/// Return the first `StateChanged` state that satisfies `predicate`, or `None`
/// on timeout.
async fn wait_for_state_on<F>(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    predicate: F,
    timeout_duration: Duration,
) -> Option<PlaybackState>
where
    F: Fn(&PlaybackState) -> bool,
{
    support::next_matching(progress_rx, timeout_duration, |event| match event {
        PlaybackProgress::StateChanged { state } if predicate(&state) => Some(state),
        _ => None,
    })
    .await
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
    let mut states = Vec::new();
    support::next_matching(progress_rx, timeout_duration, |event| {
        let PlaybackProgress::StateChanged { state } = event else {
            return None;
        };
        let stop = done(&state);
        states.push(state);
        stop.then_some(())
    })
    .await;
    states
}

/// The next `PositionUpdate`'s track-relative `position_ms`, or `None` if none
/// arrives within `timeout_duration`.
async fn next_position(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    timeout_duration: Duration,
) -> Option<i64> {
    support::next_matching(progress_rx, timeout_duration, |event| match event {
        PlaybackProgress::PositionUpdate { position_ms, .. } => Some(position_ms),
        _ => None,
    })
    .await
}

/// How long a playing track's position may take to reach the mark a test waits
/// for before the wait counts as a stall. Tests assert only that the position
/// gets there, not how fast.
const POSITION_BACKSTOP: Duration = Duration::from_secs(10);

/// The first `position_ms` reported for `track_id`. Where a track starts is
/// what tells a skipped pregap (it starts at 0) from a played one (it starts
/// counting up from a negative value).
async fn first_position_of(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_id: &str,
) -> i64 {
    wait_for_track_position(progress_rx, track_id, Duration::from_secs(30))
        .await
        .unwrap_or_else(|| panic!("no position update for {track_id} arrived within 30s"))
}

/// The first `position_ms` reported for `track_id` that satisfies `reached`,
/// or `None` if none does within `POSITION_BACKSTOP`.
async fn wait_for_track_position_where(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_id: &str,
    reached: impl Fn(i64) -> bool,
) -> Option<i64> {
    support::next_matching(progress_rx, POSITION_BACKSTOP, |event| match event {
        PlaybackProgress::PositionUpdate {
            position_ms,
            track_id: tid,
            ..
        } if tid == track_id && reached(position_ms) => Some(position_ms),
        _ => None,
    })
    .await
}

/// Return the first `position_ms` greater than the first position update seen,
/// or `None` if the position does not advance.
async fn wait_for_position_advance(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
) -> Option<i64> {
    let anchor = next_position(progress_rx, Duration::from_secs(30))
        .await
        .expect("no position update arrived within 30s of requesting one");
    support::next_matching(progress_rx, Duration::from_secs(10), |event| match event {
        PlaybackProgress::PositionUpdate { position_ms, .. } if position_ms > anchor => {
            Some(position_ms)
        }
        _ => None,
    })
    .await
}

/// How long `play` may take to reach `Playing` before the wait counts as a
/// hang. Tests assert only that `Playing` arrives, not how fast.
const PLAY_START_BACKSTOP: Duration = Duration::from_secs(30);

/// Play `track_id` and wait for it to reach `Playing`.
async fn play_and_wait_on(
    handle: &bae_core::playback::PlaybackHandle,
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_id: &str,
) {
    handle.play(track_id.to_string());
    let playing = wait_for_state_on(
        progress_rx,
        |s| matches!(s, PlaybackState::Playing { track, .. } if track.track_id == track_id),
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

/// Wait for the next `Seeked` event and return its `position_ms`, or `None` on
/// timeout.
async fn wait_for_seeked_on(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    timeout_duration: Duration,
) -> Option<u64> {
    support::next_matching(progress_rx, timeout_duration, |event| match event {
        PlaybackProgress::Seeked { position_ms, .. } => Some(
            u64::try_from(position_ms)
                .expect("a seek target cannot be inside the pregap countdown"),
        ),
        _ => None,
    })
    .await
}

/// The volume `settled_events_on` sets as its sentinel: no test sets it
/// otherwise, so its `VolumeChanged` is unmistakable.
const SENTINEL_VOLUME: f32 = 0.4321;

/// Every progress event up to a sentinel command sent now, in arrival order,
/// sentinel excluded. The service handles commands one at a time in the order
/// they were sent and emits on one channel that the fan-out forwards in order,
/// so whatever the earlier commands emit while being handled arrives before the
/// sentinel's `VolumeChanged`. Once it has arrived, `subscribe_values()` holds
/// the state those events left. Setting the volume steers neither transport
/// nor the queue, and leaves a side-pause countdown running.
async fn settled_events_on(
    handle: &bae_core::playback::PlaybackHandle,
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
) -> Vec<PlaybackProgress> {
    handle.set_volume(SENTINEL_VOLUME);
    let mut events = Vec::new();
    support::next_matching(progress_rx, Duration::from_secs(10), |event| match event {
        PlaybackProgress::VolumeChanged { volume } if volume == SENTINEL_VOLUME => Some(()),
        other => {
            events.push(other);
            None
        }
    })
    .await
    .expect("the sentinel volume change arrives within 10s");
    events
}

/// The `StateChanged` states among `events`.
fn states_in(events: &[PlaybackProgress]) -> Vec<&PlaybackState> {
    events
        .iter()
        .filter_map(|event| match event {
            PlaybackProgress::StateChanged { state } => Some(state),
            _ => None,
        })
        .collect()
}

/// Wait for `Playing` and return whether it arrived, plus the queue entries at
/// that moment. `play` gives queue entries fresh ids, so a test that edits the
/// queue must use these.
async fn wait_for_playing_capturing_queue_on(
    playback_handle: &bae_core::playback::PlaybackHandle,
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    timeout_duration: Duration,
) -> (bool, Vec<bae_core::playback::QueueEntry>) {
    let entries = support::next_matching(progress_rx, timeout_duration, |event| {
        let PlaybackProgress::StateChanged {
            state: PlaybackState::Playing { .. },
        } = event
        else {
            return None;
        };
        let mut projection = playback_handle.subscribe_queue_values().borrow().clone();
        let mut entries = projection.manual;
        if let Some(ctx) = projection.context.take() {
            entries.extend(ctx.upcoming);
        }
        Some(entries)
    })
    .await;
    (entries.is_some(), entries.unwrap_or_default())
}

/// Assert that the playing position advances. This catches a stalled audio
/// stream that a queue-only assertion would miss.
async fn assert_position_advances(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
) {
    wait_for_position_advance(progress_rx)
        .await
        .expect("position must keep advancing while playing (the audio stream stalled)");
}

/// Return the first `position_ms` reported for `track_id`, or `None` on
/// timeout. Updates for other tracks are skipped, since one for the finishing
/// track can still arrive right after a boundary.
async fn wait_for_track_position(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_id: &str,
    timeout_duration: Duration,
) -> Option<i64> {
    support::next_matching(progress_rx, timeout_duration, |event| match event {
        PlaybackProgress::PositionUpdate {
            position_ms,
            track_id: tid,
            ..
        } if tid == track_id => Some(position_ms),
        _ => None,
    })
    .await
}

/// Wait for the next `RepeatModeChanged` and return its mode, or panic on
/// timeout.
async fn wait_for_repeat_mode(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    timeout_duration: Duration,
) -> RepeatMode {
    support::next_matching(progress_rx, timeout_duration, |event| match event {
        PlaybackProgress::RepeatModeChanged { mode } => Some(mode),
        _ => None,
    })
    .await
    .expect("no RepeatModeChanged arrived within the timeout")
}

/// Wait for the next `MuteChanged` and return its flag, or panic on timeout.
async fn wait_for_mute(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    timeout_duration: Duration,
) -> bool {
    support::next_matching(progress_rx, timeout_duration, |event| match event {
        PlaybackProgress::MuteChanged { is_muted } => Some(is_muted),
        _ => None,
    })
    .await
    .expect("no MuteChanged arrived within the timeout")
}

/// The progress events a track boundary produced. A gapless handoff reports the
/// finishing track's `DecodeStats` with no `TrackCompleted`, and no `Loading`
/// state for the incoming track; a stream rebuild reports both.
struct BoundaryOutcome {
    decode_stats_for_finishing: bool,
    completed_for_finishing: bool,
    loading_for_incoming: bool,
    reached_incoming: bool,
    decode_errors: u32,
}

/// Record what the boundary from `finishing` to `incoming` produced, until
/// `incoming` reaches `Playing` or the timeout elapses.
async fn observe_boundary(
    progress_rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    finishing: &str,
    incoming: &str,
    timeout_duration: Duration,
) -> BoundaryOutcome {
    let mut outcome = BoundaryOutcome {
        decode_stats_for_finishing: false,
        completed_for_finishing: false,
        loading_for_incoming: false,
        reached_incoming: false,
        decode_errors: 0,
    };
    support::next_matching(progress_rx, timeout_duration, |event| {
        match event {
            PlaybackProgress::DecodeStats {
                track_id,
                error_count,
                ..
            } => {
                outcome.decode_errors += error_count;
                if track_id == finishing {
                    outcome.decode_stats_for_finishing = true;
                }
            }
            PlaybackProgress::TrackCompleted { track_id } if track_id == finishing => {
                outcome.completed_for_finishing = true;
            }
            PlaybackProgress::StateChanged {
                state: PlaybackState::Loading { track },
            } if track.track_id() == incoming => {
                outcome.loading_for_incoming = true;
            }
            PlaybackProgress::StateChanged {
                state: PlaybackState::Playing { track, .. },
            } if track.track_id == incoming => {
                outcome.reached_incoming = true;
                return Some(());
            }
            _ => {}
        }
        None
    })
    .await;
    outcome
}

/// Start a playback service on a capture sink paced to wall-clock time, so no
/// audio device is needed. Hold the returned receiver for the service's
/// lifetime.
#[must_use]
fn start_capture_service(
    library_manager: LibraryManager,
    runtime_handle: tokio::runtime::Handle,
) -> (bae_core::playback::PlaybackHandle, support::CaptureStreamRx) {
    start_capture_service_with_restore(library_manager, runtime_handle, true)
}

/// `start_capture_service` with the platform's "Restore on launch" preference
/// explicit, for tests that cover the restore-off launch path.
#[must_use]
fn start_capture_service_with_restore(
    library_manager: LibraryManager,
    runtime_handle: tokio::runtime::Handle,
    restore_playback: bool,
) -> (bae_core::playback::PlaybackHandle, support::CaptureStreamRx) {
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

/// The three-track FLAC album every `PlaybackTestFixture` plays, imported once
/// per process into a library that each fixture copies. The FLAC files are not
/// copied: `local_blob_refs` stores absolute paths, so every copy reads the
/// template's `album/` directory.
struct PlaybackFixtureTemplate {
    dir: TempDir,
    album_dir: std::path::PathBuf,
    track_ids: Vec<String>,
}

static PLAYBACK_FIXTURE_TEMPLATE: std::sync::LazyLock<PlaybackFixtureTemplate> =
    std::sync::LazyLock::new(|| {
        // Import on its own runtime and drop it before any copy, so no SQLite
        // connection is still open with unmerged WAL data when the files are
        // copied.
        let rt =
            tokio::runtime::Runtime::new().expect("build the playback template import's runtime");
        let template = rt.block_on(async {
            let import_ids = SequentialIdProvider::new("playback-fixture-template");
            let (_library_manager, imported) = imported_release_setup(
                create_test_album(),
                "test",
                import_ids.new_id(),
                |album_dir| {
                    let _track_data = generate_test_flac_files(album_dir);
                },
            )
            .await
            .expect("import the playback fixture template release");
            assert!(
                !imported.track_ids.is_empty(),
                "the playback fixture template should import tracks"
            );
            PlaybackFixtureTemplate {
                album_dir: imported.album_dir.clone(),
                dir: imported.temp_dir,
                track_ids: imported.track_ids,
            }
        });
        drop(rt);
        template
    });

/// Copy the playback fixture template into a fresh `TempDir`. Call it on a
/// blocking thread: the template's first use blocks on its own runtime.
fn clone_playback_fixture_library() -> (TempDir, std::path::PathBuf, Vec<String>) {
    let template = &*PLAYBACK_FIXTURE_TEMPLATE;
    let fresh = clone_template_library(template.dir.path());
    (
        fresh,
        template.album_dir.clone(),
        template.track_ids.clone(),
    )
}

/// A playback service over a copy of the three-track template album.
struct PlaybackTestFixture {
    playback_handle: bae_core::playback::PlaybackHandle,
    progress_rx: tokio::sync::mpsc::UnboundedReceiver<PlaybackProgress>,
    track_ids: Vec<String>,
    album_dir: std::path::PathBuf,
    /// Lets tests read the device-local `playback_state` row directly.
    library_manager: LibraryManager,
    /// Dropping this receiver would make stream creation fail.
    _capture_stream_rx: tokio::sync::mpsc::UnboundedReceiver<Arc<std::sync::Mutex<Vec<f32>>>>,
    _temp_dir: TempDir,
}
impl PlaybackTestFixture {
    async fn new() -> Self {
        let (temp_dir, album_dir, track_ids) =
            tokio::task::spawn_blocking(clone_playback_fixture_library)
                .await
                .expect("clone the playback fixture template library");

        let (library_manager, _database) = open_test_library(temp_dir.path()).await;
        let runtime_handle = tokio::runtime::Handle::current();

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
    /// See `wait_for_playing_capturing_queue_on`.
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
        support::next_matching(
            &mut self.progress_rx,
            timeout_duration,
            |event| match event {
                PlaybackProgress::PositionUpdate { position_ms, .. } => Some(
                    u64::try_from(position_ms)
                        .expect("this helper expects playback at or after track start"),
                ),
                _ => None,
            },
        )
        .await
    }
    /// Wait for the first position update past `floor_ms`, or `None` on
    /// timeout. The first update after a seek can still report the seek target,
    /// so only a position past it proves playback moved.
    async fn wait_for_position_past(
        &mut self,
        floor_ms: u64,
        timeout_duration: Duration,
    ) -> Option<u64> {
        let floor = i64::try_from(floor_ms).expect("position floor exceeds i64 range");
        support::next_matching(
            &mut self.progress_rx,
            timeout_duration,
            |event| match event {
                PlaybackProgress::PositionUpdate { position_ms, .. } if position_ms > floor => {
                    Some(
                        u64::try_from(position_ms)
                            .expect("position past a nonnegative floor is nonnegative"),
                    )
                }
                _ => None,
            },
        )
        .await
    }
    /// Wait for a Seeked event with timeout (returns position in ms)
    async fn wait_for_seeked(&mut self, timeout_duration: Duration) -> Option<u64> {
        wait_for_seeked_on(&mut self.progress_rx, timeout_duration).await
    }
    /// See `settled_events_on`.
    async fn settled_events(&mut self) -> Vec<PlaybackProgress> {
        settled_events_on(&self.playback_handle, &mut self.progress_rx).await
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
