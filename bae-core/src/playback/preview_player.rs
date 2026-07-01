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

use crate::playback::audio_output::{AudioOutput, AudioStream};
use crate::playback::data_source::{AudioDataReader, AudioReadConfig, LocalReader};
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
use tracing::{error, info};

/// A second audio player dedicated to previewing a local file. Holds all preview
/// state; the main player coordinates pause/resume around it but never reaches
/// into these fields.
pub(crate) struct PreviewPlayer {
    /// Progress sink, cloned from the service so preview emits the same
    /// `PlaybackProgress` stream the UI already subscribes to.
    progress_tx: tokio_mpsc::UnboundedSender<PlaybackProgress>,
    /// Command sink, used by the completion listener to post `PreviewCompleted`
    /// back to the service's command loop.
    command_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
    /// How often (ms) the audio callback sends position updates to the UI.
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
    /// Last known position for the preview file.
    position: Duration,
    /// Abort handles for preview position/completion listener tasks.
    listener_handles: Vec<JoinHandle<()>>,
    /// Sparse buffer for the current preview (retained across seeks).
    buffer: Option<SharedSparseBuffer>,
    /// JoinHandle for the preview decoder thread (needed for seek cancellation).
    decoder_handle: Option<std::thread::JoinHandle<()>>,
    /// Seek offset for the current preview (added to decoder-relative position).
    seek_offset: Duration,
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
            position: Duration::ZERO,
            listener_handles: Vec::new(),
            buffer: None,
            decoder_handle: None,
            seek_offset: Duration::ZERO,
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

        // Create sparse buffer and start local file reader
        let buffer = create_sparse_buffer(source_size);
        let read_config = AudioReadConfig {
            path: path.clone(),
            source_size,
        };
        let reader: Box<dyn AudioDataReader> = Box::new(LocalReader::new(read_config));
        reader.start_reading(buffer.clone(), self.progress_tx.clone());

        // Probe duration and sample rate from file
        let probe = {
            let probe_path = path.clone();
            tokio::task::spawn_blocking(move || {
                crate::audio_codec::probe_audio_from_path(&probe_path)
            })
            .await
            .unwrap_or(None)
        };
        let probed_duration = probe.as_ref().map(|p| p.duration).unwrap_or(Duration::ZERO);
        let sample_rate = probe.as_ref().map(|p| p.sample_rate).unwrap_or(44100);
        let channels = probe.as_ref().map(|p| p.channels).unwrap_or(2);

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
        let preview_cancel = Arc::new(std::sync::atomic::AtomicBool::new(true));
        teardown_decoder_for_seek(
            &mut self.playback_source,
            &buffer,
            &preview_cancel,
            &mut self.decoder_handle,
            false,
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
        // while playing, the position listener task picks up from the new
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

                // Record the current position so resume can continue from there.
                let position = self
                    .playback_source
                    .as_ref()
                    .map(|s| self.seek_offset + s.lock().unwrap().position())
                    .unwrap_or(self.position);
                self.position = position;

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

        // Cancel buffer to unblock decoder, then drop its handle
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
        self.position = Duration::ZERO;
        self.duration = Duration::ZERO;
        self.seek_offset = Duration::ZERO;

        if was_previewing {
            info!("Preview stopped");
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::PreviewStateChanged(PreviewState::Idle),
            );
        }
    }

    /// Abort preview position/completion listener tasks.
    fn abort_listeners(&mut self) {
        for handle in self.listener_handles.drain(..) {
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

        // Preview never chains; wrap in a PlaybackSource and drop the boundary
        // receiver so the sender's sends are no-ops. The fmt's values aren't
        // read by the preview listener (which captures `seek_offset` and the
        // duration locally), but they're set realistically for hygiene.
        let preview_fmt = TrackFmt {
            track_id: path.clone(),
            duration_ms: duration.as_millis() as u64,
            pregap_ms: None,
            position_offset: seek_to.unwrap_or(Duration::ZERO),
            // Preview plays an unimported file (no stored measurements) at unity.
            replay_gain_linear: 1.0,
        };
        let (preview_boundary_tx, _preview_boundary_rx) = tokio_mpsc::unbounded_channel();
        let source = Arc::new(Mutex::new(source::PlaybackSource::new(
            source,
            preview_fmt,
            preview_boundary_tx,
        )));

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
            for h in setup.bridge_handles {
                h.abort();
            }
            return false;
        }

        let preview_output = self.audio_output.as_ref().unwrap();
        if paused {
            preview_output.set_state(crate::playback::audio_output::AudioState::Paused);
        } else {
            preview_output.set_state(crate::playback::audio_output::AudioState::Playing);
        }

        let seek_offset = seek_to.unwrap_or(Duration::ZERO);

        self.stream = Some(setup.stream);
        self.playback_source = Some(source.clone());
        self.path = Some(path.clone());
        self.position = seek_offset;
        self.duration = duration;
        self.seek_offset = seek_offset;

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
        let mut position_rx_async = setup.position_rx;
        let mut completion_rx_async = setup.completion_rx;

        let h3 = tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Preview is single-track; the audio callback tags every
                    // tick with the same fmt we built above. Read its fields
                    // directly so the listener doesn't carry parallel copies
                    // of seek_offset/dur_ms.
                    Some((fmt, pos)) = position_rx_async.recv() => {
                        let actual_pos_ms = (fmt.position_offset + pos).as_millis() as u64;
                        emit_progress(
                            &progress_tx,
                            PlaybackProgress::PreviewPositionUpdate {
                                position_ms: actual_pos_ms,
                                progress: crate::playback::format::compute_progress(actual_pos_ms, fmt.duration_ms, fmt.pregap_ms),
                            },
                        );
                    }
                    Some((_fmt, _error_count, _samples_decoded)) = completion_rx_async.recv() => {
                        // Preview doesn't track decode stats — it's a quick
                        // playback, not a library track. The stats carried by
                        // CompletionEvent (uniform with main playback) are
                        // dropped here by design.
                        dispatch_command(&command_tx, PlaybackCommand::PreviewCompleted);
                        break;
                    }
                    else => break,
                }
            }
        });

        self.listener_handles = vec![setup.bridge_handles, vec![h3]]
            .into_iter()
            .flatten()
            .collect();

        true
    }
}
