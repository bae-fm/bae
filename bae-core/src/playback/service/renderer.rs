//! The renderer seam: where the playback service sends audio.
//!
//! The service drives one queue; a *renderer* is where the current track plays.
//! [`Renderer::Local`] is the in-process decode + cpal path (the rest of this
//! module); [`Renderer::Remote`] hands playback to a connected remote renderer —
//! a Cast receiver or a UPnP MediaRenderer — which fetches the audio over HTTP
//! itself and is driven by transport commands. The slot, queue, selection,
//! shuffle, and repeat logic are identical for both — only the transport differs,
//! and the two remote flavors differ only in the channel behind the session.
//!
//! While playing remotely, the slot still holds the current track (its metadata
//! drives the UI and the shared queue-advance path), but with no local decoder or
//! sparse buffers: the current track carries a no-op [`stub_decoder`] and
//! empty segments, so `play_track`/`handle_auto_advance`/`persist` all work
//! unchanged while the cpal pipeline stays idle.

use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::*;
use crate::renderer::{
    RendererMedia, RendererPlayerState, RendererSession, RendererSessionStatus, StreamFormatFn,
};

/// Where the current track plays. `Local` is the default; `Remote` holds the live
/// session for a fetch-a-URL device (Cast/DLNA); `AirPlay` keeps decoding locally
/// and only swaps the output sink to push audio to the receiver.
pub(super) enum Renderer {
    Local,
    Remote(RemoteRenderer),
    AirPlay(AirPlayRenderer),
}

impl Renderer {
    /// True only for a fetch-a-URL device: the local decode pipeline is torn down
    /// and the device is driven by transport commands. AirPlay is *not* remote in
    /// this sense — it decodes locally and swaps only the output sink.
    pub(super) fn is_remote(&self) -> bool {
        matches!(self, Renderer::Remote(_))
    }

    pub(super) fn is_airplay(&self) -> bool {
        matches!(self, Renderer::AirPlay(_))
    }

    pub(super) fn seek_remote(&self, position: Duration) {
        if let Self::Remote(remote) = self {
            remote.session.seek(position);
        }
    }

    pub(super) fn set_remote_volume(&self, volume: f32) {
        if let Self::Remote(remote) = self {
            remote.session.set_volume(volume);
        }
    }

    pub(super) fn pause_remote(&self) {
        if let Self::Remote(remote) = self {
            remote.session.pause();
        }
    }

    pub(super) fn play_remote(&self) {
        if let Self::Remote(remote) = self {
            remote.session.play();
        }
    }

    pub(super) fn flush_airplay(&self) {
        if let Self::AirPlay(airplay) = self {
            airplay.control.flush();
        }
    }

    pub(super) fn reanchor_airplay(&self) {
        if let Self::AirPlay(airplay) = self {
            airplay.control.reanchor();
        }
    }

    pub(super) fn airplay_failed(&self) -> bool {
        matches!(self, Self::AirPlay(airplay) if airplay.control.has_failed())
    }

    pub(super) fn airplay_latency(&self) -> Option<Duration> {
        match self {
            Self::AirPlay(airplay) => Some(Duration::from_secs_f64(
                f64::from(airplay.latency_frames) / f64::from(crate::airplay::session::SAMPLE_RATE),
            )),
            Self::Local | Self::Remote(_) => None,
        }
    }

    pub(super) fn return_to_local_for_stop(
        &mut self,
        audio_output: &mut Box<dyn crate::playback::audio_output::AudioOutput>,
    ) -> bool {
        match std::mem::replace(self, Self::Local) {
            Self::Local => false,
            Self::Remote(remote) => {
                remote.session.stop();
                true
            }
            Self::AirPlay(airplay) => {
                *audio_output = airplay.saved_output;
                true
            }
        }
    }
}

/// A live AirPlay session driven through the swapped output sink. The decode
/// pipeline runs unchanged; this holds the control handle the service drives
/// pause/resume/seek through, the local output to restore when AirPlay ends, and
/// the receiver latency the audible position is offset by.
pub(super) struct AirPlayRenderer {
    control: Arc<dyn crate::playback::airplay_output::AirPlayStreamControl>,
    /// The local (cpal/aaudio) output, put back when AirPlay playback ends.
    saved_output: Box<dyn crate::playback::audio_output::AudioOutput>,
    /// The receiver's audio latency in frames — the offset between what has been
    /// sent and what is audible, for the position the UI shows.
    latency_frames: u32,
}

impl AirPlayRenderer {
    pub(super) fn new(
        control: Arc<dyn crate::playback::airplay_output::AirPlayStreamControl>,
        saved_output: Box<dyn crate::playback::audio_output::AudioOutput>,
        latency_frames: u32,
    ) -> Self {
        Self {
            control,
            saved_output,
            latency_frames,
        }
    }
}

/// Everything a `PlayOnAirPlay` command carries: the sink that opens the RAOP
/// session (built off the service thread by bae-desktop with the device's
/// address, encryption, and latency), the device's display name, and the
/// receiver latency. A manual `Debug` keeps `PlaybackCommand`'s derive working.
pub(crate) struct AirPlayConnect {
    sink: Box<dyn crate::playback::airplay_output::AirPlaySink>,
    device_name: String,
    latency_frames: u32,
}

impl AirPlayConnect {
    pub(super) fn new(
        sink: Box<dyn crate::playback::airplay_output::AirPlaySink>,
        device_name: String,
        latency_frames: u32,
    ) -> Self {
        Self {
            sink,
            device_name,
            latency_frames,
        }
    }
}

impl std::fmt::Debug for AirPlayConnect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AirPlayConnect")
            .field("device_name", &self.device_name)
            .field("latency_frames", &self.latency_frames)
            .finish_non_exhaustive()
    }
}

/// A live remote-renderer session and the state the service needs to keep serving
/// it: the URL providers (to mint each track's media as the queue advances), the
/// flavor's stream-format gate (to pick raw vs. transcode per codec), and the
/// device's last reported position (for the handoff back to local playback when
/// remote playback stops). The device name isn't held here — it rides the
/// `RemoteStatusChanged` event out to the UI, which caches it.
pub(super) struct RemoteRenderer {
    session: RendererSession,
    stream_url_provider: crate::renderer::MediaUrlProvider,
    cover_url_provider: crate::renderer::CoverUrlProvider,
    /// The flavor-specific safe-set gate (Cast vs. DLNA differ on Opus).
    stream_format: StreamFormatFn,
    /// The device's most recent playback position, updated from each status.
    /// Local playback resumes here when remote playback ends.
    last_position: Duration,
}

/// Everything a `PlayOn` command carries to start remote playback: the connected
/// channel (built off the service thread by bae-desktop), the device's display
/// name, the injected URL providers, and the flavor's stream-format gate. A
/// manual `Debug` keeps `PlaybackCommand`'s derive working without the un-`Debug`
/// channel/closures.
pub(crate) struct RemoteConnect {
    channel: Box<dyn crate::renderer::RendererChannel>,
    device_name: String,
    stream_url_provider: crate::renderer::MediaUrlProvider,
    cover_url_provider: crate::renderer::CoverUrlProvider,
    stream_format: StreamFormatFn,
}

impl RemoteConnect {
    pub(super) fn new(
        channel: Box<dyn crate::renderer::RendererChannel>,
        device_name: String,
        stream_url_provider: crate::renderer::MediaUrlProvider,
        cover_url_provider: crate::renderer::CoverUrlProvider,
        stream_format: StreamFormatFn,
    ) -> Self {
        Self {
            channel,
            device_name,
            stream_url_provider,
            cover_url_provider,
            stream_format,
        }
    }
}

impl std::fmt::Debug for RemoteConnect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteConnect")
            .field("device_name", &self.device_name)
            .finish_non_exhaustive()
    }
}

/// The status callback the session reports through: it dispatches each device
/// status back onto the service's command loop as a `RemoteStatus` command, so
/// status handling stays serial with every other command.
pub(super) fn status_callback(
    command_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
) -> crate::renderer::StatusCallback {
    Arc::new(move |status: RendererSessionStatus| {
        dispatch_command(&command_tx, PlaybackCommand::RemoteStatus(status));
    })
}

/// A no-op stand-in for the local decoder while playing remotely: the thread has
/// already exited and the token is never read. It keeps `CurrentTrack`'s shape
/// (one always-present decoder) without a real decode running.
pub(super) fn stub_decoder() -> TrackDecoder {
    TrackDecoder {
        handle: std::thread::spawn(|| {}),
        cancel_token: Arc::new(AtomicBool::new(false)),
    }
}

impl PlaybackService {
    /// Switch to remote playback: capture the current track and position, tear the
    /// local pipeline down (keeping the queue), start the session, and reissue the
    /// current track to the device at its position.
    pub(super) async fn handle_play_on(&mut self, connect: RemoteConnect) {
        let RemoteConnect {
            channel,
            device_name,
            stream_url_provider,
            cover_url_provider,
            stream_format,
        } = connect;

        let current = self.current_track_id().map(str::to_string);
        let position = self
            .current_position_shared
            .lock()
            .unwrap()
            .unwrap_or(Duration::ZERO);
        // Carry the current play/pause intent to the device.
        let target = self.current_play_target();
        let volume = self.effective_volume();

        // Stop the local pipeline (decoder, cpal stream, byte buffers); the
        // queue and its cursor are untouched.
        self.teardown_local_playback();

        let session = RendererSession::start(channel, status_callback(self.command_tx.clone()));
        session.set_volume(volume);
        self.renderer = Renderer::Remote(RemoteRenderer {
            session,
            stream_url_provider,
            cover_url_provider,
            stream_format,
            last_position: position,
        });
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::RemoteStatusChanged {
                device_name: Some(device_name),
            },
        );

        match current {
            Some(track_id) => {
                self.play_track(
                    &track_id,
                    TrackStart::Position(position),
                    target,
                    TrackTransition::Manual,
                )
                .await;
            }
            None => {
                // Nothing was playing: remote playback is armed but idle.
                self.slot = PlaybackSlot::Stopped;
                self.sync_audio_state();
                self.emit_state();
            }
        }
    }

    /// User-initiated stop of device playback: end the session and resume local
    /// playback paused at the last position — for whichever device flavor is live.
    pub(super) async fn handle_stop_remote(&mut self) {
        if self.renderer.is_airplay() {
            self.end_airplay_and_resume_local().await;
        } else {
            self.end_remote_and_resume_local(true).await;
        }
    }

    /// Apply one device status. Ignored when playing locally (a stale update from
    /// a session that already ended). A terminal `ended` update — a device-side
    /// stop — resumes local playback; otherwise the position feed and the slot
    /// phase are reconciled to the device.
    pub(super) async fn handle_remote_status(&mut self, status: RendererSessionStatus) {
        if !self.renderer.is_remote() {
            return;
        }
        if status.ended {
            info!("remote session ended (device-side); resuming local playback");
            self.end_remote_and_resume_local(false).await;
            return;
        }
        if let (Renderer::Remote(remote), Some(position)) = (&mut self.renderer, status.position) {
            remote.last_position = position;
        }
        self.apply_remote_status(status).await;
    }

    /// End the remote session and resume the local renderer, paused at the
    /// device's last position. `stop_device` stops device playback first (a
    /// user-initiated stop); a device-side end has already stopped it.
    async fn end_remote_and_resume_local(&mut self, stop_device: bool) {
        let last_position = match &self.renderer {
            Renderer::Remote(remote) => {
                if stop_device {
                    remote.session.stop();
                }
                remote.last_position
            }
            // AirPlay ends through `end_airplay_and_resume_local`, not here.
            Renderer::Local | Renderer::AirPlay(_) => return,
        };
        let current = self.current_track_id().map(str::to_string);

        // Dropping the session ends its poll thread; back to local.
        self.renderer = Renderer::Local;
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::RemoteStatusChanged { device_name: None },
        );
        self.teardown_local_playback();

        match current {
            Some(track_id) => {
                self.play_track(
                    &track_id,
                    TrackStart::Position(last_position),
                    PlayTarget::Paused(PausePhase::Manual),
                    TrackTransition::Manual,
                )
                .await;
                self.emit_position_display(last_position.as_millis() as u64, track_id);
            }
            None => self.stop().await,
        }
    }

    /// Reconcile the slot and the progress feed to a non-terminal device status:
    /// emit a position update, and drive queue-advance / phase changes off the
    /// device's player state.
    async fn apply_remote_status(&mut self, status: RendererSessionStatus) {
        let PlaybackSlot::Active(cur) = &self.slot else {
            return;
        };
        let track_id = cur.prepared.track_info.track_id.clone();
        let raw_dur_ms = cur.prepared.duration.as_millis() as u64;
        let pregap_ms = cur.prepared.total_pregap_ms();

        // Feed the shared progress channel so every UI and the position store
        // update exactly as they do for local playback.
        if let Some(position) = status.position {
            let raw_pos_ms = position.as_millis() as u64;
            *self.current_position_shared.lock().unwrap() = Some(position);
            let progress =
                crate::playback::format::compute_progress(raw_pos_ms, raw_dur_ms, pregap_ms);
            let (adjusted_pos_ms, adjusted_dur_ms) =
                crate::playback::format::adjust_for_pregap(raw_pos_ms, raw_dur_ms, pregap_ms);
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::PositionUpdate {
                    position_ms: adjusted_pos_ms,
                    duration_ms: adjusted_dur_ms,
                    track_id: track_id.clone(),
                    progress,
                },
            );
        }

        match status.player_state {
            RendererPlayerState::Finished => {
                // The device reached end-of-track: advance the shared queue
                // through the one advance entry point, exactly like local
                // end-of-track.
                if let PlaybackSlot::Active(cur) = &mut self.slot {
                    cur.phase = TrackPhase::Completed;
                }
                self.handle_auto_advance(track_id).await;
            }
            RendererPlayerState::Playing => self.set_remote_phase(TrackPhase::Playing),
            RendererPlayerState::Paused => {
                self.set_remote_phase(TrackPhase::Paused(PausePhase::Manual))
            }
            // Buffering/idle are transient; the installed phase stands.
            RendererPlayerState::Buffering | RendererPlayerState::Idle => {}
        }
    }

    /// Move the current remote track to `phase` and emit, but only on a real
    /// change (the device reports its state every poll).
    fn set_remote_phase(&mut self, phase: TrackPhase) {
        let PlaybackSlot::Active(cur) = &mut self.slot else {
            return;
        };
        if cur.phase.intent() == phase.intent() {
            return;
        }
        cur.phase = phase;
        self.sync_audio_state();
        self.emit_state();
    }

    /// Load `track_id` onto the device: resolve its metadata (no local buffers or
    /// decoder), mint its media URL, and install it as the current track. The
    /// remote branch of `play_track`.
    pub(super) async fn play_track_remote(
        &mut self,
        track_id: &str,
        start: TrackStart,
        target: PlayTarget,
    ) {
        // Tear the previous remote track's slot down (stub decoder, empty
        // segments — nothing to release).
        self.discard_current_track();

        self.slot = PlaybackSlot::Loading {
            track_id: track_id.to_string(),
            resolved: None,
        };
        self.emit_state();

        let (resolved, track_info) = match self
            .library_manager
            .resolve_track_audio_and_info(track_id)
            .await
        {
            Ok(pair) => pair,
            Err(e) => {
                error!("remote: failed to resolve track {track_id}: {e}");
                self.fail_remote(PlaybackError::database(e).into_ui_reason())
                    .await;
                return;
            }
        };
        let content_type = resolved.content_type.clone();
        let replay_gain_mode = self.library_manager.get_config().replay_gain_mode;
        let prepared = finalize_playback_track(resolved, track_info, Vec::new(), replay_gain_mode);

        // The content-type gate is decided here, where the track's type is
        // known, through the flavor's own safe-set, and passed to the provider
        // (which renders the URL) so the URL's format and the declared MIME below
        // never disagree.
        let Renderer::Remote(remote) = &self.renderer else {
            // Raced out of remote playback before the resolve returned.
            return;
        };
        let format = (remote.stream_format)(&content_type);

        let url = match (remote.stream_url_provider)(track_id, format) {
            Ok(url) => url,
            Err(reason) => {
                error!("remote: failed to mint media URL for {track_id}: {reason}");
                self.fail_remote(crate::ui::PlaybackErrorReason::internal(
                    "Couldn't build the audio URL for the renderer.",
                ))
                .await;
                return;
            }
        };
        let media = RendererMedia {
            url,
            content_type: format.content_type_str(&content_type),
            title: prepared.track_info.track_title.clone(),
            artist: prepared.track_info.artist_names.clone(),
            album: prepared.track_info.album_title.clone(),
            cover_url: (remote.cover_url_provider)(track_id),
            duration: Some(prepared.duration),
        };
        let start_position = start.position(prepared.total_pregap_ms());
        remote.session.load(media);
        if start_position > Duration::ZERO {
            remote.session.seek(start_position);
        }
        if matches!(target, PlayTarget::Paused(_)) {
            remote.session.pause();
        }

        *self.current_position_shared.lock().unwrap() = Some(start_position);
        self.install_active_track(prepared, stub_decoder(), target.into_track_phase());
        self.emit_state();
        self.persist_playback_state().await;
    }

    /// A remote track load failed: surface the error and stop remote playback,
    /// dropping back to a stopped local renderer.
    async fn fail_remote(&mut self, reason: crate::ui::PlaybackErrorReason) {
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::PlaybackError { reason },
        );
        self.renderer = Renderer::Local;
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::RemoteStatusChanged { device_name: None },
        );
        self.stop().await;
    }

    /// The current (effective) output volume — 0 while muted, so the device is
    /// seeded to match what the user hears.
    fn effective_volume(&self) -> f32 {
        if self.is_muted {
            0.0
        } else {
            self.audio_output.get_volume()
        }
    }

    /// Switch to AirPlay: keep decoding locally but swap the output sink so the
    /// decoded PCM is pushed to the receiver instead of the DAC. The queue, the
    /// slot, the decoder, and the advance path are unchanged — only where the
    /// audio goes changes, so `play_track` takes its ordinary local branch.
    pub(super) async fn handle_play_on_airplay(&mut self, connect: AirPlayConnect) {
        let AirPlayConnect {
            sink,
            device_name,
            latency_frames,
        } = connect;

        let current = self.current_track_id().map(str::to_string);
        let position = self
            .current_position_shared
            .lock()
            .unwrap()
            .unwrap_or(Duration::ZERO);
        let target = self.current_play_target();
        let volume = self.effective_volume();

        // Tear the current local pipeline down; `play_track` rebuilds it below,
        // this time feeding the AirPlay sink.
        self.teardown_local_playback();

        let control = Arc::new(crate::playback::airplay_output::AirPlayControl::new());
        let airplay_output =
            crate::playback::airplay_output::AirPlayOutput::new(sink, volume, control.clone());
        let saved_output = std::mem::replace(&mut self.audio_output, Box::new(airplay_output));
        self.renderer =
            Renderer::AirPlay(AirPlayRenderer::new(control, saved_output, latency_frames));
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::RemoteStatusChanged {
                device_name: Some(device_name),
            },
        );

        match current {
            Some(track_id) => {
                self.play_track(
                    &track_id,
                    TrackStart::Position(position),
                    target,
                    TrackTransition::Manual,
                )
                .await;
            }
            None => {
                self.slot = PlaybackSlot::Stopped;
                self.sync_audio_state();
                self.emit_state();
            }
        }
    }

    /// End AirPlay playback: drop the AirPlay output (which tears the receiver
    /// session down), restore the saved local output, and resume local playback
    /// paused at the last position.
    pub(super) async fn end_airplay_and_resume_local(&mut self) {
        let saved_output = match std::mem::replace(&mut self.renderer, Renderer::Local) {
            Renderer::AirPlay(r) => r.saved_output,
            other => {
                self.renderer = other;
                return;
            }
        };
        // Decode is local, so the live position is the shared one the drain's ticks
        // keep current (latency-adjusted) — read it now, before teardown clears it,
        // so local resumes where playback actually is, not the switch-time point.
        let last_position = self
            .current_position_shared
            .lock()
            .unwrap()
            .unwrap_or(Duration::ZERO);
        let current = self.current_track_id().map(str::to_string);

        // Dropping the current output (the AirPlay stream) tears the receiver
        // session down; then restore the local sink.
        self.teardown_local_playback();
        self.audio_output = saved_output;
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::RemoteStatusChanged { device_name: None },
        );

        match current {
            Some(track_id) => {
                self.play_track(
                    &track_id,
                    TrackStart::Position(last_position),
                    PlayTarget::Paused(PausePhase::Manual),
                    TrackTransition::Manual,
                )
                .await;
                self.emit_position_display(last_position.as_millis() as u64, track_id);
            }
            None => self.stop().await,
        }
    }

    /// Tear down the local audio pipeline — the current decoder, the preload, the
    /// cpal output, and the file buffers — without emitting or persisting. Shared
    /// by `stop` (which then emits Stopped) and the remote switch (which then
    /// installs the remote track).
    pub(super) fn teardown_local_playback(&mut self) {
        self.stop_preview_for_main_playback();
        self.discard_current_track();
        self.clear_next_track_state();
        if let Some(out) = self.output.take() {
            out.source.lock().unwrap().cancel();
        }
        for buffer in self.shared_file_buffers.values() {
            buffer.cancel();
        }
        self.shared_file_buffers.clear();
        *self.current_position_shared.lock().unwrap() = None;
        self.reset_starvation_episode();
    }
}
