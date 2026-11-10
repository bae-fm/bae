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
    ReceiverStatus, RendererMedia, RendererMediaSource, RendererPlayerState, RendererSession,
    RendererSessionEvent,
};
use crate::{RemoteDevice, RemotePlayback};

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

    pub(super) fn seek_remote(&self, position: StreamPosition) {
        if let Self::Remote(remote) = self {
            remote.session.seek(position.as_duration());
        }
    }

    pub(super) fn set_remote_volume(&self, volume: crate::Volume) {
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

    /// Why the audio flow to the AirPlay receiver failed for good, when it
    /// has.
    pub(super) fn airplay_failure(&self) -> Option<String> {
        match self {
            Self::AirPlay(airplay) => airplay.control.failure(),
            Self::Local | Self::Remote(_) => None,
        }
    }

    /// How long the audio sent to an AirPlay receiver takes to be heard: the
    /// live session's latency. None when no audio is on its way to a receiver
    /// — playing here, on a fetching device, or with no AirPlay stream open.
    pub(super) fn airplay_latency(&self) -> Option<Duration> {
        match self {
            Self::AirPlay(airplay) => airplay.control.latency(),
            Self::Local | Self::Remote(_) => None,
        }
    }

    pub(super) fn return_to_local_for_stop(
        &mut self,
        audio_output: &mut crate::audio_output::PlayerOutput,
        volume: OutputVolume,
    ) -> bool {
        match std::mem::replace(self, Self::Local) {
            Self::Local => false,
            // Dropping the session tells the device to stop.
            Self::Remote(_) => true,
            Self::AirPlay(airplay) => {
                airplay.restore_local_output(audio_output, volume);
                true
            }
        }
    }
}

/// How remote playback ended: the person stopped it, or the link to the device
/// was lost, with why.
pub(super) enum RemoteEnd {
    Stopped,
    Lost(String),
}

impl RemoteEnd {
    /// Where playback is going once remote playback on `device` ends this way.
    fn remote_playback(self, device: RemoteDevice) -> RemotePlayback {
        match self {
            Self::Stopped => RemotePlayback::Local,
            Self::Lost(detail) => RemotePlayback::Ended { device, detail },
        }
    }
}

/// A live AirPlay session driven through the swapped output sink. The decode
/// pipeline runs unchanged; this holds the receiver, the handle on its live
/// session the service drives pause/resume/seek through and reads the receiver
/// latency from, and the local output to restore when AirPlay ends.
pub(super) struct AirPlayRenderer {
    device: RemoteDevice,
    control: Arc<crate::airplay_output::AirPlayControl>,
    /// The local (cpal/aaudio) output, put back when AirPlay playback ends.
    saved_output: crate::audio_output::PlayerOutput,
}

impl AirPlayRenderer {
    pub(super) fn new(
        device: RemoteDevice,
        control: Arc<crate::airplay_output::AirPlayControl>,
        saved_output: crate::audio_output::PlayerOutput,
    ) -> Self {
        Self { device, control, saved_output }
    }

    /// Put the local output back in place of the AirPlay one, playing at
    /// `volume` — the level set while AirPlay played, and its mute — and
    /// name the receiver AirPlay played on.
    fn restore_local_output(
        self,
        audio_output: &mut crate::audio_output::PlayerOutput,
        volume: OutputVolume,
    ) -> RemoteDevice {
        *audio_output = self.saved_output;
        volume.apply_to(audio_output);
        self.device
    }
}

/// Everything a `PlayOnAirPlay` command carries: the sink that opens the
/// receiver session (built off the service thread by bae-cast with the device's
/// address and encryption) and the device. A manual `Debug` keeps
/// `PlaybackCommand`'s derive working.
pub(crate) struct AirPlayConnect {
    sink: Box<dyn crate::airplay_output::AirPlaySink>,
    device: RemoteDevice,
}

impl AirPlayConnect {
    pub(super) fn new(
        sink: Box<dyn crate::airplay_output::AirPlaySink>,
        device: RemoteDevice,
    ) -> Self {
        Self { sink, device }
    }
}

impl std::fmt::Debug for AirPlayConnect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AirPlayConnect").field("device", &self.device).finish_non_exhaustive()
    }
}

/// A live remote-renderer session, the device it drives (named again when the
/// link is lost), and where the device reaches this library's media (to mint
/// each track's media as the queue advances). The device's position lives with
/// the current track in the slot, which every status, seek and load updates;
/// local playback resumes there when remote playback ends.
pub(super) struct RemoteRenderer {
    device: RemoteDevice,
    session: RendererSession,
    media_source: RendererMediaSource,
}

impl RemoteRenderer {
    /// Load `audio` onto the device showing `display`, from `position`, and
    /// paused when `paused`. The device shows `display` until the next load.
    /// The error is why its media URL could not be built.
    fn load(
        &self,
        audio: &ResolvedTrackAudio,
        transcode: bae_preferences::CastTranscodeFormat,
        prepared: &PlaybackPreparedTrack,
        display: crate::TrackDisplay,
        position: StreamPosition,
        paused: bool,
    ) -> Result<(), String> {
        // Serving is resolved here, where the track's stored audio is known, so
        // the URL and the MIME declared below come from the same decision.
        let served =
            self.media_source.serve_track(audio, transcode, display.cover_image.as_ref())?;
        self.session.load(RendererMedia {
            url: served.url,
            content_type: served.content_type,
            title: display.title,
            artist: display.artist_names,
            album: display.album_title,
            cover_url: served.cover_url,
            duration: prepared.timeline.stream_duration(),
        });
        if position > StreamPosition::START {
            self.session.seek(position.as_duration());
        }
        if paused {
            self.session.pause();
        }
        Ok(())
    }
}

/// Everything a `PlayOn` command carries to start remote playback: the connected
/// channel (built off the service thread by bae-cast), the device, and where
/// that device reaches this library's media. A manual `Debug` keeps
/// `PlaybackCommand`'s derive working without the un-`Debug` channel/closures.
pub(crate) struct RemoteConnect {
    channel: Box<dyn crate::renderer::RendererChannel>,
    device: RemoteDevice,
    media_source: RendererMediaSource,
}

impl RemoteConnect {
    pub(super) fn new(
        channel: Box<dyn crate::renderer::RendererChannel>,
        device: RemoteDevice,
        media_source: RendererMediaSource,
    ) -> Self {
        Self { channel, device, media_source }
    }
}

impl std::fmt::Debug for RemoteConnect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteConnect").field("device", &self.device).finish_non_exhaustive()
    }
}

/// The event callback the session reports through: it dispatches each session
/// event back onto the service's command loop as a `RemoteEvent` command, so
/// event handling stays serial with every other command.
pub(super) fn event_callback(command_tx: OwnCommands) -> crate::renderer::EventCallback {
    Arc::new(move |event: RendererSessionEvent| {
        dispatch_command(&command_tx, PlaybackCommand::RemoteEvent(event));
    })
}

/// A no-op stand-in for the local decoder while playing remotely: the thread
/// exits at once and the token is never read. It keeps `CurrentTrack`'s shape
/// (one always-present decoder) without a real decode running.
pub(super) fn stub_decoder(diagnostics: &Diagnostics) -> std::io::Result<TrackDecoder> {
    Ok(TrackDecoder {
        handle: diagnostics.spawn_thread_reported(
            "remote playback's local decoder",
            || {},
            |_| {},
        )?,
        cancel_token: Arc::new(AtomicBool::new(false)),
    })
}

impl PlaybackService {
    /// Switch to remote playback: capture the current track and position, tear the
    /// local pipeline down (keeping the queue), start the session, and reissue the
    /// current track to the device at its position.
    pub(super) async fn handle_play_on(&mut self, connect: RemoteConnect) {
        let RemoteConnect { channel, device, media_source } = connect;

        let current = self.current_track_position();
        // Carry the current play/pause intent to the device.
        let target = self.current_play_target();
        // Seed the device with what the user hears, so it matches (0 while muted).
        let volume = self.volume.audible_level();

        // Stop the local pipeline (decoder, cpal stream, byte buffers); the
        // queue and its cursor are untouched.
        self.teardown_local_playback();

        let session = match RendererSession::start(
            &self.diagnostics,
            channel,
            event_callback(self.command_tx.clone()),
        ) {
            Ok(session) => session,
            Err(error) => {
                error!("remote: failed to start the renderer session's thread: {error}");
                emit_progress(
                    &self.progress_tx,
                    PlaybackProgress::PlaybackError {
                        reason: crate::PlaybackErrorReason::internal(error),
                    },
                );
                self.stop().await;
                return;
            }
        };
        session.set_volume(volume);
        self.renderer =
            Renderer::Remote(RemoteRenderer { device: device.clone(), session, media_source });
        self.hand_over_to_device(device, current, target).await;
    }

    /// The tail both device handoffs share: announce which device is playing,
    /// then reissue what the local pipeline was playing at the position its
    /// stream had reached. Nothing playing leaves the device armed but idle.
    async fn hand_over_to_device(
        &mut self,
        device: RemoteDevice,
        current: Option<(String, StreamPosition)>,
        target: PlayTarget,
    ) {
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::RemoteStatusChanged { remote: RemotePlayback::On(device) },
        );

        match current {
            Some((track_id, position)) => {
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

    /// User-initiated stop of device playback: end the session and resume local
    /// playback paused at the last position — for whichever device flavor is live.
    pub(super) async fn handle_stop_remote(&mut self) {
        if self.renderer.is_airplay() {
            self.end_airplay_and_resume_local(RemoteEnd::Stopped).await;
        } else {
            self.end_remote_and_resume_local(RemoteEnd::Stopped).await;
        }
    }

    /// Apply one session event. Ignored when playing locally (a stale event
    /// from a session that already ended). A status reconciles the position
    /// feed and the slot phase to the device; a refused command is reported to
    /// the person; the session's end — the link lost — resumes local playback
    /// and says why.
    pub(super) async fn handle_remote_event(&mut self, event: RendererSessionEvent) {
        if !self.renderer.is_remote() {
            return;
        }
        match event {
            RendererSessionEvent::Status(status) => self.apply_remote_status(status).await,
            RendererSessionEvent::CommandFailed(error) => emit_progress(
                &self.progress_tx,
                PlaybackProgress::PlaybackError {
                    reason: crate::PlaybackErrorReason::renderer(error),
                },
            ),
            RendererSessionEvent::Ended(error) => {
                self.end_remote_and_resume_local(RemoteEnd::Lost(error.to_string())).await;
            }
        }
    }

    /// End the remote session and resume the local renderer, paused at the
    /// current track's position on the device.
    async fn end_remote_and_resume_local(&mut self, end: RemoteEnd) {
        let Renderer::Remote(remote) = &self.renderer else {
            // AirPlay ends through `end_airplay_and_resume_local`, not here.
            return;
        };
        let remote_playback = end.remote_playback(remote.device.clone());
        let current = self.current_track_position();

        // Dropping the session tells the device to stop, when the link is
        // still up to take it, and ends its poll thread; back to local.
        self.renderer = Renderer::Local;
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::RemoteStatusChanged { remote: remote_playback },
        );
        self.teardown_local_playback();

        match current {
            Some((track_id, last_position)) => {
                self.play_track(
                    &track_id,
                    TrackStart::Position(last_position),
                    PlayTarget::Paused(PausePhase::Manual),
                    TrackTransition::Manual,
                )
                .await;
            }
            None => self.stop().await,
        }
    }

    /// Reconcile the slot and the progress feed to a non-terminal device status:
    /// emit a position update, and drive queue-advance / phase changes off the
    /// device's player state.
    async fn apply_remote_status(&mut self, status: ReceiverStatus) {
        let PlaybackSlot::Active(cur) = &mut self.slot else {
            return;
        };
        let track_id = cur.prepared.track_id.clone();
        let was_playing = matches!(cur.phase, TrackPhase::Playing);

        // Feed the shared progress channel so every UI and the position store
        // update exactly as they do for local playback. The device plays the
        // track's stream from its first sample (`RendererMediaSource::
        // serve_track`), so its position is a stream position.
        if let Some(position) = status.position.map(StreamPosition::from_duration) {
            cur.position = position;
            let timeline = cur.prepared.timeline;
            emit_progress(
                &self.progress_tx,
                PlaybackProgress::position_update(timeline, track_id.clone(), position),
            );
        }

        if matches!(status.player_state, RendererPlayerState::Playing) {
            self.set_remote_phase(TrackPhase::Playing);
            if !self.begin_listening().await {
                self.stop().await;
                return;
            }
        }
        if let Some(position) = status.position.map(StreamPosition::from_duration) {
            self.observe_remote_listening(position, was_playing);
            self.checkpoint_listening().await;
        }
        match status.player_state {
            RendererPlayerState::Finished => {
                // The device reached end-of-track: advance the shared queue
                // through the one advance entry point, exactly like local
                // end-of-track.
                if let PlaybackSlot::Active(cur) = &mut self.slot {
                    cur.phase = TrackPhase::Completed;
                }
                self.finish_listening().await;
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
        if !matches!(start, TrackStart::Position(_)) {
            self.finish_listening().await;
        }

        self.slot =
            PlaybackSlot::Loading(LoadingTrack::Unprepared { track_id: track_id.to_string() });
        self.emit_state();

        let resolved = match ResolvedTrackAudio::resolve(&self.records, track_id).await {
            Ok(resolved) => resolved,
            Err(e) => {
                error!("remote: failed to resolve track {track_id}: {e}");
                self.fail_remote(PlaybackError::from(e).ui_reason()).await;
                return;
            }
        };
        // What the device shows is read now, as the library holds it at load,
        // so an edit made while an earlier track played reaches this one. A
        // device keeps what it was loaded with until the next load.
        let display = match crate::tables::track_display(&self.store, track_id).await {
            Ok(Some(display)) => display,
            Ok(None) => {
                error!("remote: track {track_id} is gone from the library");
                self.fail_remote(PlaybackError::not_found("Track", track_id).ui_reason()).await;
                return;
            }
            Err(e) => {
                error!("remote: failed to read what track {track_id} shows: {e}");
                self.fail_remote(PlaybackError::database(e).ui_reason()).await;
                return;
            }
        };
        let prefs = match bae_preferences::preferences(&self.store).await {
            Ok(prefs) => prefs,
            Err(e) => {
                error!("remote: failed to read the preferences to load track {track_id}: {e}");
                self.fail_remote(PlaybackError::database(e).ui_reason()).await;
                return;
            }
        };
        let prepared = finalize_playback_track(
            track_id.to_string(),
            &resolved,
            Vec::new(),
            prefs.replay_gain_mode,
        );

        let Renderer::Remote(remote) = &self.renderer else {
            // Raced out of remote playback before the resolve returned.
            return;
        };
        let start_position = start.position(prepared.timeline);
        let paused = matches!(target, PlayTarget::Paused(_));
        if let Err(reason) = remote.load(
            &resolved,
            prefs.cast_transcode_format,
            &prepared,
            display,
            start_position,
            paused,
        ) {
            error!("remote: failed to mint media URL for {track_id}: {reason}");
            self.fail_remote(crate::PlaybackErrorReason::internal(format!(
                "couldn't build the audio URL for the renderer: {reason}"
            )))
            .await;
            return;
        }

        let decoder = match stub_decoder(&self.diagnostics) {
            Ok(decoder) => decoder,
            Err(error) => {
                error!("remote: failed to start the stand-in decoder for {track_id}: {error}");
                self.fail_remote(crate::PlaybackErrorReason::internal(error)).await;
                return;
            }
        };
        self.install_active_track(prepared, decoder, target.into_track_phase(), start_position);
        if !self.begin_listening().await {
            self.stop().await;
            return;
        }
        self.reset_remote_listening_position(start_position);
        self.emit_state();
        self.persist_playback_state().await;
    }

    /// A remote track load failed: surface the error and stop remote playback,
    /// dropping back to a stopped local renderer.
    async fn fail_remote(&mut self, reason: crate::PlaybackErrorReason) {
        emit_progress(&self.progress_tx, PlaybackProgress::PlaybackError { reason });
        self.renderer = Renderer::Local;
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::RemoteStatusChanged { remote: RemotePlayback::Local },
        );
        self.stop().await;
    }

    /// Switch to AirPlay: keep decoding locally but swap the output sink so the
    /// decoded PCM is pushed to the receiver instead of the DAC. The queue, the
    /// slot, the decoder, and the advance path are unchanged — only where the
    /// audio goes changes, so `play_track` takes its ordinary local branch.
    pub(super) async fn handle_play_on_airplay(&mut self, connect: AirPlayConnect) {
        let AirPlayConnect { sink, device } = connect;

        let current = self.current_track_position();
        let target = self.current_play_target();
        // Seed the device with what the user hears, so it matches (0 while muted).
        let volume = self.volume.audible_level();

        // Tear the current local pipeline down; `play_track` rebuilds it below,
        // this time feeding the AirPlay sink.
        self.teardown_local_playback();

        let control = Arc::new(crate::airplay_output::AirPlayControl::new());
        let command_tx = self.command_tx.clone();
        let failed = crate::airplay_output::AirPlayFailed::new(move || {
            dispatch_command(&command_tx, PlaybackCommand::AirPlayStreamFailed)
        });
        let airplay_output = crate::audio_output::PlayerOutput::new(
            Box::new(crate::airplay_output::AirPlayOutput::new(sink, control.clone(), failed)),
            crate::audio_output::AudioOutputControls::new(volume),
        );
        let saved_output = std::mem::replace(&mut self.audio_output, airplay_output);
        self.renderer =
            Renderer::AirPlay(AirPlayRenderer::new(device.clone(), control, saved_output));
        self.hand_over_to_device(device, current, target).await;
    }

    /// End AirPlay playback: drop the AirPlay output (which tears the receiver
    /// session down), restore the saved local output, and resume local playback
    /// paused at the last position.
    pub(super) async fn end_airplay_and_resume_local(&mut self, end: RemoteEnd) {
        if !self.renderer.is_airplay() {
            return;
        }
        let current = self.current_track_position();
        self.retire_current_track();
        self.discard_buffered_listening();
        let airplay = match std::mem::replace(&mut self.renderer, Renderer::Local) {
            Renderer::AirPlay(airplay) => airplay,
            other => {
                self.renderer = other;
                return;
            }
        };
        // Dropping the current output (the AirPlay stream) tears the receiver
        // session down; then restore the local sink.
        self.teardown_local_playback();
        let device = airplay.restore_local_output(&mut self.audio_output, self.volume);
        let remote_playback = end.remote_playback(device);
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::RemoteStatusChanged { remote: remote_playback },
        );

        match current {
            Some((track_id, last_position)) => {
                self.play_track(
                    &track_id,
                    TrackStart::Position(last_position),
                    PlayTarget::Paused(PausePhase::Manual),
                    TrackTransition::Manual,
                )
                .await;
            }
            None => self.stop().await,
        }
    }

    /// Tear down the local audio pipeline — the current decoder, the preload, the
    /// cpal output, and the file buffers — without emitting or persisting. Shared
    /// by `stop` (which then emits Stopped) and the remote switch (which then
    /// installs the remote track).
    pub(super) fn teardown_local_playback(&mut self) {
        self.discard_buffered_listening();
        self.stop_preview_for_main_playback();
        self.discard_current_track();
        self.clear_next_track_state();
        self.discard_output();
        self.audio_output.playback_ended();
        self.file_buffers.cancel_all();
        self.reset_starvation_episode();
    }
}

#[cfg(test)]
#[path = "renderer_tests.rs"]
mod tests;
