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
use crate::playback::audio_output::{AudioEvent, AudioEventReceiver, AudioOutput, AudioStream};
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
    DecodeFailureReport, SegmentDecodeParams, StreamDecodeParams,
};
use api::SidePauseDecision;
pub(crate) use api::{dispatch_command, PlaybackCommand};
pub use api::{
    LoadingTrack, PlaybackHandle, PlaybackPauseReason, PlaybackSidePausePrompt, PlaybackState,
    PlaybackTrackInfo, PlaybackTrackSide, SIDE_PAUSE_CASSETTE_MESSAGE_KEY, SIDE_PAUSE_TITLE_KEY,
    SIDE_PAUSE_VINYL_MESSAGE_KEY,
};
use renderer::{RemoteConnect, Renderer};
use slot::{LoadGeneration, PausePhase, PlayIntent, PlayTarget, PlaybackSlot, TrackPhase};
use starvation::StarvationEpisode;

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

/// The playback shape of a fill-error handler: a failed byte fill reports itself
/// to the command loop, naming the buffer it failed on. The loop is the only
/// place that knows whether that buffer feeds the current track or a preloaded
/// next, which is what decides whether the failure halts playback. (The fill
/// itself cancels the buffer right after, unblocking the decoder.)
fn playback_fill_error_handler(
    command_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
    buffer_id: u64,
) -> crate::playback::data_source::FillErrorHandler {
    Box::new(move |error| {
        dispatch_command(
            &command_tx,
            PlaybackCommand::ReadFailed { buffer_id, error },
        );
    })
}

async fn prepare_track_for_playback(
    library_manager: &LibraryManager,
    track_id: &str,
    shared_file_buffers: &mut HashMap<String, SharedSparseBuffer>,
    command_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
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
                playback_fill_error_handler(command_tx.clone(), buffer.id()),
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
    queue_values: tokio::sync::watch::Sender<PlaybackQueueProjection>,
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
    /// Tracks removed from the live slot whose buffers remain available until
    /// the successor is prepared and reveals which files it reuses.
    retired_tracks: Vec<PlaybackPreparedTrack>,
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
    }
}

fn pregap_seek_position(pregap_ms: Option<i64>) -> Option<std::time::Duration> {
    pregap_ms
        .filter(|&p| p > 0)
        .map(|p| std::time::Duration::from_millis(p as u64))
}
