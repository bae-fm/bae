//! The CASTV2 wire to one connected Cast device.
//!
//! [`RustCastChannel`] is the Cast implementation of the shared
//! [`RendererChannel`] the session drives: connect once, then
//! LOAD/PLAY/PAUSE/SEEK/volume/STOP and read status, over the `rust_cast` CASTV2
//! protocol.

use std::net::IpAddr;
use std::time::Duration;

use tracing::debug;

use crate::renderer::{
    ReceiverStatus, RendererChannel, RendererError, RendererMedia, RendererPlayerState,
};

/// The one real Cast [`RendererChannel`], over `rust_cast`. Owned and driven
/// entirely on the session's thread — `rust_cast`'s channel calls block on the
/// socket, so this is never shared across threads.
///
/// Heartbeat handling is best-effort: each `poll_status` sends a PONG to keep the
/// receiver's liveness check satisfied. The live protocol path is not
/// exercisable without a physical device; the session's logic is tested against a
/// fake channel instead.
pub struct RustCastChannel {
    device: rust_cast::CastDevice<'static>,
    /// The launched media receiver's virtual-connection transport id — the
    /// destination every media command addresses.
    transport_id: String,
    session_id: String,
    /// The media session id the last LOAD established; every play/pause/seek/stop
    /// addresses it. `None` before the first successful LOAD.
    media_session_id: Option<i32>,
}

/// The default media receiver application id (Google's built-in player).
const MEDIA_RECEIVER_APP: rust_cast::channels::receiver::CastDeviceApp =
    rust_cast::channels::receiver::CastDeviceApp::DefaultMediaReceiver;

const RECEIVER_ID: &str = "receiver-0";

impl RustCastChannel {
    /// Connect to the device at `host:port`, open the virtual connection, and
    /// launch the media receiver so it is ready for a LOAD. Blocks on the
    /// network; the caller runs it off the async runtime.
    pub fn connect(host: IpAddr, port: u16) -> Result<Self, RendererError> {
        // Cast devices present self-signed certificates, so host verification is
        // skipped — the connection is LAN-local and authenticated by the media
        // URL's own credential, not TLS identity.
        let device =
            rust_cast::CastDevice::connect_without_host_verification(host.to_string(), port)
                .map_err(|e| RendererError::Connection(e.to_string()))?;

        device
            .connection
            .connect(RECEIVER_ID.to_string())
            .map_err(|e| RendererError::Connection(e.to_string()))?;

        let app = device
            .receiver
            .launch_app(&MEDIA_RECEIVER_APP)
            .map_err(|e| RendererError::Launch(e.to_string()))?;

        // Open a virtual connection to the launched app's transport so media
        // commands reach it.
        device
            .connection
            .connect(app.transport_id.clone())
            .map_err(|e| RendererError::Connection(e.to_string()))?;

        Ok(Self {
            device,
            transport_id: app.transport_id,
            session_id: app.session_id,
            media_session_id: None,
        })
    }

    fn require_media_session(&self) -> Result<i32, RendererError> {
        self.media_session_id.ok_or_else(|| {
            RendererError::Command("no media loaded on the receiver yet".to_string())
        })
    }
}

impl RendererChannel for RustCastChannel {
    fn load(&mut self, media: &RendererMedia) -> Result<(), RendererError> {
        use rust_cast::channels::media::{
            Image, Media, Metadata, MusicTrackMediaMetadata, StreamType,
        };

        let metadata = Metadata::MusicTrack(MusicTrackMediaMetadata {
            album_name: Some(media.album.clone()),
            title: Some(media.title.clone()),
            album_artist: Some(media.artist.clone()),
            artist: Some(media.artist.clone()),
            composer: None,
            track_number: None,
            disc_number: None,
            images: media
                .cover_url
                .iter()
                .map(|url| Image::new(url.clone()))
                .collect(),
            release_date: None,
        });

        let request = Media {
            content_id: media.url.clone(),
            stream_type: StreamType::Buffered,
            content_type: media.content_type.clone(),
            metadata: Some(metadata),
            duration: media.duration.map(|d| d.as_secs_f32()),
        };

        let status = self
            .device
            .media
            .load(self.transport_id.clone(), self.session_id.clone(), &request)
            .map_err(|e| RendererError::Command(e.to_string()))?;

        self.media_session_id = status.entries.first().map(|entry| entry.media_session_id);
        Ok(())
    }

    fn play(&mut self) -> Result<(), RendererError> {
        let media_session_id = self.require_media_session()?;
        self.device
            .media
            .play(self.transport_id.clone(), media_session_id)
            .map(|_| ())
            .map_err(|e| RendererError::Command(e.to_string()))
    }

    fn pause(&mut self) -> Result<(), RendererError> {
        let media_session_id = self.require_media_session()?;
        self.device
            .media
            .pause(self.transport_id.clone(), media_session_id)
            .map(|_| ())
            .map_err(|e| RendererError::Command(e.to_string()))
    }

    fn seek(&mut self, position: Duration) -> Result<(), RendererError> {
        let media_session_id = self.require_media_session()?;
        self.device
            .media
            .seek(
                self.transport_id.clone(),
                media_session_id,
                Some(position.as_secs_f32()),
                None,
            )
            .map(|_| ())
            .map_err(|e| RendererError::Command(e.to_string()))
    }

    fn set_volume(&mut self, level: f32) -> Result<(), RendererError> {
        self.device
            .receiver
            .set_volume(level.clamp(0.0, 1.0))
            .map(|_| ())
            .map_err(|e| RendererError::Command(e.to_string()))
    }

    fn stop(&mut self) -> Result<(), RendererError> {
        let media_session_id = self.require_media_session()?;
        self.device
            .media
            .stop(self.transport_id.clone(), media_session_id)
            .map(|_| ())
            .map_err(|e| RendererError::Command(e.to_string()))
    }

    fn poll_status(&mut self) -> Result<ReceiverStatus, RendererError> {
        // Keep the receiver's sender-liveness check satisfied. rust_cast's
        // blocking status read does not answer the device's PINGs, so send an
        // unsolicited PONG each cycle. A pong failure is not terminal here (the
        // status read below surfaces a real disconnect) — log it and continue.
        if let Err(error) = self.device.heartbeat.pong() {
            debug!("cast heartbeat pong failed: {error}");
        }

        let receiver_status = self
            .device
            .receiver
            .get_status()
            .map_err(|e| RendererError::Connection(e.to_string()))?;
        // The receiver's level, carried as-is (`None` when omitted) rather than
        // invented.
        let volume = receiver_status.volume.level;

        let Some(media_session_id) = self.media_session_id else {
            // Nothing loaded yet: the receiver is idle, volume is all we have.
            return Ok(ReceiverStatus {
                player_state: RendererPlayerState::Idle,
                position: None,
                duration: None,
                volume,
            });
        };

        let media_status = self
            .device
            .media
            .get_status(self.transport_id.clone(), Some(media_session_id))
            .map_err(|e| RendererError::Connection(e.to_string()))?;

        let Some(entry) = media_status.entries.first() else {
            // The media session is gone (the receiver dropped the media) but the
            // connection survives: report idle rather than ending the session.
            return Ok(ReceiverStatus {
                player_state: RendererPlayerState::Idle,
                position: None,
                duration: None,
                volume,
            });
        };

        Ok(ReceiverStatus {
            player_state: player_state_from(entry),
            position: entry
                .current_time
                .filter(|t| *t >= 0.0)
                .map(Duration::from_secs_f32),
            duration: entry
                .media
                .as_ref()
                .and_then(|m| m.duration)
                .filter(|d| *d >= 0.0)
                .map(Duration::from_secs_f32),
            volume,
        })
    }
}

/// Map a receiver media-status entry to our player state. An IDLE entry whose
/// reason is FINISHED is the natural-end signal; every other IDLE reason
/// (loading, cancelled, interrupted, error) is a plain idle.
fn player_state_from(entry: &rust_cast::channels::media::StatusEntry) -> RendererPlayerState {
    use rust_cast::channels::media::{IdleReason, PlayerState};
    match entry.player_state {
        PlayerState::Playing => RendererPlayerState::Playing,
        PlayerState::Paused => RendererPlayerState::Paused,
        PlayerState::Buffering => RendererPlayerState::Buffering,
        PlayerState::Idle => match entry.idle_reason {
            Some(IdleReason::Finished) => RendererPlayerState::Finished,
            _ => RendererPlayerState::Idle,
        },
    }
}
