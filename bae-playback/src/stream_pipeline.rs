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

use crate::audio_output::{
    audio_event_channel, AudioEvent, AudioEventReceiver, AudioStream, PlayerOutput,
};
use crate::error::PlaybackError;
use crate::service::{dispatch_command, log_streaming_decode_failure, PlaybackCommand};
use crate::source::{PlaybackSource, TrackFmt};
use bae_audio::sparse_buffer::SharedSparseBuffer;
use bae_audio::track_stream::{create_track_stream_pair, ReadyReceiver, TrackStream};
use bae_diagnostics::{Diagnostics, ReportedThread};
use bae_track_audio::StreamDecodeParams;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tracing::{debug, error, warn};

/// One live decoded-audio pipeline: a decoder thread filling a ring, wrapped in
/// a `PlaybackSource`, attached to an output stream. Built and torn down per
/// track by the preview player.
pub(crate) struct StreamPipeline {
    stream: Box<dyn AudioStream>,
    source: Arc<Mutex<PlaybackSource>>,
    decoder_handle: ReportedThread,
    /// This decoder's AVIO cancel flag — owned here, not by the prepared track.
    cancel_token: Arc<AtomicBool>,
}

/// What a decode failure does beyond the shared log: the main player's service
/// is told, and surfaces it to the UI as a `PlaybackError`; preview only logs.
#[derive(Clone)]
pub(crate) enum DecodeFailureReport {
    /// Sent as the service's own command, so a decoder that outlived its
    /// service tells no later one.
    TellService {
        commands: crate::service::OwnCommands,
    },
    LogOnly,
}

impl DecodeFailureReport {
    /// The decode failed for the reason `message` states. Teardown (seek /
    /// stop / track change) sets `cancel` before the decoder exits, so a
    /// failure after it is no failure of the track's.
    pub(crate) fn decode_failed(&self, cancel: &AtomicBool, message: impl std::fmt::Display) {
        if let Self::TellService { commands } = self {
            if !cancel.load(Ordering::Relaxed) {
                dispatch_command(
                    commands,
                    PlaybackCommand::DecodeFailed { message: message.to_string() },
                );
            }
        }
    }
}

/// One decoder thread to start: the window to decode, the PCM format the ring
/// it fills is built at, the tag its diagnostics carry, and what a genuine
/// failure does beyond the shared log.
pub(crate) struct DecoderSetup {
    decode: StreamDecodeParams,
    sample_rate: u32,
    channels: u32,
    log_context: &'static str,
    on_decode_failure: DecodeFailureReport,
}

impl DecoderSetup {
    pub(crate) fn new(
        decode: StreamDecodeParams,
        sample_rate: u32,
        channels: u32,
        log_context: &'static str,
        on_decode_failure: DecodeFailureReport,
    ) -> Self {
        Self { decode, sample_rate, channels, log_context, on_decode_failure }
    }
}

/// Spawn the streaming decoder `setup` describes: mint its AVIO cancel token,
/// create the sink/stream pair, and run `run_decoder` on a thread. A genuine
/// decode failure surfaces via the setup's `on_decode_failure` (the main player
/// emits a `PlaybackError`; preview only logs); normal teardown (seek / stop /
/// track change cancels the token first) stays silent. A decoder that panics is
/// reported through `diagnostics` and ends its track the way a failed decode
/// does. Builds no output stream — that is the caller's next step.
pub(crate) fn spawn_decoder<T, F>(
    diagnostics: &Diagnostics,
    setup: DecoderSetup,
    compose: F,
) -> Result<T, PlaybackError>
where
    F: FnOnce(TrackStream, ReportedThread, Arc<AtomicBool>, ReadyReceiver) -> T,
{
    let DecoderSetup { decode, sample_rate, channels, log_context, on_decode_failure } = setup;
    let cancel_token = Arc::new(AtomicBool::new(false));
    let (mut sink, track_stream, ready) = create_track_stream_pair(sample_rate, channels);

    let handle = {
        let decoder_cancel = cancel_token.clone();
        // Kept to tell a genuine decode failure from normal teardown (seek /
        // stop / track change all cancel the token before the decoder exits).
        let teardown_check = cancel_token.clone();
        let panic_teardown_check = cancel_token.clone();
        let decode_failure = on_decode_failure.clone();
        let decoder_diagnostics = diagnostics.clone();
        diagnostics.spawn_thread_reported(
            "playback decoder",
            move || {
                if let Err(e) = decode.run_decoder(&mut sink, decoder_cancel) {
                    if let Some(message) =
                        log_streaming_decode_failure(&decoder_diagnostics, log_context, e)
                    {
                        decode_failure.decode_failed(&teardown_check, message);
                    }
                }
            },
            move |failed| on_decode_failure.decode_failed(&panic_teardown_check, failed),
        )
    }
    .map_err(|error| PlaybackError::task(format!("starting the decoder thread failed: {error}")))?;

    Ok(compose(track_stream, handle, cancel_token, ready))
}

/// Spawn the decoder `setup` describes, build the audio stream on
/// `audio_output`, start it, and return the assembled pipeline plus its
/// event/ready channels. On any stream-build failure the spawned decoder is
/// cancelled and its handle dropped before returning `Err`, so no half-built
/// pipeline escapes. Cancelling the source's byte buffers (stopping the data
/// reader) stays with the buffer's owner — this cancels only the decoder it
/// spawned. Used by the preview player.
pub(crate) async fn start_stream_pipeline<T, F>(
    diagnostics: &Diagnostics,
    audio_output: &mut PlayerOutput,
    setup: DecoderSetup,
    fmt: TrackFmt,
    position_update_interval_ms: u32,
    compose: F,
) -> Result<T, PlaybackError>
where
    F: FnOnce(StreamPipeline, AudioEventReceiver) -> T,
{
    let (track_stream, handle, cancel_token) = spawn_decoder(
        diagnostics,
        setup,
        // Preview has no Loading state; the ready signal goes unused.
        |track_stream, handle, cancel_token, _ready| (track_stream, handle, cancel_token),
    )?;

    let (stream, source, audio_events) = match build_and_play_stream(
        diagnostics,
        audio_output,
        track_stream,
        fmt,
        position_update_interval_ms,
    )
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
        StreamPipeline { stream, source, decoder_handle: handle, cancel_token },
        audio_events,
    ))
}

/// Wrap `track_source` in a `PlaybackSource`, build the output stream over it,
/// and start it. The stream-construction path `start_stream_pipeline` runs
/// through (the main player builds its persistent stream in `service::output`
/// instead).
async fn build_and_play_stream(
    diagnostics: &Diagnostics,
    audio_output: &mut PlayerOutput,
    track_source: TrackStream,
    fmt: TrackFmt,
    position_update_interval_ms: u32,
) -> Result<(Box<dyn AudioStream>, Arc<Mutex<PlaybackSource>>, AudioEventReceiver), PlaybackError> {
    // The audio callback tags every position/completion emit with `fmt`.
    let source = Arc::new(Mutex::new(PlaybackSource::new(track_source, fmt)));

    let (source_sample_rate, source_channels) = {
        let guard = source.lock().unwrap();
        (guard.sample_rate(), guard.channels())
    };

    let (audio_event_tx, audio_events) = match audio_event_channel(diagnostics) {
        Ok(channel) => channel,
        Err(error) => {
            source.lock().unwrap().cancel();
            return Err(PlaybackError::task(format!(
                "starting the output's event relay failed: {error}"
            )));
        }
    };

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
        return Err(PlaybackError::task(format!("Failed to start streaming playback: {e:?}")));
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
    pub(crate) async fn shutdown_for_seek(
        self,
        diagnostics: &Diagnostics,
        buffers: &[SharedSparseBuffer],
    ) {
        // Drop the stream first so the audio callback stops pulling.
        drop(self.stream);

        match self.source.lock() {
            Ok(guard) => guard.cancel(),
            Err(_) => warn!("playback source lock poisoned during seek teardown; skipping cancel"),
        }

        cancel_and_join_decoder(diagnostics, &self.cancel_token, buffers, self.decoder_handle)
            .await;
    }

    /// The output stream's report, if its device made one, that the stream
    /// can't go on as built ([`AudioStream::take_invalidation`]).
    pub(crate) fn take_invalidation(&self) -> Option<String> {
        self.stream.take_invalidation()
    }

    /// How far the output stream has played: the position of the last sample
    /// it pulled from the source.
    pub(crate) fn position(&self) -> crate::StreamPosition {
        self.source.lock().unwrap().position()
    }

    /// Build a pipeline over a caller-supplied `source` and `stream` with stub
    /// parts (an already-exited decoder, a fresh token) so slot-shaped tests
    /// only specify the values they vary.
    #[cfg(test)]
    pub(crate) fn new_for_test(
        source: Arc<Mutex<PlaybackSource>>,
        stream: Box<dyn AudioStream>,
    ) -> Self {
        StreamPipeline {
            stream,
            source,
            decoder_handle: Diagnostics::noop()
                .spawn_thread_reported("finished decoder", || {}, |_| {})
                .expect("spawn a decoder thread that has already finished"),
            cancel_token: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// Cancel a decoder and wait for its thread to exit: set its AVIO cancel
/// token, wake the given byte buffers so a read-blocked decoder observes the
/// token, and join the thread off the async runtime. The buffers are left
/// uncancelled (the caller retains them across the seek). The decoder's sink
/// must already be cancelled by the caller (`PlaybackSource::replace` /
/// `cancel`), so a decoder parked writing a full ring also unparks. A decoder
/// that panicked was reported on its own thread.
pub(crate) async fn cancel_and_join_decoder(
    diagnostics: &Diagnostics,
    cancel_token: &AtomicBool,
    buffers: &[SharedSparseBuffer],
    handle: ReportedThread,
) {
    cancel_token.store(true, Ordering::Release);
    for buffer in buffers {
        buffer.wake_readers();
    }
    // The join itself can't panic, so this fails only when the runtime is
    // shutting down under it.
    if let Err(failed) = diagnostics.blocking("join a decoder thread", move || handle.join()).await
    {
        warn!("{failed}");
    }
}

/// Log the diagnostics both players log identically for Starved / StarvationEnded
/// / SourceLockMissed / SourceLockReacquired. Position / Completion / TrackCrossing
/// / OutputFailed are the caller's to handle and are ignored here — both callers
/// match those arms before falling through to this.
pub(crate) fn log_stream_diagnostic(context: &'static str, event: &AudioEvent) {
    match event {
        AudioEvent::SourceLockMissed { missed_ms } => {
            warn!(
                context,
                missed_ms, "audio callback could not lock playback source while playing"
            );
        }
        AudioEvent::SourceLockReacquired { missed_ms } => {
            debug!(context, missed_ms, "audio callback reacquired playback source lock");
        }
        AudioEvent::Starved {
            fmt,
            starved_ms,
            position,
            producer_finished,
            samples_decoded,
            decode_errors,
            has_next,
        } => {
            warn!(
                context,
                track_id = %fmt.track_id,
                starved_ms,
                stream_position_ms = position.as_millis(),
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
            position,
            samples_decoded,
            decode_errors,
        } => {
            debug!(
                context,
                track_id = %fmt.track_id,
                starved_ms,
                stream_position_ms = position.as_millis(),
                samples_decoded,
                decode_errors,
                "playback source resumed after decoded sample starvation"
            );
        }
        AudioEvent::Position(_)
        | AudioEvent::Completion(_)
        | AudioEvent::TrackCrossing(_)
        | AudioEvent::OutputFailed { .. } => {}
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
        error!(context, dropped_required, "audio callback event queue dropped required events");
        return true;
    }
    let dropped = events.take_dropped_count();
    if dropped > 0 {
        warn!(context, dropped, "audio callback event queue dropped events");
    }
    false
}

#[cfg(test)]
#[path = "stream_pipeline_tests.rs"]
mod tests;
