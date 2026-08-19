//! # Preview Player
//!
//! A self-contained second audio player for auditioning a local file before
//! import. It opens its own audio output — a second one from the same device the
//! service opened the main player's from — and runs one shared `StreamPipeline`
//! at a time, entirely separate from the main playback pipeline. The only point
//! of contact with the main player is that previewing pauses the main player and
//! stopping resumes it; that coordination lives in `PlaybackService`, not here.
//!
//! Preview has only Idle/Playing/Paused — no Loading/Buffering state — so it
//! emits Playing/Paused immediately and drops the pipeline's ready receiver (the
//! main player uses it to arm a ready-watcher). The demand-driven local fill
//! keeps the ring fed; the audio callback outputs silence until the first
//! samples land.

use crate::playback::audio_output::{
    AudioEvent, AudioEventReceiver, AudioOutput, AudioOutputDevice, AudioState,
};
use crate::playback::data_source::{AudioDataReader, LocalReader};
use crate::playback::progress::{emit_progress, PlaybackProgress, PreviewState};
use crate::playback::service::{dispatch_command, PlaybackCommand};
use crate::playback::source::TrackFmt;
use crate::playback::sparse_buffer::{create_sparse_buffer, SharedSparseBuffer};
use crate::playback::stream_pipeline::{
    log_stream_diagnostic, report_dropped_audio_events, start_stream_pipeline, DecodeFailureReport,
    SegmentDecodeParams, StreamDecodeParams, StreamPipeline,
};
use std::time::Duration;
use tokio::sync::mpsc as tokio_mpsc;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

/// The preview shape of a fill-error handler: the audition has one file and one
/// buffer, so a failed byte fill can only mean this preview is unplayable — it
/// goes straight to the UI as a `PlaybackError`. (The fill itself cancels the
/// buffer right after, unblocking the decoder.)
fn preview_fill_error_handler(
    progress_tx: tokio_mpsc::UnboundedSender<PlaybackProgress>,
) -> crate::playback::data_source::FillErrorHandler {
    Box::new(move |error| {
        emit_progress(
            &progress_tx,
            PlaybackProgress::PlaybackError {
                reason: error.into_ui_reason(),
            },
        );
    })
}

/// The outcome of a preview [`PreviewPlayer::play`] attempt. `Started` is the
/// success; the two failures name which stage failed so the service reports the
/// right `playback_failed` operation — the file couldn't be prepared (stat/probe)
/// vs. the audio output stream couldn't be built and started.
pub(crate) enum PreviewPlayOutcome {
    Started,
    SetupFailed,
    StreamStartFailed,
}

#[cfg(test)]
impl PreviewPlayOutcome {
    fn started(&self) -> bool {
        matches!(self, Self::Started)
    }
}

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
    /// Separate audio output for preview, opened from the service's audio device
    /// on first play and retained across plays. Preview's play/pause authority is
    /// this output's state atomic — its own truth, distinct from the main
    /// player's slot.
    audio_output: Option<Box<dyn AudioOutput>>,
    /// The currently-loaded preview, if any.
    active: Option<ActivePreview>,
}

/// One loaded preview: its file identity + format, the retained buffer (kept
/// across seeks so the reader isn't restarted), the event-listener task, and the
/// live streaming pipeline.
struct ActivePreview {
    path: String,
    duration: Duration,
    sample_rate: u32,
    channels: u32,
    /// Sparse buffer for the current preview, retained across seeks; the owner
    /// cancels it on stop.
    buffer: SharedSparseBuffer,
    /// The preview event-listener task (aborted on stop/seek).
    listener_handle: JoinHandle<()>,
    pipeline: StreamPipeline,
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
            active: None,
        }
    }

    /// Path of the file currently being previewed, if any.
    pub(crate) fn current_path(&self) -> Option<&str> {
        self.active.as_ref().map(|a| a.path.as_str())
    }

    /// Whether a preview file is loaded (playing or paused).
    pub(crate) fn is_active(&self) -> bool {
        self.active.is_some()
    }

    /// Start a fresh preview of `path`. Switches off any currently-loaded
    /// preview first. The outcome distinguishes a clean start from a setup
    /// failure (stat/probe) and a stream-start failure, so the service reports
    /// the matching `playback_failed` operation.
    pub(crate) async fn play(
        &mut self,
        path: String,
        audio_device: &dyn AudioOutputDevice,
    ) -> PreviewPlayOutcome {
        // Stop the active preview without resuming main — the new preview keeps
        // main paused.
        if self.active.is_some() {
            self.stop();
        }

        let source_size = match tokio::fs::metadata(&path).await {
            Ok(metadata) => metadata.len(),
            Err(e) => {
                error!("Failed to stat preview file {}: {}", path, e);
                return PreviewPlayOutcome::SetupFailed;
            }
        };

        let Some(probe) = probe_preview_audio(&path).await else {
            return PreviewPlayOutcome::SetupFailed;
        };

        let buffer = create_sparse_buffer(source_size);
        let reader: Box<dyn AudioDataReader> = Box::new(LocalReader::new(path.clone()));
        reader.start_reading(
            buffer.clone(),
            preview_fill_error_handler(self.progress_tx.clone()),
        );

        let started = self
            .start_streaming(
                path.clone(),
                probe.duration,
                probe.sample_rate,
                probe.channels,
                buffer,
                None,
                false,
                audio_device,
            )
            .await;
        if started {
            info!("Preview started: {}", path);
            PreviewPlayOutcome::Started
        } else {
            PreviewPlayOutcome::StreamStartFailed
        }
    }

    /// Seek by slider ratio (0.0–1.0) within the active preview. A preview is a
    /// whole file, so there is no pregap to offset past.
    pub(crate) async fn seek_by_ratio(&mut self, ratio: f64, audio_device: &dyn AudioOutputDevice) {
        let Some(active) = self.active.as_ref() else {
            return;
        };
        let position_ms = crate::playback::format::position_for_progress(
            ratio,
            active.duration.as_millis() as u64,
            None,
        );
        self.seek(Duration::from_millis(position_ms), audio_device)
            .await;
    }

    /// Seek within the active preview. Tears down the current pipeline (retaining
    /// the buffer) and rebuilds a fresh one seeked to `position`.
    pub(crate) async fn seek(&mut self, position: Duration, audio_device: &dyn AudioOutputDevice) {
        let Some(active) = self.active.as_ref() else {
            return;
        };
        if active.duration.is_zero() {
            return;
        }

        let buffer = active.buffer.clone();
        let path = active.path.clone();
        let duration = active.duration;
        let sample_rate = active.sample_rate;
        let channels = active.channels;

        let was_paused = self
            .audio_output
            .as_ref()
            .map(|o| o.get_state() == AudioState::Paused)
            .unwrap_or(false);

        // Shut the old pipeline down — cancelling the decoder and joining it —
        // so the fresh decoder can reuse the retained buffer.
        let active = self.active.take().unwrap();
        active.listener_handle.abort();
        active
            .pipeline
            .shutdown_for_seek(std::slice::from_ref(&buffer))
            .await;

        let started = self
            .start_streaming(
                path,
                duration,
                sample_rate,
                channels,
                buffer,
                Some(position),
                was_paused,
                audio_device,
            )
            .await;

        // The rebuild failed: the old pipeline is torn down and `start_streaming`
        // cancelled the buffer, so the preview is gone (no zombie). Surface Idle
        // so the UI stops showing a preview that no longer exists rather than
        // freezing on the pre-seek state.
        if !started {
            if let Some(preview_output) = &self.audio_output {
                preview_output.set_state(AudioState::Stopped);
            }
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::PreviewStateChanged(PreviewState::Idle),
            );
            return;
        }

        // When seeking while paused, no tick will fire to carry the new position —
        // emit explicitly so the NSView updates. When seeking while playing, the
        // event listener picks up from the new offset on its next tick.
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

    /// Toggle pause/resume on the active preview.
    pub(crate) fn toggle_pause(&mut self) {
        let Some(active) = self.active.as_ref() else {
            return;
        };
        let path = active.path.clone();
        let dur_ms = active.duration.as_millis() as u64;

        let Some(preview_output) = &self.audio_output else {
            return;
        };

        match preview_output.get_state() {
            AudioState::Playing => {
                preview_output.set_state(AudioState::Paused);
                emit_progress(
                    &self.progress_tx,
                    PlaybackProgress::PreviewStateChanged(PreviewState::Paused {
                        path,
                        duration_ms: dur_ms,
                    }),
                );
            }
            AudioState::Paused => {
                preview_output.set_state(AudioState::Playing);
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
        let was_previewing = self.active.is_some();
        if let Some(active) = self.active.take() {
            active.listener_handle.abort();
            // Cancelling the buffer after the pipeline stops the local reader and
            // unblocks a decoder parked on a read.
            active.pipeline.cancel();
            active.buffer.cancel();
        }

        if let Some(preview_output) = &self.audio_output {
            preview_output.set_state(AudioState::Stopped);
        }

        if was_previewing {
            info!("Preview stopped");
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::PreviewStateChanged(PreviewState::Idle),
            );
        }
    }

    /// Build and start the shared streaming pipeline for a preview. Shared by
    /// `play` (`seek_to=None`) and `seek`. On success installs the `ActivePreview`
    /// and returns true; on failure cancels the buffer and returns false.
    async fn start_streaming(
        &mut self,
        path: String,
        duration: Duration,
        sample_rate: u32,
        channels: u32,
        buffer: SharedSparseBuffer,
        seek_to: Option<Duration>,
        paused: bool,
        audio_device: &dyn AudioOutputDevice,
    ) -> bool {
        if self.audio_output.is_none() {
            match audio_device.open_output() {
                Ok(output) => self.audio_output = Some(output),
                Err(e) => {
                    error!("Failed to open the preview audio output: {:?}", e);
                    buffer.cancel();
                    return false;
                }
            }
        }

        // Preview is a single whole-file window; the offset into it is the seek
        // target (0 for a fresh play), and the lead-in trim makes the first
        // output sample exact. No recorded landing byte exists for an
        // unimported file, so the decoder sample-seeks.
        let start_offset = seek_to
            .map(|d| (d.as_secs_f64() * sample_rate as f64) as u64)
            .unwrap_or(0);
        let decode = StreamDecodeParams::new(
            vec![SegmentDecodeParams::new(
                buffer.clone(),
                crate::db::SegmentSpan::whole_file(), // audition the whole file
                start_offset,
            )],
            true,
            0,
            0,
        );

        // Preview never chains, and plays an unimported file (no stored loudness
        // measurements) at unity gain. The listener reads position off this fmt,
        // carried on the event stream.
        let fmt = TrackFmt {
            track_id: path.clone(),
            duration_ms: duration.as_millis() as u64,
            pregap_ms: None,
            position_offset: seek_to.unwrap_or(Duration::ZERO),
            replay_gain_linear: 1.0,
        };

        let progress_tx = self.progress_tx.clone();
        let command_tx = self.command_tx.clone();
        let active_path = path.clone();
        let active_buffer = buffer.clone();
        let active = match start_stream_pipeline(
            self.audio_output.as_deref_mut().unwrap(),
            decode,
            fmt,
            sample_rate,
            channels,
            self.position_update_interval_ms,
            "Preview decode",
            DecodeFailureReport::LogOnly,
            move |pipeline, audio_events| ActivePreview {
                path: active_path,
                duration,
                sample_rate,
                channels,
                buffer: active_buffer,
                listener_handle: spawn_preview_listener(progress_tx, command_tx, audio_events),
                pipeline,
            },
        )
        .await
        {
            Ok(start) => start,
            Err(e) => {
                error!("Failed to start preview stream: {:?}", e);
                buffer.cancel();
                return false;
            }
        };

        // Preview's play/pause authority is its own audio-output state atomic.
        let preview_output = self.audio_output.as_ref().unwrap();
        preview_output.set_state(if paused {
            AudioState::Paused
        } else {
            AudioState::Playing
        });

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

        self.active = Some(active);
        true
    }
}

/// Spawn the preview event-listener task. Keeps the preview-specific arms
/// (Position → `PreviewPositionUpdate`, Completion → `PreviewCompleted`) and
/// delegates the diagnostics and dropped-event accounting to the shared unit.
fn spawn_preview_listener(
    progress_tx: tokio_mpsc::UnboundedSender<PlaybackProgress>,
    command_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
    mut audio_events: AudioEventReceiver,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut event_tick = tokio::time::interval(Duration::from_millis(10));
        event_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            event_tick.tick().await;
            let mut completed = false;
            while let Some(event) = audio_events.pop() {
                match event {
                    // Preview is single-track: every tick carries the same fmt
                    // built above, so read its fields rather than keeping a
                    // parallel copy here.
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
                    AudioEvent::Completion(_) => {
                        // The event's decode stats are dropped: a preview is
                        // not a library track, so there's nothing to record
                        // them against.
                        dispatch_command(&command_tx, PlaybackCommand::PreviewCompleted);
                        completed = true;
                    }
                    // Diagnostics (and the never-relevant TrackCrossing) go to
                    // the shared logger.
                    other => {
                        log_stream_diagnostic("preview", &other);
                    }
                }
                if completed {
                    break;
                }
            }
            if report_dropped_audio_events(&audio_events, "preview") {
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
            if completed {
                break;
            }
        }
    })
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

#[cfg(test)]
impl PreviewPlayer {
    /// Inject an active preview over a caller-built pipeline so service-level
    /// tests can exercise the completion/stop teardown seam without real audio.
    pub(crate) fn set_active_for_test(
        &mut self,
        path: String,
        pipeline: StreamPipeline,
        buffer: SharedSparseBuffer,
    ) {
        self.active = Some(ActivePreview {
            path,
            duration: Duration::from_secs(1),
            sample_rate: 44_100,
            channels: 2,
            buffer,
            listener_handle: tokio::spawn(async {}),
            pipeline,
        });
    }
}

#[cfg(all(test, feature = "test-utils"))]
mod tests {
    use super::*;
    use crate::playback::audio_output::{
        audio_event_channel, CaptureAudioDevice, FailingAudioDevice, FailingAudioOutput,
    };
    use std::sync::Arc;

    #[tokio::test]
    async fn preview_play_rejects_unprobeable_file() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let path = temp_dir.path().join("not-audio.bin");
        std::fs::write(&path, b"not audio").unwrap();
        let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
        let (command_tx, _command_rx) = tokio_mpsc::unbounded_channel();
        let mut player = PreviewPlayer::new(progress_tx, command_tx, 50);
        let (device, _capture_rx) = CaptureAudioDevice::new();

        let started = player
            .play(path.display().to_string(), &device)
            .await
            .started();
        if started {
            player.stop();
        }

        assert!(!started);
    }

    /// A failed stream start leaves no active preview and cancels the buffer — the
    /// preview side of the shared unit's failure contract (the unit cancels the
    /// decoder; the owner cancels the buffer). The same start over a device whose
    /// outputs work leaves `active` populated with the file as the current path.
    #[tokio::test]
    async fn failed_preview_stream_start_leaves_no_active_preview() {
        let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
        let (command_tx, _command_rx) = tokio_mpsc::unbounded_channel();
        let mut player = PreviewPlayer::new(progress_tx, command_tx, 50);
        let buffer = create_sparse_buffer(0);

        let started = player
            .start_streaming(
                "preview.wav".to_string(),
                Duration::from_secs(1),
                44_100,
                2,
                buffer.clone(),
                None,
                false,
                &FailingAudioDevice,
            )
            .await;

        assert!(!started);
        assert!(player.active.is_none());
        assert!(buffer.is_cancelled());

        let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
        let (command_tx, _command_rx) = tokio_mpsc::unbounded_channel();
        let mut player = PreviewPlayer::new(progress_tx, command_tx, 50);
        let (device, _capture_rx) = CaptureAudioDevice::new();
        let next_buffer = create_sparse_buffer(0);

        let started = player
            .start_streaming(
                "preview.wav".to_string(),
                Duration::from_secs(1),
                44_100,
                2,
                next_buffer,
                None,
                false,
                &device,
            )
            .await;

        assert!(started);
        assert_eq!(player.current_path(), Some("preview.wav"));
        player.stop();
    }

    /// The preview listener maps a natural-completion audio event to a
    /// `PreviewCompleted` command — the seam the service turns into teardown.
    #[tokio::test]
    async fn preview_listener_maps_completion_to_preview_completed() {
        let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
        let (command_tx, mut command_rx) = tokio_mpsc::unbounded_channel();
        let player = PreviewPlayer::new(progress_tx, command_tx, 50);

        let (mut audio_tx, audio_rx) = audio_event_channel();
        let handle = spawn_preview_listener(
            player.progress_tx.clone(),
            player.command_tx.clone(),
            audio_rx,
        );

        audio_tx.push_required(AudioEvent::Completion((
            Arc::new(TrackFmt {
                track_id: "preview.wav".to_string(),
                duration_ms: 1_000,
                pregap_ms: None,
                position_offset: Duration::ZERO,
                replay_gain_linear: 1.0,
            }),
            0,
            0,
        )));

        let cmd = tokio::time::timeout(Duration::from_secs(1), command_rx.recv())
            .await
            .expect("the listener dispatches within the timeout")
            .expect("a command is sent");
        assert!(matches!(cmd, PlaybackCommand::PreviewCompleted));
        handle.abort();
    }

    /// A preview seek whose stream rebuild fails tears the preview down outright
    /// (no zombie) and surfaces `PreviewState::Idle` so the UI stops showing it.
    #[tokio::test]
    async fn failed_preview_seek_surfaces_idle() {
        let (progress_tx, mut progress_rx) = tokio_mpsc::unbounded_channel();
        let (command_tx, _command_rx) = tokio_mpsc::unbounded_channel();
        let mut player = PreviewPlayer::new(progress_tx, command_tx, 50);

        // Set up an active preview over a working capture output.
        let (device, _capture_rx) = CaptureAudioDevice::new();
        let buffer = create_sparse_buffer(0);
        assert!(
            player
                .start_streaming(
                    "preview.wav".to_string(),
                    Duration::from_secs(1),
                    44_100,
                    2,
                    buffer,
                    None,
                    false,
                    &device,
                )
                .await
        );
        assert!(player.is_active());

        // The retained output now fails to build a stream — a device that went
        // away mid-preview. Seeking must tear the preview down and notify the UI
        // rather than leaving a torn-down zombie. (The seek reuses the retained
        // output, so the device passed here is never opened from.)
        player.audio_output = Some(Box::new(FailingAudioOutput));
        player.seek(Duration::from_millis(500), &device).await;

        assert!(
            !player.is_active(),
            "a failed seek leaves no active preview"
        );
        let mut saw_idle = false;
        while let Ok(progress) = progress_rx.try_recv() {
            if matches!(
                progress,
                PlaybackProgress::PreviewStateChanged(PreviewState::Idle)
            ) {
                saw_idle = true;
            }
        }
        assert!(
            saw_idle,
            "a failed preview seek surfaces PreviewState::Idle to the UI"
        );
    }

    /// Neither shape of "no usable probe" — the prober ran and found nothing
    /// (`Ok(None)`, a file that isn't audio) nor the probe task itself failed
    /// (`Err`, a panic) — resolves to a probe; both fall through to the same
    /// `None` rather than a caller mistaking either for a usable format and
    /// driving a decoder with defaults.
    #[tokio::test]
    async fn resolve_preview_probe_rejects_absent_or_failed_probe() {
        let unprobeable: Result<Option<crate::audio_codec::ProbeResult>, tokio::task::JoinError> =
            Ok(None);
        assert!(resolve_preview_probe("path", unprobeable).is_none());

        let join_failed = tokio::spawn(async { panic!("preview probe panic") })
            .await
            .map(|()| Option::<crate::audio_codec::ProbeResult>::None);
        assert!(resolve_preview_probe("path", join_failed).is_none());
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
}
