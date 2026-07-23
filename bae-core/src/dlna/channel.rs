//! The SOAP wire to one connected UPnP MediaRenderer.
//!
//! [`DlnaChannel`] is the UPnP implementation of the shared
//! [`RendererChannel`]: it POSTs the [`soap`] envelopes to the device's control
//! URLs and reads status by polling. Unlike Cast there is no persistent
//! connection to establish — each action is an independent blocking HTTP POST, so
//! a connect failure surfaces per-call as a terminal [`RendererError::Connection`]
//! (the session then ends and resumes local playback).
//!
//! End-of-track is inferred here, because UPnP `GetTransportInfo` reports a bare
//! `STOPPED` with no reason: a STOPPED seen after the renderer has played through
//! to (near) the track's end is the natural end that advances the queue, while a
//! STOPPED right after our own `stop()` is not. The channel tracks just enough
//! state to tell them apart.

use std::time::Duration;

use tracing::debug;

use crate::renderer::{
    ReceiverStatus, RendererChannel, RendererError, RendererMedia, RendererPlayerState,
};

use super::soap::{self, DidlMetadata, PositionInfo, SoapRequest, TransportState};

/// How close to the reported duration a stopped renderer must have reached for
/// the stop to count as a natural end rather than a mid-track halt.
const END_OF_TRACK_SLACK: Duration = Duration::from_secs(5);

/// Timeout for a single SOAP action POST.
const ACTION_TIMEOUT: Duration = Duration::from_secs(5);

/// A connected UPnP renderer, driven by SOAP over blocking HTTP on the session
/// thread.
pub struct DlnaChannel {
    client: reqwest::blocking::Client,
    /// The AVTransport control URL (load/play/pause/seek/stop, the status polls).
    av_transport_url: String,
    /// The RenderingControl control URL (volume), when the renderer has the
    /// service. `None` renderers silently ignore a volume set.
    rendering_control_url: Option<String>,
    /// End-of-track detection state, reset on each `load`.
    playback: PlaybackTracking,
}

/// The channel-local state that distinguishes a natural end from other stops.
#[derive(Default)]
struct PlaybackTracking {
    /// The renderer has reported PLAYING at least once since the last load.
    has_played: bool,
    /// We issued `stop()`; the STOPPED that follows is ours, not an end-of-track.
    stopped_by_us: bool,
    /// Set once end-of-track has been reported, so it fires exactly once (the
    /// renderer keeps reporting STOPPED after the track ends).
    end_reported: bool,
    /// The renderer has reported a position (any `RelTime`) at least once since
    /// the last load. `false` for a renderer that only ever answers
    /// `RelTime: NOT_IMPLEMENTED` — the position rule can't apply to it.
    has_reported_position: bool,
    /// The last position observed while not stopped — the renderer resets RelTime
    /// to zero on stop, so the pre-stop value is what says whether it played
    /// through.
    last_position: Option<Duration>,
    /// The track's duration, from `GetPositionInfo` (falling back to the LOAD's).
    duration: Option<Duration>,
}

impl DlnaChannel {
    /// Build a channel for a renderer at the given control URLs. There is no
    /// network handshake — the first `load` sends the first SOAP action.
    pub fn connect(
        av_transport_url: String,
        rendering_control_url: Option<String>,
    ) -> Result<Self, RendererError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(ACTION_TIMEOUT)
            .build()
            .map_err(|e| RendererError::Connection(e.to_string()))?;
        Ok(Self {
            client,
            av_transport_url,
            rendering_control_url,
            playback: PlaybackTracking::default(),
        })
    }

    /// POST a built SOAP request to `control_url`. A transport failure (refused,
    /// timeout, unreachable) is a terminal [`RendererError::Connection`]; a SOAP
    /// fault (HTTP error status) is a non-terminal [`RendererError::Command`].
    fn post(&self, control_url: &str, request: SoapRequest) -> Result<String, RendererError> {
        let response = self
            .client
            .post(control_url)
            .header("Content-Type", "text/xml; charset=\"utf-8\"")
            .header("SOAPACTION", request.soap_action)
            .body(request.body)
            .send()
            .map_err(|e| RendererError::Connection(e.to_string()))?;
        let status = response.status();
        let body = response
            .text()
            .map_err(|e| RendererError::Connection(e.to_string()))?;
        if status.is_success() {
            Ok(body)
        } else {
            Err(RendererError::Command(format!(
                "renderer returned {status} to a SOAP action"
            )))
        }
    }

    /// Map a parsed transport state to the player state the session consumes,
    /// updating end-of-track tracking. A STOPPED is the natural end only when the
    /// renderer played through to near the track's end and we didn't stop it.
    fn player_state(&mut self, transport: TransportState) -> RendererPlayerState {
        match transport {
            TransportState::Playing => {
                self.playback.has_played = true;
                self.playback.stopped_by_us = false;
                RendererPlayerState::Playing
            }
            TransportState::PausedPlayback => RendererPlayerState::Paused,
            TransportState::Transitioning => RendererPlayerState::Buffering,
            TransportState::Stopped => self.stopped_player_state(),
            // No media / unknown: idle, not a queue-advance signal.
            TransportState::NoMediaPresent | TransportState::Other => RendererPlayerState::Idle,
        }
    }

    /// Classify a STOPPED transport state.
    fn stopped_player_state(&mut self) -> RendererPlayerState {
        if self.playback.stopped_by_us || !self.playback.has_played || self.playback.end_reported {
            return RendererPlayerState::Idle;
        }
        if self.is_end_of_track() {
            self.playback.end_reported = true;
            RendererPlayerState::Finished
        } else {
            // Stopped mid-track by something other than us (e.g. the device's own
            // remote): report idle rather than advancing the queue.
            RendererPlayerState::Idle
        }
    }

    /// Whether a STOPPED (after playing, not by us) is a natural end.
    fn is_end_of_track(&self) -> bool {
        if !self.playback.has_reported_position {
            // Position-less renderer (only ever `RelTime: NOT_IMPLEMENTED`): we
            // genuinely cannot tell a natural end from a device-remote mid-track
            // stop. Choose end-of-track so auto-advance works on these renderers;
            // the cost is a rare spurious advance when someone stops from the
            // device's own remote.
            debug!(
                "dlna: STOPPED with no position ever reported; treating as end-of-track \
                 (auto-advance; a device-remote stop can't be told apart here)"
            );
            return true;
        }
        // A position was reported, so the position rule applies: a natural end
        // reaches within slack of a known duration. An unknown duration (never
        // reported, or reported as zero while loading) can't confirm an end, so a
        // stop off it is treated as mid-track, not finished.
        match (self.playback.last_position, self.playback.duration) {
            (Some(position), Some(duration)) => position + END_OF_TRACK_SLACK >= duration,
            _ => false,
        }
    }
}

impl RendererChannel for DlnaChannel {
    fn load(&mut self, media: &RendererMedia) -> Result<(), RendererError> {
        self.playback = PlaybackTracking {
            duration: media.duration,
            ..PlaybackTracking::default()
        };
        let metadata = DidlMetadata {
            title: &media.title,
            artist: &media.artist,
            album: &media.album,
            cover_url: media.cover_url.as_deref(),
            content_type: &media.content_type,
        };
        self.post(
            &self.av_transport_url.clone(),
            soap::set_av_transport_uri(&media.url, &metadata),
        )?;
        self.post(&self.av_transport_url.clone(), soap::play())?;
        Ok(())
    }

    fn play(&mut self) -> Result<(), RendererError> {
        self.post(&self.av_transport_url.clone(), soap::play())
            .map(|_| ())
    }

    fn pause(&mut self) -> Result<(), RendererError> {
        self.post(&self.av_transport_url.clone(), soap::pause())
            .map(|_| ())
    }

    fn seek(&mut self, position: Duration) -> Result<(), RendererError> {
        self.post(&self.av_transport_url.clone(), soap::seek(position))
            .map(|_| ())
    }

    fn set_volume(&mut self, level: f32) -> Result<(), RendererError> {
        let Some(control_url) = self.rendering_control_url.clone() else {
            debug!("dlna: renderer has no RenderingControl; volume set ignored");
            return Ok(());
        };
        self.post(&control_url, soap::set_volume(level)).map(|_| ())
    }

    fn stop(&mut self) -> Result<(), RendererError> {
        // Mark the stop as ours so the STOPPED it produces isn't misread as an
        // end-of-track that would advance the queue.
        self.playback.stopped_by_us = true;
        self.post(&self.av_transport_url.clone(), soap::stop())
            .map(|_| ())
    }

    fn poll_status(&mut self) -> Result<ReceiverStatus, RendererError> {
        // Transport state is required (a connection failure ends the session);
        // position is best-effort (a renderer that faults GetPositionInfo still
        // reports a usable state).
        let transport_body =
            self.post(&self.av_transport_url.clone(), soap::get_transport_info())?;
        let transport = soap::parse_transport_state(&transport_body);

        let position_info = match self
            .post(&self.av_transport_url.clone(), soap::get_position_info())
        {
            Ok(body) => soap::parse_position_info(&body),
            Err(error) => {
                debug!("dlna: GetPositionInfo failed (position unavailable this poll): {error}");
                PositionInfo::default()
            }
        };
        if let Some(duration) = position_info.track_duration {
            self.playback.duration = Some(duration);
        }
        if let Some(position) = position_info.rel_time {
            // The renderer reports a position, so the position rule can apply to
            // its stops (a position-less renderer never reaches here).
            self.playback.has_reported_position = true;
            // Record the position only while not stopped: renderers zero RelTime
            // on stop, and the pre-stop value is what end-of-track needs.
            if !matches!(transport, TransportState::Stopped) {
                self.playback.last_position = Some(position);
            }
        }

        let player_state = self.player_state(transport);
        Ok(ReceiverStatus {
            player_state,
            position: position_info.rel_time,
            duration: self.playback.duration,
            // Volume isn't polled (it would be a second SOAP round-trip each
            // tick); carried as absent rather than invented.
            volume: None,
        })
    }
}
