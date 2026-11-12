use super::*;

use bae_util::worker_thread::{WorkerEnd, WorkerThread};

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
    /// The service that held the play ended, or the fresh one the play
    /// started did, before answering; that end was told to whoever follows
    /// playback.
    #[error("the playback service stopped before it answered the play")]
    Stopped(#[source] oneshot::error::RecvError),
    /// The service's thread had ended before the play was sent: shut down,
    /// or ended on a bug reported as it happened.
    #[error("the playback service had ended")]
    Ended(#[source] WorkerEnd),
}

/// Where a play command's answer goes.
pub(crate) type PlayReply = oneshot::Sender<Result<PlayOutcome, PlayError>>;

/// Why a queue command failed. A command naming an entry the queue no longer
/// holds is not a failure: the queue changed under the screen that asked, which
/// shows the queue as it now is.
#[derive(Debug, thiserror::Error)]
pub enum QueueEditError {
    #[error("couldn't change the queue: {0}")]
    Queue(#[source] bae_store::DbError),
    /// The service that held the command ended, or the fresh one the command
    /// started did, before answering; that end was told to whoever follows
    /// playback.
    #[error("the playback service stopped before it answered the queue command")]
    Stopped(#[source] oneshot::error::RecvError),
    /// The service's thread had ended before the command was sent: shut
    /// down, or ended on a bug reported as it happened.
    #[error("the playback service had ended")]
    Ended(#[source] WorkerEnd),
}

impl QueueEditError {
    /// The class a surface words this failure in: a database failure, or a
    /// service that had stopped or ended, which is a bug: of whoever still
    /// sent it the command, or the one its thread ended on.
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        use bae_diagnostics::UiErrorCategory as C;
        match self {
            Self::Queue(error) => bae_diagnostics::database_category(error),
            Self::Stopped(_) | Self::Ended(_) => C::Internal,
        }
    }
}

/// Where a queue command's answer goes: what the command gives back (a
/// reorder, the lane's version the move made, or nothing when it moved
/// nothing).
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
    /// Play tracks as one context, in the order given from the one at an
    /// index, or shuffled. Tracks the library does not hold are left out and
    /// named in the answer; a start index past the list or naming such a track
    /// plays nothing, and so does a shuffled list holding none.
    PlayTracks {
        track_ids: Vec<String>,
        start: TracksStart,
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
    /// The playing track's decoder failed for `message`. The loop tells the UI
    /// and, hearing itself tell it, halts playback. Sent as a service's own
    /// command, so a decoder left from a dead service is not heard by the next.
    DecodeFailed {
        message: String,
    },
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
    /// Move the output to this level while a slider is dragged: nothing is
    /// announced or stored until the drag's final `SetVolume`.
    SetVolumeLive(crate::Volume),
    /// Add tracks to the end of Up Next.
    AddToQueue(Vec<String>, QueueEditReply<()>),
    /// Add tracks to the front of Up Next.
    AddNext(Vec<String>, QueueEditReply<()>),
    /// Add a release's tracks to the end of Up Next.
    AddReleaseToQueue(String, QueueEditReply<()>),
    /// Add a release's tracks to the front of Up Next.
    AddReleaseNext(String, QueueEditReply<()>),
    /// Insert tracks into Up Next before the entry at the index.
    InsertInQueue(Vec<String>, usize, QueueEditReply<()>),
    /// Remove the queue row with this id.
    RemoveFromQueue {
        entry_id: QueueEntryId,
        reply: QueueEditReply<()>,
    },
    /// Move `entry_id` to just before `before`, or to the end when it's `None`.
    ReorderQueue {
        entry_id: QueueEntryId,
        before: Option<QueueEntryId>,
        reply: QueueEditReply<Option<u64>>,
    },
    /// Empty the manual lane, leaving the context lane playing.
    ClearUpNext(QueueEditReply<()>),
    /// Drop the context lane. The playing track keeps playing; when it ends, Up
    /// Next drains and then playback stops.
    ClearPlayingFrom(QueueEditReply<()>),
    SetRepeatMode(RepeatMode, QueueEditReply<()>),
    /// Shuffle the context lane's upcoming rows with a new seed, or put them back
    /// in the order they had when shuffle turned on. The current track keeps
    /// playing.
    SetShuffle(bool, QueueEditReply<()>),
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
    /// Test-only: replies once every earlier command has finished.
    #[cfg(any(test, feature = "test-utils"))]
    Handled(oneshot::Sender<()>),
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

/// Ask `thread` to shut down, warning if it already has.
fn ask_to_shut_down(
    thread: &tokio_mpsc::UnboundedSender<PlaybackMessage>,
    reply: oneshot::Sender<()>,
) {
    if let Err(unsent) = thread.send(PlaybackMessage::Asked(PlaybackCommand::Shutdown(reply))) {
        warn!("playback command channel closed; dropped {:?}", unsent.0);
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
    /// The playback thread and its channel. Teardown joins the thread so its
    /// `Database`, and with it the store's exclusive lock, is released before
    /// teardown returns; the thread holds its own sender, so only `Shutdown`
    /// ends it. Clones share one take-once join handle.
    worker: WorkerThread<PlaybackMessage>,
    progress_handle: PlaybackProgressHandle,
    /// The arbiter the service's reads run under, which hears whether the
    /// playing track's load waits on storage.
    arbiter: std::sync::Arc<bae_track_audio::data_source::FetchArbiter>,
    playing_bytes: tokio::sync::watch::Receiver<Option<crate::track_buffered::PlayingBytes>>,
}
impl PlaybackHandle {
    pub(super) fn new(
        worker: WorkerThread<PlaybackMessage>,
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
        self.worker.dispatch(PlaybackMessage::Asked(command));
    }

    /// Send `command`, or say how the thread ended.
    fn send(&self, command: PlaybackCommand) -> Result<(), WorkerEnd> {
        self.worker.send(PlaybackMessage::Asked(command)).map_err(|ended| ended.end)
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
        self.send(command(reply)).map_err(PlayError::Ended)?;
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
    /// Play `track_ids` from `start`, and say what it did.
    pub async fn play_tracks(
        &self,
        track_ids: Vec<String>,
        start: TracksStart,
    ) -> Result<PlayOutcome, PlayError> {
        self.play_answered(|reply| PlaybackCommand::PlayTracks { track_ids, start, reply }).await
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
    /// Move the output to `volume` as a slider is dragged, announcing nothing
    /// and storing nothing; [`Self::set_volume`] with the level the drag ends
    /// on does both. A level that is no number sets nothing.
    pub fn set_volume_live(&self, volume: f32) {
        match crate::Volume::new(volume) {
            Some(volume) => self.dispatch(PlaybackCommand::SetVolumeLive(volume)),
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
        device: crate::renderer::CastReceiver,
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
        device: crate::renderer::CastReceiver,
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

    /// Wait until playback has handled every command sent before this call,
    /// and every audio event its output reported before then.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn commands_handled(&self) {
        let (tx, rx) = oneshot::channel();
        self.dispatch(PlaybackCommand::Handled(tx));
        rx.await.expect("the playback loop answers while it runs");
    }

    #[cfg(any(test, feature = "test-utils"))]
    async fn queue_read(
        &self,
    ) -> Result<(crate::ResolvedQueueSnapshot, Vec<crate::QueueItem>), String> {
        let (tx, rx) = oneshot::channel();
        self.dispatch(PlaybackCommand::GetQueue(tx));
        rx.await.map_err(|e| format!("playback loop dropped the queue response channel: {e}"))?
    }
    /// Add tracks to the end of Up Next, answering once they are in or why
    /// they are not. The command is sent at the call, in order with the
    /// commands sent before and after it.
    pub fn add_to_queue(
        &self,
        track_ids: Vec<String>,
    ) -> impl std::future::Future<Output = Result<(), QueueEditError>> + Send + 'static {
        let (reply, answer) = oneshot::channel();
        self.edit_queue(PlaybackCommand::AddToQueue(track_ids, reply), answer)
    }
    /// Add tracks to the front of Up Next, answering once they are in or why
    /// they are not.
    pub fn add_next(
        &self,
        track_ids: Vec<String>,
    ) -> impl std::future::Future<Output = Result<(), QueueEditError>> + Send + 'static {
        let (reply, answer) = oneshot::channel();
        self.edit_queue(PlaybackCommand::AddNext(track_ids, reply), answer)
    }
    /// Add a release's tracks to the end of Up Next, answering once they are
    /// in or why they are not.
    pub fn add_release_to_queue(
        &self,
        release_id: String,
    ) -> impl std::future::Future<Output = Result<(), QueueEditError>> + Send + 'static {
        let (reply, answer) = oneshot::channel();
        self.edit_queue(PlaybackCommand::AddReleaseToQueue(release_id, reply), answer)
    }
    /// Add a release's tracks to the front of Up Next, answering once they are
    /// in or why they are not.
    pub fn add_release_next(
        &self,
        release_id: String,
    ) -> impl std::future::Future<Output = Result<(), QueueEditError>> + Send + 'static {
        let (reply, answer) = oneshot::channel();
        self.edit_queue(PlaybackCommand::AddReleaseNext(release_id, reply), answer)
    }
    /// Insert tracks into Up Next before the entry at `index`, answering once
    /// they are in or why they are not.
    pub fn insert_in_queue(
        &self,
        track_ids: Vec<String>,
        index: usize,
    ) -> impl std::future::Future<Output = Result<(), QueueEditError>> + Send + 'static {
        let (reply, answer) = oneshot::channel();
        self.edit_queue(PlaybackCommand::InsertInQueue(track_ids, index, reply), answer)
    }
    /// Remove the queue row with this id. The command is sent at the call, in
    /// order with the commands sent before and after it; the answer is the
    /// future's. A row no longer queued is already gone: the answer is `Ok`.
    pub fn remove_entry(
        &self,
        entry_id: QueueEntryId,
    ) -> impl std::future::Future<Output = Result<(), QueueEditError>> + Send + 'static {
        let (reply, answer) = oneshot::channel();
        self.edit_queue(PlaybackCommand::RemoveFromQueue { entry_id, reply }, answer)
    }
    /// Move `entry_id` to just before `before`, or to the end of its lane
    /// when it's `None`, and answer the lane's version the move made: a
    /// screen holding the moved row shows the tables' order again once the
    /// lane's read reaches it. Answers `None` when the entry, or the one it
    /// goes before, is no longer queued there, or the two are in different
    /// lanes: nothing moved.
    ///
    /// The command is sent at the call, in order with the commands sent
    /// before and after it; the answer is the future's.
    pub fn reorder_entry(
        &self,
        entry_id: QueueEntryId,
        before: Option<QueueEntryId>,
    ) -> impl std::future::Future<Output = Result<Option<u64>, QueueEditError>> + Send + 'static
    {
        let (reply, answer) = oneshot::channel();
        self.edit_queue(PlaybackCommand::ReorderQueue { entry_id, before, reply }, answer)
    }
    /// Empty the manual lane, answering once it is empty or why it isn't.
    pub fn clear_up_next(
        &self,
    ) -> impl std::future::Future<Output = Result<(), QueueEditError>> + Send + 'static {
        let (reply, answer) = oneshot::channel();
        self.edit_queue(PlaybackCommand::ClearUpNext(reply), answer)
    }
    /// Drop the context lane, answering once it is dropped or why it isn't.
    pub fn clear_playing_from(
        &self,
    ) -> impl std::future::Future<Output = Result<(), QueueEditError>> + Send + 'static {
        let (reply, answer) = oneshot::channel();
        self.edit_queue(PlaybackCommand::ClearPlayingFrom(reply), answer)
    }
    /// Send the queue command `command` at the call; the future answers
    /// with what it did, or why it didn't.
    fn edit_queue<T: Send + 'static>(
        &self,
        command: PlaybackCommand,
        answer: oneshot::Receiver<Result<T, QueueEditError>>,
    ) -> impl std::future::Future<Output = Result<T, QueueEditError>> + Send + 'static {
        let sent = self.send(command).map_err(QueueEditError::Ended);
        async move {
            sent?;
            answer.await.map_err(QueueEditError::Stopped)?
        }
    }
    /// Set how the queue repeats, answering once it is set or why it is not.
    pub fn set_repeat_mode(
        &self,
        mode: RepeatMode,
    ) -> impl std::future::Future<Output = Result<(), QueueEditError>> + Send + 'static {
        let (reply, answer) = oneshot::channel();
        self.edit_queue(PlaybackCommand::SetRepeatMode(mode, reply), answer)
    }

    /// Shuffle the context lane or put it back in order, answering once it is
    /// done or why it is not.
    pub fn set_shuffle(
        &self,
        on: bool,
    ) -> impl std::future::Future<Output = Result<(), QueueEditError>> + Send + 'static {
        let (reply, answer) = oneshot::channel();
        self.edit_queue(PlaybackCommand::SetShuffle(on, reply), answer)
    }

    /// Save playback state, stop the service, and join its thread so the store
    /// lock is released before this returns. Waits for the save, which the
    /// platform's quit path relies on. After this or [`Self::stop_and_join`], a
    /// second teardown does nothing.
    pub async fn shutdown(&self) {
        self.worker
            .stop_and_join_async(|thread| {
                let (tx, rx) = oneshot::channel();
                ask_to_shut_down(thread, tx);
                await_shutdown_ack(rx)
            })
            .await;
    }

    /// Teardown for `Drop`, needing no async runtime: stop the service and join
    /// its thread, which saves state before it exits and releases the store lock.
    pub fn stop_and_join(&self) {
        self.worker.stop_and_join(|thread| {
            let (tx, _rx) = oneshot::channel();
            ask_to_shut_down(thread, tx);
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
        self.edit_queue(PlaybackCommand::SkipTo { entry_id, reply }, answer)
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
