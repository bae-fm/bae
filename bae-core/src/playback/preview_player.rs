//! # Preview Player
//!
//! A self-contained second audio player for auditioning a local file before
//! import. It owns its own audio output, stream, decoder, buffer, and listener
//! tasks — entirely separate from the main playback pipeline. The only point of
//! contact with the main player is that previewing pauses the main player and
//! stopping resumes it; that coordination lives in `PlaybackService`, not here.
//!
//! Preview has only Idle/Playing/Paused — no Loading/Buffering state — so it
//! emits Playing/Paused immediately and skips the ready-watcher the main player
//! uses. The demand-driven local fill keeps the ring fed; the audio callback
//! outputs silence until the first samples land.

use crate::playback::audio_output::{AudioEvent, AudioOutput, AudioStream};
use crate::playback::data_source::{AudioDataReader, LocalReader};
use crate::playback::progress::{emit_progress, PlaybackProgress, PreviewState};
use crate::playback::service::{
    default_audio_output, dispatch_command, log_streaming_decode_failure, setup_audio_stream,
    teardown_decoder_for_seek, PlaybackCommand,
};
use crate::playback::source;
use crate::playback::source::TrackFmt;
use crate::playback::sparse_buffer::{create_sparse_buffer, SharedSparseBuffer};
use crate::playback::track_stream::create_track_stream_pair;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc as tokio_mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// A second audio player dedicated to previewing a local file. Holds all preview
/// state; the main player coordinates pause/resume around it but never reaches
/// into these fields.
pub(crate) struct PreviewPlayer {
    /// Progress sink, cloned from the service so preview emits the same
    /// `PlaybackProgress` stream the UI already subscribes to.
    progress_tx: tokio_mpsc::UnboundedSender<PlaybackProgress>,
    /// Command sink, used by the event listener to post `PreviewCompleted`
    /// back to the service's command loop.
    command_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
    /// How often (ms) the audio callback emits position updates to the UI.
    position_update_interval_ms: u32,
    /// Separate audio output for preview (lazily created on first play).
    audio_output: Option<Box<dyn AudioOutput>>,
    /// Stream for preview playback.
    stream: Option<Box<dyn AudioStream>>,
    /// Streaming source for preview (to cancel on stop). Wrapped in a
    /// single-track `PlaybackSource` (preview never chains) to share the audio
    /// output's stream interface.
    playback_source: Option<Arc<Mutex<source::PlaybackSource>>>,
    /// Path of the file currently being previewed.
    path: Option<String>,
    /// Last known duration for the preview file.
    duration: Duration,
    /// Abort handles for preview event listener tasks.
    listener_handle: Option<JoinHandle<()>>,
    /// Sparse buffer for the current preview (retained across seeks).
    buffer: Option<SharedSparseBuffer>,
    /// JoinHandle for the preview decoder thread (needed for seek cancellation).
    decoder_handle: Option<std::thread::JoinHandle<()>>,
    /// Token for the active preview decoder's AVIO reader.
    decoder_cancel_token: Option<Arc<std::sync::atomic::AtomicBool>>,
    sample_rate: u32,
    channels: u32,
}

impl PreviewPlayer {
    pub(crate) fn new(
        progress_tx: tokio_mpsc::UnboundedSender<PlaybackProgress>,
        command_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
        position_update_interval_ms: u32,
    ) -> Self {
        Self {
            progress_tx,
            command_tx,
            position_update_interval_ms,
            audio_output: None,
            stream: None,
            playback_source: None,
            path: None,
            duration: Duration::ZERO,
            listener_handle: None,
            buffer: None,
            decoder_handle: None,
            decoder_cancel_token: None,
            sample_rate: 44100,
            channels: 2,
        }
    }

    /// Path of the file currently being previewed, if any.
    pub(crate) fn current_path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    /// Whether a preview file is loaded (playing, paused, or finished-but-shown).
    pub(crate) fn is_active(&self) -> bool {
        self.path.is_some()
    }

    /// Whether the active preview reached its end (stream and source torn down)
    /// but is still shown on the bar.
    pub(crate) fn is_finished(&self) -> bool {
        self.stream.is_none() && self.playback_source.is_none()
    }

    /// Drop a finished preview's lingering listeners and path so a fresh play of
    /// the same file starts clean.
    pub(crate) fn clear_finished(&mut self) {
        self.abort_listeners();
        self.path = None;
    }

    /// Start a fresh preview of `path`. Switches off any currently-loaded
    /// preview first. Returns true if playback started.
    pub(crate) async fn play(&mut self, path: String) -> bool {
        // A different preview is still active: stop it first (without resuming
        // main — the new preview keeps main paused).
        if self.path.is_some() {
            self.stop();
        }

        let source_size = match tokio::fs::metadata(&path).await {
            Ok(metadata) => metadata.len(),
            Err(e) => {
                error!("Failed to stat preview file {}: {}", path, e);
                return false;
            }
        };

        // Probe duration and sample rate from file
        let Some(probe) = probe_preview_audio(&path).await else {
            return false;
        };
        let probed_duration = probe.duration;
        let sample_rate = probe.sample_rate;
        let channels = probe.channels;

        // Create sparse buffer and start local file reader
        let buffer = create_sparse_buffer(source_size);
        let reader: Box<dyn AudioDataReader> = Box::new(LocalReader::new(path.clone()));
        reader.start_reading(buffer.clone(), self.progress_tx.clone());

        self.buffer = Some(buffer.clone());
        self.sample_rate = sample_rate;
        self.channels = channels;

        let started = self
            .start_decode(
                path.clone(),
                probed_duration,
                sample_rate,
                channels,
                buffer,
                None,
                false,
            )
            .await;
        if started {
            info!("Preview started: {}", path);
        }
        started
    }

    /// Seek by slider ratio (0.0–1.0) within the active preview.
    pub(crate) async fn seek_by_ratio(&mut self, ratio: f64) {
        let duration_ms = self.duration.as_millis() as u64;
        let position_ms = (ratio.clamp(0.0, 1.0) * duration_ms as f64) as u64;
        self.seek(Duration::from_millis(position_ms)).await;
    }

    /// Seek within the active preview.
    pub(crate) async fn seek(&mut self, position: Duration) {
        let buffer = match &self.buffer {
            Some(buf) => buf.clone(),
            None => return,
        };
        if self.path.is_none() {
            return;
        }
        let duration = self.duration;
        if duration.is_zero() {
            return;
        }

        let was_paused = self
            .audio_output
            .as_ref()
            .map(|o| o.get_state() == crate::playback::audio_output::AudioState::Paused)
            .unwrap_or(false);

        // Abort old listeners immediately to prevent stale position ticks
        self.abort_listeners();
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }

        // Tear down old decoder, preserve buffer
        let preview_cancel = self
            .decoder_cancel_token
            .take()
            .expect("active preview decoder has a cancel token");
        teardown_decoder_for_seek(
            &mut self.playback_source,
            std::slice::from_ref(&buffer),
            &preview_cancel,
            &mut self.decoder_handle,
        )
        .await;

        // Start new decoder on the same buffer with seek_to
        let path = self.path.clone().unwrap();
        let sample_rate = self.sample_rate;
        let channels = self.channels;
        self.start_decode(
            path,
            duration,
            sample_rate,
            channels,
            buffer,
            Some(position),
            was_paused,
        )
        .await;

        // When seeking while paused, no tick will fire to carry the new
        // position — emit explicitly so the NSView updates. When seeking
        // while playing, the event listener task picks up from the new
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

    /// Toggle pause/resume on the active (non-finished) preview.
    pub(crate) fn toggle_pause(&mut self) {
        let Some(path) = self.path.clone() else {
            return;
        };

        let Some(preview_output) = &self.audio_output else {
            return;
        };

        let dur_ms = self.duration.as_millis() as u64;
        match preview_output.get_state() {
            crate::playback::audio_output::AudioState::Playing => {
                preview_output.set_state(crate::playback::audio_output::AudioState::Paused);

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

    /// Stop preview playback and tear down its pipeline. Does not touch the main
    /// player — the caller decides whether to resume it.
    pub(crate) fn stop(&mut self) {
        if let Some(source) = self.playback_source.take() {
            if let Ok(guard) = source.lock() {
                guard.cancel();
            }
        }

        // Cancel the token and buffer so any blocked decoder read exits.
        if let Some(cancel_token) = self.decoder_cancel_token.take() {
            cancel_token.store(true, std::sync::atomic::Ordering::Release);
        }
        if let Some(buf) = &self.buffer {
            buf.cancel();
        }
        self.buffer = None;
        self.decoder_handle = None;

        if let Some(stream) = self.stream.take() {
            drop(stream);
        }
        if let Some(preview_output) = &self.audio_output {
            preview_output.set_state(crate::playback::audio_output::AudioState::Stopped);
        }

        self.abort_listeners();

        let was_previewing = self.path.is_some();
        self.path = None;
        self.duration = Duration::ZERO;

        if was_previewing {
            info!("Preview stopped");
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::PreviewStateChanged(PreviewState::Idle),
            );
        }
    }

    /// Abort preview event listener tasks.
    fn abort_listeners(&mut self) {
        if let Some(handle) = self.listener_handle.take() {
            handle.abort();
        }
    }

    /// Start decoding and streaming for preview playback. Shared by `play`
    /// (`seek_to=None`) and `seek`. Returns true on success.
    async fn start_decode(
        &mut self,
        path: String,
        duration: Duration,
        sample_rate: u32,
        channels: u32,
        buffer: SharedSparseBuffer,
        seek_to: Option<Duration>,
        paused: bool,
    ) -> bool {
        let (mut sink, source, _) = create_track_stream_pair(sample_rate, channels);

        let decoder_buffer = buffer.clone();
        let preview_cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let decoder_cancel_token = preview_cancel.clone();
        let seek_to_sample = seek_to.map(|d| (d.as_secs_f64() * sample_rate as f64) as u64);
        let decoder_handle = std::thread::spawn(move || {
            if let Err(e) = crate::audio_codec::decode_audio_streaming(
                decoder_buffer,
                &mut sink,
                None,
                seek_to_sample,
                None,
                None,
                None, // preview auditions the whole file
                preview_cancel,
            ) {
                let _ = log_streaming_decode_failure("Preview decode", e);
            }
        });

        self.decoder_handle = Some(decoder_handle);
        self.decoder_cancel_token = Some(decoder_cancel_token);

        // Preview never chains; the fmt's values are read from the audio
        // event stream by the listener.
        let preview_fmt = TrackFmt {
            track_id: path.clone(),
            duration_ms: duration.as_millis() as u64,
            pregap_ms: None,
            position_offset: seek_to.unwrap_or(Duration::ZERO),
            // Preview plays an unimported file (no stored measurements) at unity.
            replay_gain_linear: 1.0,
        };
        let source = Arc::new(Mutex::new(source::PlaybackSource::new(source, preview_fmt)));

        if self.audio_output.is_none() {
            match default_audio_output() {
                Ok(output) => self.audio_output = Some(output),
                Err(e) => {
                    error!("Failed to create preview audio output: {:?}", e);
                    return false;
                }
            }
        }
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }

        let setup = match setup_audio_stream(
            self.audio_output.as_deref_mut().unwrap(),
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

        if let Err(e) = setup.stream.play() {
            error!("Failed to start preview playback: {:?}", e);
            return false;
        }

        let preview_output = self.audio_output.as_ref().unwrap();
        if paused {
            preview_output.set_state(crate::playback::audio_output::AudioState::Paused);
        } else {
            preview_output.set_state(crate::playback::audio_output::AudioState::Playing);
        }

        self.stream = Some(setup.stream);
        self.playback_source = Some(source.clone());
        self.path = Some(path.clone());
        self.duration = duration;

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
        let mut audio_events = setup.audio_events;

        let h3 = tokio::spawn(async move {
            let mut event_tick = tokio::time::interval(Duration::from_millis(10));
            event_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                event_tick.tick().await;
                let mut completed = false;
                while let Some(event) = audio_events.pop() {
                    match event {
                        // Preview is single-track; the audio callback tags
                        // every tick with the same fmt we built above. Read
                        // its fields directly so the listener doesn't carry
                        // parallel copies.
                        AudioEvent::Position((fmt, pos)) => {
                            let actual_pos_ms = (fmt.position_offset + pos).as_millis() as u64;
                            emit_progress(
                                &progress_tx,
                                PlaybackProgress::PreviewPositionUpdate {
                                    position_ms: actual_pos_ms,
                                    progress: crate::playback::format::compute_progress(
                                        actual_pos_ms,
                                        fmt.duration_ms,
                                        fmt.pregap_ms,
                                    ),
                                },
                            );
                        }
                        AudioEvent::Completion((_fmt, _error_count, _samples_decoded)) => {
                            // Preview doesn't track decode stats — it is not a
                            // library track. The stats carried by the uniform
                            // event are dropped here by design.
                            dispatch_command(&command_tx, PlaybackCommand::PreviewCompleted);
                            completed = true;
                        }
                        AudioEvent::TrackCrossing(_) => {}
                        AudioEvent::SourceLockMissed { missed_ms } => {
                            warn!(
                                missed_ms,
                                "preview audio callback could not lock playback source while playing"
                            );
                        }
                        AudioEvent::SourceLockReacquired { missed_ms } => {
                            debug!(
                                missed_ms,
                                "preview audio callback reacquired playback source lock"
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
                                track_id = %fmt.track_id,
                                starved_ms,
                                position_ms,
                                producer_finished,
                                samples_decoded,
                                decode_errors,
                                has_next,
                                "preview playback source has no decoded samples while current track is not finished"
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
                                track_id = %fmt.track_id,
                                starved_ms,
                                position_ms,
                                samples_decoded,
                                decode_errors,
                                "preview playback source resumed after decoded sample starvation"
                            );
                        }
                    }
                    if completed {
                        break;
                    }
                }
                let dropped_required = audio_events.take_dropped_required_count();
                if dropped_required > 0 {
                    error!(
                        dropped_required,
                        "preview audio callback event queue dropped required events"
                    );
                    emit_progress(
                        &progress_tx,
                        PlaybackProgress::PlaybackError {
                            reason: crate::ui::PlaybackErrorReason::internal(
                                "Preview event queue dropped a required audio event".to_string(),
                            ),
                        },
                    );
                    break;
                }
                let dropped = audio_events.take_dropped_count();
                if dropped > 0 {
                    warn!(dropped, "preview audio callback event queue dropped events");
                }
                if completed {
                    break;
                }
            }
        });

        self.listener_handle = Some(h3);

        true
    }
}

async fn probe_preview_audio(path: &str) -> Option<crate::audio_codec::ProbeResult> {
    let probe_path = path.to_string();
    let result =
        tokio::task::spawn_blocking(move || crate::audio_codec::probe_audio_from_path(&probe_path))
            .await;
    resolve_preview_probe(path, result)
}

fn resolve_preview_probe(
    path: &str,
    result: Result<Option<crate::audio_codec::ProbeResult>, tokio::task::JoinError>,
) -> Option<crate::audio_codec::ProbeResult> {
    match result {
        Ok(Some(probe)) if probe.sample_rate > 0 && probe.channels > 0 => Some(probe),
        Ok(Some(probe)) => {
            error!(
                "Preview probe returned unusable audio format for {}: sample_rate={}, channels={}",
                path, probe.sample_rate, probe.channels
            );
            None
        }
        Ok(None) => {
            warn!("Failed to probe preview file {}", path);
            None
        }
        Err(e) => {
            error!("Preview probe task failed for {}: {}", path, e);
            None
        }
    }
}

#[cfg(all(test, feature = "test-utils"))]
mod tests {
    use super::*;
    use crate::playback::audio_output::CaptureAudioOutput;

    #[tokio::test]
    async fn preview_play_rejects_unprobeable_file() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().join("not-audio.bin");
        std::fs::write(&path, b"not audio").unwrap();
        let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
        let (command_tx, _command_rx) = tokio_mpsc::unbounded_channel();
        let mut player = PreviewPlayer::new(progress_tx, command_tx, 50);
        let (output, _capture_rx) = CaptureAudioOutput::new();
        player.audio_output = Some(Box::new(output));

        let started = player.play(path.display().to_string()).await;
        if started {
            player.stop();
        }

        assert!(!started);
    }

    #[tokio::test]
    async fn preview_probe_join_error_does_not_use_format_defaults() {
        let result = tokio::spawn(async { panic!("preview probe panic") })
            .await
            .map(|()| Option::<crate::audio_codec::ProbeResult>::None);

        assert!(resolve_preview_probe("path", result).is_none());
    }

    fn probe(sample_rate: u32, channels: u32) -> crate::audio_codec::ProbeResult {
        crate::audio_codec::ProbeResult {
            content_type: crate::util::content_type::ContentType::Flac,
            duration: Duration::from_secs(1),
            sample_rate,
            bits_per_sample: Some(16),
            channels,
        }
    }

    /// A usable probe (positive sample rate and channels) resolves to the probe.
    #[test]
    fn resolve_preview_probe_accepts_usable_format() {
        let result: Result<_, tokio::task::JoinError> = Ok(Some(probe(44100, 2)));
        let resolved = resolve_preview_probe("path", result).expect("a usable format passes");
        assert_eq!(resolved.sample_rate, 44100);
        assert_eq!(resolved.channels, 2);
    }

    /// A probe reporting a zero sample rate is an unusable format and is
    /// rejected rather than driving a decoder with a nonsense rate.
    #[test]
    fn resolve_preview_probe_rejects_zero_sample_rate() {
        let result: Result<_, tokio::task::JoinError> = Ok(Some(probe(0, 2)));
        assert!(resolve_preview_probe("path", result).is_none());
    }

    /// A probe reporting zero channels is likewise unusable.
    #[test]
    fn resolve_preview_probe_rejects_zero_channels() {
        let result: Result<_, tokio::task::JoinError> = Ok(Some(probe(44100, 0)));
        assert!(resolve_preview_probe("path", result).is_none());
    }

    /// A file the prober couldn't read at all (`Ok(None)`) yields no probe.
    #[test]
    fn resolve_preview_probe_rejects_unprobeable_file() {
        let result: Result<Option<crate::audio_codec::ProbeResult>, tokio::task::JoinError> =
            Ok(None);
        assert!(resolve_preview_probe("path", result).is_none());
    }
}
