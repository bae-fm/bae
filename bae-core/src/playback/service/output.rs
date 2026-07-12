//! The main player's persistent output stream.
//!
//! One output stream lives per `(sample_rate, channels)` for as long as playback
//! continues in that format. Track transitions — a fresh play, a seek, a manual
//! next, a preload promotion — swap what the audio callback reads via
//! `PlaybackSource::replace`; they don't rebuild the device stream. The stream is
//! torn down and rebuilt only on a format change, `stop()`, or a stream error.
//! The in-callback gapless crossing (`PlaybackSource::pull_samples`) swaps in
//! place and never touches this.

use super::*;
use crate::playback::audio_output::{audio_event_channel, AudioStream};

/// The persistent output stream shared across tracks in one format: the device
/// stream, the `PlaybackSource` its callback pulls from, and the audio-events
/// receiver the command loop drains. The `sample_rate`/`channels` are what the
/// stream was built for; a track in a different format forces a rebuild.
pub(super) struct OutputStream {
    /// Held only for its `Drop`, which releases the device / stops the capture
    /// thread. Never read after construction, hence the underscore.
    pub(super) _stream: Box<dyn AudioStream>,
    pub(super) source: Arc<Mutex<source::PlaybackSource>>,
    pub(super) audio_events: AudioEventReceiver,
    pub(super) sample_rate: u32,
    pub(super) channels: u32,
}

impl PlaybackService {
    /// Attach `track_stream` to the output: if a stream for this format is already
    /// live, swap the callback's source in place (`PlaybackSource::replace`) and
    /// notify the sink so a capture buffer rotates; otherwise drop the old stream
    /// (releasing the device / stopping the old capture thread) and build a fresh
    /// one for this format. On a build failure the just-wrapped source is
    /// cancelled and the error returned — the caller owns the decoder's token and
    /// byte buffers.
    pub(super) async fn attach_track(
        &mut self,
        track_stream: TrackStream,
        fmt: TrackFmt,
        sample_rate: u32,
        channels: u32,
    ) -> Result<(), PlaybackError> {
        if let Some(out) = &mut self.output {
            if out.sample_rate == sample_rate && out.channels == channels {
                // Swap the source and flush the receiver under the same lock every
                // audio-event push takes, so the two are atomic: everything queued
                // at that instant belongs to the outgoing track and is dropped (a
                // stale `Completion` would otherwise stamp the incoming track
                // `Completed` and mute it), and everything pushed after belongs to
                // the new one.
                {
                    let mut source = out.source.lock().unwrap();
                    source.replace(track_stream, fmt);
                    while out.audio_events.pop().is_some() {}
                }
                // A same-format swap keeps the one device stream; tell the sink so
                // a capture buffer rotates, preserving "one buffer per non-gapless
                // transition" for the test capture sinks (a no-op on real devices).
                self.audio_output.on_source_replaced();
                return Ok(());
            }
        }

        // Format differs (or nothing is attached yet): build a fresh stream over
        // a new source for this track. A fresh source has no prior events to
        // carry — an outgoing track's would be stale — so carry none.
        let source = Arc::new(Mutex::new(source::PlaybackSource::new(track_stream, fmt)));
        self.build_output_over(source, sample_rate, channels, Vec::new())
    }

    /// Drop the current output stream (if any) and build a fresh one over
    /// `source`, resolving the current default device. Both the format-change
    /// branch of `attach_track` and the default-device-change handler go through
    /// here — the difference is only whether `source` wraps a new track or the
    /// live one. A fresh stream always needs a fresh audio-events channel (the old
    /// sender dies with the old drain); `carry_events` are events already queued
    /// for the SAME source that must survive the new channel — the device-change
    /// rebuild passes the old receiver's pending events, since they can never
    /// re-fire (a `Completion` latched the source's completion flag). A format
    /// change wraps a new track, so it carries none. On a build failure the source
    /// is cancelled (so a decoder filling its ring exits) and the error returned.
    pub(super) fn build_output_over(
        &mut self,
        source: Arc<Mutex<source::PlaybackSource>>,
        sample_rate: u32,
        channels: u32,
        carry_events: Vec<AudioEvent>,
    ) -> Result<(), PlaybackError> {
        // Drop the old stream first, so the device is released / the old capture
        // thread joins before the new one binds. Cancel its source before dropping
        // it: a format-change rebuild discards the outgoing track's source, whose
        // decoder `teardown_current_track` stopped only via its AVIO token — and a
        // decoder parked writing a full ring unparks only on the sink's cancel
        // flag, so without this it would park forever. (The device-change handler
        // took `self.output` out before calling here and passes that same source
        // back in, so there is nothing to cancel then.)
        if let Some(old) = self.output.take() {
            old.source.lock().unwrap().cancel();
        }

        let (mut audio_event_tx, audio_events) = audio_event_channel();
        for event in carry_events {
            audio_event_tx.push(event);
        }

        let stream = match self.audio_output.create_stream(
            source.clone(),
            sample_rate,
            channels,
            audio_event_tx,
            self.position_update_interval_ms,
        ) {
            Ok(stream) => stream,
            Err(e) => {
                // No output will drain this source's ring. Cancel it so a decoder
                // filling it exits rather than parking on a full ring forever.
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

        self.output = Some(OutputStream {
            _stream: stream,
            source,
            audio_events,
            sample_rate,
            channels,
        });
        Ok(())
    }

    /// Rebuild the output stream over the SAME `PlaybackSource` after the system
    /// default output device changed, re-resolving the now-current default device.
    /// No-op when nothing is playing (no stream to move). Playback position and
    /// state ride on the source and the shared atomic, so they survive the swap;
    /// a brief glitch at the switch is acceptable. A rebuild that can't get a
    /// device fails loud — emit a `PlaybackError` and stop — rather than leaving a
    /// silently dead stream. macOS-only: it's driven by the CoreAudio
    /// default-device listener, which only exists there.
    #[cfg(target_os = "macos")]
    pub(super) async fn handle_output_device_changed(&mut self) {
        let Some(out) = self.output.take() else {
            debug!("output device changed with nothing playing; no stream to rebuild");
            return;
        };
        let OutputStream {
            _stream,
            source,
            mut audio_events,
            sample_rate,
            channels,
        } = out;
        // Release the old device before binding the new one.
        drop(_stream);
        // Carry events already queued for this (unchanged) source onto the new
        // channel — the callback pushed them under the source lock before the
        // device changed, and a `Completion` among them can never re-fire.
        let mut pending = Vec::new();
        while let Some(event) = audio_events.pop() {
            pending.push(event);
        }
        info!("Default output device changed; rebuilding the output stream in place");
        if let Err(e) = self.build_output_over(source, sample_rate, channels, pending) {
            error!("Failed to rebuild output after a device change: {e}");
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::PlaybackError {
                    reason: e.into_ui_reason(),
                },
            );
            self.stop().await;
        }
    }
}
