//! # Playback Service
//!
//! Runs on its own thread and drives playback from a command channel. The
//! thread (`thread.rs`) starts a fresh service for the next command after one
//! ends.
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
use super::{NextEntry, PlaybackQueue, PreviousAction, QueueEntryId};
use crate::audio_output::{
    AudioEvent, AudioEventReceiver, AudioOutput, AudioOutputControls, AudioOutputDevice,
    AudioStream, PlayerOutput,
};
use crate::error::PlaybackError;
use crate::preview_player::PreviewPlayer;
use crate::progress::emit_progress;
use crate::progress::{PlaybackProgress, PlaybackProgressHandle};
use crate::tables::{ContextFill, ContextFilled, LeftOut, PlaybackResumeRow, ReleaseStart};
use bae_audio::DecodeError;
use bae_diagnostics::{
    AnomalyKind, Diagnostics, LocalId, PlaybackCommandKind, PlaybackOperation, PlaybackStartSource,
    ReportedThread, TelemetryEvent, TrackTransition,
};
use bae_records::DbAudioSegmentRole;
use bae_records::ResolvedTrackAudio;
use bae_track_audio::data_source::FetchArbiter;
// Imported by path so the sample feed reads `source::PlaybackSource`, apart
// from the queue's `ContextSource`.
use crate::source;
use crate::source::{TrackCrossing, TrackFmt};
use crate::timeline::{StreamPosition, TrackTimeline};
use crate::track_buffered::{PlayingBytes, TrackBytes};
use crate::PlayingTrack;
use bae_audio::sparse_buffer::{create_sparse_buffer, SharedSparseBuffer};
use bae_audio::track_stream::TrackStream;
use bae_types::pressing::PhysicalMedium;
use bae_types::track_time::TrackTime;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc as tokio_mpsc;
use tokio::sync::oneshot;
use tracing::{debug, error, info, trace, warn};

mod advance;
mod api;
mod audio_focus;
mod commands;
mod completion;
mod deadline_wait;
use completion::AudibleCompletion;
mod file_buffers;
mod history;
mod library_follow;
mod output;
mod pipeline;
mod preview;
mod queue_commands;
mod renderer;
mod seek;
mod segment_start;
mod sides;
mod slot;
mod starvation;
mod state;
mod thread;
mod volume;

use crate::stream_pipeline::{
    cancel_and_join_decoder, log_stream_diagnostic, report_dropped_audio_events, spawn_decoder,
    DecodeFailureReport, DecoderSetup,
};
use api::PlayReply;
pub(crate) use api::PlaybackCommand;
pub use api::{
    LoadingTrack, PlayError, PlayOutcome, PlaybackHandle, PlaybackPauseReason, PlaybackState,
    QueueEditError,
};
use bae_track_audio::{SegmentDecodeParams, StreamDecodeParams};
use file_buffers::{prepare_track_for_playback, FileBuffers};
use library_follow::LibraryFollow;
use renderer::{RemoteConnect, RemoteEnd, Renderer};
use segment_start::segment_start;
use sides::{side_boundary_between, SideBoundary, SidePauseDecision};
pub use sides::{
    PlaybackPauseBoundary, PlaybackSideCountdown, PlaybackSidePausePrompt, PlaybackTrackInfo,
    PlaybackTrackSide,
};
use slot::{LoadGeneration, PausePhase, PlayIntent, PlayTarget, PlaybackSlot, TrackPhase};
use starvation::StarvationEpisode;
pub(crate) use thread::{dispatch_command, OwnCommands, PlaybackMessage};
use volume::OutputVolume;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;

mod runtime;

struct TrackDecoder {
    handle: ReportedThread,
    cancel_token: Arc<std::sync::atomic::AtomicBool>,
}

struct CurrentTrack {
    prepared: PlaybackPreparedTrack,
    decoder: TrackDecoder,
    phase: TrackPhase,
    /// Where the track's stream has played to: where its load started, then
    /// each position tick (a remote device's reported position, when playing
    /// on one).
    position: StreamPosition,
}

#[derive(Clone, Copy)]
enum StagedNextOnReplace {
    Discard,
    Preserve,
}

/// Log why a streaming decode stopped, and say what the person is told: a
/// failure of the decode's own, or `None` when there's nothing to tell them
/// here. A read that panicked is a bug, reported through `diagnostics`.
pub(crate) fn log_streaming_decode_failure(
    diagnostics: &Diagnostics,
    context: &str,
    error: DecodeError,
) -> Option<String> {
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
        DecodeError::ReadPanicked(bug) => Some(diagnostics.panicked(bug).to_string()),
        DecodeError::Decode(message) => {
            error!("{context} failed: {message}");
            Some(message)
        }
    }
}

impl PlaybackPreparedTrack {
    /// The audio callback's formatting for this track, decoded from
    /// `starts_at` in its stream.
    fn track_fmt(&self, starts_at: StreamPosition) -> TrackFmt {
        TrackFmt {
            track_id: self.track_id.clone(),
            timeline: self.timeline,
            starts_at,
            replay_gain_linear: self.replay_gain_linear,
        }
    }

    /// Decoder windows for this track starting `offset` samples in.
    /// The track's decode from `start`: from its position in the stream,
    /// the pregap included or not as `start` says.
    fn decode_from(&self, start: TrackStart) -> StreamDecodeParams {
        let include_pregap = start.includes_pregap();
        let offset =
            if include_pregap { start.position(self.timeline).frames(self.sample_rate) } else { 0 };
        self.decode_params(offset, include_pregap)
    }

    fn decode_params(&self, offset: u64, include_pregap: bool) -> StreamDecodeParams {
        stream_decode(
            &self.segments,
            &self.content_type,
            self.generated_pregap_frames,
            offset,
            include_pregap,
        )
    }

    /// Whether this track reads its bytes from the buffer with this id.
    fn reads_buffer(&self, buffer_id: u64) -> bool {
        self.segments.iter().any(|segment| segment.buffer.id() == buffer_id)
    }

    /// The distinct release files this track plays from.
    fn file_ids(&self) -> HashSet<&str> {
        self.segments.iter().map(|segment| segment.file_id.as_str()).collect()
    }

    /// The bytes that play this track from INDEX 01 to its end, its main
    /// segment's, and the buffer holding them. `None` for a track whose
    /// length is unknown, which no stretch is a fraction of, and for one with
    /// no bytes here (a remote renderer plays it).
    fn playing_bytes(&self) -> Option<PlayingBytes> {
        let duration = self.timeline.duration()?;
        let main = self.segments.iter().find(|segment| segment.role == DbAudioSegmentRole::Main)?;
        let samples = match main.span.end_sample {
            Some(end) => end.saturating_sub(main.span.start_sample),
            None => bae_util::sample_time::frame_at(duration, self.sample_rate),
        };
        Some(PlayingBytes::new(
            self.track_id.clone(),
            TrackBytes::new(
                main.span,
                main.buffer.get_total_size(),
                main.scrub_points.as_slice(),
                samples,
            ),
            main.buffer.subscribe_held(),
        ))
    }
}

/// `resolved`'s stream decoded once from its first sample, as playback decodes
/// the track from its start: the silence a CUE `PREGAP` generates, then each
/// segment, pregap included, starting where playback starts it (an MP3 from
/// the stream's start or a stored scrub point). Each segment reads through
/// the buffer `buffer_of` gives it.
pub fn track_stream_decode(
    resolved: &ResolvedTrackAudio,
    mut buffer_of: impl FnMut(&bae_records::ResolvedTrackAudioSegment) -> SharedSparseBuffer,
) -> StreamDecodeParams {
    let segments: Vec<PreparedAudioSegment> = resolved
        .segments
        .iter()
        .map(|segment| PreparedAudioSegment::of(segment, buffer_of(segment)))
        .collect();
    stream_decode(&segments, &resolved.content_type, resolved.generated_pregap_frames(), 0, true)
}

/// Decoder windows for a track's stream from `offset` samples in, over its
/// `segments` of a `content_type` file, after `generated_pregap_frames` of
/// silence a CUE `PREGAP` generates. Without `include_pregap` the silence and
/// the stored pregap segment are left out, and `offset` counts from INDEX 01.
fn stream_decode(
    segments: &[PreparedAudioSegment],
    content_type: &bae_util::content_type::ContentType,
    generated_pregap_frames: u64,
    offset: u64,
    include_pregap: bool,
) -> StreamDecodeParams {
    let mut remaining_offset = offset;
    let leading_silence_frames =
        if include_pregap { generated_pregap_frames.saturating_sub(offset) } else { 0 };
    if include_pregap {
        remaining_offset = remaining_offset.saturating_sub(generated_pregap_frames);
    }

    let mut windows = Vec::new();
    for segment in segments {
        if !include_pregap && segment.role == DbAudioSegmentRole::AudioPregap {
            continue;
        }
        let segment_len =
            segment.span.end_sample.map(|end| end.saturating_sub(segment.span.start_sample));
        if let Some(len) = segment_len {
            if remaining_offset >= len {
                remaining_offset -= len;
                continue;
            }
        }
        windows.push(segment.decode_params(remaining_offset, content_type));
        remaining_offset = 0;
    }

    StreamDecodeParams::new(
        windows,
        bae_audio::byte_jump_keeps_place(content_type),
        leading_silence_frames,
        0,
    )
}

#[derive(Clone)]
struct PreparedAudioSegment {
    role: DbAudioSegmentRole,
    file_id: String,
    buffer: SharedSparseBuffer,
    /// Where this segment sits inside its backing file, in samples and bytes.
    span: bae_records::SegmentSpan,
    /// The frames a decode inside this segment starts at.
    scrub_points: bae_records::ScrubPoints,
}

impl PreparedAudioSegment {
    /// `segment`, its bytes read through `buffer`.
    fn of(segment: &bae_records::ResolvedTrackAudioSegment, buffer: SharedSparseBuffer) -> Self {
        Self {
            role: segment.role.clone(),
            file_id: segment.file_id.clone(),
            buffer,
            span: segment.span,
            scrub_points: segment.scrub_points.clone(),
        }
    }

    /// This segment's decoder window, starting `offset` samples in, in a file
    /// of `content_type`, from where [`segment_start`] says.
    fn decode_params(
        &self,
        offset: u64,
        content_type: &bae_util::content_type::ContentType,
    ) -> SegmentDecodeParams {
        let start = segment_start(
            content_type,
            &self.scrub_points,
            self.span.start_sample,
            offset,
            &self.file_id,
        );
        SegmentDecodeParams::with_start(self.buffer.clone(), self.span, offset, start)
    }
}

#[derive(Clone)]
struct PlaybackPreparedTrack {
    track_id: String,
    segments: Vec<PreparedAudioSegment>,
    /// In Hz.
    sample_rate: u32,
    channels: u32,
    /// The silent pregap a CUE `PREGAP` directive generates, in frames, which
    /// a natural start decodes before the first stored sample.
    generated_pregap_frames: u64,
    /// Where the track starts in its stream, past its pregap (stored audio or
    /// generated silence), and how long it runs from there when known.
    timeline: TrackTimeline,
    /// Picks how a decode starts: a track start by byte to `start_byte` where
    /// the codec's frames keep their place (`bae_audio::byte_jump_keeps_place`),
    /// else by sample, and an MP3 always from a stored scrub point or the
    /// stream's start.
    content_type: bae_util::content_type::ContentType,
    /// Replay gain the audio callback multiplies into the volume; `1.0` when off
    /// or unmeasured.
    replay_gain_linear: f32,
}

struct PreloadedNext {
    prepared: PlaybackPreparedTrack,
    decoder_handle: ReportedThread,
    /// The preload decoder's cancel flag, moved into its `TrackDecoder` when the
    /// preload becomes current so that track owns one token.
    cancel_token: Arc<std::sync::atomic::AtomicBool>,
    source: PreloadedNextSource,
}

enum PreloadedNextSource {
    Held(TrackStream),
    Staged,
}

/// Where a load starts in the track's stream.
#[derive(Debug, Clone, Copy)]
pub(super) enum TrackStart {
    /// A direct selection: at the track's start, INDEX 01, past the pregap.
    Direct,
    /// A natural transition: at the stream's start, pregap included.
    Natural,
    Position(StreamPosition),
}

impl TrackStart {
    fn from_natural_transition(is_natural_transition: bool) -> Self {
        if is_natural_transition {
            Self::Natural
        } else {
            Self::Direct
        }
    }

    fn position(self, timeline: TrackTimeline) -> StreamPosition {
        match self {
            Self::Natural => StreamPosition::START,
            Self::Direct => timeline.track_start(),
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
    resolved: &ResolvedTrackAudio,
    segments: Vec<PreparedAudioSegment>,
    replay_gain_mode: bae_preferences::ReplayGainMode,
) -> PlaybackPreparedTrack {
    let duration = Some(std::time::Duration::from_millis(resolved.duration_ms));

    let replay_gain_linear = crate::replay_gain_linear(resolved, replay_gain_mode);

    PlaybackPreparedTrack {
        track_id,
        segments,
        sample_rate: resolved.sample_rate.get(),
        channels: resolved.channels.get(),
        generated_pregap_frames: resolved.generated_pregap_frames(),
        timeline: TrackTimeline::new(duration, resolved.pregap()),
        content_type: resolved.content_type.clone(),
        replay_gain_linear,
    }
}

struct OutputStream {
    stream: Box<dyn AudioStream>,
    source: Arc<Mutex<source::PlaybackSource>>,
    audio_events: AudioEventReceiver,
    sample_rate: u32,
    channels: u32,
}

pub struct PlaybackService {
    /// The library's store: the tracks playback reads, the resume row and
    /// listening history it writes, the preferences it plays by, and the
    /// files it streams.
    store: bae_store::Store,
    /// The library's records: the tracks and their audio.
    records: bae_records::Records,
    /// The plays the listening history records.
    people: bae_people::People,
    /// Mints the id of each play the listening history records.
    ids: coven::IdSourceRef,
    /// Where the service, its decoders, its fills and its watchers report a
    /// panic as the bug it is.
    diagnostics: Diagnostics,
    command_tx: OwnCommands,
    progress_tx: tokio_mpsc::UnboundedSender<PlaybackProgress>,
    /// The queue: its tables, which the queue screens read, and the entry
    /// playing.
    playback_queue: PlaybackQueue,
    /// Whether the system lets bae make sound; asked before every start and
    /// released when playback stops.
    audio_focus: crate::AudioFocus,
    /// The device both players open their outputs from: `audio_output` at
    /// startup, the preview's on its first play. Held so a preview uses the
    /// device the service started with (in tests, one with no hardware).
    audio_device: Arc<dyn AudioOutputDevice>,
    /// The main player's output. AirPlay swaps this for the receiver sink and
    /// puts the local one back when it ends.
    audio_output: PlayerOutput,
    /// The output stream, kept across tracks of the same format. Rebuilt when
    /// the format changes or the device stops playing it; dropped when local
    /// playback is torn down.
    output: Option<OutputStream>,
    /// The one authority for the current track and its phase, owning its
    /// decoder; `sync_audio_state` writes the `AudioState` atomic from it.
    slot: PlaybackSlot,
    listening: Option<history::Listening>,
    buffered_listening: Vec<history::BufferedListening>,
    audible_completion: Option<AudibleCompletion>,
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
    /// The position `persist_playback_state` last saved, so the per-tick save
    /// in `handle_position_event` waits until the track has played a second
    /// past it; `None` when the row holds no position.
    persisted_position: Option<StreamPosition>,
    /// The time-to-first-audio measurement for a load headed for Playing,
    /// recorded when it gets there; `None` once recorded or for a paused load.
    first_audio_pending: Option<FirstAudioMeasurement>,
    /// Where the current track plays: locally, on a remote renderer, or through
    /// an AirPlay receiver.
    renderer: Renderer,
    /// The time source for the side-pause countdown's deadline and the wait for
    /// it.
    clock: crate::PlaybackClockRef,
    /// The sides of the staged crossing's tracks, whose crossing is taken back
    /// when an edit puts a side or disc boundary between them.
    staged_sides: LibraryFollow<Vec<String>, Vec<PlaybackTrackInfo>>,
    /// The tracks playback holds, as the library still holds them: one it
    /// no longer holds was deleted, by this device or by another one whose
    /// deletion arrived by sync, and playback moves off it.
    held_tracks: LibraryFollow<Vec<String>, Vec<PlaybackTrackInfo>>,
}

/// A pending first-audio timing: the load whose arrival at Playing it measures,
/// the track it plays, and when the play began.
struct FirstAudioMeasurement {
    generation: LoadGeneration,
    track_id: String,
    started_at: std::time::Instant,
}

/// The system's audio output device: the platform sink (cpal on desktop and
/// iOS, AAudio on Android), opened afresh for each output. Every output is
/// opened from it on the service thread, so thread-bound device handles live
/// there.
pub(crate) struct SystemAudioOutputDevice {
    /// Told when a stream on an output this device opened reports it can't
    /// go on.
    #[cfg_attr(target_os = "android", expect(dead_code, reason = "AAudio reopens its own stream"))]
    report: crate::audio_output::InvalidationReport,
    /// The formats the outputs' streams set Mac output devices to, shared by
    /// the main player's and the preview's, so a device goes back to the
    /// format it had before bae only once neither plays on it.
    #[cfg(target_os = "macos")]
    device_formats:
        Arc<crate::device_formats::DeviceFormats<crate::device_formats::CoreAudioDeviceFormats>>,
}

impl SystemAudioOutputDevice {
    pub(crate) fn new(report: crate::audio_output::InvalidationReport) -> Self {
        Self {
            report,
            #[cfg(target_os = "macos")]
            device_formats: crate::device_formats::DeviceFormats::new(
                crate::device_formats::CoreAudioDeviceFormats,
            ),
        }
    }
}

impl AudioOutputDevice for SystemAudioOutputDevice {
    #[cfg(not(target_os = "android"))]
    fn open_output(&self) -> Result<Box<dyn AudioOutput>, crate::audio_output::AudioError> {
        Ok(Box::new(crate::cpal_output::CpalAudioOutput::new(
            self.report.clone(),
            #[cfg(target_os = "macos")]
            self.device_formats.clone(),
        )?))
    }

    #[cfg(target_os = "android")]
    fn open_output(&self) -> Result<Box<dyn AudioOutput>, crate::audio_output::AudioError> {
        Ok(Box::new(crate::aaudio_output::AAudioOutput))
    }
}

/// Map a command to its telemetry kind, or `None` for one not recorded:
/// internal commands, queries, volume, mute, previews, renderer switches, and
/// track-level queue additions and clears.
fn playback_command_kind(command: &PlaybackCommand) -> Option<PlaybackCommandKind> {
    match command {
        PlaybackCommand::Play(_) => Some(PlaybackCommandKind::Play),
        PlaybackCommand::PlayRelease { .. } => Some(PlaybackCommandKind::PlayRelease),
        PlaybackCommand::PlayReleases { .. } => Some(PlaybackCommandKind::PlayReleases),
        PlaybackCommand::PlayTracks { .. } => Some(PlaybackCommandKind::PlayTracks),
        PlaybackCommand::PlayLibraryShuffled(_) => Some(PlaybackCommandKind::PlayLibraryShuffled),
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
        PlaybackCommand::RemoveFromQueue { .. } => Some(PlaybackCommandKind::RemoveFromQueue),
        PlaybackCommand::ReorderQueue { .. } => Some(PlaybackCommandKind::ReorderQueue),
        PlaybackCommand::SkipTo { .. } => Some(PlaybackCommandKind::SkipTo),
        PlaybackCommand::AutoAdvance { .. }
        | PlaybackCommand::TrackReady { .. }
        | PlaybackCommand::HaltOnError
        | PlaybackCommand::ReadFailed { .. }
        | PlaybackCommand::AddToQueue(_)
        | PlaybackCommand::AddNext(_)
        | PlaybackCommand::InsertInQueue(_, _)
        | PlaybackCommand::ClearUpNext(_)
        | PlaybackCommand::ClearPlayingFrom(_)
        | PlaybackCommand::ReevaluateSidePauseStaging
        | PlaybackCommand::SetVolume(_)
        | PlaybackCommand::SetMuted(_)
        | PlaybackCommand::PreviewPlay(_)
        | PlaybackCommand::PreviewStop
        | PlaybackCommand::PreviewTogglePause
        | PlaybackCommand::PreviewSeekByRatio(_)
        | PlaybackCommand::PreviewCompleted
        | PlaybackCommand::Shutdown(_)
        | PlaybackCommand::SaveState(_)
        | PlaybackCommand::PlayOn(_)
        | PlaybackCommand::PlayOnAirPlay(_)
        | PlaybackCommand::StopRemote
        | PlaybackCommand::AirPlayStreamFailed
        | PlaybackCommand::OutputInvalidated
        | PlaybackCommand::RemoteEvent(_) => None,
        #[cfg(any(test, feature = "test-utils"))]
        PlaybackCommand::GetQueue(_)
        | PlaybackCommand::TakeFills(_)
        | PlaybackCommand::Handled(_) => None,
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
        | PlaybackCommand::PlayReleases { .. }
        | PlaybackCommand::PlayTracks { .. }
        | PlaybackCommand::PlayLibraryShuffled(_)
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
        | PlaybackCommand::RemoveFromQueue { .. }
        | PlaybackCommand::ReorderQueue { .. }
        | PlaybackCommand::ClearUpNext(_)
        | PlaybackCommand::ClearPlayingFrom(_)
        | PlaybackCommand::SetShuffle(_)
        | PlaybackCommand::SkipTo { .. }
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
        | PlaybackCommand::Shutdown(_)
        | PlaybackCommand::SaveState(_)
        | PlaybackCommand::AirPlayStreamFailed
        | PlaybackCommand::OutputInvalidated
        | PlaybackCommand::RemoteEvent(_) => false,
        #[cfg(any(test, feature = "test-utils"))]
        PlaybackCommand::GetQueue(_)
        | PlaybackCommand::TakeFills(_)
        | PlaybackCommand::Handled(_) => false,
    }
}
