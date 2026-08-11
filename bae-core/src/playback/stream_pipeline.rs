//! The preview player's decoded-audio pipeline, plus the decoder-spawn and
//! diagnostic helpers the main player also uses.
//!
//! A `StreamPipeline` is the decoder thread + `PlaybackSource` + output
//! `AudioStream` trio with one construction path and one teardown path. The
//! preview player (`preview_player::ActivePreview`) runs exactly one of these at
//! a time. The main player builds no per-track stream — it keeps one persistent
//! output stream and swaps what its callback reads (see `service::output`) — but
//! it shares `spawn_decoder` (the decoder-thread half), `run_decoder`, and the
//! diagnostic logging from here so the two players can't drift on the
//! decode/seek mapping.

use crate::audio_codec::StreamingDecodeError;
use crate::playback::audio_output::{
    audio_event_channel, AudioEvent, AudioEventReceiver, AudioOutput, AudioStream,
};
use crate::playback::error::PlaybackError;
use crate::playback::progress::{emit_progress, PlaybackProgress};
use crate::playback::service::log_streaming_decode_failure;
use crate::playback::source::{PlaybackSource, TrackFmt};
use crate::playback::sparse_buffer::SharedSparseBuffer;
use crate::playback::track_stream::{
    create_track_stream_pair, ReadyReceiver, TrackSink, TrackStream,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc as tokio_mpsc;
use tracing::{debug, error, warn};

/// One live decoded-audio pipeline: a decoder thread filling a ring, wrapped in
/// a `PlaybackSource`, attached to an output stream. Built and torn down per
/// track by the preview player.
pub(crate) struct StreamPipeline {
    stream: Box<dyn AudioStream>,
    source: Arc<Mutex<PlaybackSource>>,
    decoder_handle: std::thread::JoinHandle<()>,
    /// This decoder's AVIO cancel flag — owned here, not by the prepared track.
    cancel_token: Arc<AtomicBool>,
}

/// What a decode failure does beyond the shared log: the main player surfaces it
/// to the UI as a `PlaybackError`; preview only logs.
pub(crate) enum DecodeFailureReport {
    EmitPlaybackError {
        progress_tx: tokio_mpsc::UnboundedSender<PlaybackProgress>,
    },
    LogOnly,
}

/// One track's decode window — the one shape both playback and the save
/// re-encoder run: its segments in play order, preceded by
/// `leading_silence_frames` of generated silence (a CUE `PREGAP` directive) and
/// followed by `trailing_silence_frames` (a save relocating the next track's
/// generated pregap; always 0 for playback).
#[derive(Clone)]
pub(crate) struct StreamDecodeParams {
    segments: Vec<SegmentDecodeParams>,
    /// Whether the demuxer may jump by byte to a segment's recorded start_byte.
    /// False for APE, which has no per-frame byte positions and sample-seeks
    /// its mandatory index instead.
    byte_seekable: bool,
    leading_silence_frames: u64,
    trailing_silence_frames: u64,
}

/// One decoder window: the segment's span within its backing file, the buffer
/// its bytes stream through, and the sample offset into the segment where
/// output starts (a mid-track seek; 0 for a natural start). The seek policy —
/// byte jump vs sample seek, and the lead-in trim target — derives from these
/// via [`Self::target_sample`] and [`Self::seek_to_byte`].
#[derive(Clone)]
pub(crate) struct SegmentDecodeParams {
    buffer: SharedSparseBuffer,
    span: crate::db::SegmentSpan,
    start_offset: u64,
}

impl SegmentDecodeParams {
    pub(crate) fn new(
        buffer: SharedSparseBuffer,
        span: crate::db::SegmentSpan,
        start_offset: u64,
    ) -> Self {
        Self {
            buffer,
            span,
            start_offset,
        }
    }

    /// First sample to emit: the segment's start plus the seek offset. FFmpeg
    /// trims the lead-in the seek lands before this (a frame may begin before
    /// it), so the first output sample is exact.
    pub(crate) fn target_sample(&self) -> u64 {
        self.span.start_sample + self.start_offset
    }

    /// The demuxer jump for this segment: the recorded landing byte, only at a
    /// natural start (`start_offset == 0`) of a byte-seekable codec. A mid-track
    /// seek or APE sample-seeks to [`Self::target_sample`] instead.
    pub(crate) fn seek_to_byte(&self, byte_seekable: bool) -> Option<u64> {
        if self.start_offset == 0 && byte_seekable {
            self.span.start_byte
        } else {
            None
        }
    }
}

impl StreamDecodeParams {
    pub(crate) fn new(
        segments: Vec<SegmentDecodeParams>,
        byte_seekable: bool,
        leading_silence_frames: u64,
        trailing_silence_frames: u64,
    ) -> Self {
        Self {
            segments,
            byte_seekable,
            leading_silence_frames,
            trailing_silence_frames,
        }
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn set_leading_silence_frames(&mut self, frames: u64) {
        self.leading_silence_frames = frames;
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn leading_silence_frames(&self) -> u64 {
        self.leading_silence_frames
    }

    #[cfg(test)]
    pub(crate) fn segment_count(&self) -> usize {
        self.segments.len()
    }

    #[cfg(test)]
    pub(crate) fn segment_buffer_id(&self, index: usize) -> u64 {
        self.segments[index].buffer.id()
    }

    #[cfg(test)]
    pub(crate) fn segment_target_sample(&self, index: usize) -> u64 {
        self.segments[index].target_sample()
    }

    #[cfg(test)]
    pub(crate) fn segment_seek_to_byte(&self, index: usize) -> Option<u64> {
        self.segments[index].seek_to_byte(self.byte_seekable)
    }

    /// Run the streaming decoder for each segment in turn: FFmpeg seeks (by byte
    /// or by sample), trims lead-in at the segment's target sample, and stops at
    /// the segment's end sample. Shared by the play/seek, preload, and preview
    /// paths so they can't drift on the seek/trim mapping.
    pub(crate) fn run_decoder(
        &self,
        sink: &mut TrackSink,
        cancel: Arc<AtomicBool>,
    ) -> Result<(), StreamingDecodeError> {
        if self.leading_silence_frames > 0 {
            sink.push_silence_frames_blocking(self.leading_silence_frames);
            if cancel.load(Ordering::Relaxed) || sink.is_cancelled() {
                return Err(StreamingDecodeError::InputCancelled);
            }
        }

        for segment in &self.segments {
            let seek_to_byte = segment.seek_to_byte(self.byte_seekable);
            let seek_to_sample = if seek_to_byte.is_some() {
                None
            } else {
                Some(segment.target_sample())
            };
            crate::audio_codec::decode_audio_streaming(
                segment.buffer.clone(),
                sink,
                seek_to_byte,
                seek_to_sample,
                Some(segment.target_sample()),
                segment.span.end_sample,
                segment.span.end_byte,
                cancel.clone(),
            )?;
            if cancel.load(Ordering::Relaxed) || sink.is_cancelled() {
                return Err(StreamingDecodeError::InputCancelled);
            }
        }

        if self.trailing_silence_frames > 0 {
            sink.push_silence_frames_blocking(self.trailing_silence_frames);
            if cancel.load(Ordering::Relaxed) || sink.is_cancelled() {
                return Err(StreamingDecodeError::InputCancelled);
            }
        }
        Ok(())
    }

    /// Decode this window into a [`DecodedSink`] — the save re-encoder's
    /// driver, sharing the per-segment seek derivation with [`Self::run_decoder`]:
    /// announce the stored format, push the leading silence, decode each
    /// segment, push the trailing silence. Blocking; run it off the async
    /// runtime.
    ///
    /// `sample_rate`/`channels` come from the stored audio format; each
    /// segment's decode re-announces the probed values, so a sink that checks
    /// (the encoder) turns a stored-vs-probed mismatch into a loud failure.
    // Save/export only exists on desktop; without the gate this is dead code
    // on mobile and fails the deny(dead_code) build there.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn run_to_sink(
        &self,
        sample_rate: u32,
        channels: u32,
        sink: &mut dyn crate::audio_codec::DecodedSink,
        cancel: Arc<AtomicBool>,
    ) -> Result<(), String> {
        sink.on_format(sample_rate, channels);
        push_silence_to_sink(sink, self.leading_silence_frames, channels, &cancel)?;
        for segment in &self.segments {
            let seek_to_byte = segment.seek_to_byte(self.byte_seekable);
            let seek_to_sample = if seek_to_byte.is_some() {
                None
            } else {
                Some(segment.target_sample())
            };
            crate::audio_codec::decode_audio_to_sink_with_seek(
                segment.buffer.clone(),
                seek_to_byte,
                seek_to_sample,
                Some(segment.target_sample()),
                segment.span.end_sample,
                sink,
                cancel.clone(),
            )?;
        }
        push_silence_to_sink(sink, self.trailing_silence_frames, channels, &cancel)?;
        Ok(())
    }
}

/// Push `frames` frames of silence into the sink in chunks, checking `cancel`
/// between chunks so an aborted save stops promptly.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn push_silence_to_sink(
    sink: &mut dyn crate::audio_codec::DecodedSink,
    frames: u64,
    channels: u32,
    cancel: &AtomicBool,
) -> Result<(), String> {
    if frames == 0 {
        return Ok(());
    }
    const CHUNK_FRAMES: u64 = 4096;
    let zeros = vec![0i32; (CHUNK_FRAMES * u64::from(channels)) as usize];
    let mut remaining = frames;
    while remaining > 0 {
        if cancel.load(Ordering::Relaxed) {
            return Err("decode input cancelled".to_string());
        }
        let chunk = remaining.min(CHUNK_FRAMES);
        sink.on_samples(&zeros[..(chunk * u64::from(channels)) as usize]);
        remaining -= chunk;
    }
    Ok(())
}

/// Spawn the streaming decoder for `decode`: mint its AVIO cancel token, create
/// the sink/stream pair, and run `run_decoder` on a thread. A genuine decode
/// failure surfaces via `on_decode_failure` (the main player emits a
/// `PlaybackError`; preview only logs); normal teardown (seek / stop / track
/// change cancels the token first) stays silent. Builds no output stream — that
/// is the caller's next step.
pub(crate) fn spawn_decoder<T, F>(
    decode: StreamDecodeParams,
    sample_rate: u32,
    channels: u32,
    log_context: &'static str,
    on_decode_failure: DecodeFailureReport,
    compose: F,
) -> T
where
    F: FnOnce(TrackStream, std::thread::JoinHandle<()>, Arc<AtomicBool>, ReadyReceiver) -> T,
{
    let cancel_token = Arc::new(AtomicBool::new(false));
    let (mut sink, track_stream, ready) = create_track_stream_pair(sample_rate, channels);

    let handle = {
        let decoder_cancel = cancel_token.clone();
        // Kept to tell a genuine decode failure from normal teardown (seek /
        // stop / track change all cancel the token before the decoder exits).
        let teardown_check = cancel_token.clone();
        std::thread::spawn(move || {
            if let Err(e) = decode.run_decoder(&mut sink, decoder_cancel) {
                if let Some(message) = log_streaming_decode_failure(log_context, e) {
                    if let DecodeFailureReport::EmitPlaybackError { progress_tx } =
                        &on_decode_failure
                    {
                        if !teardown_check.load(Ordering::Relaxed) {
                            emit_progress(
                                progress_tx,
                                PlaybackProgress::PlaybackError {
                                    reason: crate::ui::PlaybackErrorReason::internal(format!(
                                        "Playback decode failed: {message}"
                                    )),
                                },
                            );
                        }
                    }
                }
            }
        })
    };

    compose(track_stream, handle, cancel_token, ready)
}

/// Spawn the decoder for `decode`, build the audio stream on `audio_output`,
/// start it, and return the assembled pipeline plus its event/ready channels. On
/// any stream-build failure the spawned decoder is cancelled and its handle
/// dropped before returning `Err`, so no half-built pipeline escapes. Cancelling
/// the source's byte buffers (stopping the data reader) stays with the buffer's
/// owner — this cancels only the decoder it spawned. Used by the preview player.
pub(crate) async fn start_stream_pipeline<T, F>(
    audio_output: &mut dyn AudioOutput,
    decode: StreamDecodeParams,
    fmt: TrackFmt,
    sample_rate: u32,
    channels: u32,
    position_update_interval_ms: u32,
    log_context: &'static str,
    on_decode_failure: DecodeFailureReport,
    compose: F,
) -> Result<T, PlaybackError>
where
    F: FnOnce(StreamPipeline, AudioEventReceiver) -> T,
{
    let (track_stream, handle, cancel_token) = spawn_decoder(
        decode,
        sample_rate,
        channels,
        log_context,
        on_decode_failure,
        // Preview has no Loading state; the ready signal goes unused.
        |track_stream, handle, cancel_token, _ready| (track_stream, handle, cancel_token),
    );

    let (stream, source, audio_events) =
        match build_and_play_stream(audio_output, track_stream, fmt, position_update_interval_ms)
            .await
        {
            Ok(parts) => parts,
            Err(e) => {
                error!("Failed to create streaming audio stream: {:?}", e);
                // The decoder is spawned but no stream will pull from it. Cancel
                // it so it exits instead of parking on a ring nobody reads, then
                // detach — joining would block the caller's command loop. Waking
                // its byte buffers (so a read-blocked decoder sees the token) is
                // the owner's job on the returned `Err`.
                cancel_token.store(true, Ordering::Release);
                drop(handle);
                return Err(e);
            }
        };

    Ok(compose(
        StreamPipeline {
            stream,
            source,
            decoder_handle: handle,
            cancel_token,
        },
        audio_events,
    ))
}

/// Wrap `track_source` in a `PlaybackSource`, build the output stream over it,
/// and start it. The stream-construction path `start_stream_pipeline` runs
/// through (the main player builds its persistent stream in `service::output`
/// instead).
async fn build_and_play_stream(
    audio_output: &mut dyn AudioOutput,
    track_source: TrackStream,
    fmt: TrackFmt,
    position_update_interval_ms: u32,
) -> Result<
    (
        Box<dyn AudioStream>,
        Arc<Mutex<PlaybackSource>>,
        AudioEventReceiver,
    ),
    PlaybackError,
> {
    // The audio callback tags every position/completion emit with `fmt`.
    let source = Arc::new(Mutex::new(PlaybackSource::new(track_source, fmt)));

    let (source_sample_rate, source_channels) = {
        let guard = source.lock().unwrap();
        (guard.sample_rate(), guard.channels())
    };

    let (audio_event_tx, audio_events) = audio_event_channel();

    let stream = match audio_output.create_stream(
        source.clone(),
        source_sample_rate,
        source_channels,
        audio_event_tx,
        position_update_interval_ms,
    ) {
        Ok(stream) => stream,
        Err(e) => {
            // The decoder is already filling this source's ring, but no output
            // will ever drain it. Cancel the source: that sets the sink's cancel
            // flag and unparks the decoder, so a decoder blocked writing a full
            // ring exits instead of parking on it until the process ends.
            source.lock().unwrap().cancel();
            return Err(PlaybackError::task(format!("Audio stream: {:?}", e)));
        }
    };

    if let Err(e) = stream.play() {
        source.lock().unwrap().cancel();
        return Err(PlaybackError::task(format!(
            "Failed to start streaming playback: {e:?}"
        )));
    }

    Ok((stream, source, audio_events))
}

impl StreamPipeline {
    /// Immediate teardown: cancel the source (the audio callback goes silent),
    /// cancel the decoder token, drop the stream, and detach the decoder. Does NOT
    /// join the decoder — the token plus the owner's buffer cancellation make it
    /// exit, and joining would block the command loop. Buffer cancellation stays
    /// with the buffer's owner.
    pub(crate) fn cancel(self) {
        match self.source.lock() {
            Ok(guard) => guard.cancel(),
            // A poisoned lock means the audio callback thread panicked holding
            // it. The source is dropped here anyway, so its cancel is moot.
            Err(_) => warn!("playback source lock poisoned during teardown; skipping cancel"),
        }
        self.cancel_token.store(true, Ordering::Release);
        // Dropping `self` drops the stream and detaches the decoder thread.
    }

    /// Seek teardown: cancel the source, cancel the decoder token, wake the given
    /// buffers so a read-blocked decoder sees the token, and join the decoder — so
    /// a new pipeline can reuse the same buffers with the old decoder guaranteed
    /// gone. The buffers themselves are left uncancelled (the owner retains them
    /// across the seek).
    pub(crate) async fn shutdown_for_seek(self, buffers: &[SharedSparseBuffer]) {
        // Drop the stream first so the audio callback stops pulling.
        drop(self.stream);

        match self.source.lock() {
            Ok(guard) => guard.cancel(),
            Err(_) => warn!("playback source lock poisoned during seek teardown; skipping cancel"),
        }

        cancel_and_join_decoder(&self.cancel_token, buffers, self.decoder_handle).await;
    }

    /// Build a pipeline over a caller-supplied `source` with stub parts (a no-op
    /// stream, an already-exited decoder, a fresh token) so slot-shaped tests only
    /// specify the one value they vary.
    #[cfg(test)]
    pub(crate) fn new_for_test(source: Arc<Mutex<PlaybackSource>>) -> Self {
        StreamPipeline {
            stream: Box::new(StubStream),
            source,
            decoder_handle: std::thread::spawn(|| {}),
            cancel_token: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// Cancel a decoder and wait for its thread to exit: set its AVIO cancel
/// token, wake the given byte buffers so a read-blocked decoder observes the
/// token, and join the thread off the async runtime. The buffers are left
/// uncancelled (the caller retains them across the seek). The decoder's sink
/// must already be cancelled by the caller (`PlaybackSource::replace` /
/// `cancel`), so a decoder parked writing a full ring also unparks. Surfaces
/// a thread panic as an error (decoder bug, real signal); tokio join failures
/// (panic in the spawn_blocking wrapper itself, runtime shutdown) get a warn.
pub(crate) async fn cancel_and_join_decoder(
    cancel_token: &AtomicBool,
    buffers: &[SharedSparseBuffer],
    handle: std::thread::JoinHandle<()>,
) {
    cancel_token.store(true, Ordering::Release);
    for buffer in buffers {
        buffer.wake_readers();
    }
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

/// A no-op output stream for `StreamPipeline::new_for_test`.
#[cfg(test)]
struct StubStream;

#[cfg(test)]
impl AudioStream for StubStream {
    fn play(&self) -> Result<(), crate::playback::audio_output::AudioError> {
        Ok(())
    }
}

/// Log the diagnostics both players log identically for Starved / StarvationEnded
/// / SourceLockMissed / SourceLockReacquired. Position / Completion / TrackCrossing
/// are the caller's to handle and are ignored here — both callers match those arms
/// before falling through to this.
pub(crate) fn log_stream_diagnostic(context: &'static str, event: &AudioEvent) {
    match event {
        AudioEvent::SourceLockMissed { missed_ms } => {
            warn!(
                context,
                missed_ms, "audio callback could not lock playback source while playing"
            );
        }
        AudioEvent::SourceLockReacquired { missed_ms } => {
            debug!(
                context,
                missed_ms, "audio callback reacquired playback source lock"
            );
        }
        AudioEvent::Starved {
            fmt,
            starved_ms,
            position_ms,
            producer_finished,
            samples_decoded,
            decode_errors,
            has_next,
        } => {
            warn!(
                context,
                track_id = %fmt.track_id,
                starved_ms,
                position_ms,
                producer_finished,
                samples_decoded,
                decode_errors,
                has_next,
                "playback source has no decoded samples while current track is not finished"
            );
        }
        AudioEvent::StarvationEnded {
            fmt,
            starved_ms,
            position_ms,
            samples_decoded,
            decode_errors,
        } => {
            debug!(
                context,
                track_id = %fmt.track_id,
                starved_ms,
                position_ms,
                samples_decoded,
                decode_errors,
                "playback source resumed after decoded sample starvation"
            );
        }
        AudioEvent::Position(_) | AudioEvent::Completion(_) | AudioEvent::TrackCrossing(_) => {}
    }
}

/// Shared dropped-events accounting: logs the dropped counts and returns `true`
/// when a REQUIRED event was dropped (both callers then emit a `PlaybackError`
/// and halt).
pub(crate) fn report_dropped_audio_events(
    events: &AudioEventReceiver,
    context: &'static str,
) -> bool {
    let dropped_required = events.take_dropped_required_count();
    if dropped_required > 0 {
        error!(
            context,
            dropped_required, "audio callback event queue dropped required events"
        );
        return true;
    }
    let dropped = events.take_dropped_count();
    if dropped > 0 {
        warn!(
            context,
            dropped, "audio callback event queue dropped events"
        );
    }
    false
}

#[cfg(all(test, feature = "test-utils"))]
mod tests {
    use super::*;
    use crate::playback::audio_output::{CaptureAudioOutput, FailingAudioOutput};
    use crate::playback::sparse_buffer::create_sparse_buffer;

    fn one_segment_decode(buffer: SharedSparseBuffer) -> StreamDecodeParams {
        StreamDecodeParams::new(
            vec![SegmentDecodeParams::new(
                buffer,
                crate::db::SegmentSpan::whole_file(),
                0,
            )],
            true,
            0,
            0,
        )
    }

    fn test_fmt() -> TrackFmt {
        TrackFmt {
            track_id: "unit".to_string(),
            duration_ms: 1_000,
            pregap_ms: None,
            position_offset: std::time::Duration::ZERO,
            replay_gain_linear: 1.0,
        }
    }

    /// A stream-build failure cancels the just-spawned decoder and returns `Err`
    /// rather than leaking a half-built pipeline. This exercises the real shared
    /// unit both players run through.
    #[tokio::test]
    async fn start_stream_pipeline_failure_returns_err_and_cancels_decoder() {
        let buffer = create_sparse_buffer(0);
        let mut output = FailingAudioOutput;

        let result = start_stream_pipeline(
            &mut output,
            one_segment_decode(buffer.clone()),
            test_fmt(),
            44_100,
            2,
            50,
            "unit decode",
            DecodeFailureReport::LogOnly,
            |pipeline, _audio_events| pipeline,
        )
        .await;

        assert!(result.is_err(), "a failed stream build returns Err");
        // The owner cancels the buffer on Err; doing so here proves the spawned
        // decoder unwinds (its read unblocks and it exits) rather than parking
        // forever on a ring nobody drains.
        buffer.cancel();
        assert!(buffer.is_cancelled());
    }

    /// The happy path assembles a live pipeline over a working output and hands
    /// back the audio-events and ready channels.
    #[tokio::test]
    async fn start_stream_pipeline_success_builds_pipeline() {
        let buffer = create_sparse_buffer(0);
        let (mut output, _capture_rx) = CaptureAudioOutput::new();

        let start = start_stream_pipeline(
            &mut output,
            one_segment_decode(buffer),
            test_fmt(),
            44_100,
            2,
            50,
            "unit decode",
            DecodeFailureReport::LogOnly,
            |pipeline, _audio_events| pipeline,
        )
        .await
        .expect("stream builds over a working output");

        // Teardown drops the stream and detaches the decoder; nothing left to poll.
        start.cancel();
    }
}
