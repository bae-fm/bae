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
//! ## File buffers
//!
//! The byte buffers tracks stream their audio from are owned by `FileBuffers`
//! (`file_buffers.rs`), which also holds the tracks awaiting buffer release and
//! the fetch-priority arbiter shared into every reader.

use super::RepeatMode;
use super::{
    repeat_to_str, source_to_str, ContextSource, ContextStart, NextEntry, PersistedPlayback,
    PreviousAction, PublishedQueue, QueueEntryId, QueueSnapshot,
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
use crate::playback::audio_output::{
    AudioEvent, AudioEventReceiver, AudioOutput, AudioOutputDevice, AudioStream,
};
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
mod api;
mod file_buffers;
mod output;
mod pipeline;
mod preview;
mod queue_commands;
mod renderer;
mod seek;
mod slot;
mod starvation;
mod state;
mod volume;

use crate::playback::stream_pipeline::{
    cancel_and_join_decoder, log_stream_diagnostic, report_dropped_audio_events, spawn_decoder,
    DecodeFailureReport, SegmentDecodeParams, StreamDecodeParams,
};
use api::SidePauseDecision;
pub(crate) use api::{dispatch_command, PlaybackCommand};
pub use api::{
    LoadingTrack, PlaybackHandle, PlaybackPauseReason, PlaybackSidePausePrompt, PlaybackState,
    PlaybackTrackInfo, PlaybackTrackSide, SIDE_PAUSE_CASSETTE_MESSAGE_KEY, SIDE_PAUSE_TITLE_KEY,
    SIDE_PAUSE_VINYL_MESSAGE_KEY,
};
use file_buffers::{prepare_track_for_playback, FileBuffers};
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

/// The INDEX 01-to-end duration stored for the track and reported to the UI.
fn track_duration_ms(prepared: &PlaybackPreparedTrack) -> u64 {
    prepared.duration.as_millis() as u64
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
    cancel_token: &Arc<std::sync::atomic::AtomicBool>,
) {
    cancel_token.store(true, std::sync::atomic::Ordering::Release);
    for segment in &prepared.segments {
        segment.buffer.wake_readers();
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
    /// Where both players' outputs come from: `audio_output` below was opened
    /// from it at startup, and the preview player opens its own second output
    /// from it on its first play. Held so a preview follows whatever device the
    /// service was started with — in a test, one that touches no hardware.
    audio_device: Box<dyn AudioOutputDevice>,
    /// The main player's output. AirPlay swaps this for the receiver sink and
    /// puts the local one back when it ends.
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
    /// The output level and mute for `audio_output`, as one owner: mute is core
    /// state so no UI has to keep its own, and unmute restores the level the
    /// user last set.
    volume: OutputVolume,
    /// The preview player — a self-contained second player for auditioning a
    /// local file. The service only coordinates pause/resume of the main player
    /// around it; the preview's own state, including whether it paused the main
    /// player, lives entirely in `PreviewPlayer`.
    preview: PreviewPlayer,
    /// How often (ms) the audio callback sends position updates to the UI.
    position_update_interval_ms: u32,
    /// The byte buffers tracks stream their audio from, the tracks whose buffers
    /// are awaiting release, and the fetch priority between them.
    file_buffers: FileBuffers,
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

/// The system's audio output device: the platform sink (cpal on desktop, AAudio
/// on Android) opened afresh for each output the service needs, plus — on macOS
/// — the CoreAudio watch that dispatches `OutputDeviceChanged` when the system
/// default output device changes, so the persistent stream rebuilds onto it.
///
/// Constructed on the service's dedicated thread, and every output opened from
/// it likewise, so a sink owns any thread-bound device handle it opens there
/// (cpal builds lazily per stream; AAudio binds its writer thread).
pub(crate) struct SystemAudioOutputDevice {
    /// Held for its `Drop`, which unregisters the CoreAudio property listener
    /// when the service ends.
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

/// Open the device the service plays through, and the main player's output from
/// it: the caller's device when one was supplied, otherwise the system's (which
/// also registers the macOS default-device watch). `None` means the service
/// cannot run — it has no output to play through — and its thread returns.
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
    // The main player's output. The preview player opens its own second output
    // from the same device on its first play.
    match audio_device.open_output() {
        Ok(output) => Some((audio_device, output)),
        Err(e) => {
            error!("Failed to initialize audio output: {:?}", e);
            None
        }
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

fn pregap_seek_position(pregap_ms: Option<i64>) -> Option<std::time::Duration> {
    pregap_ms
        .filter(|&p| p > 0)
        .map(|p| std::time::Duration::from_millis(p as u64))
}
