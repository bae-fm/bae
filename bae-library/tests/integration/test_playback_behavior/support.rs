use super::fixtures::*;

pub(super) use bae_audio::test_samples::samples_as_f32;
pub(super) use bae_catalogs::discogs::models::DiscogsRelease;
pub(super) use bae_import::test_imports::wait_for_import_complete;
pub(super) use bae_import::{ImportCommand, ImportDestination};
pub(super) use bae_library::test_libraries::{
    imported_release_setup, open_test_library, TestLibrary,
};
pub(super) use bae_playback::{
    LoadingTrack, PlaybackPauseBoundary, PlaybackPauseReason, PlaybackProgress,
    PlaybackSideCountdown, PlaybackState, RepeatMode,
};
pub(super) use bae_preferences::SidePauseCountdown;
pub(super) use bae_types::track_time::TrackTime;
pub(super) use bae_util::test_channels::HANG_GUARD;
pub(super) use coven::{IdSource, SequentialIds};
pub(super) use std::sync::Arc;
pub(super) use std::time::Duration;
pub(super) use tempfile::TempDir;
pub(super) use tracing::debug;

/// Return the first `StateChanged` state that satisfies `predicate`, or `None`
/// if none ever comes.
pub(super) async fn wait_for_state_on<F>(
    progress_rx: &mut bae_playback::PlaybackProgressFeed,
    predicate: F,
) -> Option<PlaybackState>
where
    F: Fn(&PlaybackState) -> bool,
{
    bae_util::test_channels::next_matching(progress_rx, HANG_GUARD, |event| match event {
        PlaybackProgress::StateChanged { state } if predicate(&state) => Some(state),
        _ => None,
    })
    .await
}

/// Collect every StateChanged state in arrival order until one satisfies `done`
/// (that final state is included), or every state seen if none ever does.
pub(super) async fn collect_states_on<F>(
    progress_rx: &mut bae_playback::PlaybackProgressFeed,
    done: F,
) -> Vec<PlaybackState>
where
    F: Fn(&PlaybackState) -> bool,
{
    let mut states = Vec::new();
    bae_util::test_channels::next_matching(progress_rx, HANG_GUARD, |event| {
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
/// ever arrives.
pub(super) async fn next_position(
    progress_rx: &mut bae_playback::PlaybackProgressFeed,
) -> Option<i64> {
    bae_util::test_channels::next_matching(progress_rx, HANG_GUARD, |event| match event {
        PlaybackProgress::PositionUpdate { position_ms, .. } => Some(position_ms),
        _ => None,
    })
    .await
}

/// The first `position_ms` reported for `track_id`. Where a track starts is
/// what tells a skipped pregap (it starts at 0) from a played one (it starts
/// counting up from a negative value).
pub(super) async fn first_position_of(
    progress_rx: &mut bae_playback::PlaybackProgressFeed,
    track_id: &str,
) -> i64 {
    wait_for_track_position(progress_rx, track_id)
        .await
        .unwrap_or_else(|| panic!("no position update for {track_id} ever arrived"))
}

/// The first `position_ms` reported for `track_id` that satisfies `reached`,
/// or `None` if none ever does.
pub(super) async fn wait_for_track_position_where(
    progress_rx: &mut bae_playback::PlaybackProgressFeed,
    track_id: &str,
    reached: impl Fn(i64) -> bool,
) -> Option<i64> {
    bae_util::test_channels::next_matching(progress_rx, HANG_GUARD, |event| match event {
        PlaybackProgress::PositionUpdate { position_ms, track_id: tid, .. }
            if tid == track_id && reached(position_ms) =>
        {
            Some(position_ms)
        }
        _ => None,
    })
    .await
}

/// Return the first `position_ms` greater than the first position update seen,
/// or `None` if the position does not advance.
pub(super) async fn wait_for_position_advance(
    progress_rx: &mut bae_playback::PlaybackProgressFeed,
) -> Option<i64> {
    let anchor = next_position(progress_rx).await.expect("a position update arrives");
    bae_util::test_channels::next_matching(progress_rx, HANG_GUARD, |event| match event {
        PlaybackProgress::PositionUpdate { position_ms, .. } if position_ms > anchor => {
            Some(position_ms)
        }
        _ => None,
    })
    .await
}

/// Play `track_id` and wait for it to reach `Playing`.
pub(super) async fn play_and_wait_on(
    handle: &bae_playback::PlaybackHandle,
    progress_rx: &mut bae_playback::PlaybackProgressFeed,
    track_id: &str,
) {
    handle.play(track_id.to_string());
    let playing = wait_for_state_on(
        progress_rx,
        |s| matches!(s, PlaybackState::Playing { track, .. } if track.track_id == track_id),
    )
    .await;
    assert!(playing.is_some(), "track {track_id} never reached Playing");
}

/// Wait until `follow` tells that this device holds every byte of
/// `track_id`: one stretch from its start to its end.
pub(super) async fn wait_for_whole_track(
    follow: &mut bae_playback::TrackBufferedFollow,
    track_id: &str,
) {
    let whole = vec![bae_playback::TrackSpan { start: 0.0, end: 1.0 }];
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let told = follow.next().await.expect("playback runs");
            if told.is_some_and(|held| held.track_id == track_id && held.ranges == whole) {
                return;
            }
        }
    })
    .await
    .expect("the playing track's bytes all arrive");
}

/// Wait for the next `Seeked` event and return its `position_ms`, or `None` if
/// none ever comes.
pub(super) async fn wait_for_seeked_on(
    progress_rx: &mut bae_playback::PlaybackProgressFeed,
) -> Option<u64> {
    bae_util::test_channels::next_matching(progress_rx, HANG_GUARD, |event| match event {
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
pub(super) const SENTINEL_VOLUME: f32 = 0.4321;

/// Every progress event up to a sentinel command sent now, in arrival order,
/// sentinel excluded. The service handles commands one at a time in the order
/// they were sent and emits on one channel that the fan-out forwards in order,
/// so whatever the earlier commands emit while being handled arrives before the
/// sentinel's `VolumeChanged`. Once it has arrived, the `playback_now` row
/// and `subscribe_positions()` hold what those events left. Setting the
/// volume steers neither transport nor the queue, and leaves a side-pause
/// countdown running.
pub(super) async fn settled_events_on(
    handle: &bae_playback::PlaybackHandle,
    progress_rx: &mut bae_playback::PlaybackProgressFeed,
) -> Vec<PlaybackProgress> {
    handle.set_volume(SENTINEL_VOLUME);
    let mut events = Vec::new();
    bae_util::test_channels::next_matching(progress_rx, HANG_GUARD, |event| match event {
        PlaybackProgress::VolumeChanged { volume } if volume == SENTINEL_VOLUME => Some(()),
        other => {
            events.push(other);
            None
        }
    })
    .await
    .expect("the sentinel volume change arrives");
    events
}

/// The `StateChanged` states among `events`.
pub(super) fn states_in(events: &[PlaybackProgress]) -> Vec<&PlaybackState> {
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
pub(super) async fn wait_for_playing_capturing_queue_on(
    playback_handle: &bae_playback::PlaybackHandle,
    progress_rx: &mut bae_playback::PlaybackProgressFeed,
) -> (bool, Vec<bae_playback::QueueEntry>) {
    let playing = bae_util::test_channels::next_matching(progress_rx, HANG_GUARD, |event| {
        matches!(event, PlaybackProgress::StateChanged { state: PlaybackState::Playing { .. } })
            .then_some(())
    })
    .await;
    if playing.is_none() {
        return (false, Vec::new());
    }
    let value = playback_handle.queue_value().await.expect("queue value");
    let entries = value
        .manual
        .into_iter()
        .chain(playback_handle.queue_upcoming().await.expect("queue upcoming"))
        .map(|item| bae_playback::QueueEntry {
            id: item.entry_id.parse().expect("a queue entry id"),
            track_id: item.track_id,
        })
        .collect();
    (true, entries)
}

/// Assert that the playing position advances. This catches a stalled audio
/// stream that a queue-only assertion would miss.
pub(super) async fn assert_position_advances(progress_rx: &mut bae_playback::PlaybackProgressFeed) {
    wait_for_position_advance(progress_rx)
        .await
        .expect("position must keep advancing while playing (the audio stream stalled)");
}

/// Return the first `position_ms` reported for `track_id`, or `None` if none
/// ever comes. Updates for other tracks are skipped, since one for the
/// finishing track can still arrive right after a boundary.
pub(super) async fn wait_for_track_position(
    progress_rx: &mut bae_playback::PlaybackProgressFeed,
    track_id: &str,
) -> Option<i64> {
    bae_util::test_channels::next_matching(progress_rx, HANG_GUARD, |event| match event {
        PlaybackProgress::PositionUpdate { position_ms, track_id: tid, .. } if tid == track_id => {
            Some(position_ms)
        }
        _ => None,
    })
    .await
}

/// Wait for the next `MuteChanged` and return its flag; panics if none ever
/// comes.
pub(super) async fn wait_for_mute(progress_rx: &mut bae_playback::PlaybackProgressFeed) -> bool {
    bae_util::test_channels::next_matching(progress_rx, HANG_GUARD, |event| match event {
        PlaybackProgress::MuteChanged { is_muted } => Some(is_muted),
        _ => None,
    })
    .await
    .expect("a MuteChanged arrives")
}

/// The progress events a track boundary produced. A gapless handoff reports the
/// finishing track's `DecodeStats` with no `TrackCompleted`, and no `Loading`
/// state for the incoming track; a stream rebuild reports both.
pub(super) struct BoundaryOutcome {
    pub(super) decode_stats_for_finishing: bool,
    pub(super) completed_for_finishing: bool,
    pub(super) loading_for_incoming: bool,
    pub(super) reached_incoming: bool,
    pub(super) decode_errors: u32,
}

/// Record what the boundary from `finishing` to `incoming` produced, until
/// `incoming` reaches `Playing`, or as far as playback ever gets.
pub(super) async fn observe_boundary(
    progress_rx: &mut bae_playback::PlaybackProgressFeed,
    finishing: &str,
    incoming: &str,
) -> BoundaryOutcome {
    let mut outcome = BoundaryOutcome {
        decode_stats_for_finishing: false,
        completed_for_finishing: false,
        loading_for_incoming: false,
        reached_incoming: false,
        decode_errors: 0,
    };
    bae_util::test_channels::next_matching(progress_rx, HANG_GUARD, |event| {
        match event {
            PlaybackProgress::DecodeStats { track_id, error_count, .. } => {
                outcome.decode_errors += error_count;
                if track_id == finishing {
                    outcome.decode_stats_for_finishing = true;
                }
            }
            PlaybackProgress::TrackCompleted { track_id } if track_id == finishing => {
                outcome.completed_for_finishing = true;
            }
            PlaybackProgress::StateChanged { state: PlaybackState::Loading { track } }
                if track.track_id() == incoming =>
            {
                outcome.loading_for_incoming = true;
            }
            PlaybackProgress::StateChanged { state: PlaybackState::Playing { track, .. } }
                if track.track_id == incoming =>
            {
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
pub(super) async fn start_capture_service(
    library: &TestLibrary,
) -> (bae_playback::PlaybackHandle, bae_playback::test_capture::CaptureStreamRx) {
    start_capture_service_with_restore(library, true).await
}

/// `start_capture_service` with the platform's "Restore on launch" preference
/// explicit, for tests that cover the restore-off launch path.
pub(super) async fn start_capture_service_with_restore(
    library: &TestLibrary,
    restore_playback: bool,
) -> (bae_playback::PlaybackHandle, bae_playback::test_capture::CaptureStreamRx) {
    let (captured, capture_stream_rx) = tokio::sync::mpsc::unbounded_channel();
    let capture_device = bae_playback::RealtimeCaptureAudioDevice::new(captured);
    let handle = library
        .start_playback_on(
            Box::new(capture_device),
            bae_playback::test_capture::wall_playback_clock(),
            restore_playback,
        )
        .await;
    (handle, capture_stream_rx)
}

/// A playback service over a library in a temporary directory.
pub(super) trait PlaybackFixture {
    /// Stop playback, then close the library; the directory goes with the
    /// fixture after both.
    async fn close(self);
}

/// Run `test` over the fixture `setup` builds, then close it before its
/// directory is deleted: a library still working as its directory is
/// deleted recreates files in it and leaves the directory behind.
pub(super) async fn with_fixture<F: PlaybackFixture>(
    setup: impl std::future::Future<Output = F>,
    test: impl AsyncFnOnce(&mut F),
) {
    let mut fixture = setup.await;
    test(&mut fixture).await;
    fixture.close().await;
}

/// A playback service over an imported three-track album.
pub(super) struct PlaybackTestFixture {
    pub(super) playback_handle: bae_playback::PlaybackHandle,
    pub(super) progress_rx: bae_playback::PlaybackProgressFeed,
    pub(super) track_ids: Vec<String>,
    pub(super) album_dir: std::path::PathBuf,
    /// The library the album was imported into, which tests read and edit
    /// directly.
    pub(super) library: TestLibrary,
    /// Dropping this receiver would make stream creation fail.
    pub(super) _capture_stream_rx: bae_playback::CapturedStreams,
    pub(super) _temp_dir: TempDir,
}
impl PlaybackFixture for PlaybackTestFixture {
    async fn close(self) {
        self.playback_handle.shutdown().await;
        self.library.close().await.expect("close the test library");
    }
}

impl PlaybackTestFixture {
    pub(super) async fn new() -> Self {
        let (library, imported) = imported_release_setup(
            create_test_album(),
            "playback",
            uuid::Uuid::new_v4().to_string(),
            |directory| {
                generate_test_flac_files(directory);
            },
        )
        .await
        .expect("import the playback fixture");
        let temp_dir = imported.temp_dir;
        let album_dir = imported.album_dir;
        let track_ids = imported.track_ids;
        let (playback_handle, capture_stream_rx) = start_capture_service(&library).await;
        let progress_rx = playback_handle.subscribe_progress();
        Self {
            playback_handle,
            progress_rx,
            track_ids,
            album_dir,
            library,
            _capture_stream_rx: capture_stream_rx,
            _temp_dir: temp_dir,
        }
    }
    /// See `wait_for_state_on`.
    pub(super) async fn wait_for_state<F>(&mut self, predicate: F) -> Option<PlaybackState>
    where
        F: Fn(&PlaybackState) -> bool,
    {
        wait_for_state_on(&mut self.progress_rx, predicate).await
    }
    /// See `wait_for_playing_capturing_queue_on`.
    pub(super) async fn wait_for_playing_capturing_queue(
        &mut self,
    ) -> (bool, Vec<bae_playback::QueueEntry>) {
        wait_for_playing_capturing_queue_on(&self.playback_handle, &mut self.progress_rx).await
    }
    /// The next position update's position in ms, or `None` if none ever
    /// comes.
    pub(super) async fn wait_for_position_update(&mut self) -> Option<u64> {
        bae_util::test_channels::next_matching(&mut self.progress_rx, HANG_GUARD, |event| {
            match event {
                PlaybackProgress::PositionUpdate { position_ms, .. } => Some(
                    u64::try_from(position_ms)
                        .expect("this helper expects playback at or after track start"),
                ),
                _ => None,
            }
        })
        .await
    }
    /// Wait for the first position update past `floor_ms`, or `None` if none
    /// ever comes. The first update after a seek can still report the seek
    /// target, so only a position past it proves playback moved.
    pub(super) async fn wait_for_position_past(&mut self, floor_ms: u64) -> Option<u64> {
        let floor = i64::try_from(floor_ms).expect("position floor exceeds i64 range");
        bae_util::test_channels::next_matching(&mut self.progress_rx, HANG_GUARD, |event| {
            match event {
                PlaybackProgress::PositionUpdate { position_ms, .. } if position_ms > floor => {
                    Some(
                        u64::try_from(position_ms)
                            .expect("position past a nonnegative floor is nonnegative"),
                    )
                }
                _ => None,
            }
        })
        .await
    }
    /// See `wait_for_seeked_on`.
    pub(super) async fn wait_for_seeked(&mut self) -> Option<u64> {
        wait_for_seeked_on(&mut self.progress_rx).await
    }
    /// See `settled_events_on`.
    pub(super) async fn settled_events(&mut self) -> Vec<PlaybackProgress> {
        settled_events_on(&self.playback_handle, &mut self.progress_rx).await
    }
    /// See `collect_states_on`.
    pub(super) async fn collect_states_until<F>(&mut self, done: F) -> Vec<PlaybackState>
    where
        F: Fn(&PlaybackState) -> bool,
    {
        collect_states_on(&mut self.progress_rx, done).await
    }
}

#[derive(Clone, Copy)]
pub(super) enum SkipDirection {
    Next,
    Previous,
}

impl SkipDirection {
    pub(super) fn label(self) -> &'static str {
        match self {
            SkipDirection::Next => "Next",
            SkipDirection::Previous => "Previous",
        }
    }
}

/// The now-playing values of `fixture`'s library, as a screen reads them.
pub(super) fn playback_values(
    fixture: &PlaybackTestFixture,
) -> bae_store::live_query::LiveValues<Option<bae_playback::NowPlayingValues>> {
    fixture.library.values_until_closed(fixture.library.subscribe_now_playing())
}
