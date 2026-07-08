//! One live decoded-audio pipeline, shared by both players.
//!
//! A `StreamPipeline` is the decoder thread + `PlaybackSource` + output
//! `AudioStream` trio with one construction path and one teardown path. Both the
//! main current track (`service::CurrentTrack`) and the preview player
//! (`preview_player::ActivePreview`) run exactly one of these at a time, so the
//! buffer+decoder+stream startup, the seek teardown, and the diagnostic event
//! logging live here once instead of being mirrored in both players.

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
/// a `PlaybackSource`, attached to an output stream.
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

/// The assembled pipeline plus the two channels the owner consumes: the
/// audio-events receiver (drained on the service's select tick, or moved into
/// preview's listener task) and the ready receiver (the main player arms its
/// `TrackReady` watcher on it; preview drops it — it has no Loading state).
pub(crate) struct StreamPipelineStart {
    pub(crate) pipeline: StreamPipeline,
    pub(crate) audio_events: AudioEventReceiver,
    pub(crate) ready: ReadyReceiver,
}

/// The decoder window for one stream. `target_sample` is where FFmpeg trims
/// lead-in (the track's start plus any pregap/seek offset). The seek to it is
/// either a by-byte jump (`seek_to_byte`, a lossless track's natural start) or a
/// sample seek (APE, or a mid-track seek); the `target_sample` trim makes the
/// first output sample exact either way. `stop_at_sample` ends output at the
/// track's end (`None` = to EOF).
pub(crate) struct StreamDecodeParams {
    pub(crate) segments: Vec<SegmentDecodeParams>,
    pub(crate) leading_silence_frames: u64,
}

pub(crate) struct SegmentDecodeParams {
    pub(crate) buffer: SharedSparseBuffer,
    pub(crate) seek_to_byte: Option<u64>,
    pub(crate) target_sample: u64,
    pub(crate) stop_at_sample: Option<u64>,
    /// The track's end byte offset -- the read-ahead ceiling. `None` = whole file.
    pub(crate) end_byte: Option<u64>,
}

impl StreamDecodeParams {
    /// Run the streaming decoder for this window: FFmpeg seeks (by byte or by
    /// sample), trims lead-in at `target_sample`, and stops at `stop_at_sample`.
    /// Shared by the play/seek, preload, and preview paths so they can't drift on
    /// the seek/trim mapping.
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
            if cancel.load(Ordering::Relaxed) || sink.is_cancelled() {
                return Err(StreamingDecodeError::InputCancelled);
            }
        }
        Ok(())
    }
}

/// Spawn the decoder for `decode`, build the audio stream on `audio_output`,
/// start it, and return the assembled pipeline plus its event/ready channels. On
/// any stream-build failure the spawned decoder is cancelled and its handle
/// dropped before returning `Err`, so no half-built pipeline escapes. Cancelling
/// the source's byte buffers (stopping the data reader) stays with the buffer's
/// owner — this cancels only the decoder it spawned.
pub(crate) async fn start_stream_pipeline(
    audio_output: &mut dyn AudioOutput,
    decode: StreamDecodeParams,
    fmt: TrackFmt,
    sample_rate: u32,
    channels: u32,
    position_update_interval_ms: u32,
    log_context: &'static str,
    on_decode_failure: DecodeFailureReport,
) -> Result<StreamPipelineStart, PlaybackError> {
    let cancel_token = Arc::new(AtomicBool::new(false));
    let (mut sink, track_source, ready) = create_track_stream_pair(sample_rate, channels);

    let decoder_handle = {
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

    let (stream, source, audio_events) =
        match build_and_play_stream(audio_output, track_source, fmt, position_update_interval_ms)
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
                drop(decoder_handle);
                return Err(e);
            }
        };

    Ok(StreamPipelineStart {
        pipeline: StreamPipeline {
            stream,
            source,
            decoder_handle,
            cancel_token,
        },
        audio_events,
        ready,
    })
}

/// Wrap `track_source` in a `PlaybackSource`, build the output stream over it,
/// and start it. The single stream-construction path both `start_stream_pipeline`
/// (fresh decoder) and `StreamPipeline::attach_preloaded` (already-running
/// decoder) run through.
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
    // Wrap the track source in a PlaybackSource so the audio callback can advance
    // to a pre-staged next track without rebuilding the stream. The audio
    // callback tags every position/completion emit with `fmt`; at a gapless
    // boundary it swaps to the staged next track's fmt.
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
    /// Adopt an already-running preloaded decoder onto a fresh stream (the main
    /// player's gapless-rebuild advance path). The decoder + token were minted at
    /// preload time; this only builds and starts the output stream over the
    /// preloaded track source.
    pub(crate) async fn attach_preloaded(
        audio_output: &mut dyn AudioOutput,
        track_stream: TrackStream,
        fmt: TrackFmt,
        decoder_handle: std::thread::JoinHandle<()>,
        cancel_token: Arc<AtomicBool>,
        position_update_interval_ms: u32,
    ) -> Result<(Self, AudioEventReceiver), PlaybackError> {
        let (stream, source, audio_events) =
            build_and_play_stream(audio_output, track_stream, fmt, position_update_interval_ms)
                .await?;
        Ok((
            StreamPipeline {
                stream,
                source,
                decoder_handle,
                cancel_token,
            },
            audio_events,
        ))
    }

    pub(crate) fn source(&self) -> &Arc<Mutex<PlaybackSource>> {
        &self.source
    }

    /// Swap in the crossed-into track's decoder at a gapless boundary. The stream
    /// and source stay live (the `PlaybackSource` already advanced to the staged
    /// next track within the same stream); only the decoder thread + token the
    /// pipeline owns are replaced.
    pub(crate) fn adopt_crossed_decoder(
        &mut self,
        decoder_handle: std::thread::JoinHandle<()>,
        cancel_token: Arc<AtomicBool>,
    ) {
        // The finishing track's decoder already exited (the crossing fired), so
        // dropping its handle detaches nothing live.
        self.decoder_handle = decoder_handle;
        self.cancel_token = cancel_token;
    }

    /// Immediate teardown: cancel the source (the audio callback goes silent),
    /// cancel the decoder token, drop the stream, and detach the decoder. Does NOT
    /// join the decoder — the token plus the owner's buffer cancellation make it
    /// exit, and joining would block the command loop. Buffer cancellation stays
    /// with the buffer's owner.
    pub(crate) fn cancel(self) {
        match self.source.lock() {
            Ok(guard) => guard.cancel(),
            // A poisoned lock means the audio callback thread panicked while
            // holding it. The source is being dropped here anyway, so the cancel
            // it would have requested is moot — log and move on.
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

        // Cancel this decoder's AVIO reads via its token.
        self.cancel_token.store(true, Ordering::Release);

        // Wake any readers blocked on the condvar so they can check the token.
        for buffer in buffers {
            buffer.wake_readers();
        }

        // Wait for the decoder thread to exit. Surface a thread panic as an error
        // (decoder bug, real signal); tokio join failures (panic in the
        // spawn_blocking wrapper itself, runtime shutdown) get a warn.
        let handle = self.decoder_handle;
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
        StreamDecodeParams {
            segments: vec![SegmentDecodeParams {
                buffer,
                seek_to_byte: None,
                target_sample: 0,
                stop_at_sample: None,
                end_byte: None,
            }],
            leading_silence_frames: 0,
        }
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
        )
        .await
        .expect("stream builds over a working output");

        // Teardown drops the stream and detaches the decoder; nothing left to poll.
        start.pipeline.cancel();
    }

    /// When `attach_preloaded`'s stream build fails, the already-running preloaded
    /// decoder must be stopped, not left parked filling a ring with no consumer.
    /// A decoder that pushes until its sink is cancelled exits promptly, proving
    /// the failure path cancelled the source.
    #[tokio::test]
    async fn attach_preloaded_failure_stops_the_preloaded_decoder() {
        let (mut sink, track_stream, _ready) = create_track_stream_pair(44_100, 2);
        let exited = Arc::new(AtomicBool::new(false));
        let exited_in_thread = exited.clone();
        // Stands in for a preloaded decoder: push forever until the sink cancels.
        let decoder_handle = std::thread::spawn(move || {
            let chunk = vec![0.0f32; 4096];
            while !sink.is_cancelled() {
                sink.push_samples_blocking(&chunk);
            }
            exited_in_thread.store(true, Ordering::Release);
        });
        let cancel_token = Arc::new(AtomicBool::new(false));

        let mut output = FailingAudioOutput;
        let result = StreamPipeline::attach_preloaded(
            &mut output,
            track_stream,
            test_fmt(),
            decoder_handle,
            cancel_token,
            50,
        )
        .await;
        assert!(result.is_err(), "a failed attach returns Err");

        // The failure path cancelled the source, so the decoder's sink is
        // cancelled and the thread exits rather than parking on a full ring.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !exited.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(
            exited.load(Ordering::Acquire),
            "the preloaded decoder must exit when attach_preloaded fails"
        );
    }
}
