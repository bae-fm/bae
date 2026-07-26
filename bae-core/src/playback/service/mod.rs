//! # Playback Service
//!
//! Runs on its own thread and drives playback from a command channel.
//!
//! ## Audio state
//!
//! What the audio callback does each buffer is a shared atomic (`AudioState`:
//! `Stopped`, `Playing`, `Paused`), written as a projection of the `PlaybackSlot`
//! (`slot.rs`), which is the truth. The callback reads the atomic lock-free and
//! outputs samples when `Playing`, silence otherwise.
//!
//! ## Seek flow (`seek.rs`)
//!
//! 1. The phase goes Loading and the atomic Stopped, so the callback goes silent
//!    while the new decoder fills — no audio leaks from the old ring.
//! 2. A fresh decoder is spawned over the SAME byte buffers (they stay cached)
//!    and swapped into the persistent source (`PlaybackSource::replace`). It is
//!    spawned before the old one is joined, to keep the silent window short; two
//!    readers on one sparse buffer is supported.
//! 3. Only then is the old decoder cancelled and joined
//!    (`cancel_and_join_decoder`), so the reused buffers are free of it.
//! 4. `Seeked` is emitted; the phase stays Loading until the ready-watcher's
//!    `TrackReady` resolves it to the preserved Playing/Paused.
//!
//! ## File-buffer ownership
//!
//! `shared_file_buffers` (one sparse byte buffer per release file, shared by
//! every track that plays from that file) is the buffers' single owner. A buffer
//! is cancelled — stopping its on-demand fill task for good — exactly when it
//! leaves the cache: `release_buffers` evicts the files a departing track no
//! longer shares with the retained one, and `stop` cancels the whole cache. So a
//! cached buffer is always live, and prepare reuses it as-is.

use super::RepeatMode;
use super::{
    repeat_to_str, source_to_str, ContextSource, ContextStart, NextEntry, PersistedPlayback,
    PlaybackQueue, PreviousAction, QueueEntryId, QueueSnapshot,
};
use crate::audio_codec::StreamingDecodeError;
use crate::db::{DbAudioSegmentRole, DbPlaybackContext, DbPlaybackState};
use crate::diagnostics::{
    AnomalyKind, LocalId, PlaybackCommandKind, PlaybackOperation, PlaybackStartSource,
    TelemetryEvent, TrackTransition,
};
use crate::library::LibraryEvent;
use crate::library::LibraryManager;
use crate::library::ResolvedTrackAudio;
use crate::playback::audio_output::{AudioEvent, AudioEventReceiver, AudioOutput};
use crate::playback::data_source::{create_audio_reader, FetchArbiter};
use crate::playback::error::PlaybackError;
use crate::playback::progress::emit_progress;
use crate::playback::progress::{
    PlaybackProgress, PlaybackProgressHandle, PlaybackQueueProjection,
};
// The `source` module is imported by path so the audio sample feed reads
// `source::PlaybackSource` — distinct from the queue's `ContextSource`.
use crate::playback::preview_player::PreviewPlayer;
use crate::playback::source;
use crate::playback::source::{TrackCrossing, TrackFmt};
use crate::playback::sparse_buffer::{create_sparse_buffer, SharedSparseBuffer};
use crate::playback::TrackStream;
use crate::util::format::PhysicalSideMedium;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc as tokio_mpsc;
use tokio::sync::oneshot;
use tracing::{debug, error, info, trace, warn};

mod advance;
mod output;
mod pipeline;
mod preview;
mod queue_commands;
mod renderer;
mod seek;
mod slot;
mod starvation;
mod state;

use crate::playback::stream_pipeline::{
    cancel_and_join_decoder, log_stream_diagnostic, report_dropped_audio_events, spawn_decoder,
    DecodeFailureReport, DecoderStart, SegmentDecodeParams, StreamDecodeParams,
};
use output::OutputStream;
use renderer::{RemoteConnect, Renderer};
use slot::{
    CurrentTrack, LoadGeneration, PausePhase, PlayIntent, PlayTarget, PlaybackSlot, TrackDecoder,
    TrackPhase,
};
use starvation::StarvationEpisode;

#[cfg(test)]
mod tests;

pub(crate) fn log_streaming_decode_failure(
    context: &str,
    error: StreamingDecodeError,
) -> Option<String> {
    match error {
        StreamingDecodeError::InputCancelled => {
            debug!("{context} stopped after input cancellation");
            None
        }
        StreamingDecodeError::Decode(message) => {
            error!("{context} failed: {message}");
            Some(message)
        }
    }
}

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
    pub cover_image_id: Option<String>,
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
    fn from_prepared(prepared: &PlaybackPreparedTrack) -> Self {
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
    track_id: String,
    prompt: PlaybackSidePausePrompt,
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
    /// Preview a local audio file (toggle: same path stops, different path switches).
    PreviewPlay(String),
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
    /// Query the queue shape owned by the playback loop.
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
    /// The service's dedicated OS thread. Taken and joined on the first
    /// shutdown/stop so the `LibraryManager` clone it holds — and through the
    /// shared coven handle, the store's exclusive open lock — is released before
    /// teardown returns. The service loop holds its own `command_tx`, so it only
    /// stops on an explicit `Shutdown`; nothing else would ever join this thread.
    /// Shared across clones behind a take-once slot, so teardown is idempotent.
    thread: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
}
impl PlaybackHandle {
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
            PlaybackCommand::PlayOn(Box::new(RemoteConnect {
                channel,
                device_name,
                stream_url_provider,
                cover_url_provider,
                stream_format,
            })),
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
            PlaybackCommand::PlayOnAirPlay(Box::new(renderer::AirPlayConnect {
                sink,
                device_name,
                latency_frames,
            })),
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

    pub async fn queue_projection(&self) -> Result<PlaybackQueueProjection, String> {
        let (tx, rx) = oneshot::channel();
        dispatch_command(&self.command_tx, PlaybackCommand::GetQueueProjection(tx));
        rx.await
            .map_err(|e| format!("playback loop dropped the queue response channel: {e}"))
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
    /// Preview a local audio file. Same path toggles off, different path switches.
    pub fn preview_play(&self, path: String) {
        dispatch_command(&self.command_tx, PlaybackCommand::PreviewPlay(path));
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

/// The pregap-adjusted total duration (ms) a prepared track reports to the UI
/// — the same value `Playing`/`Paused`/`Loading` all carry.
fn pregap_adjusted_duration(prepared: &PlaybackPreparedTrack) -> u64 {
    let raw_dur = prepared.duration.as_millis() as u64;
    let (_, adjusted_dur) =
        crate::playback::format::adjust_for_pregap(0, raw_dur, prepared.total_pregap_ms());
    adjusted_dur
}

impl PlaybackPreparedTrack {
    /// Build the audio-callback formatting envelope for this track. The
    /// position offset is the in-track time the stream is about to start at
    /// (non-zero only on seek; zero for natural starts and gapless advances).
    fn track_fmt(&self, position_offset: std::time::Duration) -> TrackFmt {
        TrackFmt {
            track_id: self.track_info.track_id.clone(),
            duration_ms: self.duration.as_millis() as u64,
            pregap_ms: self.total_pregap_ms(),
            position_offset,
            replay_gain_linear: self.replay_gain_linear,
        }
    }

    /// Decoder windows for this track beginning at the in-track sample offset.
    fn decode_params(&self, offset: u64, include_pregap: bool) -> StreamDecodeParams {
        use crate::util::content_type::ContentType;
        let mut remaining_offset = offset;
        let generated_pregap_samples = self.generated_pregap_samples();
        let leading_silence_frames = if include_pregap {
            generated_pregap_samples.saturating_sub(offset)
        } else {
            0
        };
        if include_pregap {
            remaining_offset = remaining_offset.saturating_sub(generated_pregap_samples);
        }

        let mut segments = Vec::new();
        for segment in &self.segments {
            if !include_pregap && segment.role == DbAudioSegmentRole::AudioPregap {
                continue;
            }
            let segment_len = segment
                .span
                .end_sample
                .map(|end| end.saturating_sub(segment.span.start_sample));
            if let Some(len) = segment_len {
                if remaining_offset >= len {
                    remaining_offset -= len;
                    continue;
                }
            }
            segments.push(SegmentDecodeParams {
                buffer: segment.buffer.clone(),
                span: segment.span,
                start_offset: remaining_offset,
            });
            remaining_offset = 0;
        }

        StreamDecodeParams {
            segments,
            byte_seekable: self.content_type != ContentType::Ape,
            leading_silence_frames,
            trailing_silence_frames: 0,
        }
    }

    fn total_pregap_ms(&self) -> Option<i64> {
        self.pregap_ms.or(self.generated_pregap_ms)
    }

    fn generated_pregap_samples(&self) -> u64 {
        if let Some(samples) = self.generated_pregap_samples {
            if samples < 0 {
                warn!(
                    track_id = %self.track_info.track_id,
                    generated_pregap_samples = samples,
                    "Ignoring negative generated pregap samples"
                );
                return 0;
            }
            return samples as u64;
        }

        let Some(ms) = self.generated_pregap_ms else {
            return 0;
        };
        if ms < 0 {
            warn!(
                track_id = %self.track_info.track_id,
                generated_pregap_ms = ms,
                "Ignoring negative generated pregap duration"
            );
            return 0;
        }

        ((ms as f64 / 1000.0) * self.sample_rate as f64) as u64
    }

    /// The distinct release files this track plays from.
    fn file_ids(&self) -> HashSet<&str> {
        self.segments
            .iter()
            .map(|segment| segment.file_id.as_str())
            .collect()
    }

    /// Release this track's file buffers as it leaves the pipeline. A file the
    /// retained track(s) still play stays cached and alive — its readers are
    /// woken so this track's cancelled decoder observes its token instead of
    /// staying parked on a read. The rest leave the shared cache and are
    /// cancelled, which stops their fill task and unblocks anything reading
    /// them.
    fn release_buffers(
        &self,
        retained_file_ids: &HashSet<&str>,
        shared_file_buffers: &mut HashMap<String, SharedSparseBuffer>,
    ) {
        for segment in &self.segments {
            if retained_file_ids.contains(segment.file_id.as_str()) {
                segment.buffer.wake_readers();
            } else {
                segment.buffer.cancel();
                shared_file_buffers.remove(&segment.file_id);
            }
        }
    }
}

#[derive(Clone)]
struct PreparedAudioSegment {
    role: DbAudioSegmentRole,
    file_id: String,
    buffer: SharedSparseBuffer,
    /// Where this segment sits inside its backing file, in samples and bytes.
    span: crate::db::SegmentSpan,
}

#[derive(Clone)]
struct PlaybackPreparedTrack {
    track_info: PlaybackTrackInfo,
    segments: Vec<PreparedAudioSegment>,
    /// Hz — also the time-to-sample conversion factor.
    sample_rate: u32,
    channels: u32,
    /// Pregap the source audio already contains (a CUE/FLAC track).
    pregap_ms: Option<i64>,
    /// Silent pregap to generate, from a CUE `PREGAP` directive.
    generated_pregap_ms: Option<i64>,
    /// The same generated pregap in exact samples.
    generated_pregap_samples: Option<i64>,
    duration: std::time::Duration,
    /// This track's audio codec. Selects the track-start seek: FLAC/lossless
    /// byte-seek to `start_byte`; APE sample-seeks its index.
    content_type: crate::util::content_type::ContentType,
    /// Linear playback gain folded into the audio callback's volume multiply.
    /// Derived once here from the replay-gain mode and the stored loudness/peak
    /// measurements; `1.0` = no change (Off, or no usable measurement).
    replay_gain_linear: f32,
}

struct PreloadedNext {
    prepared: PlaybackPreparedTrack,
    decoder_handle: std::thread::JoinHandle<()>,
    /// The preload decoder's AVIO cancel flag, minted where the decoder is
    /// spawned. Carried into the installed `TrackDecoder` when the preload is
    /// promoted to current (manual next or gapless crossing), so the current track
    /// owns exactly one token.
    cancel_token: Arc<std::sync::atomic::AtomicBool>,
    source: PreloadedNextSource,
}

enum PreloadedNextSource {
    Held(TrackStream),
    Staged,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum TrackStart {
    Direct,
    Natural,
    Position(std::time::Duration),
}

impl TrackStart {
    fn from_natural_transition(is_natural_transition: bool) -> Self {
        if is_natural_transition {
            Self::Natural
        } else {
            Self::Direct
        }
    }

    fn position(self, pregap_ms: Option<i64>) -> std::time::Duration {
        match self {
            Self::Natural => std::time::Duration::ZERO,
            Self::Direct => pregap_seek_position(pregap_ms).unwrap_or(std::time::Duration::ZERO),
            Self::Position(position) => position,
        }
    }

    fn includes_pregap(self) -> bool {
        matches!(self, Self::Natural | Self::Position(_))
    }
}

impl PreloadedNext {
    fn track_id(&self) -> &str {
        self.prepared.track_info.track_id.as_str()
    }
}

/// Discard the preloaded next track's decoder and hand back its prepared track,
/// so the caller decides which of its file buffers survive (`release_buffers`
/// with whatever files the pipeline still plays, or nothing on a full stop).
fn discard_preloaded_next(
    preloaded_next: &mut Option<PreloadedNext>,
    current_playback_source: Option<&Arc<Mutex<source::PlaybackSource>>>,
) -> Option<PlaybackPreparedTrack> {
    let preloaded = preloaded_next.take()?;

    let PreloadedNext {
        prepared,
        source,
        cancel_token,
        ..
    } = preloaded;

    let detached = detach_preloaded_source(current_playback_source, source);
    discard_preloaded_decoder(&prepared, detached, &cancel_token);
    Some(prepared)
}

/// Stop a preloaded (not-yet-promoted) decoder. Three mechanisms, one per place
/// the decoder can be blocked:
/// - cancel the output source: sets the sink's cancel flag and unparks the
///   decoder, so one blocked writing a full ring exits;
/// - set the per-decoder token: the decoder's read-side stop signal;
/// - wake the byte buffers' readers: a decoder blocked reading one wakes and
///   observes the token. The buffers themselves stay alive — releasing them is
///   the caller's decision, since the pipeline may still play from the same
///   files.
fn discard_preloaded_decoder(
    prepared: &PlaybackPreparedTrack,
    detached_source: Option<TrackStream>,
    cancel_token: &Arc<std::sync::atomic::AtomicBool>,
) {
    if let Some(source) = detached_source {
        source.cancel();
    }
    cancel_token.store(true, std::sync::atomic::Ordering::Release);
    for segment in &prepared.segments {
        segment.buffer.wake_readers();
    }
}

fn detach_preloaded_source(
    current_playback_source: Option<&Arc<Mutex<source::PlaybackSource>>>,
    source: PreloadedNextSource,
) -> Option<TrackStream> {
    match source {
        PreloadedNextSource::Held(source) => Some(source),
        PreloadedNextSource::Staged => current_playback_source.and_then(|gapless| {
            gapless
                .lock()
                .unwrap()
                .take_next()
                .map(|(source, _)| source)
        }),
    }
}

/// Assemble a `PlaybackPreparedTrack` from the resolved audio, its display info,
/// and its segments' buffers.
fn finalize_playback_track(
    resolved: ResolvedTrackAudio,
    track_info: PlaybackTrackInfo,
    segments: Vec<PreparedAudioSegment>,
    replay_gain_mode: crate::config::ReplayGainMode,
) -> PlaybackPreparedTrack {
    let duration = resolved
        .duration_ms
        .map(|ms| std::time::Duration::from_millis(ms as u64))
        .unwrap_or_else(|| {
            debug!(
                release_id = %resolved.release_id,
                "no resolved track duration; using 5min placeholder"
            );
            std::time::Duration::from_secs(300)
        });

    let replay_gain_linear = resolved.replay_gain_linear(replay_gain_mode);

    PlaybackPreparedTrack {
        track_info,
        segments,
        sample_rate: resolved.sample_rate,
        channels: resolved.channels,
        pregap_ms: resolved.pregap_ms,
        generated_pregap_ms: resolved.generated_pregap_ms,
        generated_pregap_samples: resolved.generated_pregap_samples,
        duration,
        content_type: resolved.content_type,
        replay_gain_linear,
    }
}

fn ensure_resolved_audio_format(
    track_id: &str,
    resolved: &ResolvedTrackAudio,
) -> Result<(), PlaybackError> {
    if resolved.sample_rate == 0 || resolved.channels == 0 {
        return Err(PlaybackError::internal(format!(
            "track {track_id} has unusable audio format: sample_rate={}, channels={}",
            resolved.sample_rate, resolved.channels
        )));
    }
    Ok(())
}

/// The playback shape of a fill-error handler: a failed byte fill halts the
/// track by emitting `PlaybackProgress::PlaybackError` to the UI. (The fill
/// itself cancels the buffer right after, unblocking the decoder.)
pub(crate) fn playback_fill_error_handler(
    progress_tx: tokio_mpsc::UnboundedSender<PlaybackProgress>,
) -> crate::playback::data_source::FillErrorHandler {
    Box::new(move |error| {
        emit_progress(
            &progress_tx,
            PlaybackProgress::PlaybackError {
                reason: error.into_ui_reason(),
            },
        );
    })
}

async fn prepare_track_for_playback(
    library_manager: &LibraryManager,
    track_id: &str,
    shared_file_buffers: &mut HashMap<String, SharedSparseBuffer>,
    progress_tx: tokio_mpsc::UnboundedSender<PlaybackProgress>,
    fetch_arbiter: Arc<FetchArbiter>,
) -> Result<PlaybackPreparedTrack, PlaybackError> {
    let (resolved, track_info) = library_manager
        .resolve_track_audio_and_info(track_id)
        .await
        .map_err(PlaybackError::database)?;
    ensure_resolved_audio_format(track_id, &resolved)?;

    let mut prepared_segments = Vec::with_capacity(resolved.segments.len());
    for segment in &resolved.segments {
        // A cached buffer is live by construction: buffers are cancelled only
        // when they leave the cache (release_buffers / stop), so its fill task
        // is still serving demand.
        let cached = shared_file_buffers.get(&segment.file_id).cloned();
        let buffer = if let Some(buf) = cached {
            info!("Reusing cached file buffer");
            buf
        } else {
            let buffer = create_sparse_buffer(segment.file_size);
            let reader = create_audio_reader(
                library_manager,
                &segment.file_id,
                fetch_arbiter.clone(),
                segment.span.start_byte,
                resolved.content_type == crate::util::content_type::ContentType::Ape,
            );
            reader.start_reading(
                buffer.clone(),
                playback_fill_error_handler(progress_tx.clone()),
            );
            shared_file_buffers.insert(segment.file_id.clone(), buffer.clone());
            buffer
        };
        prepared_segments.push(PreparedAudioSegment {
            role: segment.role.clone(),
            file_id: segment.file_id.clone(),
            buffer,
            span: segment.span,
        });
    }

    // Read the replay-gain mode once, here, and pass it down — rather than a
    // config lookup buried inside `finalize_playback_track`.
    let replay_gain_mode = library_manager.get_config().replay_gain_mode;

    Ok(finalize_playback_track(
        resolved,
        track_info,
        prepared_segments,
        replay_gain_mode,
    ))
}

pub struct PlaybackService {
    library_manager: LibraryManager,
    command_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
    command_rx: tokio_mpsc::UnboundedReceiver<PlaybackCommand>,
    progress_tx: tokio_mpsc::UnboundedSender<PlaybackProgress>,
    playback_queue: PlaybackQueue,
    current_position_shared: Arc<std::sync::Mutex<Option<std::time::Duration>>>,
    audio_output: Box<dyn AudioOutput>,
    /// The persistent output stream, present whenever playback has attached a
    /// track in some format and not yet stopped. Holds the device stream, the
    /// `PlaybackSource` the callback pulls from, and the audio-events receiver the
    /// command loop drains — all shared across track transitions in the same
    /// format. Rebuilt only on a format change / stream error; dropped on `stop`.
    output: Option<OutputStream>,
    /// The single authority for what is current and in what phase. Owns the
    /// current track's decoder and prepared track plus its phase as one consistent
    /// whole; the `AudioState` atomic is written as a projection of it via
    /// `sync_audio_state`. The stream/source/audio-events live in `output`.
    slot: PlaybackSlot,
    /// Mints a fresh `LoadGeneration` per decoder load so a `TrackReady` from an
    /// abandoned load can be told from the live one.
    load_generation_counter: u64,
    /// Preloaded next track state, either staged into the current gapless source
    /// or held for a stream rebuild.
    preloaded_next: Option<PreloadedNext>,
    /// Mute is core state, so no UI has to keep its own.
    is_muted: bool,
    pre_mute_volume: f32,
    /// The preview player — a self-contained second player for auditioning a
    /// local file. The service only coordinates pause/resume of the main player
    /// around it; the preview's own state lives entirely in `PreviewPlayer`.
    preview: PreviewPlayer,
    /// The main player was playing when the preview started, so it resumes when
    /// the preview stops.
    main_was_playing_before_preview: bool,
    /// How often (ms) the audio callback sends position updates to the UI.
    position_update_interval_ms: u32,
    /// Cached full-file buffers keyed by release file id.
    shared_file_buffers: HashMap<String, SharedSparseBuffer>,
    /// Prioritizes byte fetches across tracks: the current track's reader fetches
    /// immediately, a next-track preload's reader yields to it. Shared into every
    /// reader; the current track is designated foreground via
    /// `mark_current_foreground` whenever it becomes current.
    fetch_arbiter: Arc<FetchArbiter>,
    /// The in-progress starvation-watchdog episode, if the current track is
    /// mid-starvation with no decode progress yet observed. `None` whenever
    /// the track is flowing normally — see `reset_starvation_episode`.
    starvation_episode: Option<StarvationEpisode>,
    /// When `persist_playback_state` last ran. Throttles the per-tick persist in
    /// `handle_position_event` to at most once a second. Every call refreshes it
    /// — including the ones a track change triggers (play, gapless advance) — so
    /// the periodic writer waits a full second from whichever discrete event last
    /// wrote the row.
    last_position_persist: Option<std::time::Instant>,
    /// The in-flight first-audio measurement for the live play-to-Playing load,
    /// if one is pending. Set when `play_track` begins a Playing-target load,
    /// cleared/overwritten by the next load; resolved into a `first_audio` event
    /// when that load reaches Playing. `None` once emitted or for paused loads.
    first_audio_pending: Option<FirstAudioMeasurement>,
    /// Where the current track plays: the local decode pipeline, or a connected
    /// Where the current track plays. `Local` by default; `play_on`/`stop_remote`
    /// switch it to a connected remote renderer (Cast or DLNA).
    renderer: Renderer,
}

/// A pending first-audio timing: the load whose arrival at Playing it measures,
/// the track it plays, and when the play began.
struct FirstAudioMeasurement {
    generation: LoadGeneration,
    track_id: String,
    started_at: std::time::Instant,
}

impl PlaybackService {
    /// Designate the current track's buffer as the foreground for fetch
    /// priority, so its reader fetches immediately and a next-track preload's
    /// reader yields to it. Called wherever a track becomes the current one.
    fn mark_current_foreground(&self) {
        if let PlaybackSlot::Active(cur) = &self.slot {
            if let Some(segment) = cur.prepared.segments.first() {
                self.fetch_arbiter.set_foreground(segment.buffer.id());
            }
        }
    }

    /// Mint a fresh load generation. Each fresh play or seek gets its own so a
    /// `TrackReady` from an abandoned load can be told from the live one.
    fn next_load_generation(&mut self) -> LoadGeneration {
        let generation = LoadGeneration(self.load_generation_counter);
        self.load_generation_counter += 1;
        generation
    }

    /// Drive playback down to Stopped after a mid-flight read/decode failure
    /// surfaced a `PlaybackError`. The self-handled failure paths in `play_track`
    /// emit `PlaybackError` AND call `stop()` synchronously before returning to
    /// the loop, so by the time this dequeues the slot is already Stopped and a
    /// second `stop()` would emit a duplicate Stopped — no-op there. A genuine
    /// mid-flight failure leaves the slot Active, so the teardown fires. The
    /// serial loop can't interleave a new load between `stop()` finishing and
    /// this running, so the guard is race-free. `stop()` emits no `PlaybackError`,
    /// so this can't feed back into the self-subscription that dispatched it.
    async fn halt_on_error(&mut self) {
        if matches!(self.slot, PlaybackSlot::Stopped) {
            debug!("HaltOnError: playback already stopped, nothing to halt");
        } else {
            // A slot still Active here is a genuine mid-flight read/decode failure
            // (the inline failure paths stop() before their HaltOnError dequeues,
            // leaving the slot Stopped). That is the read-failure operation.
            self.telemetry_playback_failed(PlaybackOperation::Read);
            self.stop().await;
        }
    }

    /// Telemetry sink, read off the library manager this service already holds.
    fn telemetry(&self) -> &crate::diagnostics::Diagnostics {
        self.library_manager.diagnostics()
    }

    /// Restore playback from the device-local resume cache at startup, unless
    /// "Restore on launch" is off (the row is kept either way — it stays the
    /// crash-safe resume point). A present-and-valid row replays; a corrupt row
    /// (DB-structural or a per-lane out-of-domain value) is counted and cleared;
    /// an absent row or a read failure starts fresh.
    pub(super) async fn restore_from_cache(&mut self, restore_playback: bool) {
        if !restore_playback {
            debug!("restore on launch is off; starting with nothing in playback");
            return;
        }
        use crate::db::LoadedPlaybackState;
        match self.library_manager.load_playback_state().await {
            Ok(LoadedPlaybackState::Present(state)) => match PersistedPlayback::from_row(state) {
                Some(parsed) => self.restore(parsed).await,
                // The row parsed as corrupt (a per-lane out-of-domain value; logged
                // in `from_row`). Count it and delete it so the bad row doesn't
                // linger durably across restarts.
                None => self.discard_corrupt_resume_cache().await,
            },
            // A structurally-impossible row (source XOR cursor); the DB client
            // logged the specific mismatch.
            Ok(LoadedPlaybackState::Corrupt) => self.discard_corrupt_resume_cache().await,
            Ok(LoadedPlaybackState::Absent) => {}
            Err(e) => warn!("couldn't load the saved playback state: {e}; starting fresh"),
        }
    }

    /// The resume cache was corrupt (a DB-structural mismatch or a per-lane
    /// out-of-domain value): count the anomaly and delete the row so the bad
    /// state doesn't linger durably across restarts. The pure parse/DB layers
    /// only detect and log it; the service, which owns the diagnostics sink,
    /// emits and clears.
    async fn discard_corrupt_resume_cache(&self) {
        self.telemetry_anomaly(AnomalyKind::ResumeCacheCorrupt);
        if let Err(e) = self.library_manager.clear_playback_state().await {
            warn!("couldn't clear the corrupt playback resume cache: {e}");
        }
    }

    /// A play command established a new playing context.
    fn telemetry_playback_started(&self, source: PlaybackStartSource, track_count: usize) {
        self.telemetry().event(TelemetryEvent::PlaybackStarted {
            source,
            track_count: track_count as u32,
        });
    }

    /// A track start was initiated (context established, playback requested).
    fn telemetry_track_started(&self, track_id: &str, transition: TrackTransition) {
        self.telemetry().event(TelemetryEvent::TrackStarted {
            track_id: LocalId(track_id.to_string()),
            transition,
        });
    }

    /// A playback operation failed. The free-text cause stays in the local
    /// `error!` next to the call; only the coarse operation ships.
    fn telemetry_playback_failed(&self, operation: PlaybackOperation) {
        self.telemetry()
            .event(TelemetryEvent::PlaybackFailed { operation });
    }

    /// An impossible-state site fired. The free-text detail stays in the local
    /// `warn!`/`error!` next to the call; only the kind (a count) ships.
    fn telemetry_anomaly(&self, kind: AnomalyKind) {
        self.telemetry().event(TelemetryEvent::Anomaly { kind });
    }

    /// Direct selection of `track_id`: its release becomes the playing context,
    /// with the cursor at the chosen track. `get_play_context`'s `Err` covers
    /// only DB failures and data-inconsistency (`release_id` is a required
    /// column, so there is no legitimate "track with no context" case) — fail
    /// loud rather than silently falling back to a single-track queue.
    async fn handle_play(&mut self, track_id: String) {
        self.stop_preview_for_main_playback();
        let context = match self.library_manager.get_play_context(&track_id).await {
            Ok(context) => context,
            Err(e) => {
                error!("Failed to load play context for {track_id}: {e}");
                self.telemetry_playback_failed(PlaybackOperation::LoadContext);
                emit_progress(
                    &self.progress_tx,
                    PlaybackProgress::PlaybackError {
                        reason: PlaybackError::database(e).into_ui_reason(),
                    },
                );
                return;
            }
        };
        let track_count = context.track_ids.len();
        self.playback_queue.play_release(
            ContextSource::Release(context.release_id),
            context.track_ids,
            ContextStart::Index(context.index),
        );
        self.emit_queue_update();
        self.telemetry_playback_started(PlaybackStartSource::Release, track_count);
        self.play_track(
            &track_id,
            TrackStart::Direct,
            PlayTarget::Playing,
            TrackTransition::Manual,
        )
        .await;
    }

    /// Manual skip to the next track (skip pregap). A side-pause forces the next
    /// side to start playing; any other state carries the outgoing track's
    /// play/pause intent forward (a naturally-completed track carries Playing, so
    /// the skip resumes audibly rather than inheriting the Stopped atomic).
    async fn handle_next(&mut self) {
        let target = if self.is_side_paused() {
            PlayTarget::Playing
        } else {
            self.current_play_target()
        };
        if let Some(preloaded_track_id) = self.next_track_id().map(|s| s.to_string()) {
            self.advance_and_play_preloaded(&preloaded_track_id, false, target)
                .await;
        } else {
            // Nothing preloaded: let the queue decide what plays next.
            match self.playback_queue.next_entry() {
                NextEntry::Play(next_track) => {
                    info!("No preloaded track, playing from queue: {}", next_track);
                    self.emit_queue_update();
                    self.play_track(
                        &next_track,
                        TrackStart::Direct,
                        target,
                        TrackTransition::Manual,
                    )
                    .await;
                }
                _ => {
                    info!("No next track available, stopping");
                    self.emit_queue_update();
                    self.stop().await;
                }
            }
        }
    }

    /// Manual step to the previous track, or restart the current one when it's
    /// played past the restart threshold. Carries the outgoing track's play/pause
    /// intent to the track we land on (a side-pause collapses to a plain manual
    /// pause across the track change).
    async fn handle_previous(&mut self) {
        let Some(current_track_id) = self.current_track_id().map(|s| s.to_string()) else {
            debug!("Previous command received with no current track; ignoring");
            return;
        };
        let target = self.current_play_target();
        let current_position = self
            .current_position_shared
            .lock()
            .unwrap()
            .unwrap_or(std::time::Duration::ZERO);
        let position_ms = current_position.as_millis() as u64;

        match self.playback_queue.previous_action(position_ms) {
            PreviousAction::PlayPrevious(previous_track_id) => {
                info!("Going to previous track: {}", previous_track_id);
                // previous_action already stepped the context cursor back and made
                // this track current; just play it.
                self.emit_queue_update();
                self.play_track(
                    &previous_track_id,
                    TrackStart::Direct,
                    target,
                    TrackTransition::Manual,
                )
                .await;
            }
            PreviousAction::RestartCurrent => {
                info!("Restarting current track from beginning");
                self.play_track(
                    &current_track_id,
                    TrackStart::Direct,
                    target,
                    TrackTransition::Manual,
                )
                .await;
            }
        }
    }

    /// Advance to the next track after `track_id` completed naturally (pregap
    /// played). Dispatched from the `TrackCompleted` progress event.
    ///
    /// Only advances if `track_id` is still the current track AND still in the
    /// `Completed` phase. `AutoAdvance` rides the command queue behind whatever
    /// the user did, so a `Next` (a different track is now current) or a `Seek`
    /// (the same track, but the seek reset its phase off `Completed`) that reached
    /// the loop first already moved on — advancing again would skip the track
    /// `Next` landed on, or abandon the `Seek`. Either mismatch drops the advance.
    async fn handle_auto_advance(&mut self, track_id: String) {
        let still_completed = matches!(
            &self.slot,
            PlaybackSlot::Active(cur)
                if cur.prepared.track_info.track_id == track_id
                    && matches!(cur.phase, TrackPhase::Completed)
        );
        if !still_completed {
            debug!(
                track_id,
                "ignoring stale AutoAdvance: the completed track is no longer current-and-Completed"
            );
            return;
        }
        match self.side_pause_for_queue_front().await {
            Ok(Some(decision)) => {
                self.pause_for_side_end(decision);
                return;
            }
            Ok(None) => {}
            Err(()) => {
                self.stop().await;
                return;
            }
        }

        // Repeat-track replays the current track, so its preload (the queue's
        // next) is not what plays.
        if self.playback_queue.repeat_mode() != RepeatMode::Track {
            if let Some(preloaded_track_id) = self.next_track_id().map(|s| s.to_string()) {
                self.advance_and_play_preloaded(&preloaded_track_id, true, PlayTarget::Playing)
                    .await;
                return;
            }
        }

        match self.playback_queue.next_entry() {
            NextEntry::RepeatCurrent(next_track) => {
                info!("Repeat mode: track, replaying {}", next_track);
                self.play_track(
                    &next_track,
                    TrackStart::Natural,
                    PlayTarget::Playing,
                    TrackTransition::Repeat,
                )
                .await;
            }
            NextEntry::Play(next_track) => {
                info!("Playing from queue: {}", next_track);
                self.emit_queue_update();
                self.play_track(
                    &next_track,
                    TrackStart::Natural,
                    PlayTarget::Playing,
                    TrackTransition::AutoAdvance,
                )
                .await;
            }
            NextEntry::Stop => {
                info!("No next track available, stopping");
                self.emit_queue_update();
                self.stop().await;
            }
        }
    }

    async fn handle_position_event(&mut self, fmt: Arc<TrackFmt>, pos: std::time::Duration) {
        // A dead AirPlay receiver surfaces here — the audio thread keeps ticking
        // while its sends fail, so this regular path catches the failure and ends
        // AirPlay, resuming local paused (the shape a Cast/DLNA `ended` status has).
        if self
            .renderer
            .airplay_control()
            .is_some_and(|c| c.has_failed())
        {
            info!("airplay receiver unreachable; ending AirPlay and resuming local");
            self.end_airplay_and_resume_local().await;
            return;
        }

        // Samples are flowing, so any starvation episode is over.
        self.reset_starvation_episode();
        let mut actual_pos = fmt.position_offset + pos;
        // On AirPlay the drain has pulled (and the stream sent) audio the receiver
        // has not yet played — the ~2 s buffer ahead. Offset the position back by
        // the receiver latency so the bar matches what is audible, not what has
        // been transmitted.
        if let Renderer::AirPlay(airplay) = &self.renderer {
            let latency = std::time::Duration::from_secs_f64(
                f64::from(airplay.latency_frames) / f64::from(crate::airplay::session::SAMPLE_RATE),
            );
            actual_pos = actual_pos.saturating_sub(latency);
        }
        *self.current_position_shared.lock().unwrap() = Some(actual_pos);
        let raw_pos_ms = actual_pos.as_millis() as u64;
        let progress =
            crate::playback::format::compute_progress(raw_pos_ms, fmt.duration_ms, fmt.pregap_ms);
        let (adjusted_pos_ms, adjusted_dur_ms) =
            crate::playback::format::adjust_for_pregap(raw_pos_ms, fmt.duration_ms, fmt.pregap_ms);
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::PositionUpdate {
                position_ms: adjusted_pos_ms,
                duration_ms: adjusted_dur_ms,
                track_id: fmt.track_id.clone(),
                progress,
            },
        );

        // Persist the resume point at most once a second while the track is
        // actually advancing (Playing) — that is what keeps `playback_state`
        // crash-safe between the discrete events (load/pause/seek/stop) that
        // persist on their own. A Loading or Paused tick skips it: position isn't
        // moving, and pause/seek already wrote the row.
        let playing = matches!(
            &self.slot,
            PlaybackSlot::Active(cur) if matches!(cur.phase, TrackPhase::Playing)
        );
        if playing {
            let due = match self.last_position_persist {
                Some(last) => last.elapsed() >= std::time::Duration::from_secs(1),
                None => true,
            };
            if due {
                self.persist_playback_state().await;
            }
        }
    }

    fn handle_completion_event(
        &mut self,
        fmt: Arc<TrackFmt>,
        error_count: u32,
        samples_decoded: u64,
    ) {
        info!(
            "Track completed: {} ({} decode errors, {} samples)",
            fmt.track_id, error_count, samples_decoded
        );
        self.telemetry().event(TelemetryEvent::TrackCompleted {
            track_id: LocalId(fmt.track_id.clone()),
            decode_errors: error_count as u64,
        });
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::TrackCompleted {
                track_id: fmt.track_id.clone(),
            },
        );
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::DecodeStats {
                track_id: fmt.track_id.clone(),
                error_count,
                samples_decoded,
            },
        );
        // The track drained: mark the phase Completed. The persistent output stays
        // live because AutoAdvance and the side-pause decision still read the
        // source; the audio callback already flipped the atomic to Stopped (which
        // `sync_audio_state` below confirms), so it emits nothing further until
        // the source is replaced.
        if let PlaybackSlot::Active(cur) = &mut self.slot {
            cur.phase = TrackPhase::Completed;
        }
        self.sync_audio_state();
    }

    async fn handle_audio_event(&mut self, event: AudioEvent) {
        // Position/Completion/TrackCrossing carry their own logging (or none);
        // every other kind goes to the diagnostic log both players share.
        if !matches!(
            event,
            AudioEvent::Position(_) | AudioEvent::Completion(_) | AudioEvent::TrackCrossing(_)
        ) {
            log_stream_diagnostic("playback", &event);
        }
        match event {
            AudioEvent::Position((fmt, pos)) => self.handle_position_event(fmt, pos).await,
            AudioEvent::Completion((fmt, error_count, samples_decoded)) => {
                self.handle_completion_event(fmt, error_count, samples_decoded);
            }
            AudioEvent::TrackCrossing(crossing) => self.handle_track_crossed(crossing).await,
            AudioEvent::Starved {
                fmt,
                starved_ms,
                producer_finished,
                samples_decoded,
                ..
            } => {
                // `producer_finished` means a drained track awaiting AutoAdvance,
                // not the stall this watchdog targets.
                if !producer_finished {
                    self.handle_starvation(&fmt.track_id, samples_decoded, starved_ms)
                        .await;
                }
            }
            AudioEvent::StarvationEnded { .. } => {
                self.reset_starvation_episode();
            }
            AudioEvent::SourceLockMissed { .. } | AudioEvent::SourceLockReacquired { .. } => {}
        }
    }

    async fn drain_current_audio_events(&mut self) {
        while let Some(event) = self.output.as_mut().and_then(|o| o.audio_events.pop()) {
            self.handle_audio_event(event).await;
        }

        let dropped_required = self
            .output
            .as_ref()
            .map(|o| report_dropped_audio_events(&o.audio_events, "playback"))
            .unwrap_or(false);
        if dropped_required {
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::PlaybackError {
                    reason: crate::ui::PlaybackErrorReason::internal(
                        "Playback event queue dropped a required audio event".to_string(),
                    ),
                },
            );
            self.stop().await;
        }
    }
}

fn side_pause_prompt_between(
    current: &PlaybackTrackInfo,
    next: &PlaybackTrackInfo,
) -> Option<PlaybackSidePausePrompt> {
    if current.release_id != next.release_id {
        return None;
    }
    let current_side = current.side.as_ref()?;
    let next_side = next.side.as_ref()?;
    if current_side.side_letter == next_side.side_letter {
        return None;
    }
    let message_key = match current_side.medium {
        PhysicalSideMedium::Vinyl => SIDE_PAUSE_VINYL_MESSAGE_KEY,
        PhysicalSideMedium::Cassette => SIDE_PAUSE_CASSETTE_MESSAGE_KEY,
    };
    Some(PlaybackSidePausePrompt {
        id: format!(
            "{}:{}:{:?}",
            next.track_id, current_side.side_letter, current_side.medium
        ),
        title_key: SIDE_PAUSE_TITLE_KEY,
        side_letter: current_side.side_letter.clone(),
        message_key,
    })
}

/// The platform's audio output, with no default-device change listener. Called on
/// the service's dedicated thread so the sink owns any thread-bound device handle
/// it opens there (cpal builds lazily per stream; AAudio binds its writer thread).
/// This is the preview player's output, and the main player's on every platform
/// but macOS, which uses `default_audio_output_with_device_listener`.
#[cfg(not(target_os = "android"))]
pub(crate) fn default_audio_output(
) -> Result<Box<dyn AudioOutput>, crate::playback::audio_output::AudioError> {
    Ok(Box::new(
        crate::playback::cpal_output::CpalAudioOutput::new()?,
    ))
}

#[cfg(target_os = "android")]
pub(crate) fn default_audio_output(
) -> Result<Box<dyn AudioOutput>, crate::playback::audio_output::AudioError> {
    Ok(Box::new(
        crate::playback::aaudio_output::AAudioOutput::new()?
    ))
}

/// The main player's macOS output, which additionally registers a CoreAudio
/// listener that dispatches `OutputDeviceChanged` through `command_tx` when the
/// system default output device changes — so the persistent stream rebuilds onto
/// the new default. The preview player never gets a listener (it uses
/// `default_audio_output`), so exactly one listener exists at a time.
#[cfg(target_os = "macos")]
pub(crate) fn default_audio_output_with_device_listener(
    command_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
) -> Result<Box<dyn AudioOutput>, crate::playback::audio_output::AudioError> {
    Ok(Box::new(
        crate::playback::cpal_output::CpalAudioOutput::with_device_listener(move || {
            dispatch_command(&command_tx, PlaybackCommand::OutputDeviceChanged)
        })?,
    ))
}

impl PlaybackService {
    /// `restore_playback` is the platform's "Restore on launch" preference:
    /// `true` restores the saved queue/track/position from the `playback_state`
    /// row at startup; `false` starts with nothing in playback. The row itself
    /// is kept current either way (it's the crash-safe resume point): a track
    /// load, stop, or queue edit writes it immediately, and while a track is
    /// actually playing `handle_position_event` additionally writes it at
    /// most once a second, so `position_ms` never goes stale for longer than
    /// that — so flipping the preference on takes effect at the next launch.
    pub fn start(
        library_manager: LibraryManager,
        runtime_handle: tokio::runtime::Handle,
        position_update_interval_ms: u32,
        restore_playback: bool,
    ) -> PlaybackHandle {
        Self::start_inner(
            library_manager,
            runtime_handle,
            position_update_interval_ms,
            restore_playback,
            None,
        )
    }

    /// Start over a caller-supplied audio output, for tests that capture samples.
    pub fn start_with_output(
        library_manager: LibraryManager,
        runtime_handle: tokio::runtime::Handle,
        position_update_interval_ms: u32,
        restore_playback: bool,
        audio_output: Box<dyn AudioOutput>,
    ) -> PlaybackHandle {
        Self::start_inner(
            library_manager,
            runtime_handle,
            position_update_interval_ms,
            restore_playback,
            Some(audio_output),
        )
    }

    fn start_inner(
        library_manager: LibraryManager,
        runtime_handle: tokio::runtime::Handle,
        position_update_interval_ms: u32,
        restore_playback: bool,
        custom_output: Option<Box<dyn AudioOutput>>,
    ) -> PlaybackHandle {
        let (command_tx, command_rx) = tokio_mpsc::unbounded_channel();
        let (progress_tx, progress_rx) = tokio_mpsc::unbounded_channel();
        let progress_handle = PlaybackProgressHandle::new(progress_rx, runtime_handle.clone());
        let thread_slot = Arc::new(Mutex::new(None));
        let handle = PlaybackHandle {
            command_tx: command_tx.clone(),
            progress_handle: progress_handle.clone(),
            thread: thread_slot.clone(),
        };
        let command_tx_for_completion = command_tx.clone();
        let progress_handle_for_completion = progress_handle.clone();
        runtime_handle.spawn(async move {
            let mut progress_rx = progress_handle_for_completion.subscribe_all();
            while let Some(progress) = progress_rx.recv().await {
                match progress {
                    PlaybackProgress::TrackCompleted { track_id } => {
                        info!(
                            "Auto-advance: Track completed, sending AutoAdvance command: {}",
                            track_id
                        );
                        dispatch_command(
                            &command_tx_for_completion,
                            PlaybackCommand::AutoAdvance { track_id },
                        );
                    }
                    // A mid-flight read failure cancels the buffer and the decoder
                    // exits without a TrackCompleted, so without this the UI would
                    // sit in Playing forever.
                    PlaybackProgress::PlaybackError { .. } => {
                        dispatch_command(&command_tx_for_completion, PlaybackCommand::HaltOnError);
                    }
                    _ => {}
                }
            }
        });
        let join_handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create runtime");
            rt.block_on(async move {
                let audio_output: Box<dyn AudioOutput> = match custom_output {
                    Some(output) => output,
                    None => {
                        // The main player registers the macOS default-device
                        // listener; other platforms build the listener-less output.
                        #[cfg(target_os = "macos")]
                        let built = default_audio_output_with_device_listener(command_tx.clone());
                        #[cfg(not(target_os = "macos"))]
                        let built = default_audio_output();
                        match built {
                            Ok(output) => output,
                            Err(e) => {
                                error!("Failed to initialize audio output: {:?}", e);
                                return;
                            }
                        }
                    }
                };
                let queue_ids = library_manager.ids().clone();
                let preview = PreviewPlayer::new(
                    progress_tx.clone(),
                    command_tx.clone(),
                    position_update_interval_ms,
                );
                let mut service = PlaybackService {
                    library_manager,
                    command_tx: command_tx.clone(),
                    command_rx,
                    progress_tx,
                    playback_queue: PlaybackQueue::new(queue_ids),
                    current_position_shared: Arc::new(std::sync::Mutex::new(None)),
                    audio_output,
                    output: None,
                    slot: PlaybackSlot::Stopped,
                    load_generation_counter: 0,
                    preloaded_next: None,
                    preview,
                    main_was_playing_before_preview: false,
                    is_muted: false,
                    pre_mute_volume: 1.0,
                    position_update_interval_ms,
                    shared_file_buffers: HashMap::new(),
                    fetch_arbiter: FetchArbiter::new(),
                    starvation_episode: None,
                    last_position_persist: None,
                    first_audio_pending: None,
                    renderer: Renderer::Local,
                };
                service.restore_from_cache(restore_playback).await;
                service.run().await;
            });
        });
        *thread_slot.lock().unwrap() = Some(join_handle);
        handle
    }

    async fn apply_repeat_mode(&mut self, mode: RepeatMode) {
        if self.playback_queue.repeat_mode() == mode {
            return;
        }

        self.playback_queue.set_repeat_mode(mode);
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::RepeatModeChanged { mode },
        );
        self.emit_queue_update();
        self.persist_playback_state().await;
    }

    async fn run(&mut self) {
        info!("PlaybackService started");
        let mut library_event_rx = self.library_manager.subscribe_events();
        let mut audio_event_tick = tokio::time::interval(std::time::Duration::from_millis(10));
        audio_event_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = audio_event_tick.tick(), if self.output.is_some() => {
                    self.drain_current_audio_events().await;
                }
                Some(command) = self.command_rx.recv() => {
            // One event per user-intent command, at the point the loop picks it
            // up. Internal/system commands, queries, and continuous inputs map to
            // `None` and ship nothing.
            if let Some(kind) = playback_command_kind(&command) {
                self.telemetry().event(TelemetryEvent::PlaybackCommand { command: kind });
            }
            match command {
                PlaybackCommand::Play(track_id) => {
                    self.handle_play(track_id).await;
                }
                PlaybackCommand::PlayRelease { release_id, start_track_index, shuffle } => {
                    let track_ids = match self.library_manager.get_track_ids(&release_id).await {
                        Ok(ids) => ids,
                        Err(e) => {
                            error!("Failed to get tracks for release {release_id}: {e}");
                            self.telemetry_playback_failed(PlaybackOperation::LoadContext);
                            continue;
                        }
                    };

                    if track_ids.is_empty() {
                        warn!("PlayRelease: release {release_id} has no tracks");
                        continue;
                    }

                    self.stop_preview_for_main_playback();
                    let start = if shuffle {
                        // The seed is minted once, here, and carried into the
                        // context, so the order is reproducible and `Context`
                        // repeat can re-derive it.
                        ContextStart::Shuffled {
                            seed: rand::random(),
                        }
                    } else {
                        // `None` means "from the first track"; an out-of-range
                        // index is a bad caller value — clamp and log.
                        let index = match start_track_index {
                            Some(i) if i < track_ids.len() => i,
                            Some(i) => {
                                warn!(
                                    "PlayRelease: start index {i} out of range for {} tracks; starting at 0",
                                    track_ids.len()
                                );
                                0
                            }
                            None => 0,
                        };
                        ContextStart::Index(index)
                    };
                    let track_count = track_ids.len();
                    let first_track = self.playback_queue.play_release(
                        ContextSource::Release(release_id),
                        track_ids,
                        start,
                    );
                    self.emit_queue_update();
                    self.telemetry_playback_started(PlaybackStartSource::Release, track_count);
                    self.play_track(
                        &first_track,
                        TrackStart::Direct,
                        PlayTarget::Playing,
                        TrackTransition::Manual,
                    )
                    .await;
                }
                PlaybackCommand::PlayReleases(release_ids) => {
                    // The source is exactly the releases that contributed tracks,
                    // so a later shuffle/restore re-fetch stays in lockstep.
                    let (playable_ids, track_ids) =
                        self.load_release_set_tracks(release_ids).await;
                    if track_ids.is_empty() {
                        warn!("PlayReleases: no playable releases; nothing to play");
                        continue;
                    }
                    self.stop_preview_for_main_playback();
                    let track_count = track_ids.len();
                    let first_track = self.playback_queue.play_release(
                        ContextSource::releases(playable_ids),
                        track_ids,
                        ContextStart::Index(0),
                    );
                    self.emit_queue_update();
                    self.telemetry_playback_started(PlaybackStartSource::Releases, track_count);
                    self.play_track(
                        &first_track,
                        TrackStart::Direct,
                        PlayTarget::Playing,
                        TrackTransition::Manual,
                    )
                    .await;
                }
                PlaybackCommand::PlayLibraryShuffled => {
                    let track_ids = match self.fetch_source_tracks(&ContextSource::Library).await {
                        Ok(ids) => ids,
                        Err(e) => {
                            error!("PlayLibraryShuffled: couldn't load library tracks: {e}");
                            self.telemetry_playback_failed(PlaybackOperation::LoadContext);
                            continue;
                        }
                    };
                    if track_ids.is_empty() {
                        warn!("PlayLibraryShuffled: the library has no tracks; nothing to play");
                        continue;
                    }
                    self.stop_preview_for_main_playback();
                    // A fresh seed, so the order is reproducible and `Context`
                    // repeat can re-derive it.
                    let track_count = track_ids.len();
                    let first_track = self.playback_queue.play_release(
                        ContextSource::Library,
                        track_ids,
                        ContextStart::Shuffled {
                            seed: rand::random(),
                        },
                    );
                    self.emit_queue_update();
                    self.telemetry_playback_started(
                        PlaybackStartSource::LibraryShuffled,
                        track_count,
                    );
                    self.play_track(
                        &first_track,
                        TrackStart::Direct,
                        PlayTarget::Playing,
                        TrackTransition::Manual,
                    )
                    .await;
                }
                PlaybackCommand::Pause => {
                    self.pause();
                }
                PlaybackCommand::Resume => {
                    self.resume().await;
                }
                PlaybackCommand::Stop => {
                    self.stop().await;
                }
                PlaybackCommand::Next => {
                    self.handle_next().await;
                }
                PlaybackCommand::AutoAdvance { track_id } => {
                    self.handle_auto_advance(track_id).await;
                }
                PlaybackCommand::TrackReady {
                    track_id,
                    generation,
                } => {
                    self.resolve_track_ready(track_id, generation);
                }
                PlaybackCommand::HaltOnError => {
                    self.halt_on_error().await;
                }
                #[cfg(target_os = "macos")]
                PlaybackCommand::OutputDeviceChanged => {
                    self.handle_output_device_changed().await;
                }
                PlaybackCommand::Previous => {
                    self.handle_previous().await;
                }
                PlaybackCommand::Seek(position) => {
                    self.seek(position).await;
                }
                PlaybackCommand::SeekByRatio(ratio) => {
                    let position_ms = if let PlaybackSlot::Active(cur) = &self.slot {
                        let prepared = &cur.prepared;
                        Some(crate::playback::format::position_for_progress(
                            ratio,
                            prepared.duration.as_millis() as u64,
                            prepared.total_pregap_ms(),
                        ))
                    } else {
                        None
                    };
                    if let Some(position_ms) = position_ms {
                        self.seek(std::time::Duration::from_millis(position_ms))
                            .await;
                    }
                }
                PlaybackCommand::SetVolume(volume) => {
                    self.set_volume(volume);
                }
                PlaybackCommand::SetMuted(muted) => {
                    if muted == self.is_muted {
                        // Already there: nothing changes, nothing emits.
                    } else if muted {
                        self.pre_mute_volume = self.audio_output.get_volume();
                        self.is_muted = true;
                        self.audio_output.set_volume(0.0);
                        // Receiver mute is flaky across devices, so muting maps
                        // to receiver volume 0; the pre-mute level is remembered
                        // above and restored on unmute.
                        if let Renderer::Remote(remote) = &self.renderer {
                            remote.session.set_volume(0.0);
                        }
                        emit_progress(
                            &self.progress_tx,
                            PlaybackProgress::VolumeChanged { volume: 0.0 },
                        );
                        emit_progress(
                            &self.progress_tx,
                            PlaybackProgress::MuteChanged { is_muted: true },
                        );
                    } else {
                        self.is_muted = false;
                        let vol = self.pre_mute_volume;
                        self.audio_output.set_volume(vol);
                        if let Renderer::Remote(remote) = &self.renderer {
                            remote.session.set_volume(vol);
                        }
                        emit_progress(
                            &self.progress_tx,
                            PlaybackProgress::VolumeChanged { volume: vol },
                        );
                        emit_progress(
                            &self.progress_tx,
                            PlaybackProgress::MuteChanged { is_muted: false },
                        );
                    }
                }
                PlaybackCommand::AddToQueue(track_ids) => {
                    let count = track_ids.len() as u32;
                    self.playback_queue.add_to_queue(track_ids);
                    self.emit_queue_items_added(count);
                    self.on_queue_mutated().await;
                }
                PlaybackCommand::AddNext(track_ids) => {
                    let count = track_ids.len() as u32;
                    self.playback_queue.add_next(track_ids);
                    self.emit_queue_items_added(count);
                    self.on_queue_mutated().await;
                }
                PlaybackCommand::AddReleaseToQueue(release_id) => {
                    match self.library_manager.get_track_ids(&release_id).await {
                        Ok(track_ids) if !track_ids.is_empty() => {
                            let count = track_ids.len() as u32;
                            self.playback_queue.add_to_queue(track_ids);
                            self.emit_queue_items_added(count);
                            self.on_queue_mutated().await;
                        }
                        Ok(_) => {}
                        Err(e) => {
                            error!("Failed to add release {release_id} to queue: {e}");
                            self.telemetry_playback_failed(PlaybackOperation::QueueAdd);
                        }
                    }
                }
                PlaybackCommand::AddReleaseNext(release_id) => {
                    match self.library_manager.get_track_ids(&release_id).await {
                        Ok(track_ids) if !track_ids.is_empty() => {
                            let count = track_ids.len() as u32;
                            self.playback_queue.add_next(track_ids);
                            self.emit_queue_items_added(count);
                            self.on_queue_mutated().await;
                        }
                        Ok(_) => {}
                        Err(e) => {
                            error!("Failed to add release {release_id} next in queue: {e}");
                            self.telemetry_playback_failed(PlaybackOperation::QueueAdd);
                        }
                    }
                }
                PlaybackCommand::InsertInQueue(track_ids, index) => {
                    let count = track_ids.len() as u32;
                    self.playback_queue.insert_at(index, track_ids);
                    self.emit_queue_items_added(count);
                    self.on_queue_mutated().await;
                }
                PlaybackCommand::RemoveFromQueue(entry_id) => {
                    if let Some(removed) = self.playback_queue.remove(&entry_id) {
                        if self
                            .current_track_id()
                            .map(|id| id == removed.track_id)
                            .unwrap_or(false)
                        {
                            self.stop().await;
                            self.emit_queue_update();
                        } else {
                            self.on_queue_mutated().await;
                        }
                    } else {
                        // The id named no queued entry — a stale UI action against a
                        // queue that already moved on.
                        self.telemetry_anomaly(AnomalyKind::QueueEntryUnknown);
                    }
                }
                PlaybackCommand::ReorderQueue { entry_id, before } => {
                    if !self.playback_queue.reorder(&entry_id, before.as_ref()) {
                        self.telemetry_anomaly(AnomalyKind::QueueEntryUnknown);
                    }
                    self.on_queue_mutated().await;
                }
                PlaybackCommand::ClearUpNext => {
                    self.playback_queue.clear_up_next();
                    self.on_queue_mutated().await;
                }
                PlaybackCommand::ClearPlayingFrom => {
                    self.playback_queue.clear_playing_from();
                    self.on_queue_mutated().await;
                }
                PlaybackCommand::SetRepeatMode(mode) => {
                    self.apply_repeat_mode(mode).await;
                }
                PlaybackCommand::SetShuffle(on) => {
                    // The seed for the permutation `set_shuffle` performs on the
                    // rows it already holds; it uses it only when turning shuffle
                    // on. Shuffling changes what plays next, so the preload and
                    // the staged side-pause reconcile like any queue mutation.
                    let seed: u64 = rand::random();
                    self.playback_queue.set_shuffle(on, seed);
                    self.on_queue_mutated().await;
                }
                PlaybackCommand::ReevaluateSidePauseStaging => {
                    self.reevaluate_side_pause_staging().await;
                }
                PlaybackCommand::SkipTo(entry_id) => {
                    if let Some(entry) = self.playback_queue.skip_to(&entry_id) {
                        info!(
                            "SkipTo: jumping to queue entry {}, track {}",
                            entry.id.0, entry.track_id
                        );

                        self.emit_queue_update();
                        self.play_track(
                            &entry.track_id,
                            TrackStart::Direct,
                            PlayTarget::Playing,
                            TrackTransition::Manual,
                        )
                        .await;
                    } else {
                        // The id named no queued entry — a stale UI action.
                        self.telemetry_anomaly(AnomalyKind::QueueEntryUnknown);
                    }
                }
                PlaybackCommand::PreviewPlay(path) => {
                    self.preview_play(path).await;
                }
                PlaybackCommand::PreviewStop => {
                    self.preview_stop();
                }
                PlaybackCommand::PreviewTogglePause => {
                    self.preview_toggle_pause();
                }
                PlaybackCommand::PreviewSeekByRatio(ratio) => {
                    self.preview.seek_by_ratio(ratio).await;
                }
                PlaybackCommand::PreviewCompleted => {
                    self.preview_completed();
                }
                PlaybackCommand::GetVolume(reply) => {
                    let _ = reply.send(self.audio_output.get_volume());
                }
                PlaybackCommand::GetQueueProjection(reply) => {
                    if reply.send(self.queue_projection()).is_err() {
                        warn!("get queue projection receiver dropped before reply");
                    }
                }
                PlaybackCommand::Shutdown(reply) => {
                    self.persist_playback_state().await;
                    let _ = reply.send(());
                    break;
                }
                PlaybackCommand::SaveState(reply) => {
                    self.persist_playback_state().await;
                    let _ = reply.send(());
                }
                PlaybackCommand::PlayOn(connect) => {
                    self.handle_play_on(*connect).await;
                }
                PlaybackCommand::PlayOnAirPlay(connect) => {
                    self.handle_play_on_airplay(*connect).await;
                }
                PlaybackCommand::StopRemote => {
                    self.handle_stop_remote().await;
                }
                PlaybackCommand::RemoteStatus(status) => {
                    self.handle_remote_status(status).await;
                }
            }
                }
                Ok(event) = library_event_rx.recv() => {
                    if let LibraryEvent::TracksDeleted { track_ids } = event {
                        self.handle_tracks_deleted(track_ids).await;
                    }
                }
                else => break,
            }
        }
        info!("PlaybackService stopped");
    }
}

/// Map a command to its telemetry kind, or `None` for commands that don't ship:
/// internal/system (auto-advance, shutdown, track-ready), pure queries (get
/// volume/queue), and continuous inputs (volume, position, mute). Track-level
/// queue edits (add/insert/clear) are not user-intent milestones and ship
/// nothing; only the release-level and transport commands do.
fn playback_command_kind(command: &PlaybackCommand) -> Option<PlaybackCommandKind> {
    match command {
        PlaybackCommand::Play(_) => Some(PlaybackCommandKind::Play),
        PlaybackCommand::PlayRelease { .. } => Some(PlaybackCommandKind::PlayRelease),
        PlaybackCommand::PlayReleases(_) => Some(PlaybackCommandKind::PlayReleases),
        PlaybackCommand::PlayLibraryShuffled => Some(PlaybackCommandKind::PlayLibraryShuffled),
        PlaybackCommand::Next => Some(PlaybackCommandKind::Next),
        PlaybackCommand::Previous => Some(PlaybackCommandKind::Previous),
        PlaybackCommand::Seek(_) | PlaybackCommand::SeekByRatio(_) => {
            Some(PlaybackCommandKind::Seek)
        }
        PlaybackCommand::Pause => Some(PlaybackCommandKind::Pause),
        PlaybackCommand::Resume => Some(PlaybackCommandKind::Resume),
        PlaybackCommand::Stop => Some(PlaybackCommandKind::Stop),
        PlaybackCommand::SetShuffle(_) => Some(PlaybackCommandKind::SetShuffle),
        PlaybackCommand::SetRepeatMode(_) => Some(PlaybackCommandKind::SetRepeat),
        PlaybackCommand::AddReleaseToQueue(_) => Some(PlaybackCommandKind::AddReleaseToQueue),
        PlaybackCommand::AddReleaseNext(_) => Some(PlaybackCommandKind::AddReleaseNext),
        PlaybackCommand::RemoveFromQueue(_) => Some(PlaybackCommandKind::RemoveFromQueue),
        PlaybackCommand::ReorderQueue { .. } => Some(PlaybackCommandKind::ReorderQueue),
        PlaybackCommand::SkipTo(_) => Some(PlaybackCommandKind::SkipTo),
        PlaybackCommand::AutoAdvance { .. }
        | PlaybackCommand::TrackReady { .. }
        | PlaybackCommand::HaltOnError
        | PlaybackCommand::AddToQueue(_)
        | PlaybackCommand::AddNext(_)
        | PlaybackCommand::InsertInQueue(_, _)
        | PlaybackCommand::ClearUpNext
        | PlaybackCommand::ClearPlayingFrom
        | PlaybackCommand::ReevaluateSidePauseStaging
        | PlaybackCommand::SetVolume(_)
        | PlaybackCommand::SetMuted(_)
        | PlaybackCommand::PreviewPlay(_)
        | PlaybackCommand::PreviewStop
        | PlaybackCommand::PreviewTogglePause
        | PlaybackCommand::PreviewSeekByRatio(_)
        | PlaybackCommand::PreviewCompleted
        | PlaybackCommand::GetVolume(_)
        | PlaybackCommand::GetQueueProjection(_)
        | PlaybackCommand::Shutdown(_)
        | PlaybackCommand::SaveState(_)
        | PlaybackCommand::PlayOn(_)
        | PlaybackCommand::PlayOnAirPlay(_)
        | PlaybackCommand::StopRemote
        | PlaybackCommand::RemoteStatus(_) => None,
        #[cfg(target_os = "macos")]
        PlaybackCommand::OutputDeviceChanged => None,
    }
}

fn pregap_seek_position(pregap_ms: Option<i64>) -> Option<std::time::Duration> {
    pregap_ms
        .filter(|&p| p > 0)
        .map(|p| std::time::Duration::from_millis(p as u64))
}
