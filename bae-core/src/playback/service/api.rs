use super::*;

use crate::util::worker_thread::WorkerThread;

/// What the playback service decides a side or disc pause with about a track:
/// the release and side it plays on, read from the library when the decision
/// is made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackTrackInfo {
    pub track_id: String,
    pub release_id: String,
    pub side: Option<PlaybackTrackSide>,
}

impl PlaybackTrackInfo {
    /// The facts of `track`, which plays on `release`.
    pub(crate) fn of(track: &crate::db::DbTrack, release: &crate::db::DbRelease) -> Self {
        let side = release
            .pressing
            .facts
            .physical_medium()
            .zip(track.side)
            .map(|(medium, number)| PlaybackTrackSide { medium, number });
        Self {
            track_id: track.id.clone(),
            release_id: release.id.clone(),
            side,
        }
    }
}

/// The side or disc a track is on, which decides where playback pauses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackTrackSide {
    pub medium: PhysicalMedium,
    pub number: i32,
}

impl PlayingTrack {
    pub(super) fn from_prepared(prepared: &PlaybackPreparedTrack) -> Self {
        Self {
            track_id: prepared.track_id.clone(),
            duration_ms: prepared.timeline.duration_ms(),
        }
    }
}

/// What the side/disc pause prompt shows: which boundary playback stopped at,
/// and whether the next side starts on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackSidePausePrompt {
    pub id: String,
    /// Whether a side or a disc ended; the prompt's wording follows it.
    pub boundary: PlaybackPauseBoundary,
    pub side_label: String,
    /// The countdown to the next side starting on its own, or `None` when the
    /// pause waits for Play.
    pub countdown: Option<PlaybackSideCountdown>,
}

/// The kind of boundary a pause between sides stopped at: a record's or a
/// cassette's side, or a CD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackPauseBoundary {
    Side,
    Disc,
}

/// A running side-pause countdown. Every UI counts down to `resumes_at`, so
/// they all show the same number; core decides when the next side starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackSideCountdown {
    pub resumes_at: chrono::DateTime<chrono::Utc>,
}

/// Why playback is paused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackPauseReason {
    Manual,
    SideEnded(PlaybackSidePausePrompt),
}

/// The side or disc boundary between two tracks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SideBoundary {
    pub(super) id: String,
    pub(super) kind: PlaybackPauseBoundary,
    pub(super) side_label: String,
}

/// A pause at a side boundary, resuming into `track_id` on Play or, when set,
/// on its own at `resumes_at`.
#[derive(Debug, Clone)]
pub(super) struct SidePauseDecision {
    pub(super) track_id: String,
    pub(super) boundary: SideBoundary,
    pub(super) resumes_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl SidePauseDecision {
    pub(super) fn prompt(&self) -> PlaybackSidePausePrompt {
        let SideBoundary {
            id,
            kind,
            side_label,
        } = &self.boundary;
        PlaybackSidePausePrompt {
            id: id.clone(),
            boundary: *kind,
            side_label: side_label.clone(),
            countdown: self
                .resumes_at
                .map(|resumes_at| PlaybackSideCountdown { resumes_at }),
        }
    }
}

#[derive(Debug)]
pub(crate) enum PlaybackCommand {
    Play(String),
    /// Play a release from `start_track_id`, or from its first track.
    PlayRelease {
        release_id: String,
        start_track_id: Option<String>,
        shuffle: bool,
    },
    /// Play several releases in order as one context, from the first track.
    /// Releases with no playable tracks are skipped, none left is a no-op, and a
    /// single one left plays exactly like `PlayRelease`.
    PlayReleases(Vec<String>),
    /// Play the whole library in a new shuffle; an empty library is a no-op.
    PlayLibraryShuffled,
    Pause,
    Resume,
    /// The prompt's Close: stop a running side-pause countdown and stay paused
    /// at the boundary until Play. A no-op when no countdown runs.
    CancelSidePauseCountdown,
    Stop,
    /// Manual next track (pregap skipped).
    Next,
    /// Advance after `track_id` played to its end, playing the next track's
    /// pregap. Dropped unless that track is still current and Completed, since a
    /// Next or Seek handled first has already moved on.
    AutoAdvance {
        track_id: String,
    },
    /// A load's decoder buffered enough to play (or hit the end). Resolves the
    /// track to its target phase only if `generation` is the live load: a repeat
    /// or re-Play reloads the same track id, so the id can't tell loads apart. The
    /// id is only for the log.
    TrackReady {
        track_id: String,
        generation: LoadGeneration,
    },
    /// A read failure mid-track emitted a `PlaybackProgress::PlaybackError`; stop
    /// playback rather than leave it frozen in Playing.
    HaltOnError,
    /// A track buffer's byte fill failed. Only the command loop knows which track
    /// the buffer serves right now, so it decides what breaks.
    ReadFailed {
        buffer_id: u64,
        error: Arc<PlaybackError>,
    },
    /// The system default output device changed: rebuild the output stream over
    /// the same source so playback follows it. macOS-only, since only CoreAudio
    /// reports the change; elsewhere the next stream rebuild picks up the device.
    #[cfg(target_os = "macos")]
    OutputDeviceChanged,
    Previous,
    /// Seek to this time in the current track, as the player shows it.
    Seek(TrackTime),
    /// Seek by slider ratio (0.0–1.0) of the current track's duration and pregap.
    SeekByRatio(f64),
    SetVolume(f32),
    AddToQueue(Vec<String>),
    AddNext(Vec<String>),
    AddReleaseToQueue(String),
    AddReleaseNext(String),
    InsertInQueue(Vec<String>, usize),
    /// Remove the queue entry with this per-instance id.
    RemoveFromQueue(QueueEntryId),
    /// Move `entry_id` to just before `before`, or to the end when it's `None`.
    ReorderQueue {
        entry_id: QueueEntryId,
        before: Option<QueueEntryId>,
    },
    /// Empty the manual lane, leaving the context lane playing.
    ClearUpNext,
    /// Drop the context lane. The playing track keeps playing; when it ends, Up
    /// Next drains and then playback stops.
    ClearPlayingFrom,
    SetRepeatMode(RepeatMode),
    /// Shuffle the context lane's upcoming rows with a new seed, or put them back
    /// in the order they had when shuffle turned on. The current track keeps
    /// playing.
    SetShuffle(bool),
    /// Sent when `pause_between_sides` turns on: staging is decided at preload
    /// time, so a next track already staged for gapless playback is held here if
    /// its boundary needs a pause.
    ReevaluateSidePauseStaging,
    /// Skip to the queue entry with this per-instance id (manual, pregap skipped).
    SkipTo(QueueEntryId),
    /// Preview a local source window (the same target stops; another switches).
    PreviewPlay(crate::playback::PreviewTarget),
    /// Stop any active preview.
    PreviewStop,
    /// Toggle pause/resume on the active preview.
    PreviewTogglePause,
    /// Seek by slider ratio (0.0–1.0) within the active preview.
    PreviewSeekByRatio(f64),
    /// The preview file finished playing naturally.
    PreviewCompleted,
    /// Muting saves the volume and drives output to 0; unmuting restores it.
    /// Setting the current state changes nothing.
    SetMuted(bool),
    GetVolume(oneshot::Sender<f32>),
    /// Test-only: replies with the queue once every earlier command has finished.
    #[cfg(any(test, feature = "test-utils"))]
    GetQueueProjection(oneshot::Sender<PlaybackQueueProjection>),
    /// Save state, reply, then stop the loop.
    Shutdown(oneshot::Sender<()>),
    /// Save state without stopping playback, for a mobile app going to the
    /// background, where `Shutdown` would stop its audio.
    SaveState(oneshot::Sender<()>),
    /// Stop local playback, keeping the queue, and send the current track to a
    /// remote renderer at its current position.
    PlayOn(Box<RemoteConnect>),
    /// Keep decoding locally and send the output to an AirPlay receiver.
    PlayOnAirPlay(Box<renderer::AirPlayConnect>),
    /// End the remote or AirPlay session and return to local playback, paused at
    /// the last position.
    StopRemote,
    /// A status update from the active remote session: drives progress, advances
    /// the queue when the device finishes a track, and notices a stop on the
    /// device. Ignored when playing locally (a late update from an ended session).
    RemoteStatus(crate::renderer::RendererSessionStatus),
}
/// Current playback state: the track and its phase. Position flows through
/// `PlaybackProgress::PositionUpdate` and `PlaybackProgress::Seeked` instead, so
/// the frequent position updates stay apart from this rarer event.
///
/// `Track` is what the state holds of its track: the service's
/// [`PlayingTrack`], or the [`NowPlayingTrack`](crate::playback::NowPlayingTrack)
/// surfaces show, which joins the library's current display onto it.
#[derive(Debug, Clone)]
pub enum PlaybackState<Track = PlayingTrack> {
    Stopped,
    Playing {
        track: Track,
    },
    Paused {
        track: Track,
        reason: PlaybackPauseReason,
    },
    Loading {
        track: LoadingTrack<Track>,
    },
}

/// The track a load is for: its id alone until the service has prepared it,
/// then the prepared track, which carries the id.
#[derive(Debug, Clone)]
pub enum LoadingTrack<Track = PlayingTrack> {
    Unprepared { track_id: String },
    Prepared(Track),
}

impl<Track> LoadingTrack<Track> {
    /// The same load holding `f`'s result for its prepared track, or `f`'s
    /// error.
    pub fn try_map_track<Mapped, Error>(
        self,
        f: impl FnOnce(Track) -> Result<Mapped, Error>,
    ) -> Result<LoadingTrack<Mapped>, Error> {
        Ok(match self {
            Self::Unprepared { track_id } => LoadingTrack::Unprepared { track_id },
            Self::Prepared(track) => LoadingTrack::Prepared(f(track)?),
        })
    }
}

impl LoadingTrack {
    /// The id of the track being loaded.
    pub fn track_id(&self) -> &str {
        match self {
            Self::Unprepared { track_id } => track_id,
            Self::Prepared(track) => &track.track_id,
        }
    }
}

impl<Track> PlaybackState<Track> {
    /// The same state holding `f`'s result for its track, or `f`'s error.
    pub fn try_map_track<Mapped, Error>(
        self,
        mut f: impl FnMut(Track) -> Result<Mapped, Error>,
    ) -> Result<PlaybackState<Mapped>, Error> {
        Ok(match self {
            Self::Stopped => PlaybackState::Stopped,
            Self::Playing { track } => PlaybackState::Playing { track: f(track)? },
            Self::Paused { track, reason } => PlaybackState::Paused {
                track: f(track)?,
                reason,
            },
            Self::Loading { track } => PlaybackState::Loading {
                track: track.try_map_track(f)?,
            },
        })
    }
}

impl PlaybackState {
    /// The id of the track the state names, or `None` when stopped.
    pub fn track_id(&self) -> Option<&str> {
        match self {
            Self::Stopped => None,
            Self::Loading { track } => Some(track.track_id()),
            Self::Playing { track } | Self::Paused { track, .. } => Some(&track.track_id),
        }
    }
}

/// Send a command to the playback service, warning if the service has shut
/// down.
pub(crate) fn dispatch_command(
    tx: &tokio_mpsc::UnboundedSender<PlaybackCommand>,
    cmd: PlaybackCommand,
) {
    if let Err(err) = tx.send(cmd) {
        warn!("playback command channel closed; dropped {:?}", err.0);
    }
}

/// Wait for the service to acknowledge shutdown, warning if it exited without
/// replying.
async fn await_shutdown_ack(rx: oneshot::Receiver<()>) {
    if let Err(err) = rx.await {
        warn!("playback service exited before acknowledging shutdown: {err}");
    }
}

/// Handle for sending commands to the playback service.
#[derive(Clone)]
pub struct PlaybackHandle {
    /// The service thread and its command channel. Teardown joins the thread so
    /// its `LibraryManager`, and with it the store's exclusive lock, is released
    /// before teardown returns; the service holds its own sender, so only
    /// `Shutdown` ends it. Clones share one take-once join handle.
    worker: WorkerThread<PlaybackCommand>,
    progress_handle: PlaybackProgressHandle,
    queue_values: tokio::sync::watch::Receiver<PlaybackQueueProjection>,
}
impl PlaybackHandle {
    pub(super) fn new(
        worker: WorkerThread<PlaybackCommand>,
        progress_handle: PlaybackProgressHandle,
        queue_values: tokio::sync::watch::Receiver<PlaybackQueueProjection>,
    ) -> Self {
        Self {
            worker,
            progress_handle,
            queue_values,
        }
    }

    /// Fire-and-forget; the service runs commands in order on its own thread.
    fn dispatch(&self, command: PlaybackCommand) {
        self.worker.dispatch(command);
    }

    pub fn play(&self, track_id: String) {
        self.dispatch(PlaybackCommand::Play(track_id));
    }
    pub fn play_release(&self, release_id: String, start_track_id: Option<String>, shuffle: bool) {
        self.dispatch(PlaybackCommand::PlayRelease {
            release_id,
            start_track_id,
            shuffle,
        });
    }
    pub fn play_releases(&self, release_ids: Vec<String>) {
        self.dispatch(PlaybackCommand::PlayReleases(release_ids));
    }
    pub fn play_library_shuffled(&self) {
        self.dispatch(PlaybackCommand::PlayLibraryShuffled);
    }
    pub fn pause(&self) {
        self.dispatch(PlaybackCommand::Pause);
    }
    pub fn resume(&self) {
        self.dispatch(PlaybackCommand::Resume);
    }
    /// Stop a running side-pause countdown, keeping playback paused at the
    /// boundary until Play.
    pub fn cancel_side_pause_countdown(&self) {
        self.dispatch(PlaybackCommand::CancelSidePauseCountdown);
    }
    pub fn stop(&self) {
        self.dispatch(PlaybackCommand::Stop);
    }
    pub fn next(&self) {
        self.dispatch(PlaybackCommand::Next);
    }
    pub fn previous(&self) {
        self.dispatch(PlaybackCommand::Previous);
    }
    /// Seek the current track to `time`, from its start (INDEX 01) as the
    /// player shows it; a negative time lands in its pregap.
    pub fn seek(&self, time: TrackTime) {
        self.dispatch(PlaybackCommand::Seek(time));
    }
    pub fn seek_by_ratio(&self, ratio: f64) {
        self.dispatch(PlaybackCommand::SeekByRatio(ratio));
    }
    pub fn set_volume(&self, volume: f32) {
        self.dispatch(PlaybackCommand::SetVolume(volume));
    }
    pub fn set_muted(&self, muted: bool) {
        self.dispatch(PlaybackCommand::SetMuted(muted));
    }
    /// Switch playback to a remote renderer over the already connected
    /// `channel`, serving each track's media through `media_source`.
    pub fn play_on(
        &self,
        channel: Box<dyn crate::renderer::RendererChannel>,
        device: crate::playback::RemoteDevice,
        media_source: crate::renderer::RendererMediaSource,
    ) {
        self.dispatch(PlaybackCommand::PlayOn(Box::new(RemoteConnect::new(
            channel,
            device,
            media_source,
        ))));
    }
    /// Switch playback to an AirPlay receiver through `sink`; decoding stays
    /// local.
    pub fn play_on_airplay(
        &self,
        sink: Box<dyn crate::playback::airplay_output::AirPlaySink>,
        device: crate::playback::RemoteDevice,
        latency_frames: u32,
    ) {
        self.dispatch(PlaybackCommand::PlayOnAirPlay(Box::new(
            renderer::AirPlayConnect::new(sink, device, latency_frames),
        )));
    }
    /// Stop remote or AirPlay playback and resume local playback, paused at the
    /// last position.
    pub fn stop_remote(&self) {
        self.dispatch(PlaybackCommand::StopRemote);
    }
    pub fn subscribe_progress(&self) -> tokio_mpsc::UnboundedReceiver<PlaybackProgress> {
        self.progress_handle.subscribe_all()
    }

    pub fn subscribe_values(
        &self,
    ) -> tokio::sync::watch::Receiver<crate::playback::progress::PlaybackValues> {
        self.progress_handle.subscribe_values()
    }

    pub fn subscribe_queue_values(&self) -> tokio::sync::watch::Receiver<PlaybackQueueProjection> {
        self.queue_values.clone()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn queue_projection(&self) -> Result<PlaybackQueueProjection, String> {
        let (tx, rx) = oneshot::channel();
        self.dispatch(PlaybackCommand::GetQueueProjection(tx));
        rx.await
            .map_err(|e| format!("playback loop dropped the queue response channel: {e}"))
    }
    pub fn add_to_queue(&self, track_ids: Vec<String>) {
        self.dispatch(PlaybackCommand::AddToQueue(track_ids));
    }
    pub fn add_next(&self, track_ids: Vec<String>) {
        self.dispatch(PlaybackCommand::AddNext(track_ids));
    }
    pub fn add_release_to_queue(&self, release_id: String) {
        self.dispatch(PlaybackCommand::AddReleaseToQueue(release_id));
    }
    pub fn add_release_next(&self, release_id: String) {
        self.dispatch(PlaybackCommand::AddReleaseNext(release_id));
    }
    pub fn insert_in_queue(&self, track_ids: Vec<String>, index: usize) {
        self.dispatch(PlaybackCommand::InsertInQueue(track_ids, index));
    }
    pub fn remove_entry(&self, entry_id: QueueEntryId) {
        self.dispatch(PlaybackCommand::RemoveFromQueue(entry_id));
    }
    pub fn reorder_entry(&self, entry_id: QueueEntryId, before: Option<QueueEntryId>) {
        self.dispatch(PlaybackCommand::ReorderQueue { entry_id, before });
    }
    pub fn clear_up_next(&self) {
        self.dispatch(PlaybackCommand::ClearUpNext);
    }
    pub fn clear_playing_from(&self) {
        self.dispatch(PlaybackCommand::ClearPlayingFrom);
    }
    pub fn set_repeat_mode(&self, mode: RepeatMode) {
        self.dispatch(PlaybackCommand::SetRepeatMode(mode));
    }

    pub fn set_shuffle(&self, on: bool) {
        self.dispatch(PlaybackCommand::SetShuffle(on));
    }

    pub async fn get_volume(&self) -> f32 {
        let (tx, rx) = oneshot::channel();
        self.dispatch(PlaybackCommand::GetVolume(tx));
        rx.await.unwrap_or_else(|e| {
            warn!("get_volume: playback loop dropped the response channel: {e}");
            1.0
        })
    }

    /// Save playback state, stop the service, and join its thread so the store
    /// lock is released before this returns. Waits for the save, which the
    /// platform's quit path relies on. After this or [`Self::stop_and_join`], a
    /// second teardown does nothing.
    pub async fn shutdown(&self) {
        self.worker
            .stop_and_join_async(|command_tx| {
                let (tx, rx) = oneshot::channel();
                dispatch_command(command_tx, PlaybackCommand::Shutdown(tx));
                await_shutdown_ack(rx)
            })
            .await;
    }

    /// Teardown for `Drop`, needing no async runtime: stop the service and join
    /// its thread, which saves state before it exits and releases the store lock.
    pub fn stop_and_join(&self) {
        self.worker.stop_and_join(|command_tx| {
            let (tx, _rx) = oneshot::channel();
            dispatch_command(command_tx, PlaybackCommand::Shutdown(tx));
        });
    }

    /// Save playback state without stopping playback, for a mobile app going to
    /// the background, and wait until it's written before the OS suspends it.
    pub async fn save_state(&self) {
        let (tx, rx) = oneshot::channel();
        self.dispatch(PlaybackCommand::SaveState(tx));
        let _ = rx.await;
    }

    pub fn skip_to_entry(&self, entry_id: QueueEntryId) {
        self.dispatch(PlaybackCommand::SkipTo(entry_id));
    }
    /// Called when `pause_between_sides` turns on, so a next track already staged
    /// for gapless playback is held if its boundary needs a pause.
    pub fn reevaluate_side_pause_staging(&self) {
        self.dispatch(PlaybackCommand::ReevaluateSidePauseStaging);
    }
    /// Preview a local source window. The same target stops; another switches.
    pub fn preview_play(&self, target: crate::playback::PreviewTarget) {
        self.dispatch(PlaybackCommand::PreviewPlay(target));
    }
    /// Stop any active preview.
    pub fn preview_stop(&self) {
        self.dispatch(PlaybackCommand::PreviewStop);
    }
    /// Toggle pause/resume on the active preview.
    pub fn preview_toggle_pause(&self) {
        self.dispatch(PlaybackCommand::PreviewTogglePause);
    }
    /// Seek by slider ratio (0.0–1.0) within the active preview.
    pub fn preview_seek_by_ratio(&self, ratio: f64) {
        self.dispatch(PlaybackCommand::PreviewSeekByRatio(ratio));
    }
}
