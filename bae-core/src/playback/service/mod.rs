//! # Playback Service
//!
//! Runs on its own thread and drives playback from a command channel.
//!
//! ## Audio state
//!
//! The audio callback reads a shared `AudioState` atomic (`Stopped`, `Playing`,
//! `Paused`) without locking and outputs samples only when `Playing`. The atomic
//! is written from the `PlaybackSlot` (`slot.rs`), which holds the real state.
//!
//! ## Seek flow (`seek.rs`)
//!
//! 1. Playback goes Loading and the atomic Stopped, so the callback stays silent
//!    while the new decoder fills.
//! 2. A new decoder is spawned over the same byte buffers and swapped into the
//!    persistent source (`PlaybackSource::replace`) before the old one is
//!    joined, to keep the silence short; two readers on one sparse buffer is
//!    supported.
//! 3. The old decoder is then cancelled and joined (`cancel_and_join_decoder`).
//! 4. `Seeked` is emitted, and playback stays Loading until `TrackReady`
//!    resolves it to the Playing or Paused it had before.
//!
//! ## File buffers
//!
//! `FileBuffers` (`file_buffers.rs`) owns the byte buffers tracks stream from,
//! the tracks waiting for their buffers' release, and the fetch-priority arbiter
//! every reader shares.

use super::RepeatMode;
use super::{
    repeat_to_str, source_to_str, ContextSource, ContextStart, NextEntry, PersistedPlayback,
    PreviousAction, PublishedQueue, QueueEntryId, QueueSnapshot,
};
use crate::audio_codec::DecodeError;
use crate::db::{DbAudioSegmentRole, DbPlaybackContext, DbPlaybackState};
use crate::diagnostics::{
    AnomalyKind, LocalId, PlaybackCommandKind, PlaybackOperation, PlaybackStartSource,
    TelemetryEvent, TrackTransition,
};
use crate::library::LibraryManager;
use crate::library::ResolvedTrackAudio;
use crate::playback::audio_output::{
    AudioEvent, AudioEventReceiver, AudioOutput, AudioOutputDevice, AudioStream,
};
use crate::playback::data_source::{create_audio_reader, FetchArbiter};
use crate::playback::error::PlaybackError;
use crate::playback::preview_player::PreviewPlayer;
use crate::playback::progress::emit_progress;
use crate::playback::progress::{
    PlaybackProgress, PlaybackProgressHandle, PlaybackQueueProjection,
};
// Imported by path so the sample feed reads `source::PlaybackSource`, apart
// from the queue's `ContextSource`.
use crate::playback::source;
use crate::playback::source::{TrackCrossing, TrackFmt};
use crate::playback::sparse_buffer::{create_sparse_buffer, SharedSparseBuffer};
use crate::playback::{PlayingTrack, TrackStream};
use crate::pressing::PhysicalMedium;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc as tokio_mpsc;
use tokio::sync::oneshot;
use tracing::{debug, error, info, trace, warn};

mod advance;
mod api;
mod file_buffers;
mod library_follow;
mod output;
mod pipeline;
mod preview;
mod queue_commands;
mod renderer;
mod seek;
mod side_countdown;
mod slot;
mod starvation;
mod state;
mod volume;

use crate::playback::stream_pipeline::{
    cancel_and_join_decoder, log_stream_diagnostic, report_dropped_audio_events, spawn_decoder,
    DecodeFailureReport, DecoderSetup, SegmentDecodeParams, StreamDecodeParams,
};
pub(crate) use api::{dispatch_command, PlaybackCommand};
pub use api::{
    PlaybackHandle, PlaybackPauseBoundary, PlaybackPauseReason, PlaybackSideCountdown,
    PlaybackSidePausePrompt, PlaybackState, PlaybackTrackInfo, PlaybackTrackSide,
};
use api::{SideBoundary, SidePauseDecision};
use file_buffers::{prepare_track_for_playback, FileBuffers};
use library_follow::{LibraryChange, ServiceFollows};
use renderer::{RemoteConnect, Renderer};
use slot::{LoadGeneration, PausePhase, PlayIntent, PlayTarget, PlaybackSlot, TrackPhase};
use starvation::StarvationEpisode;
use volume::OutputVolume;

#[cfg(test)]
mod tests;

mod runtime;

struct TrackDecoder {
    handle: std::thread::JoinHandle<()>,
    cancel_token: Arc<std::sync::atomic::AtomicBool>,
}

struct CurrentTrack {
    prepared: PlaybackPreparedTrack,
    decoder: TrackDecoder,
    phase: TrackPhase,
}

#[derive(Clone, Copy)]
enum StagedNextOnReplace {
    Discard,
    Preserve,
}

pub(crate) fn log_streaming_decode_failure(context: &str, error: DecodeError) -> Option<String> {
    match error {
        DecodeError::InputCancelled => {
            debug!("{context} stopped after input cancellation");
            None
        }
        // The failed fill already reported this to the command loop
        // (`ReadFailed`), which decides whether playback halts.
        DecodeError::SourceRead(error) => {
            debug!("{context} stopped on a source read failure the fill reported: {error}");
            None
        }
        DecodeError::Decode(message) => {
            error!("{context} failed: {message}");
            Some(message)
        }
    }
}

/// The INDEX 01-to-end duration stored for the track and reported to the UI.
fn track_duration_ms(prepared: &PlaybackPreparedTrack) -> u64 {
    prepared.duration.as_millis() as u64
}

impl PlaybackPreparedTrack {
    /// The audio callback's formatting for this track, with `position_offset`
    /// the in-track time the stream starts at.
    fn track_fmt(&self, position_offset: std::time::Duration) -> TrackFmt {
        TrackFmt {
            track_id: self.track_id.clone(),
            duration_ms: self.duration.as_millis() as u64,
            pregap_ms: self.total_pregap_ms(),
            position_offset,
            replay_gain_linear: self.replay_gain_linear,
        }
    }

    /// Decoder windows for this track starting `offset` samples in.
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
            segments.push(SegmentDecodeParams::new(
                segment.buffer.clone(),
                segment.span,
                remaining_offset,
            ));
            remaining_offset = 0;
        }

        StreamDecodeParams::new(
            segments,
            self.content_type != ContentType::Ape,
            leading_silence_frames,
            0,
        )
    }

    fn total_pregap_ms(&self) -> Option<i64> {
        self.pregap_ms.or(self.generated_pregap_ms)
    }

    fn generated_pregap_samples(&self) -> u64 {
        if let Some(samples) = self.generated_pregap_samples {
            return samples;
        }

        let Some(ms) = self.generated_pregap_ms else {
            return 0;
        };
        if ms < 0 {
            warn!(
                track_id = %self.track_id,
                generated_pregap_ms = ms,
                "Ignoring negative generated pregap duration"
            );
            return 0;
        }

        ((ms as f64 / 1000.0) * self.sample_rate as f64) as u64
    }

    /// Whether this track reads its bytes from the buffer with this id.
    fn reads_buffer(&self, buffer_id: u64) -> bool {
        self.segments
            .iter()
            .any(|segment| segment.buffer.id() == buffer_id)
    }

    /// The distinct release files this track plays from.
    fn file_ids(&self) -> HashSet<&str> {
        self.segments
            .iter()
            .map(|segment| segment.file_id.as_str())
            .collect()
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
    track_id: String,
    segments: Vec<PreparedAudioSegment>,
    /// In Hz.
    sample_rate: u32,
    channels: u32,
    /// Pregap the source audio already contains (a CUE/FLAC track).
    pregap_ms: Option<i64>,
    /// Silent pregap to generate, from a CUE `PREGAP` directive.
    generated_pregap_ms: Option<i64>,
    /// The same generated pregap in exact samples.
    generated_pregap_samples: Option<u64>,
    duration: std::time::Duration,
    /// Picks how a track start seeks: by byte to `start_byte`, except APE, which
    /// seeks by sample.
    content_type: crate::util::content_type::ContentType,
    /// Replay gain the audio callback multiplies into the volume; `1.0` when off
    /// or unmeasured.
    replay_gain_linear: f32,
}

struct PreloadedNext {
    prepared: PlaybackPreparedTrack,
    decoder_handle: std::thread::JoinHandle<()>,
    /// The preload decoder's cancel flag, moved into its `TrackDecoder` when the
    /// preload becomes current so that track owns one token.
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
        self.prepared.track_id.as_str()
    }
}

/// Stop a preloaded decoder's reads: set its cancel token and wake any read
/// blocked on its byte buffers so it sees the token. The caller cancels its
/// output source (`discard_preloaded_source`) and decides whether to release
/// the buffers, since the pipeline may still play from the same files.
fn discard_preloaded_decoder(
    prepared: &PlaybackPreparedTrack,
    cancel_token: &Arc<std::sync::atomic::AtomicBool>,
) {
    cancel_token.store(true, std::sync::atomic::Ordering::Release);
    for segment in &prepared.segments {
        segment.buffer.wake_readers();
    }
}

/// Assemble a `PlaybackPreparedTrack` from the resolved audio and its
/// segments' buffers.
fn finalize_playback_track(
    track_id: String,
    resolved: ResolvedTrackAudio,
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
        track_id,
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

struct OutputStream {
    _stream: Box<dyn AudioStream>,
    source: Arc<Mutex<source::PlaybackSource>>,
    audio_events: AudioEventReceiver,
    sample_rate: u32,
    channels: u32,
}

pub struct PlaybackService {
    library_manager: LibraryManager,
    command_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
    command_rx: tokio_mpsc::UnboundedReceiver<PlaybackCommand>,
    progress_tx: tokio_mpsc::UnboundedSender<PlaybackProgress>,
    /// The queue and the projection stream the UIs read, as one owner: every
    /// mutation goes through `apply`, which republishes.
    playback_queue: PublishedQueue,
    current_position_shared: Arc<std::sync::Mutex<Option<std::time::Duration>>>,
    /// The device both players open their outputs from: `audio_output` at
    /// startup, the preview's on its first play. Held so a preview uses the
    /// device the service started with (in tests, one with no hardware).
    audio_device: Box<dyn AudioOutputDevice>,
    /// The main player's output. AirPlay swaps this for the receiver sink and
    /// puts the local one back when it ends.
    audio_output: Box<dyn AudioOutput>,
    /// The output stream, kept across tracks of the same format. Rebuilt when
    /// the format or the default device changes; dropped when local playback is
    /// torn down.
    output: Option<OutputStream>,
    /// The one authority for the current track and its phase, owning its
    /// decoder; `sync_audio_state` writes the `AudioState` atomic from it.
    slot: PlaybackSlot,
    /// Numbers each decoder load so a `TrackReady` from an abandoned load can be
    /// told from the live one.
    load_generation_counter: u64,
    /// Preloaded next track state, either staged into the current gapless source
    /// or held for a stream rebuild.
    preloaded_next: Option<PreloadedNext>,
    /// The output level and mute, kept in core so no UI keeps its own and unmute
    /// restores the level the user last set.
    volume: OutputVolume,
    /// A second player for auditioning a local file. It keeps its own state,
    /// including whether it paused the main player.
    preview: PreviewPlayer,
    /// How often (ms) the audio callback sends position updates to the UI.
    position_update_interval_ms: u32,
    /// The byte buffers tracks stream from and the fetch priority between them.
    file_buffers: FileBuffers,
    /// The starvation watchdog's episode while the current track is starved;
    /// `None` while audio flows.
    starvation_episode: Option<StarvationEpisode>,
    /// When `persist_playback_state` last ran, so the per-tick save in
    /// `handle_position_event` waits a second after any save.
    last_position_persist: Option<std::time::Instant>,
    /// The time-to-first-audio measurement for a load headed for Playing,
    /// recorded when it gets there; `None` once recorded or for a paused load.
    first_audio_pending: Option<FirstAudioMeasurement>,
    /// Where the current track plays: locally, on a remote renderer, or through
    /// an AirPlay receiver.
    renderer: Renderer,
    /// The time source for the side-pause countdown's deadline and the wait for
    /// it.
    clock: crate::playback::PlaybackClockRef,
    /// The library reads the service keeps pointed at what it plays.
    follows: ServiceFollows,
}

/// A pending first-audio timing: the load whose arrival at Playing it measures,
/// the track it plays, and when the play began.
struct FirstAudioMeasurement {
    generation: LoadGeneration,
    track_id: String,
    started_at: std::time::Instant,
}

/// The side or disc boundary between `current` and `next`, or `None` when they
/// play on the same side (or aren't on one release's sides at all).
fn side_boundary_between(
    current: &PlaybackTrackInfo,
    next: &PlaybackTrackInfo,
) -> Option<SideBoundary> {
    if current.release_id != next.release_id {
        return None;
    }
    let current_side = current.side.as_ref()?;
    let next_side = next.side.as_ref()?;
    if current_side.number == next_side.number {
        return None;
    }
    let (kind, side_label) = match current_side.medium {
        PhysicalMedium::Record | PhysicalMedium::Cassette => (
            PlaybackPauseBoundary::Side,
            crate::util::format::side_letter(current_side.number),
        ),
        PhysicalMedium::Cd => (PlaybackPauseBoundary::Disc, current_side.number.to_string()),
    };
    Some(SideBoundary {
        id: format!(
            "{}:{}:{:?}",
            next.track_id, current_side.number, current_side.medium
        ),
        kind,
        side_label,
    })
}

/// The system's audio output device: the platform sink (cpal on desktop,
/// AAudio on Android), opened afresh for each output, plus on macOS the watch
/// that sends `OutputDeviceChanged` when the default device changes. Built on
/// the service thread, as is every output opened from it, so thread-bound
/// device handles live there.
pub(crate) struct SystemAudioOutputDevice {
    /// Held for its `Drop`, which unregisters the CoreAudio listener.
    #[cfg(target_os = "macos")]
    _default_device_watch: crate::playback::cpal_output::device_listener::DefaultDeviceListener,
}

impl SystemAudioOutputDevice {
    /// Open the system device, registering the macOS default-device watch that
    /// dispatches `OutputDeviceChanged` through `command_tx`.
    pub(crate) fn open(
        command_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
    ) -> Result<Self, crate::playback::audio_output::AudioError> {
        #[cfg(not(target_os = "macos"))]
        let _ = command_tx;
        Ok(Self {
            #[cfg(target_os = "macos")]
            _default_device_watch: crate::playback::cpal_output::watch_default_output_device(
                move || dispatch_command(&command_tx, PlaybackCommand::OutputDeviceChanged),
            )?,
        })
    }
}

impl AudioOutputDevice for SystemAudioOutputDevice {
    #[cfg(not(target_os = "android"))]
    fn open_output(
        &self,
    ) -> Result<Box<dyn AudioOutput>, crate::playback::audio_output::AudioError> {
        Ok(Box::new(
            crate::playback::cpal_output::CpalAudioOutput::new()?,
        ))
    }

    #[cfg(target_os = "android")]
    fn open_output(
        &self,
    ) -> Result<Box<dyn AudioOutput>, crate::playback::audio_output::AudioError> {
        Ok(Box::new(
            crate::playback::aaudio_output::AAudioOutput::new()?
        ))
    }
}

/// Open the caller's device, or the system's, and the main player's output from
/// it. `None` means there is no output, so the service thread returns.
fn open_audio_device_and_output(
    custom_device: Option<Box<dyn AudioOutputDevice>>,
    command_tx: &tokio_mpsc::UnboundedSender<PlaybackCommand>,
) -> Option<(Box<dyn AudioOutputDevice>, Box<dyn AudioOutput>)> {
    let audio_device: Box<dyn AudioOutputDevice> = match custom_device {
        Some(device) => device,
        None => match SystemAudioOutputDevice::open(command_tx.clone()) {
            Ok(device) => Box::new(device),
            Err(e) => {
                error!("Failed to open the system audio device: {:?}", e);
                return None;
            }
        },
    };
    match audio_device.open_output() {
        Ok(output) => Some((audio_device, output)),
        Err(e) => {
            error!("Failed to initialize audio output: {:?}", e);
            None
        }
    }
}

/// Map a command to its telemetry kind, or `None` for one not recorded:
/// internal commands, queries, volume, mute, previews, renderer switches, and
/// track-level queue additions and clears.
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
        PlaybackCommand::CancelSidePauseCountdown => {
            Some(PlaybackCommandKind::CancelSidePauseCountdown)
        }
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
        | PlaybackCommand::ReadFailed { .. }
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
        | PlaybackCommand::Shutdown(_)
        | PlaybackCommand::SaveState(_)
        | PlaybackCommand::PlayOn(_)
        | PlaybackCommand::PlayOnAirPlay(_)
        | PlaybackCommand::StopRemote
        | PlaybackCommand::RemoteStatus(_) => None,
        #[cfg(target_os = "macos")]
        PlaybackCommand::OutputDeviceChanged => None,
        #[cfg(any(test, feature = "test-utils"))]
        PlaybackCommand::GetQueueProjection(_) => None,
    }
}

/// Whether a command stops a running side-pause countdown before it is handled.
/// Any command a person steers playback with does, so the next side never then
/// starts on its own, including ones that keep the pause (a seek within the
/// ended side, a queue edit, a preview, a renderer switch). Resume doesn't: it
/// starts the next side, as the countdown would have.
fn cancels_side_pause_countdown(command: &PlaybackCommand) -> bool {
    match command {
        PlaybackCommand::Play(_)
        | PlaybackCommand::PlayRelease { .. }
        | PlaybackCommand::PlayReleases(_)
        | PlaybackCommand::PlayLibraryShuffled
        | PlaybackCommand::Pause
        | PlaybackCommand::Stop
        | PlaybackCommand::Next
        | PlaybackCommand::Previous
        | PlaybackCommand::Seek(_)
        | PlaybackCommand::SeekByRatio(_)
        | PlaybackCommand::AddToQueue(_)
        | PlaybackCommand::AddNext(_)
        | PlaybackCommand::AddReleaseToQueue(_)
        | PlaybackCommand::AddReleaseNext(_)
        | PlaybackCommand::InsertInQueue(_, _)
        | PlaybackCommand::RemoveFromQueue(_)
        | PlaybackCommand::ReorderQueue { .. }
        | PlaybackCommand::ClearUpNext
        | PlaybackCommand::ClearPlayingFrom
        | PlaybackCommand::SetShuffle(_)
        | PlaybackCommand::SkipTo(_)
        | PlaybackCommand::PreviewPlay(_)
        | PlaybackCommand::PlayOn(_)
        | PlaybackCommand::PlayOnAirPlay(_)
        | PlaybackCommand::StopRemote => true,
        PlaybackCommand::Resume
        | PlaybackCommand::CancelSidePauseCountdown
        | PlaybackCommand::AutoAdvance { .. }
        | PlaybackCommand::TrackReady { .. }
        | PlaybackCommand::HaltOnError
        | PlaybackCommand::ReadFailed { .. }
        | PlaybackCommand::SetVolume(_)
        | PlaybackCommand::SetMuted(_)
        | PlaybackCommand::SetRepeatMode(_)
        | PlaybackCommand::ReevaluateSidePauseStaging
        | PlaybackCommand::PreviewStop
        | PlaybackCommand::PreviewTogglePause
        | PlaybackCommand::PreviewSeekByRatio(_)
        | PlaybackCommand::PreviewCompleted
        | PlaybackCommand::GetVolume(_)
        | PlaybackCommand::Shutdown(_)
        | PlaybackCommand::SaveState(_)
        | PlaybackCommand::RemoteStatus(_) => false,
        #[cfg(target_os = "macos")]
        PlaybackCommand::OutputDeviceChanged => false,
        #[cfg(any(test, feature = "test-utils"))]
        PlaybackCommand::GetQueueProjection(_) => false,
    }
}

fn pregap_seek_position(pregap_ms: Option<i64>) -> Option<std::time::Duration> {
    pregap_ms
        .filter(|&p| p > 0)
        .map(|p| std::time::Duration::from_millis(p as u64))
}
