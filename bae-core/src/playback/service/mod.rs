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
    repeat_to_str, source_to_str, ContextSource, ContextStart, NextEntry, PersistedPlayback,
    PlaybackQueue, PreviousAction, QueueEntryId, QueueSnapshot, Traversal,
};
use crate::audio_codec::StreamingDecodeError;
use crate::db::{DbAudioSegmentRole, DbPlaybackContext, DbPlaybackState};
use crate::library::LibraryEvent;
use crate::library::LibraryManager;
use crate::library::ResolvedTrackAudio;
use crate::playback::audio_output::{AudioOutput, AudioStream, CompletionEvent, PositionEvent};
use crate::playback::data_source::{create_audio_reader, FetchArbiter};
use crate::playback::error::PlaybackError;
use crate::playback::progress::emit_progress;
use crate::playback::progress::{PlaybackProgress, PlaybackProgressHandle};
// The `source` module is imported by path so the audio sample feed reads
// `source::PlaybackSource` — distinct from the queue's `ContextSource`.
use crate::playback::preview_player::PreviewPlayer;
use crate::playback::source;
use crate::playback::source::{TrackCrossing, TrackFmt};
use crate::playback::sparse_buffer::{create_sparse_buffer, SharedSparseBuffer};
use crate::playback::{create_track_stream_pair, TrackStream};
use crate::util::format::PhysicalSideMedium;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc as tokio_mpsc;
use tokio::sync::oneshot;
use tracing::{debug, error, info, trace, warn};

mod advance;
mod pipeline;
mod preview;
mod queue_commands;
mod seek;
mod state;

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
    /// Play the whole library in a freshly seeded shuffle. An empty library is a
    /// no-op (logged); the seed is minted in the handler.
    PlayLibraryShuffled,
    Pause,
    Resume,
    Stop,
    /// Manual next track (skip pregap)
    Next,
    /// Auto-advance from track completion (play pregap)
    AutoAdvance,
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
    /// Set the playing context to sequential or shuffled order. `true` mints a
    /// fresh seed and materializes a shuffled order; `false` restores source order.
    /// The current track keeps playing.
    SetShuffle(bool),
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

/// Handle to the playback service for sending commands
#[derive(Clone)]
pub struct PlaybackHandle {
    command_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
    progress_handle: PlaybackProgressHandle,
    last_position_display: std::sync::Arc<std::sync::Mutex<Option<f64>>>,
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

    pub fn set_shuffle(&self, on: bool) {
        dispatch_command(&self.command_tx, PlaybackCommand::SetShuffle(on));
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

    /// Read the most recent position display progress. Used by late-mounting
    /// views (e.g. the progress NSView after startup restore) to populate
    /// themselves immediately instead of waiting for the next position tick.
    pub fn get_last_position_display(&self) -> Option<f64> {
        *self.last_position_display.lock().unwrap()
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
                .end_sample
                .map(|end| end.saturating_sub(segment.start_sample));
            if let Some(len) = segment_len {
                if remaining_offset >= len {
                    remaining_offset -= len;
                    continue;
                }
            }
            let target_sample = segment.start_sample + remaining_offset;
            let seek_to_byte = if remaining_offset == 0 && self.content_type != ContentType::Ape {
                segment.start_byte
            } else {
                None
            };
            segments.push(SegmentDecodeParams {
                buffer: segment.buffer.clone(),
                seek_to_byte,
                target_sample,
                stop_at_sample: segment.end_sample,
                end_byte: segment.end_byte,
            });
            remaining_offset = 0;
        }

        StreamDecodeParams {
            segments,
            leading_silence_frames,
        }
    }

    fn total_pregap_ms(&self) -> Option<i64> {
        self.pregap_ms.or(self.generated_pregap_ms)
    }

    fn generated_pregap_samples(&self) -> u64 {
        let samples = self.generated_pregap_samples.unwrap_or_else(|| {
            self.generated_pregap_ms.map_or(0, |ms| {
                assert!(ms >= 0, "audio_format generated_pregap_ms is non-negative");
                ((ms as f64 / 1000.0) * self.sample_rate as f64) as i64
            })
        });
        u64::try_from(samples).expect("audio_format generated_pregap_samples is non-negative")
    }

    fn cancel_unshared_buffers(&self) {
        for segment in &self.segments {
            if !segment.buffer_shared {
                segment.buffer.cancel();
            }
        }
    }
}

#[derive(Clone)]
struct PreparedAudioSegment {
    role: DbAudioSegmentRole,
    buffer: SharedSparseBuffer,
    buffer_shared: bool,
    start_sample: u64,
    end_sample: Option<u64>,
    start_byte: Option<u64>,
    end_byte: Option<u64>,
}

#[derive(Clone)]
struct PlaybackPreparedTrack {
    track_info: PlaybackTrackInfo,
    segments: Vec<PreparedAudioSegment>,
    /// Per-decoder cancellation token. Set to true to stop the AVIO read callback
    /// without affecting other decoders on the same buffer.
    cancel_token: Arc<std::sync::atomic::AtomicBool>,
    /// Sample rate in Hz for time-to-sample conversion
    sample_rate: u32,
    channels: u32,
    /// Pre-gap duration in ms (for CUE/FLAC tracks)
    pregap_ms: Option<i64>,
    /// Generated silent pregap duration in ms from a CUE `PREGAP` directive.
    generated_pregap_ms: Option<i64>,
    /// Generated silent pregap duration in exact samples.
    generated_pregap_samples: Option<i64>,
    /// Track duration from metadata
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

fn clear_preloaded_next(
    preloaded_next: &mut Option<PreloadedNext>,
    current_playback_source: Option<&Arc<Mutex<source::PlaybackSource>>>,
) {
    let Some(preloaded) = preloaded_next.take() else {
        return;
    };

    let PreloadedNext {
        prepared, source, ..
    } = preloaded;

    if let Some(source) = detach_preloaded_source(current_playback_source, source) {
        source.cancel();
    }
    prepared.cancel_unshared_buffers();
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

/// Finalize a PlaybackPreparedTrack from resolved audio, display info, and buffer.
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
        cancel_token: Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
        let cached = shared_file_buffers.get(&segment.file_id).cloned();
        let buffer_shared = cached.is_some();
        let buffer = if let Some(buf) = cached {
            info!("Reusing cached file buffer");
            buf.uncancel();
            buf
        } else {
            let buffer = create_sparse_buffer(segment.file_size);
            let reader = create_audio_reader(
                library_manager,
                &segment.file_id,
                segment.cloud_path.as_deref(),
                segment.file_size,
                fetch_arbiter.clone(),
                segment.start_byte,
                resolved.content_type == crate::util::content_type::ContentType::Ape,
            );
            reader.start_reading(buffer.clone(), progress_tx.clone());
            shared_file_buffers.insert(segment.file_id.clone(), buffer.clone());
            buffer
        };
        prepared_segments.push(PreparedAudioSegment {
            role: segment.role.clone(),
            buffer,
            buffer_shared,
            start_sample: segment.start_sample,
            end_sample: segment.end_sample,
            start_byte: segment.start_byte,
            end_byte: segment.end_byte,
        });
    }

    // Read the replay-gain mode once here and pass it down (DI: one read at the
    // top, not a static lookup buried in `finalize_playback_track`).
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
    stream: Option<Box<dyn AudioStream>>,
    /// Current track prepared data and streaming state
    current_prepared: Option<PlaybackPreparedTrack>,
    /// Current playing source. A `PlaybackSource` wraps the current track and an
    /// optional pre-staged next track so the audio callback can advance across a
    /// track boundary without rebuilding the stream.
    current_playback_source: Option<Arc<Mutex<source::PlaybackSource>>>,
    /// JoinHandle for the current decoder thread (needed for seek cancellation)
    current_decoder_handle: Option<std::thread::JoinHandle<()>>,
    /// Listener task handles for the current track (position ticks + completion)
    current_listener_handle: Option<tokio::task::JoinHandle<()>>,
    /// Preloaded next track state, either staged into the current gapless source
    /// or held for a stream rebuild.
    preloaded_next: Option<PreloadedNext>,
    /// Mute state — core tracks this so UI doesn't need to.
    is_muted: bool,
    pre_mute_volume: f32,
    /// The preview player — a self-contained second player for auditioning a
    /// local file. The service only coordinates pause/resume of the main player
    /// around it; the preview's own state lives entirely in `PreviewPlayer`.
    preview: PreviewPlayer,
    /// Whether the main player was playing before preview started (to resume on stop)
    main_was_playing_before_preview: bool,
    /// How often (ms) the audio callback sends position updates to the UI.
    position_update_interval_ms: u32,
    /// Cached full-file buffers keyed by release file id.
    shared_file_buffers: HashMap<String, SharedSparseBuffer>,
    /// Shared with PlaybackHandle. Written on every position tick and by
    /// `emit_position_display`; read by late-mounting views.
    last_position_display: Arc<std::sync::Mutex<Option<f64>>>,
    /// Sender cloned into each `PlaybackSource`; fired by the audio callback
    /// when it crosses a gapless track boundary, carrying the finishing and
    /// incoming track identities + the finishing track's decode stats.
    boundary_tx: tokio_mpsc::UnboundedSender<TrackCrossing>,
    /// Receiver side of `boundary_tx`, drained by the service loop.
    boundary_rx: tokio_mpsc::UnboundedReceiver<TrackCrossing>,
    pending_side_pause: Option<SidePauseDecision>,
    /// Prioritizes byte fetches across tracks: the current track's reader fetches
    /// immediately, a next-track preload's reader yields to it. Shared into every
    /// reader; the current track is designated foreground via
    /// `mark_current_foreground` whenever it becomes current.
    fetch_arbiter: Arc<FetchArbiter>,
}

impl PlaybackService {
    /// Designate the current track's buffer as the foreground for fetch
    /// priority, so its reader fetches immediately and a next-track preload's
    /// reader yields to it. Called wherever a track becomes the current one.
    fn mark_current_foreground(&self) {
        if let Some(prepared) = &self.current_prepared {
            if let Some(segment) = prepared.segments.first() {
                self.fetch_arbiter.set_foreground(segment.buffer.id());
            }
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

/// Construct the platform's concrete audio output. Called on the service's
/// dedicated thread so the sink owns any thread-bound device handle it opens
/// there (cpal builds lazily per stream; AAudio binds its writer thread).
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

pub(crate) struct StreamSetup {
    pub(crate) stream: Box<dyn AudioStream>,
    pub(crate) position_rx: tokio_mpsc::UnboundedReceiver<PositionEvent>,
    pub(crate) completion_rx: tokio_mpsc::UnboundedReceiver<CompletionEvent>,
}

pub(crate) async fn setup_audio_stream(
    audio_output: &mut dyn AudioOutput,
    source: Arc<Mutex<source::PlaybackSource>>,
    position_update_interval_ms: u32,
) -> Result<StreamSetup, PlaybackError> {
    let (source_sample_rate, source_channels) = {
        let guard = source.lock().unwrap();
        (guard.sample_rate(), guard.channels())
    };

    let (position_tx, position_rx) = tokio_mpsc::unbounded_channel();
    let (completion_tx, completion_rx) = tokio_mpsc::unbounded_channel();

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
            return Err(PlaybackError::task(format!("Audio stream: {:?}", e)));
        }
    };

    Ok(StreamSetup {
        stream,
        position_rx,
        completion_rx,
    })
}

/// Tear down the decoded-audio pipeline for a seek.
///
/// Cancels this decoder's token to unblock the old decoder while the data
/// reader keeps filling the buffer. After this, the buffer is ready for a new
/// decoder to read from position 0. Shared by the main player's seek and the
/// preview player's seek.
pub(crate) async fn teardown_decoder_for_seek(
    source: &mut Option<Arc<Mutex<source::PlaybackSource>>>,
    buffers: &[SharedSparseBuffer],
    cancel_token: &Arc<std::sync::atomic::AtomicBool>,
    decoder_handle: &mut Option<std::thread::JoinHandle<()>>,
) {
    // Cancel streaming source (cpal callback outputs silence)
    if let Some(src) = source.take() {
        if let Ok(guard) = src.lock() {
            guard.cancel();
        }
    }

    // Cancel this decoder's AVIO reads via its token.
    cancel_token.store(true, std::sync::atomic::Ordering::Release);

    // Wake any readers blocked on the condvar so they can check the token.
    for buffer in buffers {
        buffer.wake_readers();
    }

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
}

/// The decoder window for one stream. `target_sample` is where FFmpeg trims
/// lead-in (the track's start plus any pregap/seek offset). The seek to it is
/// either a by-byte jump (`seek_to_byte`, a lossless track's natural start) or a
/// sample seek (APE, or a mid-track seek); the `target_sample` trim makes the
/// first output sample exact either way. `stop_at_sample` ends output at the
/// track's end (`None` = to EOF).
struct StreamDecodeParams {
    segments: Vec<SegmentDecodeParams>,
    leading_silence_frames: u64,
}

struct SegmentDecodeParams {
    buffer: SharedSparseBuffer,
    seek_to_byte: Option<u64>,
    target_sample: u64,
    stop_at_sample: Option<u64>,
    /// The track's end byte offset -- the read-ahead ceiling. `None` = whole file.
    end_byte: Option<u64>,
}

impl StreamDecodeParams {
    /// Run the streaming decoder for this window: FFmpeg seeks (by byte or by
    /// sample), trims lead-in at `target_sample`, and stops at `stop_at_sample`.
    /// Shared by the play/seek and preload paths so they can't drift on the
    /// seek/trim mapping.
    fn run_decoder(
        &self,
        _buffer: SharedSparseBuffer,
        sink: &mut crate::playback::track_stream::TrackSink,
        cancel: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<(), StreamingDecodeError> {
        if self.leading_silence_frames > 0 {
            sink.push_silence_frames_blocking(self.leading_silence_frames);
            if cancel.load(std::sync::atomic::Ordering::Relaxed) || sink.is_cancelled() {
                return Err(StreamingDecodeError::InputCancelled);
            }
        }

        for segment in &self.segments {
            let seek_to_sample = if segment.seek_to_byte.is_some() {
                None
            } else {
                Some(segment.target_sample)
            };
            crate::audio_codec::decode_audio_streaming(
                segment.buffer.clone(),
                sink,
                segment.seek_to_byte,
                seek_to_sample,
                Some(segment.target_sample),
                segment.stop_at_sample,
                segment.end_byte,
                cancel.clone(),
            )?;
            if cancel.load(std::sync::atomic::Ordering::Relaxed) || sink.is_cancelled() {
                return Err(StreamingDecodeError::InputCancelled);
            }
        }
        Ok(())
    }
}

impl PlaybackService {
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
        let (boundary_tx, boundary_rx) = tokio_mpsc::unbounded_channel::<TrackCrossing>();
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
                    stream: None,
                    current_prepared: None,
                    current_playback_source: None,
                    current_decoder_handle: None,
                    current_listener_handle: None,
                    preloaded_next: None,
                    preview,
                    main_was_playing_before_preview: false,
                    is_muted: false,
                    pre_mute_volume: 1.0,
                    position_update_interval_ms,
                    shared_file_buffers: HashMap::new(),
                    last_position_display,
                    boundary_tx,
                    boundary_rx,
                    pending_side_pause: None,
                    fetch_arbiter: FetchArbiter::new(),
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
        loop {
            tokio::select! {
                Some(command) = self.command_rx.recv() => {
            match command {
                PlaybackCommand::Play(track_id) => {
                    self.stop_preview_for_main_playback();
                    // Direct selection: the track's release becomes the playing
                    // context, with the cursor at the chosen track.
                    match self.library_manager.get_play_context(&track_id).await {
                        Ok(context) => {
                            self.playback_queue.play_release(
                                ContextSource::Release(context.release_id),
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
                    self.play_track(&track_id, TrackStart::Direct, false).await;
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

                    self.stop_preview_for_main_playback();
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
                    let first_track = self.playback_queue.play_release(
                        ContextSource::Release(release_id),
                        track_ids,
                        start,
                    );
                    self.pending_side_pause = None;
                    self.emit_queue_update();
                    self.play_track(&first_track, TrackStart::Direct, false).await;
                }
                PlaybackCommand::PlayLibraryShuffled => {
                    let track_ids = match self.fetch_source_tracks(&ContextSource::Library).await {
                        Ok(ids) => ids,
                        Err(e) => {
                            error!("PlayLibraryShuffled: couldn't load library tracks: {e}");
                            continue;
                        }
                    };
                    if track_ids.is_empty() {
                        warn!("PlayLibraryShuffled: the library has no tracks; nothing to play");
                        continue;
                    }
                    self.stop_preview_for_main_playback();
                    // A fresh seed minted at the command boundary so the shuffled
                    // order is reproducible and `Context` repeat can re-derive it.
                    let first_track = self.playback_queue.play_release(
                        ContextSource::Library,
                        track_ids,
                        ContextStart::Shuffled {
                            seed: rand::random(),
                        },
                    );
                    self.pending_side_pause = None;
                    self.emit_queue_update();
                    self.play_track(&first_track, TrackStart::Direct, false).await;
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
                                self.play_track(&next_track, TrackStart::Direct, !was_side_paused)
                                    .await;
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
                            self.play_track(&track_id, TrackStart::Natural, false).await;
                        }
                        NextEntry::Play(next_track) => {
                            info!("Playing from queue: {}", next_track);
                            self.emit_queue_update();
                            self.play_track(&next_track, TrackStart::Natural, false).await;
                        }
                        NextEntry::Stop => {
                            info!("No next track available, stopping");
                            self.emit_queue_update();
                            self.stop().await;
                        }
                    }
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
                                self.emit_queue_update();
                                self.play_track(&previous_track_id, TrackStart::Direct, true)
                                    .await;
                            }
                            PreviousAction::RestartCurrent => {
                                info!("Restarting current track from beginning");
                                self.play_track(&current_track_id, TrackStart::Direct, true).await;
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
                    self.apply_repeat_mode(mode).await;
                }
                PlaybackCommand::CycleRepeatMode => {
                    let next = self.playback_queue.repeat_mode().next();
                    self.apply_repeat_mode(next).await;
                }
                PlaybackCommand::SetShuffle(on) => {
                    match self.playback_queue.context_source().cloned() {
                        Some(source) => match self.fetch_source_tracks(&source).await {
                            Ok(source_tracks) => {
                                // A fresh seed minted here at the command boundary so
                                // a shuffled order is reproducible and `Context`
                                // repeat can re-derive it. `set_shuffle` consumes the
                                // seed only when turning shuffle on; off ignores it.
                                let seed = rand::random();
                                self.playback_queue.set_shuffle(on, source_tracks, seed);
                                self.emit_queue_update();
                                self.persist_playback_state().await;
                            }
                            Err(e) => warn!(
                                "SetShuffle: couldn't fetch source tracks for {source:?}: {e}; \
                                 leaving the queue unchanged"
                            ),
                        },
                        None => warn!("SetShuffle: no playing context; nothing to shuffle"),
                    }
                }
                PlaybackCommand::SkipTo(entry_id) => {
                    if let Some(entry) = self.playback_queue.skip_to(&entry_id) {
                        info!(
                            "SkipTo: jumping to queue entry {}, track {}",
                            entry.id.0, entry.track_id
                        );

                        self.pending_side_pause = None;
                        self.emit_queue_update();
                        self.play_track(&entry.track_id, TrackStart::Direct, false).await;
                    }
                }
                PlaybackCommand::PreviewPlay(path) => {
                    self.preview_play(path).await;
                }
                PlaybackCommand::PreviewStop => {
                    self.preview_stop();
                }
                PlaybackCommand::PreviewTogglePause => {
                    self.preview_toggle_pause().await;
                }
                PlaybackCommand::PreviewSeekByRatio(ratio) => {
                    self.preview.seek_by_ratio(ratio).await;
                }
                PlaybackCommand::PreviewCompleted => {
                    self.preview_completed();
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
                Some(crossing) = self.boundary_rx.recv() => {
                    self.handle_track_crossed(crossing).await;
                }
                else => break,
            }
        }
        info!("PlaybackService stopped");
    }
}

fn pregap_seek_position(pregap_ms: Option<i64>) -> Option<std::time::Duration> {
    pregap_ms
        .filter(|&p| p > 0)
        .map(|p| std::time::Duration::from_millis(p as u64))
}
