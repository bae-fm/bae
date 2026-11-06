use super::*;

use bae_util::worker_thread::WorkerThread;

impl PlayingTrack {
    pub(super) fn from_prepared(prepared: &PlaybackPreparedTrack) -> Self {
        Self { track_id: prepared.track_id.clone(), duration_ms: prepared.timeline.duration_ms() }
    }
}

/// Why playback is paused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackPauseReason {
    Manual,
    SideEnded(PlaybackSidePausePrompt),
}

/// What a play command did, as its caller is told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayOutcome {
    /// Playing everything the play named.
    Playing,
    /// Playing, without the releases named that hold no tracks, listed in the
    /// order named.
    PlayingWithoutReleases(Vec<String>),
    /// Playing, without the tracks of the list that the library doesn't hold,
    /// listed in the list's order.
    PlayingWithoutTracks(Vec<String>),
    /// What the play named holds no tracks: a release without any, releases
    /// none of which has any, an empty library or an empty list.
    NothingToPlay,
    /// The track to start from is gone: no longer on the release, or no longer
    /// in the library, or the list has no track at the start index.
    StartMissing,
}

/// Why a play command has no outcome to tell.
#[derive(Debug, thiserror::Error)]
pub enum PlayError {
    #[error("couldn't fill the queue: {0}")]
    Queue(#[source] bae_store::DbError),
    #[error("the playback service stopped before it answered the play")]
    Stopped(#[source] oneshot::error::RecvError),
}

/// Where a play command's answer goes.
pub(crate) type PlayReply = oneshot::Sender<Result<PlayOutcome, PlayError>>;

/// Why a queue command that names an entry, or empties a part of the queue,
/// changed nothing.
#[derive(Debug, thiserror::Error)]
pub enum QueueEditError {
    /// The entry, or the one it goes before, is no longer queued, or the two
    /// are in different lanes: the queue changed under the screen that asked.
    #[error("the queue changed before the command, so nothing changed")]
    QueueChanged,
    #[error("couldn't change the queue: {0}")]
    Queue(#[source] bae_store::DbError),
    #[error("the playback service stopped before it answered the queue command")]
    Stopped(#[source] oneshot::error::RecvError),
}

impl QueueEditError {
    /// The class a surface words this failure in: the queue changed under
    /// the screen that asked, a database failure, or a service that had
    /// stopped, which is a bug of whoever still sent it the command.
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        use bae_diagnostics::UiErrorCategory as C;
        match self {
            Self::QueueChanged => C::QueueChanged,
            Self::Queue(_) => C::Database,
            Self::Stopped(_) => C::Internal,
        }
    }
}

/// Where a queue command's answer goes: what the command gives back (a
/// reorder, the lane's version the move made).
pub(crate) type QueueEditReply<T> = oneshot::Sender<Result<T, QueueEditError>>;

#[derive(Debug)]
pub(crate) enum PlaybackCommand {
    Play(String),
    /// Play a release from `start_track_id`, or from its first track.
    PlayRelease {
        release_id: String,
        start_track_id: Option<String>,
        shuffle: bool,
        reply: PlayReply,
    },
    /// Play several releases in order as one context, from the first track.
    /// Releases with no tracks are left out and named in the answer, none left
    /// plays nothing, and a single one left plays exactly like `PlayRelease`.
    PlayReleases {
        release_ids: Vec<String>,
        reply: PlayReply,
    },
    /// Play tracks in the order given as one context, from the one at
    /// `start_index`. Tracks the library does not hold are left out and named
    /// in the answer; a start index past the list or naming such a track plays
    /// nothing.
    PlayTracks {
        track_ids: Vec<String>,
        start_index: usize,
        reply: PlayReply,
    },
    /// Play the whole library in a new shuffle; an empty library plays nothing.
    PlayLibraryShuffled(PlayReply),
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
        error: Arc<bae_track_audio::TrackReadError>,
    },
    Previous,
    /// Seek to this time in the current track, as the player shows it.
    Seek(TrackTime),
    /// Seek by slider ratio (0.0–1.0) of the current track's duration and pregap.
    SeekByRatio(f64),
    SetVolume(crate::Volume),
    AddToQueue(Vec<String>),
    AddNext(Vec<String>),
    AddReleaseToQueue(String),
    AddReleaseNext(String),
    InsertInQueue(Vec<String>, usize),
    /// Remove the queue row with this id.
    RemoveFromQueue {
        entry_id: QueueEntryId,
        reply: QueueEditReply<()>,
    },
    /// Move `entry_id` to just before `before`, or to the end when it's `None`.
    ReorderQueue {
        entry_id: QueueEntryId,
        before: Option<QueueEntryId>,
        reply: QueueEditReply<u64>,
    },
    /// Empty the manual lane, leaving the context lane playing.
    ClearUpNext(QueueEditReply<()>),
    /// Drop the context lane. The playing track keeps playing; when it ends, Up
    /// Next drains and then playback stops.
    ClearPlayingFrom(QueueEditReply<()>),
    SetRepeatMode(RepeatMode),
    /// Shuffle the context lane's upcoming rows with a new seed, or put them back
    /// in the order they had when shuffle turned on. The current track keeps
    /// playing.
    SetShuffle(bool),
    /// Sent when `pause_between_sides` turns on: staging is decided at preload
    /// time, so a next track already staged for gapless playback is held here if
    /// its boundary needs a pause.
    ReevaluateSidePauseStaging,
    /// Skip to the queue row with this id (manual, pregap skipped).
    SkipTo {
        entry_id: QueueEntryId,
        reply: QueueEditReply<()>,
    },
    /// Preview a local source window (the same target stops; another switches).
    PreviewPlay(bae_track_audio::PreviewTarget),
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
    /// Test-only: replies with the queue once every earlier command has finished.
    #[cfg(any(test, feature = "test-utils"))]
    GetQueue(
        oneshot::Sender<Result<(crate::ResolvedQueueSnapshot, Vec<crate::QueueItem>), String>>,
    ),
    /// Test-only: replies with every fill started so far once every earlier
    /// command has finished.
    #[cfg(any(test, feature = "test-utils"))]
    TakeFills(oneshot::Sender<Vec<bae_track_audio::data_source::FillTask>>),
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
    /// A stream to an AirPlay receiver failed for good. The live session's
    /// failure says why; a report from a session no longer live finds none and
    /// changes nothing.
    AirPlayStreamFailed,
    /// A stream's device reported the stream can't go on. The streams' own
    /// reports say which; a stream since replaced took its report with it.
    OutputInvalidated,
    /// An event from the active remote session: a status drives progress and
    /// advances the queue when the device finishes a track, a refused command
    /// is reported, and the session's end returns playback here with why.
    /// Ignored when playing locally (a late event from an ended session).
    RemoteEvent(crate::renderer::RendererSessionEvent),
}
/// Current playback state: the track and its phase. Position flows through
/// `PlaybackProgress::PositionUpdate` and `PlaybackProgress::Seeked` instead, so
/// the frequent position updates stay apart from this rarer event.
///
/// `Track` is what the state holds of its track: the service's
/// [`PlayingTrack`], or the [`NowPlayingTrack`](crate::NowPlayingTrack)
/// surfaces show, which joins the library's current display onto it.
#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackState<Track = PlayingTrack> {
    Stopped,
    Playing { track: Track },
    Paused { track: Track, reason: PlaybackPauseReason },
    Loading { track: LoadingTrack<Track> },
}

/// The track a load is for: its id alone until the service has prepared it,
/// then the prepared track, which carries the id.
#[derive(Debug, Clone, PartialEq)]
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
            Self::Paused { track, reason } => PlaybackState::Paused { track: f(track)?, reason },
            Self::Loading { track } => PlaybackState::Loading { track: track.try_map_track(f)? },
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
    /// its `Database`, and with it the store's exclusive lock, is released
    /// before teardown returns; the service holds its own sender, so only
    /// `Shutdown` ends it. Clones share one take-once join handle.
    worker: WorkerThread<PlaybackCommand>,
    progress_handle: PlaybackProgressHandle,
    /// The arbiter the service's reads run under, which hears whether the
    /// playing track's load waits on storage.
    arbiter: std::sync::Arc<bae_track_audio::data_source::FetchArbiter>,
    playing_bytes: tokio::sync::watch::Receiver<Option<crate::track_buffered::PlayingBytes>>,
}
impl PlaybackHandle {
    pub(super) fn new(
        worker: WorkerThread<PlaybackCommand>,
        progress_handle: PlaybackProgressHandle,
        arbiter: std::sync::Arc<bae_track_audio::data_source::FetchArbiter>,
        playing_bytes: tokio::sync::watch::Receiver<Option<crate::track_buffered::PlayingBytes>>,
    ) -> Self {
        Self { worker, progress_handle, arbiter, playing_bytes }
    }

    /// The playing track while the person waits on its load and the load
    /// reads from storage, by id; `None` otherwise.
    pub fn subscribe_storage_waits(&self) -> bae_track_audio::StorageWaitWatch {
        self.arbiter.subscribe_storage_waits()
    }

    /// The parts of the playing track whose bytes this device holds, as
    /// they change.
    pub fn follow_track_buffered(&self) -> crate::TrackBufferedFollow {
        crate::TrackBufferedFollow::new(self.playing_bytes.clone())
    }

    /// Fire-and-forget; the service runs commands in order on its own thread.
    fn dispatch(&self, command: PlaybackCommand) {
        self.worker.dispatch(command);
    }

    pub fn play(&self, track_id: String) {
        self.dispatch(PlaybackCommand::Play(track_id));
    }
    /// Send the play `command` builds around its reply, and wait for what it
    /// did. Commands run in order, so every one sent before has run.
    async fn play_answered(
        &self,
        command: impl FnOnce(PlayReply) -> PlaybackCommand,
    ) -> Result<PlayOutcome, PlayError> {
        let (reply, answer) = oneshot::channel();
        self.dispatch(command(reply));
        answer.await.map_err(PlayError::Stopped)?
    }
    /// Play a release from `start_track_id`, or from its first track, or
    /// shuffled, and say what it did.
    pub async fn play_release(
        &self,
        release_id: String,
        start_track_id: Option<String>,
        shuffle: bool,
    ) -> Result<PlayOutcome, PlayError> {
        self.play_answered(|reply| PlaybackCommand::PlayRelease {
            release_id,
            start_track_id,
            shuffle,
            reply,
        })
        .await
    }
    /// Play several releases one after another, and say what it did.
    pub async fn play_releases(&self, release_ids: Vec<String>) -> Result<PlayOutcome, PlayError> {
        self.play_answered(|reply| PlaybackCommand::PlayReleases { release_ids, reply }).await
    }
    /// Play `track_ids` in that order from the one at `start_index`, and say
    /// what it did.
    pub async fn play_tracks(
        &self,
        track_ids: Vec<String>,
        start_index: usize,
    ) -> Result<PlayOutcome, PlayError> {
        self.play_answered(|reply| PlaybackCommand::PlayTracks { track_ids, start_index, reply })
            .await
    }
    /// Play the whole library in a new shuffle, and say what it did.
    pub async fn play_library_shuffled(&self) -> Result<PlayOutcome, PlayError> {
        self.play_answered(PlaybackCommand::PlayLibraryShuffled).await
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
    /// Set the level to `volume`, held to silent and full; a level that is
    /// no number sets nothing.
    pub fn set_volume(&self, volume: f32) {
        match crate::Volume::new(volume) {
            Some(volume) => self.dispatch(PlaybackCommand::SetVolume(volume)),
            None => warn!(volume, "a volume that is no number sets nothing"),
        }
    }
    pub fn set_muted(&self, muted: bool) {
        self.dispatch(PlaybackCommand::SetMuted(muted));
    }
    /// Switch playback to a remote renderer over the already connected
    /// `channel`, serving each track's media through `media_source`.
    pub fn play_on(
        &self,
        channel: Box<dyn crate::renderer::RendererChannel>,
        device: crate::RemoteDevice,
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
        sink: Box<dyn crate::airplay_output::AirPlaySink>,
        device: crate::RemoteDevice,
    ) {
        self.dispatch(PlaybackCommand::PlayOnAirPlay(Box::new(renderer::AirPlayConnect::new(
            sink, device,
        ))));
    }
    /// Stop remote or AirPlay playback and resume local playback, paused at the
    /// last position.
    pub fn stop_remote(&self) {
        self.dispatch(PlaybackCommand::StopRemote);
    }
    pub fn subscribe_progress(&self) -> crate::PlaybackProgressFeed {
        self.progress_handle.subscribe_all()
    }

    pub fn subscribe_positions(&self) -> crate::PlaybackPositionsWatch {
        self.progress_handle.subscribe_positions()
    }

    /// The queue as its screens show it, read after every command sent before
    /// this one has changed it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn queue_value(&self) -> Result<crate::ResolvedQueueSnapshot, String> {
        Ok(self.queue_read().await?.0)
    }

    /// Every one of the context lane's tracks after the playing one, read
    /// after every command sent before this one has changed the queue.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn queue_upcoming(&self) -> Result<Vec<crate::QueueItem>, String> {
        Ok(self.queue_read().await?.1)
    }

    /// Wait until every file fill playback has started, up to the commands
    /// sent before this call, has ended and closed its file. A fill ends once
    /// its buffer is cancelled, which stopping playback does.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn fills_ended(&self) {
        let (tx, rx) = oneshot::channel();
        self.dispatch(PlaybackCommand::TakeFills(tx));
        let fills = rx.await.expect("the playback loop answers while it runs");
        for fill in fills {
            fill.ended().await;
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    async fn queue_read(
        &self,
    ) -> Result<(crate::ResolvedQueueSnapshot, Vec<crate::QueueItem>), String> {
        let (tx, rx) = oneshot::channel();
        self.dispatch(PlaybackCommand::GetQueue(tx));
        rx.await.map_err(|e| format!("playback loop dropped the queue response channel: {e}"))?
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
    /// Remove the queue row with this id. The command is sent at the call, in
    /// order with the commands sent before and after it; the answer is the
    /// future's: that the queue changed when the row is no longer queued.
    pub fn remove_entry(
        &self,
        entry_id: QueueEntryId,
    ) -> impl std::future::Future<Output = Result<(), QueueEditError>> + Send + 'static {
        let (reply, answer) = oneshot::channel();
        self.dispatch(PlaybackCommand::RemoveFromQueue { entry_id, reply });
        async move { answer.await.map_err(QueueEditError::Stopped)? }
    }
    /// Move `entry_id` to just before `before`, or to the end of its lane
    /// when it's `None`, and answer the lane's version the move made: a
    /// screen holding the moved row shows the tables' order again once the
    /// lane's read reaches it.
    ///
    /// The command is sent at the call, in order with the commands sent
    /// before and after it; the answer is the future's.
    pub fn reorder_entry(
        &self,
        entry_id: QueueEntryId,
        before: Option<QueueEntryId>,
    ) -> impl std::future::Future<Output = Result<u64, QueueEditError>> + Send + 'static {
        let (reply, answer) = oneshot::channel();
        self.dispatch(PlaybackCommand::ReorderQueue { entry_id, before, reply });
        async move { answer.await.map_err(QueueEditError::Stopped)? }
    }
    /// Empty the manual lane, answering once it is empty or why it isn't.
    pub fn clear_up_next(
        &self,
    ) -> impl std::future::Future<Output = Result<(), QueueEditError>> + Send + 'static {
        let (reply, answer) = oneshot::channel();
        self.dispatch(PlaybackCommand::ClearUpNext(reply));
        async move { answer.await.map_err(QueueEditError::Stopped)? }
    }
    /// Drop the context lane, answering once it is dropped or why it isn't.
    pub fn clear_playing_from(
        &self,
    ) -> impl std::future::Future<Output = Result<(), QueueEditError>> + Send + 'static {
        let (reply, answer) = oneshot::channel();
        self.dispatch(PlaybackCommand::ClearPlayingFrom(reply));
        async move { answer.await.map_err(QueueEditError::Stopped)? }
    }
    pub fn set_repeat_mode(&self, mode: RepeatMode) {
        self.dispatch(PlaybackCommand::SetRepeatMode(mode));
    }

    pub fn set_shuffle(&self, on: bool) {
        self.dispatch(PlaybackCommand::SetShuffle(on));
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
        if rx.await.is_err() {
            debug!("playback service stopped before saving its state");
        }
    }

    /// Skip to the queue row with this id, answering once its track is
    /// loading, or that the queue changed when the row is no longer queued.
    pub fn skip_to_entry(
        &self,
        entry_id: QueueEntryId,
    ) -> impl std::future::Future<Output = Result<(), QueueEditError>> + Send + 'static {
        let (reply, answer) = oneshot::channel();
        self.dispatch(PlaybackCommand::SkipTo { entry_id, reply });
        async move { answer.await.map_err(QueueEditError::Stopped)? }
    }
    /// Called when `pause_between_sides` turns on, so a next track already staged
    /// for gapless playback is held if its boundary needs a pause.
    pub fn reevaluate_side_pause_staging(&self) {
        self.dispatch(PlaybackCommand::ReevaluateSidePauseStaging);
    }
    /// Preview a local source window. The same target stops; another switches.
    pub fn preview_play(&self, target: bae_track_audio::PreviewTarget) {
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
