//! The renderer seam: where the playback service sends audio.
//!
//! The service drives one queue; a *renderer* is where the current track plays.
//! [`Renderer::Local`] is the in-process decode + cpal path (the rest of this
//! module); [`Renderer::Cast`] hands playback to a connected Cast device, which
//! fetches audio over HTTP itself. The slot, queue, selection, shuffle, and
//! repeat logic are identical for both — only the transport differs.
//!
//! While casting, the slot still holds the current track (its metadata drives
//! the UI and the shared queue-advance path), but with no local decoder or
//! sparse buffers: the current track carries a no-op [`stub_decoder`] and
//! empty segments, so `play_track`/`handle_auto_advance`/`persist` all work
//! unchanged while the cpal pipeline stays idle.

use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::*;
use crate::cast::{cast_stream_format, CastMedia, CastPlayerState, CastSession, CastSessionStatus};

/// Where the current track plays. `Local` is the default; `Cast` holds the live
/// session and everything needed to keep loading tracks onto the receiver as the
/// queue advances.
pub(super) enum Renderer {
    Local,
    Cast(CastRenderer),
}

impl Renderer {
    pub(super) fn is_casting(&self) -> bool {
        matches!(self, Renderer::Cast(_))
    }
}

/// A live cast session and the state the service needs to keep serving it: the
/// URL providers (to mint each track's media as the queue advances) and the
/// receiver's last reported position (for the handoff back to local playback
/// when casting stops). The device name isn't held here — it rides the
/// `CastStatusChanged` event out to the UI, which caches it.
pub(super) struct CastRenderer {
    pub(super) session: CastSession,
    pub(super) stream_url_provider: crate::cast::MediaUrlProvider,
    pub(super) cover_url_provider: crate::cast::CoverUrlProvider,
    /// The receiver's most recent playback position, updated from each status.
    /// Local playback resumes here when casting ends.
    pub(super) last_position: Duration,
}

/// Everything a `CastTo` command carries to start casting: the connected channel
/// (built off the service thread by bae-desktop), the device's display name, and
/// the injected URL providers. A manual `Debug` keeps `PlaybackCommand`'s derive
/// working without the un-`Debug` channel/closures.
pub(crate) struct CastConnect {
    pub(crate) channel: Box<dyn crate::cast::CastChannel>,
    pub(crate) device_name: String,
    pub(crate) stream_url_provider: crate::cast::MediaUrlProvider,
    pub(crate) cover_url_provider: crate::cast::CoverUrlProvider,
}

impl std::fmt::Debug for CastConnect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CastConnect")
            .field("device_name", &self.device_name)
            .finish_non_exhaustive()
    }
}

/// The status callback the session reports through: it dispatches each receiver
/// status back onto the service's command loop as a `CastStatus` command, so
/// status handling stays serial with every other command.
pub(super) fn status_callback(
    command_tx: tokio_mpsc::UnboundedSender<PlaybackCommand>,
) -> crate::cast::StatusCallback {
    Arc::new(move |status: CastSessionStatus| {
        dispatch_command(&command_tx, PlaybackCommand::CastStatus(status));
    })
}

/// A no-op stand-in for the local decoder while casting: the thread has already
/// exited and the token is never read. It keeps `CurrentTrack`'s shape (one
/// always-present decoder) without a real decode running.
pub(super) fn stub_decoder() -> TrackDecoder {
    TrackDecoder {
        handle: std::thread::spawn(|| {}),
        cancel_token: Arc::new(AtomicBool::new(false)),
    }
}

impl PlaybackService {
    /// Switch to casting: capture the current track and position, tear the local
    /// pipeline down (keeping the queue), start the session, and reissue the
    /// current track to the receiver at its position.
    pub(super) async fn handle_cast_to(&mut self, connect: CastConnect) {
        let CastConnect {
            channel,
            device_name,
            stream_url_provider,
            cover_url_provider,
        } = connect;

        let current = self.current_track_id().map(str::to_string);
        let position = self
            .current_position_shared
            .lock()
            .unwrap()
            .unwrap_or(Duration::ZERO);
        // Carry the current play/pause intent to the receiver.
        let target = self.current_play_target();
        let volume = self.effective_volume();

        // Stop the local pipeline (decoder, cpal stream, byte buffers); the
        // queue and its cursor are untouched.
        self.teardown_local_playback();

        let session = CastSession::start(channel, status_callback(self.command_tx.clone()));
        session.set_volume(volume);
        self.renderer = Renderer::Cast(CastRenderer {
            session,
            stream_url_provider,
            cover_url_provider,
            last_position: position,
        });
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::CastStatusChanged {
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
                // Nothing was playing: casting is armed but idle.
                self.slot = PlaybackSlot::Stopped;
                self.sync_audio_state();
                self.emit_state();
            }
        }
    }

    /// User-initiated stop casting: stop the receiver, then resume local
    /// playback paused at the receiver's last position.
    pub(super) async fn handle_stop_casting(&mut self) {
        self.end_cast_and_resume_local(true).await;
    }

    /// Apply one receiver status. Ignored when not casting (a stale update from a
    /// session that already ended). A terminal `ended` update — a receiver-side
    /// stop — resumes local playback; otherwise the position feed and the slot
    /// phase are reconciled to the receiver.
    pub(super) async fn handle_cast_status(&mut self, status: CastSessionStatus) {
        if !self.renderer.is_casting() {
            return;
        }
        if status.ended {
            info!("cast session ended (receiver-side); resuming local playback");
            self.end_cast_and_resume_local(false).await;
            return;
        }
        if let (Renderer::Cast(cast), Some(position)) = (&mut self.renderer, status.position) {
            cast.last_position = position;
        }
        self.apply_cast_status(status).await;
    }

    /// End the cast session and resume the local renderer, paused at the
    /// receiver's last position. `stop_receiver` stops receiver playback first
    /// (a user-initiated stop); a receiver-side end has already stopped it.
    async fn end_cast_and_resume_local(&mut self, stop_receiver: bool) {
        let last_position = match &self.renderer {
            Renderer::Cast(cast) => {
                if stop_receiver {
                    cast.session.stop();
                }
                cast.last_position
            }
            Renderer::Local => return,
        };
        let current = self.current_track_id().map(str::to_string);

        // Dropping the session ends its poll thread; back to local.
        self.renderer = Renderer::Local;
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::CastStatusChanged { device_name: None },
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

    /// Reconcile the slot and the progress feed to a non-terminal receiver
    /// status: emit a position update, and drive queue-advance / phase changes
    /// off the receiver's player state.
    async fn apply_cast_status(&mut self, status: CastSessionStatus) {
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
            CastPlayerState::Finished => {
                // The receiver reached end-of-track: advance the shared queue
                // through the one advance entry point, exactly like local
                // end-of-track.
                if let PlaybackSlot::Active(cur) = &mut self.slot {
                    cur.phase = TrackPhase::Completed;
                }
                self.handle_auto_advance(track_id).await;
            }
            CastPlayerState::Playing => self.set_cast_phase(TrackPhase::Playing),
            CastPlayerState::Paused => self.set_cast_phase(TrackPhase::Paused(PausePhase::Manual)),
            // Buffering/idle are transient; the installed phase stands.
            CastPlayerState::Buffering | CastPlayerState::Idle => {}
        }
    }

    /// Move the current cast track to `phase` and emit, but only on a real
    /// change (the receiver reports its state every poll).
    fn set_cast_phase(&mut self, phase: TrackPhase) {
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

    /// Load `track_id` onto the receiver: resolve its metadata (no local buffers
    /// or decoder), mint its media URL, and install it as the current track.
    /// The cast branch of `play_track`.
    pub(super) async fn play_track_cast(
        &mut self,
        track_id: &str,
        start: TrackStart,
        target: PlayTarget,
    ) {
        // Tear the previous cast track's slot down (stub decoder, empty
        // segments — nothing to release).
        self.teardown_current_track();

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
                error!("cast: failed to resolve track {track_id}: {e}");
                self.fail_cast(PlaybackError::database(e).into_ui_reason())
                    .await;
                return;
            }
        };
        let content_type = resolved.content_type.clone();
        let replay_gain_mode = self.library_manager.get_config().replay_gain_mode;
        let prepared = finalize_playback_track(resolved, track_info, Vec::new(), replay_gain_mode);

        // The content-type gate is decided here, where the track's type is
        // known, and passed to the provider (which renders the URL) so the URL's
        // format and the declared MIME below never disagree.
        let format = cast_stream_format(&content_type);

        // Build and send the receiver media under the cast renderer's providers.
        let Renderer::Cast(cast) = &self.renderer else {
            // Raced out of casting before the resolve returned; nothing to do.
            return;
        };
        let url = match (cast.stream_url_provider)(track_id, format) {
            Ok(url) => url,
            Err(reason) => {
                error!("cast: failed to mint media URL for {track_id}: {reason}");
                self.fail_cast(crate::ui::PlaybackErrorReason::internal(
                    "Couldn't build the audio URL for the Cast device.",
                ))
                .await;
                return;
            }
        };
        let media = CastMedia {
            url,
            content_type: format.content_type_str(&content_type),
            title: prepared.track_info.track_title.clone(),
            artist: prepared.track_info.artist_names.clone(),
            album: prepared.track_info.album_title.clone(),
            cover_url: (cast.cover_url_provider)(track_id),
            duration: Some(prepared.duration),
        };
        let start_position = start.position(prepared.total_pregap_ms());
        cast.session.load(media);
        if start_position > Duration::ZERO {
            cast.session.seek(start_position);
        }
        if matches!(target, PlayTarget::Paused(_)) {
            cast.session.pause();
        }

        *self.current_position_shared.lock().unwrap() = Some(start_position);
        self.install_active_track(prepared, stub_decoder(), target.into_track_phase());
        self.emit_state();
        self.persist_playback_state().await;
    }

    /// A cast track load failed: surface the error and stop casting, dropping
    /// back to a stopped local renderer.
    async fn fail_cast(&mut self, reason: crate::ui::PlaybackErrorReason) {
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::PlaybackError { reason },
        );
        self.renderer = Renderer::Local;
        emit_progress(
            &self.progress_tx,
            PlaybackProgress::CastStatusChanged { device_name: None },
        );
        self.stop().await;
    }

    /// The current (effective) output volume — 0 while muted, so the receiver is
    /// seeded to match what the user hears.
    fn effective_volume(&self) -> f32 {
        if self.is_muted {
            0.0
        } else {
            self.audio_output.get_volume()
        }
    }

    /// Tear down the local audio pipeline — the current decoder, the preload, the
    /// cpal output, and the file buffers — without emitting or persisting. Shared
    /// by `stop` (which then emits Stopped) and the cast switch (which then
    /// installs the cast track).
    pub(super) fn teardown_local_playback(&mut self) {
        self.stop_preview_for_main_playback();
        self.teardown_current_track();
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
