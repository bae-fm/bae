//! # Playback Service
//!
//! The playback service manages audio playback through a command-based architecture.
//! It runs in its own thread and processes commands from a channel.
//!
//! ## Audio State
//!
//! Audio state is a shared atomic (`AudioState` enum: `Stopped`, `Playing`, `Paused`).
//! The audio callback reads it on every iteration and outputs samples if `Playing`,
//! silence otherwise. Infrastructure (streams, buffers, decoders) is set up
//! separately via `init_streaming()`.
//!
//! ## Seek Flow
//!
//! 1. Cancel old streaming source (makes callback output silence)
//! 2. Cancel buffer to unblock old decoder
//! 3. Wait for old decoder to exit cleanly
//! 4. Uncancel buffer for reuse, reset read position to 0
//! 5. Spawn new decoder on same buffer with `seek_to` (FFmpeg-level seek)
//! 6. Call `init_streaming()` which drops old stream and creates new one
//! 7. State remains unchanged (Playing or Paused) - new stream inherits it
//! 8. Send `Seeked` progress event

use super::RepeatMode;
use super::{
    repeat_to_str, ContextStart, NextEntry, PersistedPlayback, PlaybackQueue, PreviousAction,
    QueueEntryId, QueueSnapshot, Traversal,
};
use crate::audio_codec::StreamingDecodeError;
use crate::db::{DbPlaybackContext, DbPlaybackState};
use crate::library::LibraryEvent;
use crate::library::LibraryManager;
use crate::library::ResolvedTrackAudio;
use crate::playback::audio_output::{AudioOutput, AudioStream, CompletionEvent, PositionEvent};
use crate::playback::data_source::{
    create_audio_reader, AudioDataReader, AudioReadConfig, LocalReader,
};
use crate::playback::error::PlaybackError;
use crate::playback::progress::emit_progress;
use crate::playback::progress::PreviewState;
use crate::playback::progress::{PlaybackProgress, PlaybackProgressHandle};
use crate::playback::source::{PlaybackSource, TrackCrossing, TrackFmt};
use crate::playback::sparse_buffer::{create_sparse_buffer, SharedSparseBuffer};
use crate::playback::{create_track_stream_pair, TrackStream};
use crate::util::format::PhysicalSideMedium;
use std::collections::HashSet;
use std::sync::{mpsc, Arc, Mutex};
use tokio::sync::mpsc as tokio_mpsc;
use tokio::sync::oneshot;
use tracing::{debug, error, info, trace, warn};

fn log_streaming_decode_failure(context: &str, error: StreamingDecodeError) -> Option<String> {
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

/// Snapshot of the most recent position display values.
///
/// Written on every position tick and by `emit_position_display` so that
/// late-mounting UI elements (e.g. the NSView created after startup restore)
/// can populate themselves immediately instead of waiting for the next tick.
#[derive(Debug, Clone)]
pub struct PositionDisplay {
    pub progress: f64,
}

/// Track metadata resolved once at prepare time, cached for the duration of playback.
/// Used to populate PlaybackState emissions so the bridge doesn't need DB access.
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

#[derive(Clone)]
struct SidePauseDecision {
    track_id: String,
    prompt: PlaybackSidePausePrompt,
}

/// Playback commands sent to the service
#[derive(Debug)]
pub enum PlaybackCommand {
    Play(String),
    PlayRelease {
        release_id: String,
        start_track_index: Option<usize>,
        shuffle: bool,
    },
    Pause,
    Resume,
    Stop,
    /// Manual next track (skip pregap)
    Next,
    /// Auto-advance from track completion (play pregap)
    AutoAdvance,
    /// Internal: the audio callback crossed a gapless track boundary — it
    /// advanced from the current track into the pre-staged next track within
    /// one persistent stream. The advance already happened in the audio layer;
    /// the handler reports the finishing track's decode stats, syncs track
    /// bookkeeping, and preloads the following track. It does not rebuild the
    /// stream. The payload carries both tracks' identities so the handler is
    /// a pure function of its input — no shared cell to consult for either
    /// the finishing track or the new current.
    TrackCrossed(TrackCrossing),
    /// Internal: the in-core decoder for `track_id` filled its ring buffer to
    /// the play threshold (or reached EOF). Sent from a watcher task awaiting
    /// the decoder's ready signal; the handler emits Playing/Paused only if this
    /// is still the live load. Identity is the prepared track's `cancel_token`
    /// Arc, not the track id: RepeatCurrent / RestartCurrent / re-Play replay the
    /// SAME id through a fresh load, so an id match would accept a ready signal
    /// from the abandoned load. A new load mints a new token, so comparing the
    /// Arc by pointer against `current_prepared` rejects the stale signal. The
    /// id is carried only to name the dropped track in the debug log.
    TrackReady {
        track_id: String,
        cancel_token: Arc<std::sync::atomic::AtomicBool>,
    },
    /// Internal: a mid-flight read failure (cloud or local) emitted a
    /// `PlaybackProgress::PlaybackError`. Sent from the progress self-subscription
    /// so the command loop tears playback down to Stopped rather than leaving a
    /// frozen Playing state with a stalled position bar.
    HaltOnError,
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
    ClearQueue,
    SetRepeatMode(RepeatMode),
    CycleRepeatMode,
    /// Skip to the queue entry with this per-instance id (manual action, skip pregap)
    SkipTo(QueueEntryId),
    /// Preview a local audio file (toggle: same path stops, different path switches).
    PreviewPlay(String),
    /// Stop any active preview.
    PreviewStop,
    /// Toggle pause/resume on the active preview.
    PreviewTogglePause,
    /// Seek by slider ratio (0.0–1.0) within the active preview.
    PreviewSeekByRatio(f64),
    /// Internal: preview file finished playing naturally.
    PreviewCompleted,
    /// Toggle between play and pause based on current audio state.
    TogglePlayPause,
    /// Toggle mute. Core saves pre-mute volume, sets to 0 or restores.
    ToggleMute,
    /// Query current volume. Response sent via oneshot.
    GetVolume(oneshot::Sender<f32>),
    /// Graceful shutdown: save state to disk, reply, then stop.
    Shutdown(oneshot::Sender<()>),
    /// Persist the current playback state without tearing down playback. Mobile
    /// calls this when backgrounded — it can't call `Shutdown` (that stops the
    /// background audio), so this snapshots state for a later cold launch.
    SaveState(oneshot::Sender<()>),
}
/// Current playback state — carries track metadata + total duration only.
///
/// Position data (progress, elapsed, remaining) flows through
/// `PlaybackProgress::PositionUpdate` (ticks) and `PlaybackProgress::Seeked`
/// (seeks, restore, pause/resume display refresh). Keeping position out of the
/// state event avoids the "dual-sink" problem where one event drives both
/// the SwiftUI store (slow) and NSView (fast).
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
fn dispatch_command(tx: &tokio_mpsc::UnboundedSender<PlaybackCommand>, cmd: PlaybackCommand) {
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

/// Handle to the playback service for sending commands
#[derive(Clone)]
pub struct PlaybackHandle {
    command_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
    progress_handle: PlaybackProgressHandle,
    last_position_display: std::sync::Arc<std::sync::Mutex<Option<PositionDisplay>>>,
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
    pub fn toggle_mute(&self) {
        dispatch_command(&self.command_tx, PlaybackCommand::ToggleMute);
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
    pub fn clear_queue(&self) {
        dispatch_command(&self.command_tx, PlaybackCommand::ClearQueue);
    }
    pub fn set_repeat_mode(&self, mode: RepeatMode) {
        dispatch_command(&self.command_tx, PlaybackCommand::SetRepeatMode(mode));
    }

    pub fn cycle_repeat_mode(&self) {
        dispatch_command(&self.command_tx, PlaybackCommand::CycleRepeatMode);
    }

    pub fn toggle_play_pause(&self) {
        dispatch_command(&self.command_tx, PlaybackCommand::TogglePlayPause);
    }

    pub async fn get_volume(&self) -> f32 {
        let (tx, rx) = oneshot::channel();
        dispatch_command(&self.command_tx, PlaybackCommand::GetVolume(tx));
        rx.await.unwrap_or_else(|e| {
            warn!("get_volume: playback loop dropped the response channel: {e}");
            1.0
        })
    }

    /// Graceful shutdown: saves playback state to disk, then stops the service.
    pub async fn shutdown(&self) {
        let (tx, rx) = oneshot::channel();
        dispatch_command(&self.command_tx, PlaybackCommand::Shutdown(tx));
        await_shutdown_ack(rx).await;
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

    /// Read the most recent position display values. Used by late-mounting
    /// views (e.g. the progress NSView after startup restore) to populate
    /// themselves immediately instead of waiting for the next position tick.
    pub fn get_last_position_display(&self) -> Option<PositionDisplay> {
        self.last_position_display.lock().unwrap().clone()
    }
    pub fn skip_to_entry(&self, entry_id: QueueEntryId) {
        dispatch_command(&self.command_tx, PlaybackCommand::SkipTo(entry_id));
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
        crate::playback::format::adjust_for_pregap(0, raw_dur, prepared.pregap_ms);
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
            pregap_ms: self.pregap_ms,
            position_offset,
            replay_gain_linear: self.replay_gain_linear,
        }
    }

    /// Decoder window for this track beginning at `start_sample + offset` (FFmpeg
    /// seeks and trims lead-in there) and stopping at the track's end. `offset` is
    /// the in-track sample to begin at -- 0 for a natural start or gapless
    /// advance, the pregap skip or a seek position otherwise. Shared by the
    /// play/preload/seek paths so they can't drift on the seek/trim mapping.
    fn decode_params(&self, offset: u64) -> StreamDecodeParams {
        StreamDecodeParams {
            target_sample: self.start_sample + offset,
            stop_at_sample: self.end_sample,
            end_byte: self.end_byte,
        }
    }
}

struct PlaybackPreparedTrack {
    track_info: PlaybackTrackInfo,
    /// Raw audio buffer (may have headers prepended for CUE/FLAC).
    /// Reused across seeks -- cancel_reads, join decoder, uncancel, new decoder.
    /// The data reader stays alive across seeks (only reads are cancelled, not the reader).
    buffer: SharedSparseBuffer,
    /// If true, the buffer is shared with other tracks (from the cache).
    /// Don't cancel() it during teardown — other decoders may be using it.
    buffer_shared: bool,
    /// Per-decoder cancellation token. Set to true to stop the AVIO read callback
    /// without affecting other decoders on the same buffer.
    cancel_token: Arc<std::sync::atomic::AtomicBool>,
    /// Sample rate in Hz for time-to-sample conversion
    sample_rate: u32,
    channels: u32,
    /// Pre-gap duration in ms (for CUE/FLAC tracks)
    pregap_ms: Option<i64>,
    /// Track duration from metadata
    duration: std::time::Duration,
    /// This track's sample window in its backing file (start 0 / end None = whole file).
    start_sample: u64,
    end_sample: Option<u64>,
    /// This track's byte span in its backing file (frame-granular; end None =
    /// whole file). The decoder hands `end_byte` to its reader as the read-ahead
    /// ceiling so the fill buffers the rest of the current track.
    end_byte: Option<u64>,
    /// Linear playback gain folded into the audio callback's volume multiply.
    /// Derived once here from the replay-gain mode and the stored loudness/peak
    /// measurements; `1.0` = no change (Off, or no usable measurement).
    replay_gain_linear: f32,
}

/// Finalize a PlaybackPreparedTrack from resolved audio, display info, and buffer.
fn finalize_playback_track(
    resolved: ResolvedTrackAudio,
    track_info: PlaybackTrackInfo,
    buffer: SharedSparseBuffer,
    buffer_shared: bool,
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
        buffer,
        buffer_shared,
        cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        sample_rate: resolved.sample_rate,
        channels: resolved.channels,
        pregap_ms: resolved.pregap_ms,
        duration,
        start_sample: resolved.start_sample,
        end_sample: resolved.end_sample,
        end_byte: resolved.end_byte,
        replay_gain_linear,
    }
}

async fn prepare_track_for_playback(
    library_manager: &LibraryManager,
    track_id: &str,
    shared_file_buffer: &mut Option<(String, SharedSparseBuffer)>,
    progress_tx: tokio_mpsc::UnboundedSender<PlaybackProgress>,
) -> Result<PlaybackPreparedTrack, PlaybackError> {
    let (resolved, track_info) = library_manager
        .resolve_track_audio_and_info(track_id)
        .await
        .map_err(PlaybackError::database)?;

    let source_size = resolved.file_size;

    // Tracks sharing a source file (the tracks of a CUE image) share one buffer.
    // Key a readable source by its local path, a cloud-only source by its file
    // id; a source with no readable bytes (upload pending, or unreachable) is
    // never cached -- it falls through to the reader, which reports the error.
    use crate::library::manager::ReadableFileSource;
    let cache_key: Option<String> = match &resolved.source {
        ReadableFileSource::Local(path) => Some(path.to_string_lossy().into_owned()),
        ReadableFileSource::CloudOnly => Some(resolved.file_id.clone()),
        ReadableFileSource::Unreachable | ReadableFileSource::UploadPendingSourceMissing => None,
    };

    let cached = cache_key.as_ref().and_then(|key| {
        shared_file_buffer
            .as_ref()
            .filter(|(k, _)| k == key)
            .map(|(_, buf)| buf.clone())
    });

    let mut is_shared = cached.is_some();
    let buffer = if let Some(buf) = cached {
        info!("Reusing cached file buffer");
        buf.uncancel();
        buf
    } else {
        let buffer = create_sparse_buffer(source_size);
        let reader = create_audio_reader(
            resolved.source.clone(),
            &resolved.file_id,
            &resolved.cloud_key,
            library_manager,
            move |path| AudioReadConfig { path, source_size },
        )?;
        reader.start_reading(buffer.clone(), progress_tx);
        if let Some(key) = cache_key {
            *shared_file_buffer = Some((key, buffer.clone()));
            is_shared = true;
        }
        buffer
    };

    // Read the replay-gain mode once here and pass it down (DI: one read at the
    // top, not a static lookup buried in `finalize_playback_track`).
    let replay_gain_mode = library_manager.get_config().replay_gain_mode;

    Ok(finalize_playback_track(
        resolved,
        track_info,
        buffer,
        is_shared,
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
    stream: Option<Box<dyn AudioStream>>,
    /// Current track prepared data and streaming state
    current_prepared: Option<PlaybackPreparedTrack>,
    /// Current playing source. A `PlaybackSource` wraps the current track and an
    /// optional pre-staged next track so the audio callback can advance across a
    /// track boundary without rebuilding the stream.
    current_playback_source: Option<Arc<Mutex<PlaybackSource>>>,
    /// JoinHandle for the current decoder thread (needed for seek cancellation)
    current_decoder_handle: Option<std::thread::JoinHandle<()>>,
    /// Listener task handles for the current track (position ticks + completion)
    current_listener_handles: Vec<tokio::task::JoinHandle<()>>,
    /// Preloaded next track prepared data
    next_prepared: Option<PlaybackPreparedTrack>,
    /// Preloaded next-track source held for the stream-rebuild path (a format
    /// change vs the live stream, or no live stream yet). When the next track
    /// shares the live stream's format it is staged into the `PlaybackSource`
    /// instead and this stays `None`.
    next_track_stream: Option<TrackStream>,
    /// JoinHandle for the preloaded next track decoder thread
    next_decoder_handle: Option<std::thread::JoinHandle<()>>,
    /// Mute state — core tracks this so UI doesn't need to.
    is_muted: bool,
    pre_mute_volume: f32,
    /// Cached track info for the currently playing track.
    current_track_info: Option<PlaybackTrackInfo>,
    // -- Preview playback state --
    /// Separate audio output for preview (lazily created)
    preview_audio_output: Option<Box<dyn AudioOutput>>,
    /// Stream for preview playback
    preview_stream: Option<Box<dyn AudioStream>>,
    /// Streaming source for preview (to cancel on stop). Wrapped in a
    /// single-track `PlaybackSource` (preview never chains) to share the audio
    /// output's stream interface.
    preview_playback_source: Option<Arc<Mutex<PlaybackSource>>>,
    /// Path of the file currently being previewed
    preview_path: Option<String>,
    /// Whether the main player was playing before preview started (to resume on stop)
    main_was_playing_before_preview: bool,
    /// Last known duration for the preview file
    preview_duration: std::time::Duration,
    /// Last known position for the preview file
    preview_position: std::time::Duration,
    /// Abort handles for preview position listener tasks
    preview_listener_handles: Vec<tokio::task::JoinHandle<()>>,
    /// Sparse buffer for the current preview (retained across seeks)
    preview_buffer: Option<SharedSparseBuffer>,
    /// JoinHandle for the preview decoder thread (needed for seek cancellation)
    preview_decoder_handle: Option<std::thread::JoinHandle<()>>,
    /// Seek offset for the current preview (added to decoder-relative position)
    preview_seek_offset: std::time::Duration,
    preview_sample_rate: u32,
    preview_channels: u32,
    /// How often (ms) the audio callback sends position updates to the UI.
    position_update_interval_ms: u32,
    /// Cached full-file buffer for frame-dependent codecs (APE).
    /// Tracks from the same source file share a buffer to avoid re-reading.
    /// Key is the resolved local file path (or file ID for cloud).
    shared_file_buffer: Option<(String, SharedSparseBuffer)>,
    /// Shared with PlaybackHandle. Written on every position tick and by
    /// `emit_position_display`; read by late-mounting views.
    last_position_display: Arc<std::sync::Mutex<Option<PositionDisplay>>>,
    /// Sender cloned into each `PlaybackSource`; fired by the audio callback
    /// when it crosses a gapless track boundary, carrying the finishing and
    /// incoming track identities + the finishing track's decode stats.
    /// Bridged to `TrackCrossed`.
    boundary_tx: tokio_mpsc::UnboundedSender<TrackCrossing>,
    pending_side_pause: Option<SidePauseDecision>,
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

/// Construct the platform's concrete audio output. Called on the service's
/// dedicated thread so the sink owns any thread-bound device handle it opens
/// there (cpal builds lazily per stream; AAudio binds its writer thread).
#[cfg(not(target_os = "android"))]
fn default_audio_output() -> Result<Box<dyn AudioOutput>, crate::playback::audio_output::AudioError>
{
    Ok(Box::new(
        crate::playback::cpal_output::CpalAudioOutput::new()?,
    ))
}

#[cfg(target_os = "android")]
fn default_audio_output() -> Result<Box<dyn AudioOutput>, crate::playback::audio_output::AudioError>
{
    Ok(Box::new(
        crate::playback::aaudio_output::AAudioOutput::new()?
    ))
}

struct StreamSetup {
    stream: Box<dyn AudioStream>,
    bridge_handles: Vec<tokio::task::JoinHandle<()>>,
    position_rx: tokio_mpsc::UnboundedReceiver<PositionEvent>,
    completion_rx: tokio_mpsc::UnboundedReceiver<CompletionEvent>,
}

/// Spawn a task that forwards events from the audio callback's sync `mpsc`
/// receiver to a tokio async sender, exiting once either side closes. The
/// two channels into the audio callback share this shape — separate spawn
/// blocks were a near-duplicate.
fn spawn_sync_to_async_bridge<T: Send + 'static>(
    sync_rx: mpsc::Receiver<T>,
    async_tx: tokio_mpsc::UnboundedSender<T>,
    name: &'static str,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn({
        let sync_rx = Arc::new(std::sync::Mutex::new(sync_rx));
        async move {
            loop {
                let rx = sync_rx.clone();
                match tokio::task::spawn_blocking(move || rx.lock().unwrap().recv()).await {
                    Ok(Ok(event)) => {
                        if async_tx.send(event).is_err() {
                            // Async receiver gone (listener dropped).
                            debug!("{name} bridge: async receiver dropped; exiting");
                            break;
                        }
                    }
                    _ => break,
                }
            }
        }
    })
}

async fn setup_audio_stream(
    audio_output: &mut dyn AudioOutput,
    source: Arc<Mutex<PlaybackSource>>,
    position_update_interval_ms: u32,
) -> Result<StreamSetup, PlaybackError> {
    let (source_sample_rate, source_channels) = {
        let guard = source.lock().unwrap();
        (guard.sample_rate(), guard.channels())
    };

    let (position_tx, position_rx) = mpsc::channel::<PositionEvent>();
    let (completion_tx, completion_rx) = mpsc::channel::<CompletionEvent>();
    let (position_tx_async, position_rx_async) = tokio_mpsc::unbounded_channel();
    let (completion_tx_async, completion_rx_async) = tokio_mpsc::unbounded_channel();

    let h1 = spawn_sync_to_async_bridge(position_rx, position_tx_async, "Position");
    let h2 = spawn_sync_to_async_bridge(completion_rx, completion_tx_async, "Completion");

    let stream = match audio_output.create_stream(
        source,
        source_sample_rate,
        source_channels,
        position_tx,
        completion_tx,
        position_update_interval_ms,
    ) {
        Ok(stream) => stream,
        Err(e) => {
            h1.abort();
            h2.abort();
            return Err(PlaybackError::task(format!("Audio stream: {:?}", e)));
        }
    };

    Ok(StreamSetup {
        stream,
        bridge_handles: vec![h1, h2],
        position_rx: position_rx_async,
        completion_rx: completion_rx_async,
    })
}

/// The sample-space decoder window for one stream: `target_sample` is where
/// FFmpeg seeks and trims lead-in (the track's start plus any pregap/seek
/// offset); `stop_at_sample` ends output at the track's end (`None` = to EOF).
struct StreamDecodeParams {
    target_sample: u64,
    stop_at_sample: Option<u64>,
    /// The track's end byte offset -- the read-ahead ceiling. `None` = whole file.
    end_byte: Option<u64>,
}

impl StreamDecodeParams {
    /// Run the streaming decoder for this window: FFmpeg seeks and trims at
    /// `target_sample` and stops at `stop_at_sample`. Shared by the play/seek and
    /// preload paths so they can't drift on the seek/trim mapping.
    fn run_decoder(
        &self,
        buffer: SharedSparseBuffer,
        sink: &mut crate::playback::track_stream::TrackSink,
        cancel: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<(), StreamingDecodeError> {
        crate::audio_codec::decode_audio_streaming(
            buffer,
            sink,
            Some(self.target_sample),
            Some(self.target_sample),
            self.stop_at_sample,
            self.end_byte,
            cancel,
        )
    }
}

impl PlaybackService {
    /// Compute the display values for `position_ms` on the current track,
    /// write them to `last_position_display`, and emit a `Seeked` progress event.
    ///
    /// This is the single sink for non-tick position updates (seek, restore,
    /// pause/resume refresh). Writing the Arc without emitting would leave the
    /// NSView stale; emitting without writing the Arc would leave late-mounting
    /// views without a cached value to query. Always go through this helper.
    fn emit_position_display(&self, position_ms: u64, track_id: String) {
        let Some(prepared) = &self.current_prepared else {
            return;
        };
        let raw_dur_ms = prepared.duration.as_millis() as u64;
        let pregap_ms = prepared.pregap_ms;
        let (adjusted_pos_ms, adjusted_dur_ms) =
            crate::playback::format::adjust_for_pregap(position_ms, raw_dur_ms, pregap_ms);
        let progress =
            crate::playback::format::compute_progress(position_ms, raw_dur_ms, pregap_ms);

        *self.last_position_display.lock().unwrap() = Some(PositionDisplay { progress });

        emit_progress(
            &self.progress_tx,
            PlaybackProgress::Seeked {
                position_ms: adjusted_pos_ms,
                duration_ms: adjusted_dur_ms,
                track_id,
                progress,
            },
        );
    }

    /// The fields the Playing and Paused states share, read from the current
    /// prepared track. Position data is excluded — it flows through
    /// PositionUpdate/Seeked events.
    fn current_state_fields(&self) -> (PlaybackTrackInfo, u64) {
        let track_info = self
            .current_track_info
            .clone()
            .expect("no current_track_info");
        let prepared = self.current_prepared.as_ref().expect("no current_prepared");
        (track_info, pregap_adjusted_duration(prepared))
    }

    /// Build a Playing state from the current prepared track and track info.
    fn make_playing_state(&self) -> PlaybackState {
        let (track_info, duration_ms) = self.current_state_fields();
        PlaybackState::Playing {
            track_info,
            duration_ms,
        }
    }

    /// Build a Paused state from the current prepared track and track info.
    fn make_paused_state(&self, reason: PlaybackPauseReason) -> PlaybackState {
        let (track_info, duration_ms) = self.current_state_fields();
        PlaybackState::Paused {
            track_info,
            duration_ms,
            reason,
        }
    }

    /// Emit a `StateChanged` for the current track's play/pause state. Shared
    /// by the play, gapless-advance, and rebuild-advance paths.
    fn emit_current_state(&self) {
        let state = if self.audio_output.is_paused() {
            self.make_paused_state(PlaybackPauseReason::Manual)
        } else {
            self.make_playing_state()
        };
        emit_progress(&self.progress_tx, PlaybackProgress::StateChanged { state });
    }

    // Helper accessors for current/next track state
    fn current_track_id(&self) -> Option<&str> {
        self.current_prepared
            .as_ref()
            .map(|p| p.track_info.track_id.as_str())
    }

    fn next_track_id(&self) -> Option<&str> {
        self.next_prepared
            .as_ref()
            .map(|p| p.track_info.track_id.as_str())
    }

    /// Abort current track listener tasks (position ticks + completion).
    fn abort_current_listeners(&mut self) {
        for handle in self.current_listener_handles.drain(..) {
            handle.abort();
        }
    }

    /// Tear down the outgoing track before a manual switch (Play / SkipTo /
    /// Previous / Next-without-preload). Mirrors `stop()`'s current-track
    /// teardown — cancel the playback source so the callback goes silent, signal
    /// the decoder cancel token and cancel the buffer (so the decoder thread
    /// exits its park loop instead of filling a ring nobody pulls, and the
    /// data reader's fill loop stops fetching), and abort listeners — but leaves
    /// the audio state and the shared-buffer cache alone so the incoming track
    /// owns the transition. The buffer is spared when it's shared (the
    /// same-source reuse path appends into it via `uncancel`); a shared buffer
    /// is dropped wholesale by `stop()` instead.
    ///
    /// The decoder thread is not joined here: joining would block the command
    /// loop, and the cancel token plus buffer cancel already make it exit
    /// promptly. `play_track` overwrites `current_decoder_handle` with the new
    /// thread's handle right after.
    fn teardown_current_track(&mut self) {
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }
        if let Some(source) = self.current_playback_source.take() {
            match source.lock() {
                Ok(guard) => guard.cancel(),
                // A poisoned lock means the cpal callback thread panicked while
                // holding it. The source is being dropped here anyway, so the
                // cancel it would have requested is moot — log and move on.
                Err(_) => warn!("playback source lock poisoned during teardown; skipping cancel"),
            }
        }
        if let Some(prepared) = &self.current_prepared {
            prepared
                .cancel_token
                .store(true, std::sync::atomic::Ordering::Release);
            if !prepared.buffer_shared {
                prepared.buffer.cancel();
            }
        }
        self.abort_current_listeners();
        self.current_prepared = None;
        self.current_track_info = None;
        self.current_decoder_handle = None;
    }

    /// Tear down the decoded-audio pipeline for a seek.
    ///
    /// Cancels reads to unblock the old decoder, but the data reader keeps
    /// filling the buffer. After this, the buffer is ready for a new decoder
    /// to read from position 0.
    async fn teardown_decoder_for_seek(
        source: &mut Option<Arc<Mutex<PlaybackSource>>>,
        buffer: &SharedSparseBuffer,
        cancel_token: &Arc<std::sync::atomic::AtomicBool>,
        decoder_handle: &mut Option<std::thread::JoinHandle<()>>,
        buffer_shared: bool,
    ) {
        // Cancel streaming source (cpal callback outputs silence)
        if let Some(src) = source.take() {
            if let Ok(guard) = src.lock() {
                guard.cancel();
            }
        }

        // Cancel this decoder's AVIO reads via its token.
        cancel_token.store(true, std::sync::atomic::Ordering::Release);

        // For non-shared buffers, also cancel buffer reads to unblock the reader.
        // For shared buffers, only the cancel_token is used — other decoders
        // (e.g. preloaded next track) must not be disturbed.
        if !buffer_shared {
            buffer.cancel_reads();
        }
        // Wake up any readers blocked on the condvar so they can check the cancel token
        buffer.wake_readers();

        // Wait for decoder thread to exit. Surface a thread panic as an error
        // (decoder bug, real signal); tokio join failures (panic in the
        // spawn_blocking wrapper itself, runtime shutdown) get a warn.
        if let Some(handle) = decoder_handle.take() {
            match tokio::task::spawn_blocking(move || handle.join()).await {
                Ok(Ok(())) => {}
                Ok(Err(panic)) => {
                    error!("Decoder thread panicked during seek teardown: {:?}", panic);
                }
                Err(e) => {
                    warn!("spawn_blocking failed while joining decoder thread: {e}");
                }
            }
        }

        // Uncancel buffer reads for new decoders (only needed for non-shared)
        if !buffer_shared {
            buffer.uncancel();
        }
    }

    /// Initialize streaming infrastructure without changing audio state.
    ///
    /// Sets up the cpal stream, position listeners, and completion handlers.
    /// The audio output state remains unchanged - caller must explicitly
    /// call `audio_output.set_state(Playing)` to start audio output.
    ///
    /// Returns true if initialization succeeded, false on error.
    async fn init_streaming(&mut self, source: TrackStream, fmt: TrackFmt) -> bool {
        // Drop old stream first
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }

        // fmt is the per-stream formatting envelope; the caller built it from
        // its prepared track (see `PlaybackPreparedTrack::track_fmt`). The
        // audio callback tags every position/completion emit with it; at a
        // gapless boundary the callback swaps to the staged next track's fmt.
        let position_offset = fmt.position_offset;

        // Wrap the track source in a PlaybackSource so the audio callback can
        // advance to a pre-staged next track without rebuilding the stream.
        let gapless = Arc::new(Mutex::new(PlaybackSource::new(
            source,
            fmt,
            self.boundary_tx.clone(),
        )));

        let setup = match setup_audio_stream(
            &mut *self.audio_output,
            gapless.clone(),
            self.position_update_interval_ms,
        )
        .await
        {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to create streaming audio stream: {:?}", e);
                return false;
            }
        };

        if let Err(e) = setup.stream.play() {
            error!("Failed to start streaming playback: {:?}", e);
            for h in setup.bridge_handles {
                h.abort();
            }
            return false;
        }

        // Update state
        self.stream = Some(setup.stream);
        self.current_playback_source = Some(gapless);
        *self.current_position_shared.lock().unwrap() = Some(position_offset);

        // Spawn position/completion listener. Each event arrives tagged with
        // the fmt of the track it belongs to (set by the audio callback at
        // emit time), so the handlers are pure functions of their payload.
        let progress_tx = self.progress_tx.clone();
        let current_position_shared = self.current_position_shared.clone();
        let last_position_display = self.last_position_display.clone();
        let mut position_rx_async = setup.position_rx;
        let mut completion_rx_async = setup.completion_rx;

        let listener_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some((fmt, pos)) = position_rx_async.recv() => {
                        let actual_pos = fmt.position_offset + pos;
                        *current_position_shared.lock().unwrap() = Some(actual_pos);
                        let raw_pos_ms = actual_pos.as_millis() as u64;
                        let progress = crate::playback::format::compute_progress(raw_pos_ms, fmt.duration_ms, fmt.pregap_ms);
                        *last_position_display.lock().unwrap() = Some(PositionDisplay {
                            progress,
                        });
                        let (adjusted_pos_ms, adjusted_dur_ms) =
                            crate::playback::format::adjust_for_pregap(raw_pos_ms, fmt.duration_ms, fmt.pregap_ms);
                        emit_progress(
                            &progress_tx,
                            PlaybackProgress::PositionUpdate {
                                position_ms: adjusted_pos_ms,
                                duration_ms: adjusted_dur_ms,
                                track_id: fmt.track_id.clone(),
                                progress,
                            },
                        );
                    }
                    Some((fmt, error_count, samples_decoded)) = completion_rx_async.recv() => {
                        info!("Track completed: {} ({} decode errors, {} samples)", fmt.track_id, error_count, samples_decoded);
                        emit_progress(
                            &progress_tx,
                            PlaybackProgress::TrackCompleted {
                                track_id: fmt.track_id.clone(),
                            },
                        );
                        emit_progress(
                            &progress_tx,
                            PlaybackProgress::DecodeStats {
                                track_id: fmt.track_id.clone(),
                                error_count,
                                samples_decoded,
                            },
                        );
                        break;
                    }
                    else => break,
                }
            }
        });

        self.current_listener_handles = vec![setup.bridge_handles, vec![listener_handle]]
            .into_iter()
            .flatten()
            .collect();

        true
    }

    /// Spawn the in-core decoder for the current prepared track, build the audio
    /// stream, and arm the ready-watcher — the shared tail of the play and seek
    /// paths. Reads the buffer and cancel token from `current_prepared` (the
    /// caller stages it first), so the watcher's `TrackReady` carries the live
    /// load's token and the handler ignores a stale signal.
    ///
    /// Returns whether `init_streaming` succeeded; the caller owns the
    /// failure path (its cleanup differs) and any audio-state change.
    /// Audio doesn't flow until the ring fills, so the caller may set the
    /// audio state after this returns without racing the watcher.
    async fn start_decoder_and_watch(
        &mut self,
        decode: StreamDecodeParams,
        fmt: TrackFmt,
        sample_rate: u32,
        channels: u32,
        track_id: String,
    ) -> bool {
        let prepared = self
            .current_prepared
            .as_ref()
            .expect("start_decoder_and_watch requires a staged current_prepared");
        let decoder_buffer = prepared.buffer.clone();
        let cancel_token = prepared.cancel_token.clone();

        // Create decoder sink/source with the track's sample rate and spawn the
        // in-core FFmpeg decoder thread that fills the sink's ring buffer.
        let (mut sink, source, ready_rx) = create_track_stream_pair(sample_rate, channels);

        let decoder_handle = {
            let decoder_cancel = cancel_token.clone();
            // Kept to tell a genuine decode failure from normal teardown (seek /
            // stop / track change all cancel the token before the decoder exits).
            let teardown_check = cancel_token.clone();
            let error_tx = self.progress_tx.clone();
            std::thread::spawn(move || {
                if let Err(e) = decode.run_decoder(decoder_buffer, &mut sink, decoder_cancel) {
                    if let Some(message) = log_streaming_decode_failure("Streaming decode", e) {
                        if !teardown_check.load(std::sync::atomic::Ordering::Relaxed) {
                            emit_progress(
                                &error_tx,
                                PlaybackProgress::PlaybackError {
                                    reason: crate::ui::PlaybackErrorReason::internal(format!(
                                        "Playback decode failed: {message}"
                                    )),
                                },
                            );
                        }
                    }
                }
            })
        };
        self.current_decoder_handle = Some(decoder_handle);

        if !self.init_streaming(source, fmt).await {
            return false;
        }

        // Hold Playing until audio is actually flowing. The in-core decoder
        // signals `ready_rx` when the ring buffer fills to the play threshold
        // (or hits EOF for a short track); a watcher task turns that into a
        // `TrackReady` command so the command loop stays responsive to
        // Stop/Pause during a slow cloud load. Awaiting inline would wedge the
        // loop.
        let command_tx = self.command_tx.clone();
        tokio::spawn(async move {
            // Err means the decoder dropped its sink before signalling ready (it
            // died or was cancelled). The decode-failure path drives playback to
            // Stopped, and a cancelled load is no longer current so TrackReady
            // would be ignored anyway — the error path owns recovery, so just
            // record the dropped watcher.
            match ready_rx.await {
                Ok(()) => dispatch_command(
                    &command_tx,
                    PlaybackCommand::TrackReady {
                        track_id,
                        cancel_token,
                    },
                ),
                Err(_) => {
                    debug!("ready watcher dropped before signal for track {track_id}")
                }
            }
        });

        true
    }

    pub fn start(
        library_manager: LibraryManager,
        runtime_handle: tokio::runtime::Handle,
        position_update_interval_ms: u32,
    ) -> PlaybackHandle {
        Self::start_inner(
            library_manager,
            runtime_handle,
            position_update_interval_ms,
            None,
        )
    }

    /// Start with a custom audio output (for tests that need to capture samples).
    pub fn start_with_output(
        library_manager: LibraryManager,
        runtime_handle: tokio::runtime::Handle,
        position_update_interval_ms: u32,
        audio_output: Box<dyn AudioOutput>,
    ) -> PlaybackHandle {
        Self::start_inner(
            library_manager,
            runtime_handle,
            position_update_interval_ms,
            Some(audio_output),
        )
    }

    fn start_inner(
        library_manager: LibraryManager,
        runtime_handle: tokio::runtime::Handle,
        position_update_interval_ms: u32,
        custom_output: Option<Box<dyn AudioOutput>>,
    ) -> PlaybackHandle {
        let (command_tx, command_rx) = tokio_mpsc::unbounded_channel();
        let (progress_tx, progress_rx) = tokio_mpsc::unbounded_channel();
        let progress_handle = PlaybackProgressHandle::new(progress_rx, runtime_handle.clone());
        let last_position_display = Arc::new(std::sync::Mutex::new(None));
        let handle = PlaybackHandle {
            command_tx: command_tx.clone(),
            progress_handle: progress_handle.clone(),
            last_position_display: last_position_display.clone(),
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
                        dispatch_command(&command_tx_for_completion, PlaybackCommand::AutoAdvance);
                    }
                    // A mid-flight read failure cancelled the buffer; the decoder
                    // exits without TrackCompleted, so without this the UI would
                    // sit in Playing forever. Drive playback down to Stopped.
                    PlaybackProgress::PlaybackError { .. } => {
                        dispatch_command(&command_tx_for_completion, PlaybackCommand::HaltOnError);
                    }
                    _ => {}
                }
            }
        });
        // Bridge gapless-boundary signals (sent from the audio callback via each
        // PlaybackSource) to the command loop. Async so it drains without a
        // blocking thread and is cancelled cleanly on runtime shutdown — the
        // sender outlives any single stream, so a blocking receiver would wedge
        // runtime teardown.
        let (boundary_tx, mut boundary_rx) = tokio_mpsc::unbounded_channel::<TrackCrossing>();
        let command_tx_for_boundary = command_tx.clone();
        runtime_handle.spawn(async move {
            while let Some(crossing) = boundary_rx.recv().await {
                dispatch_command(
                    &command_tx_for_boundary,
                    PlaybackCommand::TrackCrossed(crossing),
                );
            }
        });
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create runtime");
            rt.block_on(async move {
                let audio_output: Box<dyn AudioOutput> = match custom_output {
                    Some(output) => output,
                    None => match default_audio_output() {
                        Ok(output) => output,
                        Err(e) => {
                            error!("Failed to initialize audio output: {:?}", e);
                            return;
                        }
                    },
                };
                let queue_ids = library_manager.ids().clone();
                let mut service = PlaybackService {
                    library_manager,
                    command_tx: command_tx.clone(),
                    command_rx,
                    progress_tx,
                    playback_queue: PlaybackQueue::new(queue_ids),
                    current_position_shared: Arc::new(std::sync::Mutex::new(None)),
                    audio_output,
                    stream: None,
                    current_prepared: None,
                    current_playback_source: None,
                    current_decoder_handle: None,
                    current_listener_handles: Vec::new(),
                    next_prepared: None,
                    next_track_stream: None,
                    next_decoder_handle: None,
                    current_track_info: None,
                    preview_audio_output: None,
                    preview_stream: None,
                    preview_playback_source: None,
                    preview_path: None,
                    main_was_playing_before_preview: false,
                    preview_duration: std::time::Duration::ZERO,
                    preview_position: std::time::Duration::ZERO,
                    preview_listener_handles: Vec::new(),
                    preview_buffer: None,
                    preview_decoder_handle: None,
                    preview_seek_offset: std::time::Duration::ZERO,
                    preview_sample_rate: 44100,
                    preview_channels: 2,
                    is_muted: false,
                    pre_mute_volume: 1.0,
                    position_update_interval_ms,
                    shared_file_buffer: None,
                    last_position_display,
                    boundary_tx,
                    pending_side_pause: None,
                };
                match service.library_manager.load_playback_state().await {
                    Ok(Some(state)) => match PersistedPlayback::from_row(state) {
                        Some(parsed) => service.restore(parsed).await,
                        // The row was corrupt (logged in `from_row`). Delete it so
                        // the bad row doesn't linger durably across restarts.
                        None => {
                            if let Err(e) = service.library_manager.clear_playback_state().await {
                                warn!("couldn't clear the corrupt playback resume cache: {e}");
                            }
                        }
                    },
                    Ok(None) => {}
                    Err(e) => {
                        warn!("couldn't load the saved playback state: {e}; starting fresh")
                    }
                }
                service.run().await;
            });
        });
        handle
    }

    /// Restore playback from the validated device-local resume cache.
    ///
    /// Atomic: every fallible step (fetching the context's tracks, validating the
    /// queue against library deletions) runs *before* anything touches the queue
    /// or audio. A DB error abandons the whole restore — the queue is left empty
    /// (a fresh start), never half-populated. Only once all fetches succeed does
    /// the commit run, and the commit is infallible — `parsed` is already
    /// fully-valid, so no field needs defaulting.
    ///
    /// StateChanged emissions are suppressed because the UI isn't ready yet;
    /// display state is written to the shared Arc for the UI to query later.
    async fn restore(&mut self, parsed: PersistedPlayback) {
        info!(
            "Restoring playback state: track={:?}",
            parsed.queue.current_track_id
        );

        // -- All fallible work first; the queue is untouched until it succeeds. --

        // Re-materialize the context from its source release's current tracks
        // (deleted tracks fall out of `get_track_ids`). A fetch error abandons the
        // restore; an empty result means the source release is gone; a result
        // shorter than the saved cursor means the release shrank below where we
        // were playing. Either way we drop the context and restore the manual
        // lane only, so `build_context` only ever sees an in-range cursor.
        let (context, context_tracks) = match &parsed.queue.context {
            Some(cs) => match self.library_manager.get_track_ids(&cs.source).await {
                Ok(tracks) if tracks.is_empty() => {
                    debug!(
                        "resume context release {} is gone; restoring the manual lane only",
                        cs.source
                    );
                    (None, Vec::new())
                }
                Ok(tracks) if cs.cursor >= tracks.len() => {
                    warn!(
                        "saved cursor {} is past the {} current tracks of {}; \
                         restoring the manual lane only",
                        cs.cursor,
                        tracks.len(),
                        cs.source
                    );
                    (None, Vec::new())
                }
                Ok(tracks) => (parsed.queue.context, tracks),
                Err(e) => {
                    warn!(
                        "couldn't load the resume context tracks for {}: {e}; starting fresh",
                        cs.source
                    );
                    return;
                }
            },
            None => (None, Vec::new()),
        };

        // Drop manual-lane tracks and a current track that were deleted from the
        // library between sessions (deleted context tracks already fell out of the
        // re-fetch above). A validation error abandons the restore.
        let mut to_check = parsed.queue.manual.clone();
        to_check.extend(parsed.queue.current_track_id.clone());
        let existing = match self
            .library_manager
            .filter_existing_track_ids(&to_check)
            .await
        {
            Ok(existing) => existing,
            Err(e) => {
                warn!(
                    "couldn't validate restored tracks {to_check:?} against deletions: {e}; \
                     starting fresh"
                );
                return;
            }
        };
        let dropped: Vec<&String> = to_check.iter().filter(|t| !existing.contains(*t)).collect();
        if !dropped.is_empty() {
            warn!("dropping playback tracks deleted from the library: {dropped:?}");
        }
        let manual: Vec<String> = parsed
            .queue
            .manual
            .into_iter()
            .filter(|t| existing.contains(t))
            .collect();
        let current_track_id = parsed
            .queue
            .current_track_id
            .filter(|t| existing.contains(t));
        let repeat = parsed.queue.repeat;

        // -- Commit (infallible): everything below applies validated state. --

        self.playback_queue.restore(
            QueueSnapshot {
                context,
                manual,
                current_track_id,
                repeat,
            },
            context_tracks,
        );
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::RepeatModeChanged { mode: repeat },
        );

        // Volume + mute.
        self.audio_output.set_volume(parsed.volume);
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::VolumeChanged {
                volume: parsed.volume,
            },
        );
        if parsed.is_muted {
            self.is_muted = true;
            self.pre_mute_volume = parsed.volume;
            self.audio_output.set_volume(0.0);
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::MuteChanged { is_muted: true },
            );
        }

        self.emit_queue_update();

        // Start the current track paused at the saved position, if there is one.
        if let Some(track_id) = self
            .playback_queue
            .current_track_id()
            .map(|s| s.to_string())
        {
            self.audio_output
                .set_state(crate::playback::audio_output::AudioState::Paused);
            self.play_track(&track_id, false, true).await;

            // A missing position means none was captured: resume from the start,
            // don't force a seek to 0.
            if let Some(pos) = parsed.position_ms {
                if pos > 0 {
                    self.seek(std::time::Duration::from_millis(pos)).await;
                }
            }
            // Emit the position we restored to so late-mounting views can read it
            // on mount. `None` means none was captured — the track's start (0).
            let restored_pos = parsed.position_ms.unwrap_or(0);
            self.emit_position_display(restored_pos, track_id);
        }

        // Write the reconciled state back: dropping a dead context or
        // library-deleted tracks corrected the in-memory queue, so persist makes
        // that correction durable now (or clears the row if nothing is playing)
        // rather than leaving the saved row stale until the next change.
        self.persist_playback_state().await;

        info!("Playback state restored");
    }

    /// Build the device-local `playback_state` row from the current queue and
    /// playback state, and save it — or clear it when playback has stopped. The
    /// queue is device-local; this never syncs.
    ///
    /// The write is logged-best-effort, not propagated as fatal: a failed
    /// resume-cache write only costs the resume point; playback is unaffected.
    /// The log is the never-mask escape hatch — a write failure is recorded, not
    /// conflated with "nothing was playing".
    async fn persist_playback_state(&self) {
        use crate::playback::audio_output::AudioState;
        if self.audio_output.get_state() == AudioState::Stopped {
            if let Err(e) = self.library_manager.clear_playback_state().await {
                warn!("couldn't clear playback state: {e}");
            }
            return;
        }
        let snap = self.playback_queue.snapshot();
        let context = snap.context.map(|ctx| DbPlaybackContext {
            source: ctx.source,
            shuffle_seed: match ctx.traversal {
                Traversal::Shuffled { seed } => Some(seed as i64),
                Traversal::Sequential => None,
            },
            cursor: ctx.cursor as i64,
        });
        let position_ms =
            (*self.current_position_shared.lock().unwrap()).map(|d| d.as_millis() as i64);
        let row = DbPlaybackState {
            context,
            manual: serde_json::to_string(&snap.manual)
                .expect("serializing a Vec<String> to JSON cannot fail"),
            repeat: repeat_to_str(snap.repeat).to_string(),
            current_track_id: snap.current_track_id,
            position_ms,
            volume: if self.is_muted {
                self.pre_mute_volume
            } else {
                self.audio_output.get_volume()
            },
            is_muted: self.is_muted,
        };
        if let Err(e) = self.library_manager.save_playback_state(&row).await {
            warn!(
                "couldn't persist playback state (current track {:?}): {e}",
                row.current_track_id
            );
        }
    }

    async fn run(&mut self) {
        info!("PlaybackService started");
        let mut library_event_rx = self.library_manager.subscribe_events();
        loop {
            tokio::select! {
                Some(command) = self.command_rx.recv() => {
            match command {
                PlaybackCommand::Play(track_id) => {
                    self.stop_preview_without_resume();
                    self.main_was_playing_before_preview = false;
                    if let Some(stream) = self.stream.take() {
                        drop(stream);
                    }
                    self.audio_output
                        .set_state(crate::playback::audio_output::AudioState::Stopped);
                    self.clear_next_track_state();
                    // Direct selection: the track's release becomes the playing
                    // context, with the cursor at the chosen track.
                    match self.library_manager.get_play_context(&track_id).await {
                        Ok(context) => {
                            self.playback_queue.play_release(
                                context.release_id,
                                context.track_ids,
                                ContextStart::Index(context.index),
                            );
                        }
                        Err(e) => {
                            warn!(
                                "Failed to load play context for {track_id}: {e}; playing the single track"
                            );
                            self.playback_queue.play_single(track_id.clone());
                        }
                    }
                    self.pending_side_pause = None;
                    self.emit_queue_update();
                    self.play_track(&track_id, false, false).await; // Direct selection: skip pregap, start playing
                }
                PlaybackCommand::PlayRelease { release_id, start_track_index, shuffle } => {
                    let track_ids = match self.library_manager.get_track_ids(&release_id).await {
                        Ok(ids) => ids,
                        Err(e) => {
                            error!("Failed to get tracks for release {release_id}: {e}");
                            continue;
                        }
                    };

                    if track_ids.is_empty() {
                        warn!("PlayRelease: release {release_id} has no tracks");
                        continue;
                    }

                    self.stop_preview_without_resume();
                    self.main_was_playing_before_preview = false;
                    let start = if shuffle {
                        // The shuffle seed is read once here, at the command
                        // boundary, and carried into the context so the order is
                        // reproducible and `Context` repeat can re-derive it.
                        ContextStart::Shuffled {
                            seed: rand::random(),
                        }
                    } else {
                        // None means "from the first track"; an out-of-range index
                        // is a bad caller value, so clamp to the start and log it.
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
                    let first_track = self
                        .playback_queue
                        .play_release(release_id, track_ids, start);
                    self.pending_side_pause = None;
                    self.emit_queue_update();
                    self.play_track(&first_track, false, false).await;
                }
                PlaybackCommand::Pause => {
                    self.pause().await;
                }
                PlaybackCommand::Resume => {
                    self.resume().await;
                }
                PlaybackCommand::Stop => {
                    self.stop().await;
                }
                PlaybackCommand::Next => {
                    let was_side_paused = self.pending_side_pause.is_some();
                    if was_side_paused {
                        self.pending_side_pause = None;
                    }
                    info!("Next command received");
                    // If we have a preloaded track, use it directly
                    if let Some(preloaded_track_id) = self.next_track_id().map(|s| s.to_string()) {
                        // skip pregap, preserve paused
                        self.advance_and_play_preloaded(
                            &preloaded_track_id,
                            false,
                            !was_side_paused,
                        )
                            .await;
                    } else {
                        // No preloaded track, use PlaybackQueue decision logic
                        match self.playback_queue.next_entry() {
                            NextEntry::Play(next_track) => {
                                info!("No preloaded track, playing from queue: {}", next_track);
                                self.emit_queue_update();
                                self.play_track(&next_track, false, !was_side_paused).await;
                                // preserve paused
                            }
                            _ => {
                                info!("No next track available, stopping");
                                self.emit_queue_update();
                                self.stop().await;
                            }
                        }
                    }
                }
                PlaybackCommand::AutoAdvance => {
                    info!("AutoAdvance command received (natural transition)");

                    match self.side_pause_for_queue_front().await {
                        Ok(Some(decision)) => {
                            self.pause_for_side_end(decision);
                            continue;
                        }
                        Ok(None) => {}
                        Err(()) => {
                            self.stop().await;
                            continue;
                        }
                    }

                    // If we have a preloaded track (and not in repeat-track mode), use it
                    if self.playback_queue.repeat_mode() != RepeatMode::Track {
                        if let Some(preloaded_track_id) =
                            self.next_track_id().map(|s| s.to_string())
                        {
                            // natural transition, start playing
                            self.advance_and_play_preloaded(&preloaded_track_id, true, false)
                                .await;
                            continue;
                        }
                    }

                    // No preloaded track (or repeat-track mode), use PlaybackQueue decision logic
                    match self.playback_queue.next_entry() {
                        NextEntry::RepeatCurrent(track_id) => {
                            info!("Repeat mode: track, replaying {}", track_id);
                            self.clear_next_track_state();
                            self.play_track(&track_id, true, false).await;
                        }
                        NextEntry::Play(next_track) => {
                            info!("Playing from queue: {}", next_track);
                            self.emit_queue_update();
                            self.play_track(&next_track, true, false).await;
                        }
                        NextEntry::Stop => {
                            info!("No next track available, stopping");
                            self.emit_queue_update();
                            self.stop().await;
                        }
                    }
                }
                PlaybackCommand::TrackCrossed(crossing) => {
                    self.handle_track_crossed(crossing).await;
                }
                PlaybackCommand::TrackReady {
                    track_id,
                    cancel_token,
                } => {
                    // Emit Playing/Paused now that audio is actually flowing. A
                    // signal from an abandoned load (the user switched tracks, or
                    // replayed the same track via RepeatCurrent / RestartCurrent /
                    // re-Play) carries the old load's token; only the live load's
                    // token is the current prepared track's. Comparing the id
                    // alone would accept a same-id replay's stale signal, so match
                    // the token Arc by pointer and drop anything that isn't it.
                    let is_live = self
                        .current_prepared
                        .as_ref()
                        .is_some_and(|p| Arc::ptr_eq(&p.cancel_token, &cancel_token));
                    if is_live {
                        self.emit_current_state();
                    } else {
                        debug!(
                            track_id,
                            "ignoring stale TrackReady from an abandoned load"
                        );
                    }
                }
                PlaybackCommand::HaltOnError => {
                    // The self-handled failure paths in play_track (prepare
                    // failure, init_streaming failure) emit PlaybackError AND call
                    // stop() synchronously before returning to this loop. The
                    // command loop is serial: stop() has fully run by the time the
                    // HaltOnError it triggered is dequeued, so playback is already
                    // Stopped and a second stop() would emit a duplicate Stopped.
                    // No-op when nothing is prepared and the output is stopped —
                    // race-free because the serial loop can't interleave a new
                    // load between stop() finishing and this command running.
                    // A genuine mid-flight reader/decoder failure leaves a track
                    // prepared and Playing, so the no-op doesn't fire there.
                    let already_stopped = self.current_prepared.is_none()
                        && self.audio_output.get_state()
                            == crate::playback::audio_output::AudioState::Stopped;
                    if already_stopped {
                        debug!("HaltOnError: playback already stopped, nothing to halt");
                    } else {
                        // stop() does not emit PlaybackError, so this can't feed
                        // back into the self-subscription that dispatched it.
                        self.stop().await;
                    }
                }
                PlaybackCommand::Previous => {
                    self.pending_side_pause = None;
                    if let Some(current_track_id) = self.current_track_id().map(|s| s.to_string()) {
                        let current_position = self
                            .current_position_shared
                            .lock()
                            .unwrap()
                            .unwrap_or(std::time::Duration::ZERO);
                        let position_ms = current_position.as_millis() as u64;

                        match self.playback_queue.previous_action(position_ms) {
                            PreviousAction::PlayPrevious(previous_track_id) => {
                                info!("Going to previous track: {}", previous_track_id);
                                // previous_action already stepped the context cursor
                                // back and made this track current; just play it.
                                self.clear_next_track_state();
                                self.emit_queue_update();
                                self.play_track(&previous_track_id, false, true).await;
                            }
                            PreviousAction::RestartCurrent => {
                                info!("Restarting current track from beginning");
                                self.play_track(&current_track_id, false, true).await; // preserve paused
                            }
                        }
                    }
                }
                PlaybackCommand::Seek(position) => {
                    self.seek(position).await;
                }
                PlaybackCommand::SeekByRatio(ratio) => {
                    if let Some(prepared) = &self.current_prepared {
                        let pregap_ms = prepared.pregap_ms.unwrap_or(0).max(0) as u64;
                        let duration_ms = prepared.duration.as_millis() as u64;
                        let track_duration = duration_ms.saturating_sub(pregap_ms);
                        let position_ms =
                            pregap_ms + (ratio.clamp(0.0, 1.0) * track_duration as f64) as u64;
                        self.seek(std::time::Duration::from_millis(position_ms))
                            .await;
                    }
                }
                PlaybackCommand::SetVolume(volume) => {
                    self.audio_output.set_volume(volume);

                    if volume > 0.0 && self.is_muted {
                        self.is_muted = false;
                        emit_progress(
                            &self.progress_tx,
                            PlaybackProgress::MuteChanged { is_muted: false },
                        );
                    }

                    emit_progress(
                        &self.progress_tx,
                        PlaybackProgress::VolumeChanged { volume },
                    );
                }
                PlaybackCommand::ToggleMute => {
                    if self.is_muted {
                        self.is_muted = false;
                        let vol = self.pre_mute_volume;
                        self.audio_output.set_volume(vol);
                        emit_progress(
                            &self.progress_tx,
                            PlaybackProgress::VolumeChanged { volume: vol },
                        );
                        emit_progress(
                            &self.progress_tx,
                            PlaybackProgress::MuteChanged { is_muted: false },
                        );
                    } else {
                        self.pre_mute_volume = self.audio_output.get_volume();
                        self.is_muted = true;
                        self.audio_output.set_volume(0.0);
                        emit_progress(
                            &self.progress_tx,
                            PlaybackProgress::VolumeChanged { volume: 0.0 },
                        );
                        emit_progress(
                            &self.progress_tx,
                            PlaybackProgress::MuteChanged { is_muted: true },
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
                        Err(e) => error!("Failed to add release {release_id} to queue: {e}"),
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
                        Err(e) => error!("Failed to add release {release_id} next in queue: {e}"),
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
                    }
                }
                PlaybackCommand::ReorderQueue { entry_id, before } => {
                    self.playback_queue.reorder(&entry_id, before.as_ref());
                    self.on_queue_mutated().await;
                }
                PlaybackCommand::ClearQueue => {
                    self.playback_queue.clear();
                    self.on_queue_mutated().await;
                }
                PlaybackCommand::SetRepeatMode(mode) => {
                    if self.playback_queue.repeat_mode() != mode {
                        self.playback_queue.set_repeat_mode(mode);
                        emit_progress(
                            &self.progress_tx,
                            PlaybackProgress::RepeatModeChanged { mode },
                        );
                        self.emit_queue_update();
                        self.persist_playback_state().await;
                    }
                }
                PlaybackCommand::CycleRepeatMode => {
                    let next = match self.playback_queue.repeat_mode() {
                        RepeatMode::Off => RepeatMode::Context,
                        RepeatMode::Context => RepeatMode::Track,
                        RepeatMode::Track => RepeatMode::Off,
                    };
                    self.playback_queue.set_repeat_mode(next);
                    emit_progress(
                        &self.progress_tx,
                        PlaybackProgress::RepeatModeChanged { mode: next },
                    );
                    self.emit_queue_update();
                    self.persist_playback_state().await;
                }
                PlaybackCommand::SkipTo(entry_id) => {
                    if let Some(entry) = self.playback_queue.skip_to(&entry_id) {
                        info!(
                            "SkipTo: jumping to queue entry {}, track {}",
                            entry.id.0, entry.track_id
                        );

                        self.pending_side_pause = None;
                        self.clear_next_track_state();
                        self.emit_queue_update();
                        self.play_track(&entry.track_id, false, false).await;
                    }
                }
                PlaybackCommand::PreviewPlay(path) => {
                    self.handle_preview_play(path).await;
                }
                PlaybackCommand::PreviewStop => {
                    self.stop_preview();
                }
                PlaybackCommand::PreviewTogglePause => {
                    self.handle_preview_toggle_pause().await;
                }
                PlaybackCommand::PreviewSeekByRatio(ratio) => {
                    let duration_ms = self.preview_duration.as_millis() as u64;
                    let position_ms = (ratio.clamp(0.0, 1.0) * duration_ms as f64) as u64;
                    self.handle_preview_seek(std::time::Duration::from_millis(position_ms))
                        .await;
                }
                PlaybackCommand::PreviewCompleted => {
                    self.handle_preview_completed();
                }
                PlaybackCommand::TogglePlayPause => {
                    use crate::playback::audio_output::AudioState;
                    match self.audio_output.get_state() {
                        AudioState::Playing => self.pause().await,
                        AudioState::Paused => self.resume().await,
                        AudioState::Stopped => {}
                    }
                }
                PlaybackCommand::GetVolume(reply) => {
                    let _ = reply.send(self.audio_output.get_volume());
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

    async fn handle_tracks_deleted(&mut self, track_ids: Vec<String>) {
        let ids: HashSet<String> = track_ids.into_iter().collect();

        let current_deleted = self
            .playback_queue
            .current_track_id()
            .map(|s| ids.contains(s))
            .unwrap_or(false);

        // Purge deleted tracks from the queue first
        self.playback_queue.remove_by_ids(&ids);

        if current_deleted {
            // Stop current playback (tears down streams, decoder, next track state)
            self.stop().await;

            // Advance to next track if queue has one, otherwise stay stopped
            if let Some(next_id) = self.playback_queue.advance_to_front() {
                self.play_track(&next_id, false, false).await;
            }
        } else {
            // Current track is fine, but check preloaded next track
            if let Some(ref next_prepared) = self.next_prepared {
                if ids.contains(&next_prepared.track_info.track_id) {
                    self.clear_next_track_state();
                }
            }
        }

        self.emit_queue_update();
    }

    /// Play a track.
    /// - `is_natural_transition`: if true, plays from INDEX 00 (pregap included)
    /// - `preserve_paused`: if true, inherits current paused state; if false, always starts playing
    async fn play_track(
        &mut self,
        track_id: &str,
        is_natural_transition: bool,
        preserve_paused: bool,
    ) {
        info!(
            "Playing track: {} (natural_transition: {}, preserve_paused: {})",
            track_id, is_natural_transition, preserve_paused
        );

        // Tear down the outgoing track up front so a manual switch silences the
        // old audio immediately and frees the old decoder + reader, instead of
        // leaving them running until this method overwrites the current state at
        // the end. Spares a shared source buffer the incoming track reuses.
        self.teardown_current_track();

        emit_progress(
            &self.progress_tx,
            PlaybackProgress::StateChanged {
                state: PlaybackState::Loading {
                    track_id: track_id.to_string(),
                    resolved: None,
                },
            },
        );

        // Prepare track: fetch metadata, create buffer, start reading
        let prepared = prepare_track_for_playback(
            &self.library_manager,
            track_id,
            &mut self.shared_file_buffer,
            self.progress_tx.clone(),
        )
        .await;
        let prepared = match prepared {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to prepare track {}: {}", track_id, e);
                emit_progress(
                    &self.progress_tx,
                    PlaybackProgress::PlaybackError {
                        reason: e.into_ui_reason(),
                    },
                );
                self.stop().await;
                return;
            }
        };

        // Metadata is resolved now: re-emit Loading carrying the target track's
        // info so the bar switches from the prior track to the target while the
        // first audio bytes are still downloading.
        let loading_duration_ms = pregap_adjusted_duration(&prepared);
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::StateChanged {
                state: PlaybackState::Loading {
                    track_id: track_id.to_string(),
                    resolved: Some(LoadingTrack {
                        track_info: prepared.track_info.clone(),
                        duration_ms: loading_duration_ms,
                    }),
                },
            },
        );

        // Calculate pregap seek position if needed (direct selection skips pregap)
        let pregap_skip_duration = pregap_seek_position(prepared.pregap_ms, is_natural_transition);

        // Seek to the track's first sample plus any pregap skip; trim lead-in
        // there and stop at the track's end.
        let pregap_offset = match pregap_skip_duration {
            Some(d) => (d.as_secs_f64() * prepared.sample_rate as f64) as u64,
            None => 0,
        };
        let decode = prepared.decode_params(pregap_offset);

        // Position offset: when we skip pregap, decoder positions start at 0 but actual
        // track position is pregap_ms
        let position_offset = if pregap_skip_duration.is_some() {
            std::time::Duration::from_millis(prepared.pregap_ms.unwrap_or(0).max(0) as u64)
        } else {
            std::time::Duration::ZERO
        };
        let sample_rate = prepared.sample_rate;
        let channels = prepared.channels;
        self.current_track_info = Some(prepared.track_info.clone());
        let fmt = prepared.track_fmt(position_offset);

        // Store prepared track state so the shared tail reads this load's buffer
        // and cancel token.
        self.current_prepared = Some(prepared);
        if !self
            .start_decoder_and_watch(decode, fmt, sample_rate, channels, track_id.to_string())
            .await
        {
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::PlaybackError {
                    reason: crate::ui::PlaybackErrorReason::internal(
                        "Couldn't start audio output for the track.",
                    ),
                },
            );
            self.stop().await;
            return;
        }

        // Set audio state: always Playing unless preserving paused state. Audio
        // doesn't flow until the ring fills, so this doesn't race the watcher.
        if !preserve_paused {
            self.audio_output
                .set_state(crate::playback::audio_output::AudioState::Playing);
        }

        info!("Streaming playback started for track: {}", track_id);

        self.preload_queue_front().await;

        // Persist the now-playing state so a restart on this device resumes here.
        self.persist_playback_state().await;
    }

    async fn preload_queue_front(&mut self) {
        if let Some(next_id) = self.playback_queue.front().map(str::to_string) {
            self.preload_next_track(&next_id).await;
        }
    }

    /// Preload the next track for gapless playback.
    /// This eagerly starts the decoder so samples are ready when we switch tracks.
    async fn preload_next_track(&mut self, track_id: &str) {
        // Prepare track: fetch metadata, create buffer, start reading
        let prepared = prepare_track_for_playback(
            &self.library_manager,
            track_id,
            &mut self.shared_file_buffer,
            self.progress_tx.clone(),
        )
        .await;
        let prepared = match prepared {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to preload track {}: {}", track_id, e);
                return;
            }
        };

        // Create decoder sink/source and start decoder eagerly for gapless playback
        let (mut sink, source, _ready) =
            create_track_stream_pair(prepared.sample_rate, prepared.channels);
        let decoder_buffer = prepared.buffer.clone();
        let cancel_token = prepared.cancel_token.clone();

        // Preload params (natural transition: no pregap skip): seek to the
        // track's first sample, trim there, stop at its end.
        let decode = prepared.decode_params(0);

        let decoder_handle = std::thread::spawn(move || {
            if let Err(e) = decode.run_decoder(decoder_buffer, &mut sink, cancel_token) {
                let _ = log_streaming_decode_failure("Preload streaming decode", e);
            }
        });

        // Store preloaded state. If the next track shares the live stream's
        // format, stage it into the PlaybackSource so the audio callback can
        // cross the boundary without rebuilding the stream (true gapless).
        // Otherwise hold it for the rebuild path (format change, no live
        // stream yet, or repeat-track mode where the current track replays
        // instead) which the completion → AutoAdvance flow handles.
        let stage_target = match (&self.current_prepared, &self.current_playback_source) {
            (Some(current), Some(gapless))
                if current.sample_rate == prepared.sample_rate
                    && current.channels == prepared.channels
                    && self.playback_queue.repeat_mode() != RepeatMode::Track
                    && !self.should_hold_for_side_pause(current, &prepared) =>
            {
                Some(gapless)
            }
            _ => None,
        };
        if let Some(gapless) = stage_target {
            info!(
                "Preload: staged next track into gapless chain: {}",
                track_id
            );
            gapless
                .lock()
                .unwrap()
                .stage_next(source, prepared.track_fmt(std::time::Duration::ZERO));
            self.next_track_stream = None;
        } else {
            info!(
                "Preload: holding next track for stream-rebuild path: {}",
                track_id
            );
            self.next_track_stream = Some(source);
        }
        self.next_prepared = Some(prepared);
        self.next_decoder_handle = Some(decoder_handle);

        info!("Preloaded next track (streaming): {}", track_id);
    }

    async fn side_pause_for_queue_front(&mut self) -> Result<Option<SidePauseDecision>, ()> {
        if !self.side_pause_enabled() {
            return Ok(None);
        }

        let Some(current) = self.current_track_info.clone() else {
            error!("side-pause decision requested without current track metadata");
            return Err(());
        };

        let Some(next_track_id) = self
            .playback_queue
            .next_sequential_context_track()
            .map(str::to_string)
        else {
            return Ok(None);
        };

        let next_info = match self.next_prepared.as_ref() {
            Some(prepared) if prepared.track_info.track_id == next_track_id => {
                prepared.track_info.clone()
            }
            Some(prepared) => {
                debug!(
                    preloaded_track_id = %prepared.track_info.track_id,
                    queue_next_track_id = %next_track_id,
                    "side-pause decision ignoring stale preloaded next track"
                );
                self.playback_info_for_side_pause(&next_track_id).await?
            }
            None => self.playback_info_for_side_pause(&next_track_id).await?,
        };

        Ok(self
            .side_pause_prompt_for_infos(&current, &next_info)
            .map(|prompt| SidePauseDecision {
                track_id: next_track_id,
                prompt,
            }))
    }

    async fn playback_info_for_side_pause(&self, track_id: &str) -> Result<PlaybackTrackInfo, ()> {
        self.library_manager
            .get_playback_track_info(track_id)
            .await
            .map_err(|error| {
                error!(
                    "failed to resolve playback metadata for side-pause decision on {track_id}: {error}"
                );
            })
    }

    fn should_hold_for_side_pause(
        &self,
        current: &PlaybackPreparedTrack,
        next: &PlaybackPreparedTrack,
    ) -> bool {
        self.side_pause_prompt_for_infos(&current.track_info, &next.track_info)
            .is_some()
    }

    fn side_pause_prompt_for_infos(
        &self,
        current: &PlaybackTrackInfo,
        next: &PlaybackTrackInfo,
    ) -> Option<PlaybackSidePausePrompt> {
        if !self.side_pause_enabled() {
            return None;
        }
        side_pause_prompt_between(current, next)
    }

    fn side_pause_enabled(&self) -> bool {
        self.library_manager.get_config().pause_between_sides
            && self.playback_queue.repeat_mode() != RepeatMode::Track
    }

    fn pause_for_side_end(&mut self, decision: SidePauseDecision) {
        self.pending_side_pause = Some(decision.clone());
        self.audio_output
            .set_state(crate::playback::audio_output::AudioState::Paused);
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::StateChanged {
                state: self.make_paused_state(PlaybackPauseReason::SideEnded(decision.prompt)),
            },
        );
        self.emit_queue_update();
    }

    async fn resume_from_side_pause(&mut self) {
        let Some(pending) = self.pending_side_pause.clone() else {
            warn!("side-pause resume requested without pending side-pause state");
            return;
        };
        let pending_track_id = pending.track_id;

        if self.next_track_id() == Some(pending_track_id.as_str()) {
            self.advance_and_play_preloaded(&pending_track_id, true, false)
                .await;
            self.pending_side_pause = None;
            return;
        }

        let Some(front) = self.playback_queue.front().map(str::to_string) else {
            error!("side-pause resume expected {pending_track_id}, but the queue is empty");
            self.pending_side_pause = None;
            return;
        };
        if front != pending_track_id {
            error!("side-pause resume expected {pending_track_id}, but queue front is {front}");
            self.pending_side_pause = None;
            return;
        }
        match self.playback_queue.next_entry() {
            NextEntry::Play(track_id) => {
                self.pending_side_pause = None;
                self.emit_queue_update();
                self.play_track(&track_id, true, false).await;
            }
            other => {
                error!("side-pause resume expected Play for {pending_track_id}, got {other:?}");
                self.pending_side_pause = None;
            }
        }
    }

    async fn pause(&mut self) {
        self.pending_side_pause = None;
        self.audio_output
            .set_state(crate::playback::audio_output::AudioState::Paused);
        if self.current_prepared.is_some() && self.current_track_info.is_some() {
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::StateChanged {
                    state: self.make_paused_state(PlaybackPauseReason::Manual),
                },
            );
        }
    }

    async fn resume(&mut self) {
        // Stop preview when user explicitly resumes main playback
        if self.preview_path.is_some() {
            self.main_was_playing_before_preview = false;
            self.stop_preview_without_resume();
        }

        if self.pending_side_pause.is_some() {
            self.resume_from_side_pause().await;
            return;
        }

        self.audio_output
            .set_state(crate::playback::audio_output::AudioState::Playing);
        if self.current_prepared.is_some() && self.current_track_info.is_some() {
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::StateChanged {
                    state: self.make_playing_state(),
                },
            );
        }
    }

    /// If a track is preloaded but no longer matches the queue front, discard it and
    /// preload the new front instead.  Called after queue mutations that insert at position 0.
    async fn refresh_preload_for_queue_front(&mut self) {
        let preloaded_id = match self.next_track_id() {
            Some(id) => id.to_string(),
            None => return,
        };
        let queue_front = match self.playback_queue.front() {
            Some(id) => id.to_string(),
            None => {
                self.clear_next_track_state();
                return;
            }
        };
        if preloaded_id != queue_front {
            self.clear_next_track_state();
            self.preload_next_track(&queue_front).await;
        }
    }

    fn clear_next_track_state(&mut self) {
        // Cancel the preloaded next source, whether staged in the gapless chain
        // or held for the rebuild path.
        if let Some(source) = self.take_preloaded_next() {
            source.cancel();
        }
        // Cancel any active sparse buffer for the next track (unless shared)
        if let Some(prepared) = &self.next_prepared {
            if !prepared.buffer_shared {
                prepared.buffer.cancel();
            }
        }
        self.next_prepared = None;
        self.next_decoder_handle = None;
    }

    /// Whether a preloaded next-track source is available — staged in the gapless
    /// chain or held for the rebuild path.
    fn has_preloaded_next(&self) -> bool {
        if self.next_track_stream.is_some() {
            return true;
        }
        match &self.current_playback_source {
            Some(g) => g.lock().unwrap().has_next(),
            None => false,
        }
    }

    /// Take ownership of the preloaded next-track source for a stream rebuild,
    /// from wherever it is held (gapless chain or holding field). The fmt is
    /// discarded — the rebuild path constructs a fresh fmt from `next_prepared`
    /// when initializing the new stream.
    fn take_preloaded_next(&mut self) -> Option<TrackStream> {
        if let Some(source) = self.next_track_stream.take() {
            return Some(source);
        }
        if let Some(gapless) = &self.current_playback_source {
            return gapless
                .lock()
                .unwrap()
                .take_next()
                .map(|(source, _fmt)| source);
        }
        None
    }

    /// Promote track bookkeeping after the audio callback crossed a gapless
    /// track boundary. The `PlaybackSource` has already advanced to the staged
    /// next track within the same stream; here we report the finishing track's
    /// decode stats, update service state to match, and preload the following
    /// track. No stream rebuild, no UI gap.
    ///
    /// Pure in the `TrackCrossing` payload: both track ids and the finishing
    /// stats come from the event, not from a shared cell.
    async fn handle_track_crossed(&mut self, crossing: TrackCrossing) {
        // Report the finishing track's decode stats here: a gaplessly-advanced
        // track never reaches the completion path, so this is the per-track
        // completion log + stats for every track except the album's last.
        info!(
            "Track completed (gapless): {} ({} decode errors, {} samples)",
            crossing.finished_track_id, crossing.decode_error_count, crossing.samples_decoded
        );
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::DecodeStats {
                track_id: crossing.finished_track_id,
                error_count: crossing.decode_error_count,
                samples_decoded: crossing.samples_decoded,
            },
        );

        let next_prepared = match self.next_prepared.take() {
            Some(p) => p,
            None => {
                warn!(
                    track_id = %crossing.incoming_track_id,
                    "Gapless boundary fired with no preloaded track; ignoring"
                );
                return;
            }
        };
        let track_id = crossing.incoming_track_id;
        info!("Gapless boundary: now playing {}", track_id);

        // The previous track's decoder has finished; the next track's decoder
        // becomes the current one.
        self.current_decoder_handle = self.next_decoder_handle.take();

        // Release the previous track's buffer (unless shared with the new one).
        if let Some(prev) = self.current_prepared.take() {
            if !prev.buffer_shared {
                prev.buffer.cancel();
            }
        }

        self.current_track_info = Some(next_prepared.track_info.clone());
        self.current_prepared = Some(next_prepared);

        // Advance the queue position to the now-playing track.
        self.advance_to_preloaded();

        // Natural transition starts at position 0 (pregap included).
        *self.current_position_shared.lock().unwrap() = Some(std::time::Duration::ZERO);

        // Tell the UI which track is now playing (StateChanged covers the transition).
        self.emit_current_state();

        self.preload_queue_front().await;

        // The gapless boundary advanced the current track without play_track.
        self.persist_playback_state().await;
    }

    /// Advance the queue's current pointer past the finished track to the front
    /// and emit the queue update. Used by `Next`, `AutoAdvance` (preloaded path),
    /// and `TrackCrossed`. The front is the track being played: the preload
    /// refreshes whenever the queue mutates, so the advanced front and the track
    /// these callers go on to play are the same one.
    fn advance_to_preloaded(&mut self) {
        if self.playback_queue.advance_to_front().is_none() {
            warn!("advance_to_preloaded: queue had no front to advance to");
        }
        self.emit_queue_update();
    }

    /// Play the preloaded next track: advance the queue to it, then start its
    /// buffered stream if ready, or a fresh play of it otherwise. `natural`
    /// (pregap included) and `preserve_paused` pass through to the player. Shared
    /// by `Next` and `AutoAdvance`, which differ only in those two booleans.
    async fn advance_and_play_preloaded(
        &mut self,
        preloaded_track_id: &str,
        natural: bool,
        preserve_paused: bool,
    ) {
        if self.has_preloaded_next() {
            info!("Using preloaded track: {}", preloaded_track_id);
            self.advance_to_preloaded();
            self.play_preloaded_track(natural, preserve_paused).await;
        } else {
            // Preload started but the streaming source isn't ready yet.
            self.advance_to_preloaded();
            self.clear_next_track_state();
            self.play_track(preloaded_track_id, natural, preserve_paused)
                .await;
        }
    }

    /// Play a preloaded track by swapping next state to current and starting the audio stream.
    /// Play a preloaded track. The decoder is already running from preload_next_track.
    /// Play the preloaded next track.
    /// - `is_natural_transition`: if true, plays from INDEX 00 (pregap included)
    /// - `preserve_paused`: if true, inherits current paused state; if false, always starts playing
    async fn play_preloaded_track(&mut self, is_natural_transition: bool, preserve_paused: bool) {
        let next_prepared = match self.next_prepared.take() {
            Some(p) => p,
            None => {
                error!("play_preloaded_track called but no next_prepared");
                return;
            }
        };

        let pregap_ms = next_prepared.pregap_ms;
        let track_id = next_prepared.track_info.track_id.clone();

        // If we need to skip pregap (direct selection), the preloaded state won't work
        // because it was set up for auto-advance (starting at byte 0).
        // Fall back to play_track which handles pregap at decoder start.
        if !is_natural_transition && pregap_ms.is_some_and(|p| p > 0) {
            info!("Pregap skip needed for preloaded track - falling back to play_track");
            if !next_prepared.buffer_shared {
                next_prepared.buffer.cancel();
            }
            if let Some(source) = self.take_preloaded_next() {
                source.cancel();
            }
            self.play_track(&track_id, is_natural_transition, preserve_paused)
                .await;
            return;
        }

        self.current_track_info = Some(next_prepared.track_info.clone());

        // Recover the preloaded next source (staged in the gapless chain or held
        // for the rebuild path) BEFORE tearing down the current stream.
        let source = self
            .take_preloaded_next()
            .expect("Preloaded track has no streaming source");

        // Cancel current streaming state
        if let Some(gapless) = self.current_playback_source.take() {
            if let Ok(guard) = gapless.lock() {
                guard.cancel();
            }
        }
        if let Some(prepared) = &self.current_prepared {
            if !prepared.buffer_shared {
                prepared.buffer.cancel();
            }
        }

        // Swap next to current. The preloaded track's decoder becomes the current
        // one (its handle was held in next_decoder_handle); the previous track's
        // decoder was cancelled above via the source.
        self.current_decoder_handle = self.next_decoder_handle.take();
        self.current_prepared = Some(next_prepared);

        // Natural transition: start at position 0 (INDEX 00, pregap plays).
        let fmt = self
            .current_prepared
            .as_ref()
            .expect("current_prepared just set above")
            .track_fmt(std::time::Duration::ZERO);

        // Initialize streaming with the preloaded source
        if !self.init_streaming(source, fmt).await {
            self.stop().await;
            return;
        }

        // Set audio state: always Playing unless preserving paused state
        if !preserve_paused {
            self.audio_output
                .set_state(crate::playback::audio_output::AudioState::Playing);
        }

        // Send state notification
        self.emit_current_state();

        self.preload_queue_front().await;

        // The preloaded advance doesn't go through play_track, so persist the
        // now-playing track here.
        self.persist_playback_state().await;
    }

    /// Pause main player for preview. Called after all fallible setup so that
    /// error paths don't leave the main player paused with no preview.
    fn pause_main_for_preview(&mut self) {
        if self.audio_output.get_state() == crate::playback::audio_output::AudioState::Playing {
            self.main_was_playing_before_preview = true;
            self.audio_output
                .set_state(crate::playback::audio_output::AudioState::Paused);

            if self.current_prepared.is_some() && self.current_track_info.is_some() {
                emit_progress(
                    &self.progress_tx,
                    PlaybackProgress::StateChanged {
                        state: self.make_paused_state(PlaybackPauseReason::Manual),
                    },
                );
            }
        }
    }

    /// Resume main player if it was paused for preview.
    fn maybe_resume_main_player(&mut self) {
        if !self.main_was_playing_before_preview {
            return;
        }
        self.main_was_playing_before_preview = false;
        self.audio_output
            .set_state(crate::playback::audio_output::AudioState::Playing);

        if self.current_prepared.is_some() && self.current_track_info.is_some() {
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::StateChanged {
                    state: self.make_playing_state(),
                },
            );
        }
    }

    /// Start decoding and streaming for preview playback.
    ///
    /// Shared by `handle_preview_play` (`seek_to=None`) and `handle_preview_seek`.
    /// Uses the buffer from `self.preview_buffer`. Returns true on success.
    async fn start_preview_decode(
        &mut self,
        path: String,
        duration: std::time::Duration,
        sample_rate: u32,
        channels: u32,
        buffer: SharedSparseBuffer,
        seek_to: Option<std::time::Duration>,
        paused: bool,
        pause_main: bool,
    ) -> bool {
        // Preview has only Idle/Playing/Paused — no Loading/Buffering state to
        // confirm — so it emits Playing/Paused immediately and skips the
        // ready-watcher. The demand-driven local fill keeps the ring fed; the
        // audio callback outputs silence until the first samples land rather
        // than blocking on a fixed wait that could dead-end into a frozen state.
        let (mut sink, source, _) = create_track_stream_pair(sample_rate, channels);

        let decoder_buffer = buffer.clone();
        let preview_cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let seek_to_sample = seek_to.map(|d| (d.as_secs_f64() * sample_rate as f64) as u64);
        let decoder_handle = std::thread::spawn(move || {
            if let Err(e) = crate::audio_codec::decode_audio_streaming(
                decoder_buffer,
                &mut sink,
                seek_to_sample,
                None,
                None,
                None, // preview auditions the whole file
                preview_cancel,
            ) {
                let _ = log_streaming_decode_failure("Preview decode", e);
            }
        });

        self.preview_decoder_handle = Some(decoder_handle);

        // Preview never chains; wrap in a PlaybackSource and drop the boundary
        // receiver so the sender's sends are no-ops. The fmt's values aren't
        // read by the preview listener (which captures `seek_offset` and the
        // duration locally), but they're set realistically for hygiene.
        let preview_fmt = TrackFmt {
            track_id: path.clone(),
            duration_ms: duration.as_millis() as u64,
            pregap_ms: None,
            position_offset: seek_to.unwrap_or(std::time::Duration::ZERO),
            // Preview plays an unimported file (no stored measurements) at unity.
            replay_gain_linear: 1.0,
        };
        let (preview_boundary_tx, _preview_boundary_rx) = tokio_mpsc::unbounded_channel();
        let source = Arc::new(Mutex::new(PlaybackSource::new(
            source,
            preview_fmt,
            preview_boundary_tx,
        )));

        if self.preview_audio_output.is_none() {
            match default_audio_output() {
                Ok(output) => self.preview_audio_output = Some(output),
                Err(e) => {
                    error!("Failed to create preview audio output: {:?}", e);
                    return false;
                }
            }
        }
        if let Some(stream) = self.preview_stream.take() {
            drop(stream);
        }

        let setup = match setup_audio_stream(
            self.preview_audio_output.as_deref_mut().unwrap(),
            source.clone(),
            self.position_update_interval_ms,
        )
        .await
        {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to create preview audio stream: {:?}", e);
                return false;
            }
        };

        if pause_main {
            self.pause_main_for_preview();
        }

        if let Err(e) = setup.stream.play() {
            error!("Failed to start preview playback: {:?}", e);
            for h in setup.bridge_handles {
                h.abort();
            }
            return false;
        }

        let preview_output = self.preview_audio_output.as_ref().unwrap();
        if paused {
            preview_output.set_state(crate::playback::audio_output::AudioState::Paused);
        } else {
            preview_output.set_state(crate::playback::audio_output::AudioState::Playing);
        }

        let seek_offset = seek_to.unwrap_or(std::time::Duration::ZERO);

        self.preview_stream = Some(setup.stream);
        self.preview_playback_source = Some(source.clone());
        self.preview_path = Some(path.clone());
        self.preview_position = seek_offset;
        self.preview_duration = duration;
        self.preview_seek_offset = seek_offset;

        let dur_ms = duration.as_millis() as u64;
        let preview_state = if paused {
            PreviewState::Paused {
                path: path.clone(),
                duration_ms: dur_ms,
            }
        } else {
            PreviewState::Playing {
                path: path.clone(),
                duration_ms: dur_ms,
            }
        };

        emit_progress(
            &self.progress_tx,
            PlaybackProgress::PreviewStateChanged(preview_state),
        );

        let progress_tx = self.progress_tx.clone();
        let command_tx = self.command_tx.clone();
        let mut position_rx_async = setup.position_rx;
        let mut completion_rx_async = setup.completion_rx;

        let h3 = tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Preview is single-track; the audio callback tags every
                    // tick with the same fmt we built above. Read its fields
                    // directly so the listener doesn't carry parallel copies
                    // of seek_offset/dur_ms.
                    Some((fmt, pos)) = position_rx_async.recv() => {
                        let actual_pos_ms = (fmt.position_offset + pos).as_millis() as u64;
                        emit_progress(
                            &progress_tx,
                            PlaybackProgress::PreviewPositionUpdate {
                                position_ms: actual_pos_ms,
                                progress: crate::playback::format::compute_progress(actual_pos_ms, fmt.duration_ms, fmt.pregap_ms),
                            },
                        );
                    }
                    Some((_fmt, _error_count, _samples_decoded)) = completion_rx_async.recv() => {
                        // Preview doesn't track decode stats — it's a quick
                        // playback, not a library track. The stats carried by
                        // CompletionEvent (uniform with main playback) are
                        // dropped here by design.
                        dispatch_command(&command_tx, PlaybackCommand::PreviewCompleted);
                        break;
                    }
                    else => break,
                }
            }
        });

        self.preview_listener_handles = vec![setup.bridge_handles, vec![h3]]
            .into_iter()
            .flatten()
            .collect();

        true
    }

    /// Handle preview play: toggle same file off, switch files, or start new preview.
    async fn handle_preview_play(&mut self, path: String) {
        // Same path: if playing or paused, dismiss (stop). If finished, replay.
        if self.preview_path.as_deref() == Some(&path) {
            let is_finished =
                self.preview_stream.is_none() && self.preview_playback_source.is_none();
            if is_finished {
                self.abort_preview_listeners();
                self.preview_path = None;
            } else {
                self.stop_preview();
                return;
            }
        }

        // If a different preview is active, stop it first (without resuming main)
        if self.preview_path.is_some() {
            self.stop_preview_without_resume();
        }

        let source_size = match tokio::fs::metadata(&path).await {
            Ok(metadata) => metadata.len(),
            Err(e) => {
                error!("Failed to stat preview file {}: {}", path, e);
                return;
            }
        };

        // Create sparse buffer and start local file reader
        let buffer = create_sparse_buffer(source_size);
        let read_config = AudioReadConfig {
            path: path.clone(),
            source_size,
        };
        let reader: Box<dyn AudioDataReader> = Box::new(LocalReader::new(read_config));
        reader.start_reading(buffer.clone(), self.progress_tx.clone());

        // Probe duration and sample rate from file
        let probe = {
            let probe_path = path.clone();
            tokio::task::spawn_blocking(move || {
                crate::audio_codec::probe_audio_from_path(&probe_path)
            })
            .await
            .unwrap_or(None)
        };
        let probed_duration = probe
            .as_ref()
            .map(|p| p.duration)
            .unwrap_or(std::time::Duration::ZERO);
        let sample_rate = probe.as_ref().map(|p| p.sample_rate).unwrap_or(44100);
        let channels = probe.as_ref().map(|p| p.channels).unwrap_or(2);

        self.preview_buffer = Some(buffer.clone());
        self.preview_sample_rate = sample_rate;
        self.preview_channels = channels;

        if self
            .start_preview_decode(
                path.clone(),
                probed_duration,
                sample_rate,
                channels,
                buffer,
                None,
                false,
                true,
            )
            .await
        {
            info!("Preview started: {}", path);
        }
    }

    /// Abort preview position/completion listener tasks.
    fn abort_preview_listeners(&mut self) {
        for handle in self.preview_listener_handles.drain(..) {
            handle.abort();
        }
    }

    /// Stop preview playback and resume main player if it was paused for preview.
    fn stop_preview(&mut self) {
        self.stop_preview_without_resume();
        self.maybe_resume_main_player();
    }

    /// Stop preview playback without resuming main player.
    fn stop_preview_without_resume(&mut self) {
        if let Some(source) = self.preview_playback_source.take() {
            if let Ok(guard) = source.lock() {
                guard.cancel();
            }
        }

        // Cancel buffer to unblock decoder, then drop its handle
        if let Some(buf) = &self.preview_buffer {
            buf.cancel();
        }
        self.preview_buffer = None;
        self.preview_decoder_handle = None;

        if let Some(stream) = self.preview_stream.take() {
            drop(stream);
        }
        if let Some(preview_output) = &self.preview_audio_output {
            preview_output.set_state(crate::playback::audio_output::AudioState::Stopped);
        }

        self.abort_preview_listeners();

        let was_previewing = self.preview_path.is_some();
        self.preview_path = None;
        self.preview_position = std::time::Duration::ZERO;
        self.preview_duration = std::time::Duration::ZERO;
        self.preview_seek_offset = std::time::Duration::ZERO;

        if was_previewing {
            info!("Preview stopped");
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::PreviewStateChanged(PreviewState::Idle),
            );
        }
    }

    /// Handle preview toggle pause/resume.
    async fn handle_preview_toggle_pause(&mut self) {
        let Some(path) = self.preview_path.clone() else {
            return;
        };

        let is_finished = self.preview_stream.is_none() && self.preview_playback_source.is_none();

        if is_finished {
            self.handle_preview_play(path).await;
            return;
        }

        let Some(preview_output) = &self.preview_audio_output else {
            return;
        };

        let dur_ms = self.preview_duration.as_millis() as u64;
        match preview_output.get_state() {
            crate::playback::audio_output::AudioState::Playing => {
                preview_output.set_state(crate::playback::audio_output::AudioState::Paused);

                // Record the current position so resume can continue from there.
                let position = self
                    .preview_playback_source
                    .as_ref()
                    .map(|s| self.preview_seek_offset + s.lock().unwrap().position())
                    .unwrap_or(self.preview_position);
                self.preview_position = position;

                emit_progress(
                    &self.progress_tx,
                    PlaybackProgress::PreviewStateChanged(PreviewState::Paused {
                        path,
                        duration_ms: dur_ms,
                    }),
                );
            }
            crate::playback::audio_output::AudioState::Paused => {
                preview_output.set_state(crate::playback::audio_output::AudioState::Playing);

                emit_progress(
                    &self.progress_tx,
                    PlaybackProgress::PreviewStateChanged(PreviewState::Playing {
                        path,
                        duration_ms: dur_ms,
                    }),
                );
            }
            _ => {}
        }
    }

    /// Handle natural preview completion: fully stop preview and resume main player.
    fn handle_preview_completed(&mut self) {
        info!("Preview finished");
        self.stop_preview();
    }

    /// Seek within the active preview.
    async fn handle_preview_seek(&mut self, position: std::time::Duration) {
        let buffer = match &self.preview_buffer {
            Some(buf) => buf.clone(),
            None => return,
        };
        if self.preview_path.is_none() {
            return;
        }
        let duration = self.preview_duration;
        if duration.is_zero() {
            return;
        }

        let was_paused = self
            .preview_audio_output
            .as_ref()
            .map(|o| o.get_state() == crate::playback::audio_output::AudioState::Paused)
            .unwrap_or(false);

        // Abort old listeners immediately to prevent stale position ticks
        self.abort_preview_listeners();
        if let Some(stream) = self.preview_stream.take() {
            drop(stream);
        }

        // Tear down old decoder, preserve buffer
        let preview_cancel = Arc::new(std::sync::atomic::AtomicBool::new(true));
        Self::teardown_decoder_for_seek(
            &mut self.preview_playback_source,
            &buffer,
            &preview_cancel,
            &mut self.preview_decoder_handle,
            false,
        )
        .await;

        // Start new decoder on the same buffer with seek_to
        let path = self.preview_path.clone().unwrap();
        let sample_rate = self.preview_sample_rate;
        let channels = self.preview_channels;
        self.start_preview_decode(
            path,
            duration,
            sample_rate,
            channels,
            buffer,
            Some(position),
            was_paused,
            false,
        )
        .await;

        // When seeking while paused, no tick will fire to carry the new
        // position — emit explicitly so the NSView updates. When seeking
        // while playing, the position listener task picks up from the new
        // offset on its next tick, so no explicit emit is needed.
        if was_paused {
            let pos_ms = position.as_millis() as u64;
            let dur_ms = duration.as_millis() as u64;
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::PreviewPositionUpdate {
                    position_ms: pos_ms,
                    progress: crate::playback::format::compute_progress(pos_ms, dur_ms, None),
                },
            );
        }
    }

    async fn stop(&mut self) {
        self.pending_side_pause = None;
        // Stop any active preview first (without resuming main, since we're stopping)
        self.stop_preview_without_resume();
        self.main_was_playing_before_preview = false;

        // Tear down the current track (stream, source, buffer, decoder,
        // listeners) — the half `stop()` shares with a manual track switch.
        self.teardown_current_track();

        // Stop-specific teardown beyond the current track:
        self.clear_next_track_state();
        // Drop shared buffer cache — stop means we're done with this album
        self.shared_file_buffer = None;
        *self.current_position_shared.lock().unwrap() = None;
        self.audio_output
            .set_state(crate::playback::audio_output::AudioState::Stopped);
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::StateChanged {
                state: PlaybackState::Stopped,
            },
        );
        // Audio is now Stopped, so this clears the durable row — covering every
        // stop path (natural end, halt-on-error, the current track removed),
        // not just the explicit Stop command.
        self.persist_playback_state().await;
    }
    async fn seek(&mut self, position: std::time::Duration) {
        // Verify streaming state is available
        if self.current_playback_source.is_none() {
            error!("Cannot seek: no streaming source active");
            return;
        }

        let prepared = match &self.current_prepared {
            Some(p) => p,
            None => {
                error!("Cannot seek: no current_prepared");
                return;
            }
        };

        let track_id = prepared.track_info.track_id.clone();
        let sample_rate = prepared.sample_rate;
        let channels = prepared.channels;
        let buffer = prepared.buffer.clone();

        // Check for same-position seek (difference < 100ms)
        let current_position = self
            .current_position_shared
            .lock()
            .unwrap()
            .unwrap_or(std::time::Duration::ZERO);
        let position_diff = position.abs_diff(current_position);
        if position_diff < std::time::Duration::from_millis(100) {
            trace!(
                "Seek: Skipping seek to same position (difference: {:?} < 100ms)",
                position_diff
            );
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::SeekSkipped {
                    requested_position: position,
                    current_position,
                },
            );
            return;
        }

        // Abort old listeners immediately to prevent stale position ticks
        self.abort_current_listeners();

        // Preserve any staged gapless next track across the stream rebuild so
        // playback stays gapless after a seek. Removing it from the old chain
        // keeps the teardown below from cancelling its (still-running) decoder;
        // its source + fmt are re-staged into the new stream once it's built.
        let staged_next = if let Some(gapless) = &self.current_playback_source {
            gapless.lock().unwrap().take_next()
        } else {
            None
        };
        let staged_next: Option<(TrackStream, TrackFmt)> =
            staged_next.map(|(s, fmt)| (s, (*fmt).clone()));

        // Tear down old decoder, preserve buffer
        let cancel_token = self
            .current_prepared
            .as_ref()
            .map(|p| p.cancel_token.clone())
            .unwrap_or_else(|| Arc::new(std::sync::atomic::AtomicBool::new(true)));
        let is_shared = self
            .current_prepared
            .as_ref()
            .is_some_and(|p| p.buffer_shared);
        Self::teardown_decoder_for_seek(
            &mut self.current_playback_source,
            &buffer,
            &cancel_token,
            &mut self.current_decoder_handle,
            is_shared,
        )
        .await;

        // Seek to the track's start plus the requested in-track position.
        let position_samples = (position.as_secs_f64() * sample_rate as f64) as u64;

        // Mint a fresh cancel token for the seek's decoder so the ready-watcher's
        // TrackReady is tied to this seek, and a later seek/switch supersedes it.
        let new_cancel_token = Arc::new(std::sync::atomic::AtomicBool::new(false));
        if let Some(prepared) = &mut self.current_prepared {
            prepared.cancel_token = new_cancel_token;
        }

        // Show buffering at the target immediately (the same Loading→Playing arc
        // the play path uses): the bar jumps to the seek position via Seeked
        // below, Loading covers the wait for the demanded window, and the
        // ready-watcher confirms Playing once audio flows. The decoder reads
        // immediately and the demand-driven fill fetches the seek target first,
        // so there's no fixed wait and no frozen-but-Playing bar.
        let prepared = self
            .current_prepared
            .as_ref()
            .expect("seek requires a current track");
        let decode = prepared.decode_params(position_samples);
        info!(
            "Seek: position {:?}, seek_to {}",
            position, decode.target_sample
        );
        let loading_duration_ms = pregap_adjusted_duration(prepared);
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::StateChanged {
                state: PlaybackState::Loading {
                    track_id: track_id.clone(),
                    resolved: Some(LoadingTrack {
                        track_info: prepared.track_info.clone(),
                        duration_ms: loading_duration_ms,
                    }),
                },
            },
        );

        // Seek keeps the same current track, with the seek target as the new
        // stream's in-track offset.
        let fmt = prepared.track_fmt(position);

        if !self
            .start_decoder_and_watch(decode, fmt, sample_rate, channels, track_id.clone())
            .await
        {
            // The rebuilt stream failed. The preserved next track was taken out
            // of the old chain, so stop()'s teardown can't reach it — cancel it
            // here (otherwise its decoder parks forever filling a buffer with no
            // consumer). Then resolve the Loading we just emitted to Stopped via
            // stop(), the same hard-failure outcome the play path takes when
            // audio output can't start, so the bar doesn't hang in buffering.
            if let Some((next_source, _next_fmt)) = staged_next {
                next_source.cancel();
            }
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::PlaybackError {
                    reason: crate::ui::PlaybackErrorReason::internal(
                        "Couldn't restart audio output after the seek.",
                    ),
                },
            );
            self.stop().await;
            return;
        }

        // Re-stage the preserved gapless next track into the rebuilt stream so
        // post-seek auto-advance stays gapless without re-decoding. The init
        // above guarantees current_playback_source is set on the success
        // path — a silent skip here would leak the staged decoder, since
        // TrackStream doesn't cancel on drop.
        if let Some((next_source, next_fmt)) = staged_next {
            self.current_playback_source
                .as_ref()
                .expect("init_streaming succeeded above; gapless source must be set")
                .lock()
                .unwrap()
                .stage_next(next_source, next_fmt);
        }

        let raw_pos_ms = position.as_millis() as u64;
        self.emit_position_display(raw_pos_ms, track_id);
    }
    /// Emit queue update to all subscribers
    async fn on_queue_mutated(&mut self) {
        self.pending_side_pause = None;
        self.refresh_preload_for_queue_front().await;
        self.emit_queue_update();
        self.persist_playback_state().await;
    }

    fn emit_queue_update(&self) {
        let has_next = self.playback_queue.has_upcoming()
            || self.playback_queue.repeat_mode() != RepeatMode::Off;
        let has_previous = self.playback_queue.has_previous();
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::QueueUpdated {
                manual: self.playback_queue.manual_entries(),
                context: self.playback_queue.context_projection(),
                has_next,
                has_previous,
            },
        );
    }

    fn emit_queue_items_added(&self, count: u32) {
        if count == 0 {
            return;
        }
        let event = PlaybackProgress::QueueItemsAdded { count };
        let _ = self.progress_tx.send(event);
    }
}

fn pregap_seek_position(
    pregap_ms: Option<i64>,
    is_natural_transition: bool,
) -> Option<std::time::Duration> {
    if is_natural_transition {
        // Natural transition: start at INDEX 00, play the pregap
        None
    } else {
        // Direct selection: skip to INDEX 01 if there's a pregap
        pregap_ms
            .filter(|&p| p > 0)
            .map(|p| std::time::Duration::from_millis(p as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pregap_seek_position_cases() {
        use std::time::Duration;
        // (pregap_ms, is_natural_transition) -> seek position.
        // A natural transition always plays from the start (None, so the pregap
        // is heard); a direct selection skips a positive pregap and otherwise
        // needs no seek.
        let cases = [
            (Some(3000i64), false, Some(Duration::from_millis(3000))),
            (Some(3000i64), true, None),
            (None, false, None),
            (None, true, None),
        ];
        for (pregap_ms, is_natural_transition, expected) in cases {
            assert_eq!(
                pregap_seek_position(pregap_ms, is_natural_transition),
                expected,
                "pregap_ms={pregap_ms:?} natural={is_natural_transition}"
            );
        }
    }

    // Seek tests for SparseStreamingBuffer integration
    use crate::playback::sparse_buffer::SparseStreamingBuffer;

    #[test]
    fn test_seek_within_buffer() {
        let buffer = SparseStreamingBuffer::new(10000);
        // Buffer has first 10000 bytes
        buffer.append_at(0, &vec![0u8; 10000]);

        // Seek to byte 5000 - should be buffered
        assert!(
            buffer.is_buffered(5000),
            "Position 5000 should be within buffered range"
        );
    }

    #[test]
    fn test_seek_past_buffer() {
        let buffer = SparseStreamingBuffer::new(60000);
        // Buffer has first 10000 bytes
        buffer.append_at(0, &vec![0u8; 10000]);

        // Seek to byte 50000 - should NOT be buffered
        assert!(
            !buffer.is_buffered(50000),
            "Position 50000 should be past buffered range"
        );
    }

    #[test]
    fn test_seek_multiple_ranges() {
        let buffer = SparseStreamingBuffer::new(60000);
        // Buffer has 0-10000 and 50000-60000
        buffer.append_at(0, &vec![0u8; 10000]);
        buffer.append_at(50000, &vec![0u8; 10000]);

        // Currently at 55000, seek back to 5000 should reuse first range
        assert!(buffer.is_buffered(5000), "Position 5000 should be buffered");
        assert!(
            buffer.is_buffered(55000),
            "Position 55000 should be buffered"
        );
        assert!(
            !buffer.is_buffered(30000),
            "Position 30000 should NOT be buffered (gap)"
        );
    }

    #[test]
    fn test_seek_back_after_forward_seek() {
        use crate::playback::sparse_buffer::create_sparse_buffer;

        let buffer = create_sparse_buffer(90000);

        // Initial download: 0-30000
        buffer.append_at(0, &vec![0u8; 30000]);

        // User seeks forward to byte 70000 - new download starts there
        // Simulating: 70000-90000
        buffer.append_at(70000, &vec![0u8; 20000]);

        // Now we have two ranges: 0-30000 and 70000-90000
        assert_eq!(
            buffer.get_ranges(),
            vec![(0, 30000), (70000, 90000)],
            "Should have two non-contiguous ranges"
        );

        // User seeks back to byte 15000 - should be buffered (first range)
        assert!(buffer.is_buffered(15000), "15000 should be in first range");

        // User seeks to byte 75000 - should be buffered (second range)
        assert!(buffer.is_buffered(75000), "75000 should be in second range");

        // User seeks to byte 50000 - gap between ranges, not buffered
        assert!(!buffer.is_buffered(50000), "50000 should be in the gap");
    }

    #[test]
    fn test_ranges_merge_when_gap_filled() {
        use crate::playback::sparse_buffer::create_sparse_buffer;

        let buffer = create_sparse_buffer(30000);

        // Initial download: 0-10000
        buffer.append_at(0, &vec![0u8; 10000]);

        // Seek forward creates second range: 20000-30000
        buffer.append_at(20000, &vec![0u8; 10000]);

        assert_eq!(buffer.get_ranges().len(), 2, "Should have two ranges");

        // Original download continues and fills gap: 10000-20000
        buffer.append_at(10000, &vec![0u8; 10000]);

        // Ranges should now be merged
        assert_eq!(buffer.get_ranges().len(), 1, "Ranges should be merged");
        assert_eq!(
            buffer.get_ranges(),
            vec![(0, 30000)],
            "Should be single contiguous range"
        );
    }
}
