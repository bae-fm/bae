use super::*;

/// Track metadata resolved once at prepare time and held for the track's
/// playback, so `PlaybackState` emissions carry it and the bridge needs no DB
/// access.
#[derive(Debug, Clone)]
pub struct PlaybackTrackInfo {
    pub track_id: String,
    pub track_title: String,
    pub artist_names: String,
    pub artist_id: String,
    pub album_id: String,
    pub album_title: String,
    /// The track's own release's cover, versioned — `None` when that release has
    /// no cover row. The UI keys its decoded copy on the whole reference, so new
    /// bytes for the same release replace it.
    pub cover_image: Option<crate::album_detail::ImageRef>,
    pub release_id: String,
    pub side: Option<PlaybackTrackSide>,
}

/// Physical side metadata for a track on a side-based release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackTrackSide {
    pub medium: PhysicalSideMedium,
    pub side_letter: String,
}

/// The track metadata a `Loading` state carries once `prepare_track_for_playback`
/// has resolved it. Absent in the first `Loading` emission (before the DB lookup
/// completes) and present in the second, so the bar can switch from the prior
/// track to the target the moment its identity is known. The duration is
/// pregap-adjusted — the same value `Playing`/`Paused` carry.
#[derive(Debug, Clone)]
pub struct LoadingTrack {
    pub track_info: PlaybackTrackInfo,
    pub duration_ms: u64,
}

impl LoadingTrack {
    pub(super) fn from_prepared(prepared: &PlaybackPreparedTrack) -> Self {
        Self {
            track_info: prepared.track_info.clone(),
            duration_ms: pregap_adjusted_duration(prepared),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackSidePausePrompt {
    pub id: String,
    pub title_key: &'static str,
    pub side_letter: String,
    pub message_key: &'static str,
}

pub const SIDE_PAUSE_TITLE_KEY: &str = "core.playback.pause.side_ended.title";
pub const SIDE_PAUSE_VINYL_MESSAGE_KEY: &str = "core.playback.pause.side_ended.message.vinyl";
pub const SIDE_PAUSE_CASSETTE_MESSAGE_KEY: &str = "core.playback.pause.side_ended.message.cassette";

/// Why playback is paused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackPauseReason {
    Manual,
    SideEnded(PlaybackSidePausePrompt),
}

#[derive(Debug, Clone)]
pub(super) struct SidePauseDecision {
    pub(super) track_id: String,
    pub(super) prompt: PlaybackSidePausePrompt,
}

#[derive(Debug)]
pub(crate) enum PlaybackCommand {
    Play(String),
    PlayRelease {
        release_id: String,
        start_track_index: Option<usize>,
        shuffle: bool,
    },
    /// Play several releases as one context, concatenated in the given order and
    /// starting at the first track. Each release's tracks that fail to load are
    /// skipped (logged); an all-empty result is a no-op. A single playable release
    /// collapses to a `Release` context, identical to `PlayRelease`.
    PlayReleases(Vec<String>),
    /// Play the whole library in a freshly seeded shuffle. An empty library is a
    /// no-op (logged); the seed is minted in the handler.
    PlayLibraryShuffled,
    Pause,
    Resume,
    Stop,
    /// Manual next track (pregap skipped).
    Next,
    /// Auto-advance from the natural completion of `track_id` (pregap played).
    /// The id is validated when handled: a user Next/Seek that reached the command
    /// loop first already moved on, so a stale advance for a no-longer-current or
    /// no-longer-Completed track is dropped rather than double-advancing.
    AutoAdvance {
        track_id: String,
    },
    /// A load's decoder filled its ring to the play threshold (or hit EOF). Sent
    /// by a watcher task awaiting the decoder's ready signal; the handler resolves
    /// the current track's phase to its target (Playing/Paused) only if this is
    /// still the live load. Identity is the load `generation`, not the track id:
    /// RepeatCurrent / RestartCurrent / re-Play replay the SAME id through a fresh
    /// load, so an id match would accept a ready signal from an abandoned one. The
    /// id is carried only to name the dropped track in the debug log.
    TrackReady {
        track_id: String,
        generation: LoadGeneration,
    },
    /// A mid-flight read failure (cloud or local) emitted a
    /// `PlaybackProgress::PlaybackError`. Sent from the progress
    /// self-subscription so the command loop tears playback down to Stopped
    /// rather than leaving a frozen Playing state with a stalled position bar.
    HaltOnError,
    /// A track buffer's byte fill failed. Which track that breaks depends on what
    /// the buffer is serving right now, and only the command loop knows that —
    /// the fill task reports the failure with the buffer's id and the loop
    /// decides (see `handle_read_failed`).
    ReadFailed {
        buffer_id: u64,
        error: PlaybackError,
    },
    /// The system default output device changed. Rebuilds the persistent output
    /// stream over the same source so playback follows the new default (a no-op
    /// when nothing is playing) — the stream is otherwise rebuilt only on stop or
    /// a format change, so without this it would stay pinned to the old device.
    /// macOS-only: only CoreAudio gives us a default-device listener; elsewhere a
    /// switch takes effect at the next rebuild.
    #[cfg(target_os = "macos")]
    OutputDeviceChanged,
    Previous,
    Seek(std::time::Duration),
    /// Seek by slider ratio (0.0–1.0). The service converts to position using
    /// current duration and pregap.
    SeekByRatio(f64),
    SetVolume(f32),
    AddToQueue(Vec<String>),
    AddNext(Vec<String>),
    AddReleaseToQueue(String),
    AddReleaseNext(String),
    InsertInQueue(Vec<String>, usize),
    /// Remove the queue entry with this per-instance id.
    RemoveFromQueue(QueueEntryId),
    /// Move the entry `entry_id` to sit immediately before `before`.
    /// `before = None` moves it to the end of the queue.
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
    /// Set the context lane to shuffled or sequential order. `true` mints a fresh
    /// seed and permutes the upcoming rows; `false` puts them back in the order
    /// the lane had when shuffle turned on. The current track keeps playing.
    SetShuffle(bool),
    /// Re-run the side-pause staging decision for the currently preloaded next
    /// track. Sent after `pause_between_sides` is turned on: staging is decided
    /// once, at preload time, so without this a boundary already staged into the
    /// gapless chain would keep crossing gaplessly. A no-op when there's no active
    /// track, no preloaded next, or the preload is already held (the drain-time
    /// gate re-reads the config in that case).
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
    /// Set mute to an absolute state. Muting saves the pre-mute volume and
    /// drives output to 0; unmuting restores it. Setting the current state
    /// changes nothing (a repeated dispatch lands in the same place).
    SetMuted(bool),
    /// Query current volume. Response sent via oneshot.
    GetVolume(oneshot::Sender<f32>),
    /// Test-only command-loop barrier that returns the queue after every
    /// preceding command has finished.
    #[cfg(any(test, feature = "test-utils"))]
    GetQueueProjection(oneshot::Sender<PlaybackQueueProjection>),
    /// Graceful shutdown: save state to disk, reply, then stop.
    Shutdown(oneshot::Sender<()>),
    /// Persist the current playback state without tearing down playback. Mobile
    /// calls this when backgrounded — it can't call `Shutdown` (that stops the
    /// background audio), so this snapshots state for a later cold launch.
    SaveState(oneshot::Sender<()>),
    /// Switch playback to a remote renderer: stop the local renderer (keeping the
    /// queue) and reissue the current track to the device at its current
    /// position. The connected channel, URL providers, and format gate ride in
    /// the payload.
    PlayOn(Box<RemoteConnect>),
    /// Switch playback to an AirPlay receiver: keep decoding locally but swap the
    /// output sink to push audio to the device. The sink, name, and latency ride
    /// in the payload.
    PlayOnAirPlay(Box<renderer::AirPlayConnect>),
    /// Stop remote or AirPlay playback: end the session and resume the local
    /// renderer, paused at the last position.
    StopRemote,
    /// A status update from the active remote session's poll loop — drives the
    /// progress feed, queue-advance on device end-of-track, and detection of a
    /// device-side stop. Ignored when playing locally (a stale update from a
    /// session that already ended).
    RemoteStatus(crate::renderer::RendererSessionStatus),
}
/// Current playback state: track metadata and total duration only. Position
/// (progress, elapsed, remaining) flows through `PlaybackProgress::PositionUpdate`
/// (ticks) and `PlaybackProgress::Seeked` (seeks, restore, pause/resume refresh)
/// instead — keeping it out of the state event means one event never has to drive
/// both the SwiftUI store (slow) and the NSView (fast).
#[derive(Debug, Clone)]
pub enum PlaybackState {
    Stopped,
    Playing {
        track_info: PlaybackTrackInfo,
        duration_ms: u64,
    },
    Paused {
        track_info: PlaybackTrackInfo,
        duration_ms: u64,
        reason: PlaybackPauseReason,
    },
    Loading {
        track_id: String,
        /// The target track's metadata, once resolved. `None` in the first
        /// emission (before the DB lookup), `Some` once `play_track` has the
        /// prepared track in hand.
        resolved: Option<LoadingTrack>,
    },
}
/// Send a command to the playback service. Logs at warn-level if the service
/// has shut down (receiver dropped). Calls are otherwise fire-and-forget; the
/// service processes commands serially on its own thread.
pub(crate) fn dispatch_command(
    tx: &tokio_mpsc::UnboundedSender<PlaybackCommand>,
    cmd: PlaybackCommand,
) {
    if let Err(err) = tx.send(cmd) {
        warn!("playback command channel closed; dropped {:?}", err.0);
    }
}

/// Wait for the service to acknowledge a shutdown request. The acknowledgment
/// is best-effort — if the service died before responding, surface that as a
/// warning rather than blocking shutdown.
async fn await_shutdown_ack(rx: oneshot::Receiver<()>) {
    if let Err(err) = rx.await {
        warn!("playback service exited before acknowledging shutdown: {err}");
    }
}

/// Handle for sending commands to the playback service.
#[derive(Clone)]
pub struct PlaybackHandle {
    command_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
    progress_handle: PlaybackProgressHandle,
    queue_values: tokio::sync::watch::Receiver<PlaybackQueueProjection>,
    /// The service's dedicated OS thread. Taken and joined on the first
    /// shutdown/stop so the `LibraryManager` clone it holds — and through the
    /// shared coven handle, the store's exclusive open lock — is released before
    /// teardown returns. The service loop holds its own `command_tx`, so it only
    /// stops on an explicit `Shutdown`; nothing else would ever join this thread.
    /// Shared across clones behind a take-once slot, so teardown is idempotent.
    thread: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
}
impl PlaybackHandle {
    pub(super) fn new(
        command_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
        progress_handle: PlaybackProgressHandle,
        queue_values: tokio::sync::watch::Receiver<PlaybackQueueProjection>,
        thread: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
    ) -> Self {
        Self {
            command_tx,
            progress_handle,
            queue_values,
            thread,
        }
    }

    pub fn play(&self, track_id: String) {
        dispatch_command(&self.command_tx, PlaybackCommand::Play(track_id));
    }
    pub fn play_release(
        &self,
        release_id: String,
        start_track_index: Option<usize>,
        shuffle: bool,
    ) {
        dispatch_command(
            &self.command_tx,
            PlaybackCommand::PlayRelease {
                release_id,
                start_track_index,
                shuffle,
            },
        );
    }
    pub fn play_releases(&self, release_ids: Vec<String>) {
        dispatch_command(&self.command_tx, PlaybackCommand::PlayReleases(release_ids));
    }
    pub fn play_library_shuffled(&self) {
        dispatch_command(&self.command_tx, PlaybackCommand::PlayLibraryShuffled);
    }
    pub fn pause(&self) {
        dispatch_command(&self.command_tx, PlaybackCommand::Pause);
    }
    pub fn resume(&self) {
        dispatch_command(&self.command_tx, PlaybackCommand::Resume);
    }
    pub fn stop(&self) {
        dispatch_command(&self.command_tx, PlaybackCommand::Stop);
    }
    pub fn next(&self) {
        dispatch_command(&self.command_tx, PlaybackCommand::Next);
    }
    pub fn previous(&self) {
        dispatch_command(&self.command_tx, PlaybackCommand::Previous);
    }
    pub fn seek(&self, position: std::time::Duration) {
        dispatch_command(&self.command_tx, PlaybackCommand::Seek(position));
    }
    pub fn seek_by_ratio(&self, ratio: f64) {
        dispatch_command(&self.command_tx, PlaybackCommand::SeekByRatio(ratio));
    }
    pub fn set_volume(&self, volume: f32) {
        dispatch_command(&self.command_tx, PlaybackCommand::SetVolume(volume));
    }
    pub fn set_muted(&self, muted: bool) {
        dispatch_command(&self.command_tx, PlaybackCommand::SetMuted(muted));
    }
    /// Switch playback to a remote renderer over `channel`, minting each track's
    /// media URL through the injected providers and gating its served format
    /// through `stream_format` (the flavor's safe-set). The channel is already
    /// connected (bae-desktop builds it off the service thread).
    pub fn play_on(
        &self,
        channel: Box<dyn crate::renderer::RendererChannel>,
        device_name: String,
        stream_url_provider: crate::renderer::MediaUrlProvider,
        cover_url_provider: crate::renderer::CoverUrlProvider,
        stream_format: crate::renderer::StreamFormatFn,
    ) {
        dispatch_command(
            &self.command_tx,
            PlaybackCommand::PlayOn(Box::new(RemoteConnect::new(
                channel,
                device_name,
                stream_url_provider,
                cover_url_provider,
                stream_format,
            ))),
        );
    }
    /// Switch playback to an AirPlay receiver via `sink` (built off the service
    /// thread by bae-desktop with the device's address, encryption, and reported
    /// latency). Decode stays local; only the output sink is swapped.
    pub fn play_on_airplay(
        &self,
        sink: Box<dyn crate::playback::airplay_output::AirPlaySink>,
        device_name: String,
        latency_frames: u32,
    ) {
        dispatch_command(
            &self.command_tx,
            PlaybackCommand::PlayOnAirPlay(Box::new(renderer::AirPlayConnect::new(
                sink,
                device_name,
                latency_frames,
            ))),
        );
    }
    /// Stop remote or AirPlay playback and resume local playback, paused at the
    /// last position.
    pub fn stop_remote(&self) {
        dispatch_command(&self.command_tx, PlaybackCommand::StopRemote);
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
        dispatch_command(&self.command_tx, PlaybackCommand::GetQueueProjection(tx));
        rx.await
            .map_err(|e| format!("playback loop dropped the queue response channel: {e}"))
    }
    pub fn add_to_queue(&self, track_ids: Vec<String>) {
        dispatch_command(&self.command_tx, PlaybackCommand::AddToQueue(track_ids));
    }
    pub fn add_next(&self, track_ids: Vec<String>) {
        dispatch_command(&self.command_tx, PlaybackCommand::AddNext(track_ids));
    }
    pub fn add_release_to_queue(&self, release_id: String) {
        dispatch_command(
            &self.command_tx,
            PlaybackCommand::AddReleaseToQueue(release_id),
        );
    }
    pub fn add_release_next(&self, release_id: String) {
        dispatch_command(
            &self.command_tx,
            PlaybackCommand::AddReleaseNext(release_id),
        );
    }
    pub fn insert_in_queue(&self, track_ids: Vec<String>, index: usize) {
        dispatch_command(
            &self.command_tx,
            PlaybackCommand::InsertInQueue(track_ids, index),
        );
    }
    pub fn remove_entry(&self, entry_id: QueueEntryId) {
        dispatch_command(&self.command_tx, PlaybackCommand::RemoveFromQueue(entry_id));
    }
    pub fn reorder_entry(&self, entry_id: QueueEntryId, before: Option<QueueEntryId>) {
        dispatch_command(
            &self.command_tx,
            PlaybackCommand::ReorderQueue { entry_id, before },
        );
    }
    pub fn clear_up_next(&self) {
        dispatch_command(&self.command_tx, PlaybackCommand::ClearUpNext);
    }
    pub fn clear_playing_from(&self) {
        dispatch_command(&self.command_tx, PlaybackCommand::ClearPlayingFrom);
    }
    pub fn set_repeat_mode(&self, mode: RepeatMode) {
        dispatch_command(&self.command_tx, PlaybackCommand::SetRepeatMode(mode));
    }

    pub fn set_shuffle(&self, on: bool) {
        dispatch_command(&self.command_tx, PlaybackCommand::SetShuffle(on));
    }

    pub async fn get_volume(&self) -> f32 {
        let (tx, rx) = oneshot::channel();
        dispatch_command(&self.command_tx, PlaybackCommand::GetVolume(tx));
        rx.await.unwrap_or_else(|e| {
            warn!("get_volume: playback loop dropped the response channel: {e}");
            1.0
        })
    }

    /// Graceful shutdown: persist playback state, stop the service loop, and join
    /// its thread so the `LibraryManager` clone it holds — and coven's store lock —
    /// is released before this returns. Awaits the state-save ack (the platform's
    /// quit path relies on it being durable). Idempotent with [`Self::stop_and_join`]:
    /// they share the take-once join handle, so a later teardown is a no-op.
    pub async fn shutdown(&self) {
        let Some(join_handle) = self.thread.lock().unwrap().take() else {
            return;
        };
        let (tx, rx) = oneshot::channel();
        dispatch_command(&self.command_tx, PlaybackCommand::Shutdown(tx));
        await_shutdown_ack(rx).await;
        // The loop has broken; join so the thread fully exits and drops its
        // LibraryManager clone. Off-worker via spawn_blocking so the blocking join
        // doesn't stall a runtime thread.
        match tokio::task::spawn_blocking(move || join_handle.join()).await {
            Ok(Ok(())) => {}
            Ok(Err(panic)) => warn!("playback service thread panicked before join: {panic:?}"),
            Err(join_err) => warn!("joining the playback service thread failed: {join_err}"),
        }
    }

    /// Synchronous teardown for `Drop`: stop the service loop and join its thread,
    /// releasing the `LibraryManager` clone (and the store lock) before returning.
    /// The join subsumes the state save (the `Shutdown` handler persists before it
    /// breaks). No runtime needed — the loop runs on its own — so this is safe from
    /// `Drop`. Idempotent with [`Self::shutdown`] via the shared take-once join handle.
    pub fn stop_and_join(&self) {
        let Some(join_handle) = self.thread.lock().unwrap().take() else {
            return;
        };
        let (tx, _rx) = oneshot::channel();
        dispatch_command(&self.command_tx, PlaybackCommand::Shutdown(tx));
        // A panicked service thread already reported itself; joining from Drop
        // must not repropagate, but the panic shouldn't vanish either.
        if let Err(panic) = join_handle.join() {
            warn!("playback service thread panicked before join: {panic:?}");
        }
    }

    /// Persist the current playback state without stopping playback. Mobile
    /// calls this when backgrounded (it can't `shutdown` — that would kill the
    /// background audio), so the queue, current track, and position survive a
    /// later process death / cold launch. Awaits the write so the snapshot is
    /// durable before the OS suspends the app.
    pub async fn save_state(&self) {
        let (tx, rx) = oneshot::channel();
        dispatch_command(&self.command_tx, PlaybackCommand::SaveState(tx));
        let _ = rx.await;
    }

    pub fn skip_to_entry(&self, entry_id: QueueEntryId) {
        dispatch_command(&self.command_tx, PlaybackCommand::SkipTo(entry_id));
    }
    /// Re-evaluate the side-pause staging decision for the currently preloaded
    /// next track. Called after `pause_between_sides` turns on, so a track
    /// already staged into the gapless chain is held instead if a pause is now
    /// due at its boundary.
    pub fn reevaluate_side_pause_staging(&self) {
        dispatch_command(
            &self.command_tx,
            PlaybackCommand::ReevaluateSidePauseStaging,
        );
    }
    /// Preview a local source window. The same target stops; another switches.
    pub fn preview_play(&self, target: crate::playback::PreviewTarget) {
        dispatch_command(&self.command_tx, PlaybackCommand::PreviewPlay(target));
    }
    /// Stop any active preview playback.
    pub fn preview_stop(&self) {
        dispatch_command(&self.command_tx, PlaybackCommand::PreviewStop);
    }
    /// Toggle pause/resume on the active preview.
    pub fn preview_toggle_pause(&self) {
        dispatch_command(&self.command_tx, PlaybackCommand::PreviewTogglePause);
    }
    /// Seek by slider ratio (0.0–1.0) within the active preview.
    pub fn preview_seek_by_ratio(&self, ratio: f64) {
        dispatch_command(&self.command_tx, PlaybackCommand::PreviewSeekByRatio(ratio));
    }
}
