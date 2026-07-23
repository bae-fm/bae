//! The wire to one connected Cast device.
//!
//! [`CastChannel`] is the seam the session drives: connect once, then
//! LOAD/PLAY/PAUSE/SEEK/volume/STOP and read status. Tests supply a fake
//! implementation; [`RustCastChannel`] is the one real implementation, over the
//! `rust_cast` CASTV2 protocol.

use std::fmt;
use std::net::IpAddr;
use std::time::Duration;

/// The media a LOAD hands the receiver: the URL it fetches over HTTP and the
/// metadata it shows on-screen.
#[derive(Debug, Clone, PartialEq)]
pub struct CastMedia {
    /// The HTTP URL the receiver fetches the audio from.
    pub url: String,
    /// The MIME type of `url`'s bytes (`audio/flac`, `audio/mpeg`, …).
    pub content_type: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// An HTTP URL to cover art, shown on TV receivers. `None` when the track
    /// has no cover.
    pub cover_url: Option<String>,
    /// The track's total duration, when known.
    pub duration: Option<Duration>,
}

/// What the receiver's media player is doing, read from its media status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastPlayerState {
    /// No media is playing for a non-terminal reason — the player is idle before
    /// the first LOAD, or between a LOAD and its buffering, or a load was
    /// cancelled/interrupted. Not a queue-advance signal.
    Idle,
    /// The loaded media reached its natural end (receiver IDLE with reason
    /// FINISHED). This is the queue-advance signal.
    Finished,
    /// The player is filling its buffer; position is not advancing yet.
    Buffering,
    Playing,
    Paused,
}

/// A snapshot of the receiver's state from one `poll_status`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReceiverStatus {
    pub player_state: CastPlayerState,
    /// The receiver's playback position into the current media, when reported.
    pub position: Option<Duration>,
    /// The current media's duration, when the receiver reports it.
    pub duration: Option<Duration>,
    /// Receiver volume level, 0.0–1.0.
    pub volume: f32,
}

/// A failure talking to the receiver. `Connection` is terminal — it means the
/// link to the device is gone (the receiver app was stopped, the network
/// dropped), which the session reads as the cast session ending.
#[derive(Debug)]
pub enum CastError {
    /// The TLS/TCP link could not be established or was lost.
    Connection(String),
    /// Launching the media receiver application failed.
    Launch(String),
    /// A media/receiver command (load, play, seek, …) failed.
    Command(String),
}

impl fmt::Display for CastError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CastError::Connection(detail) => write!(f, "cast connection failed: {detail}"),
            CastError::Launch(detail) => write!(f, "cast receiver launch failed: {detail}"),
            CastError::Command(detail) => write!(f, "cast command failed: {detail}"),
        }
    }
}

impl std::error::Error for CastError {}

/// The operations the session performs against a connected receiver. Each call
/// blocks on the wire, so the session drives it from its own thread. A
/// [`CastError::Connection`] from any call is terminal (the session ends);
/// tests fake this to record commands and hand back canned statuses.
pub trait CastChannel: Send {
    /// Load `media` on the receiver and start it playing.
    fn load(&mut self, media: &CastMedia) -> Result<(), CastError>;
    fn play(&mut self) -> Result<(), CastError>;
    fn pause(&mut self) -> Result<(), CastError>;
    fn seek(&mut self, position: Duration) -> Result<(), CastError>;
    /// Set the receiver volume to `level` (0.0–1.0).
    fn set_volume(&mut self, level: f32) -> Result<(), CastError>;
    /// Stop playback on the receiver (the loaded media is unloaded).
    fn stop(&mut self) -> Result<(), CastError>;
    /// Read the receiver's current status.
    fn poll_status(&mut self) -> Result<ReceiverStatus, CastError>;
}

/// The one real [`CastChannel`], over `rust_cast`. Owned and driven entirely on
/// the session's thread — `rust_cast`'s channel calls block on the socket, so
/// this is never shared across threads.
///
/// Heartbeat handling is best-effort: each `poll_status` sends a PONG to keep
/// the receiver's liveness check satisfied. The live protocol path is not
/// exercisable without a physical device; the session's logic is tested against
/// a fake channel instead.
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
    pub fn connect(host: IpAddr, port: u16) -> Result<Self, CastError> {
        // Cast devices present self-signed certificates, so host verification is
        // skipped — the connection is LAN-local and authenticated by the media
        // URL's own credential, not TLS identity.
        let device =
            rust_cast::CastDevice::connect_without_host_verification(host.to_string(), port)
                .map_err(|e| CastError::Connection(e.to_string()))?;

        device
            .connection
            .connect(RECEIVER_ID.to_string())
            .map_err(|e| CastError::Connection(e.to_string()))?;

        let app = device
            .receiver
            .launch_app(&MEDIA_RECEIVER_APP)
            .map_err(|e| CastError::Launch(e.to_string()))?;

        // Open a virtual connection to the launched app's transport so media
        // commands reach it.
        device
            .connection
            .connect(app.transport_id.clone())
            .map_err(|e| CastError::Connection(e.to_string()))?;

        Ok(Self {
            device,
            transport_id: app.transport_id,
            session_id: app.session_id,
            media_session_id: None,
        })
    }

    fn require_media_session(&self) -> Result<i32, CastError> {
        self.media_session_id
            .ok_or_else(|| CastError::Command("no media loaded on the receiver yet".to_string()))
    }
}

impl CastChannel for RustCastChannel {
    fn load(&mut self, media: &CastMedia) -> Result<(), CastError> {
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
            .map_err(|e| CastError::Command(e.to_string()))?;

        self.media_session_id = status.entries.first().map(|entry| entry.media_session_id);
        Ok(())
    }

    fn play(&mut self) -> Result<(), CastError> {
        let media_session_id = self.require_media_session()?;
        self.device
            .media
            .play(self.transport_id.clone(), media_session_id)
            .map(|_| ())
            .map_err(|e| CastError::Command(e.to_string()))
    }

    fn pause(&mut self) -> Result<(), CastError> {
        let media_session_id = self.require_media_session()?;
        self.device
            .media
            .pause(self.transport_id.clone(), media_session_id)
            .map(|_| ())
            .map_err(|e| CastError::Command(e.to_string()))
    }

    fn seek(&mut self, position: Duration) -> Result<(), CastError> {
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
            .map_err(|e| CastError::Command(e.to_string()))
    }

    fn set_volume(&mut self, level: f32) -> Result<(), CastError> {
        self.device
            .receiver
            .set_volume(level.clamp(0.0, 1.0))
            .map(|_| ())
            .map_err(|e| CastError::Command(e.to_string()))
    }

    fn stop(&mut self) -> Result<(), CastError> {
        let media_session_id = self.require_media_session()?;
        self.device
            .media
            .stop(self.transport_id.clone(), media_session_id)
            .map(|_| ())
            .map_err(|e| CastError::Command(e.to_string()))
    }

    fn poll_status(&mut self) -> Result<ReceiverStatus, CastError> {
        // Keep the receiver's sender-liveness check satisfied. rust_cast's
        // blocking status read does not answer the device's PINGs, so send an
        // unsolicited PONG each cycle.
        let _ = self.device.heartbeat.pong();

        let receiver_status = self
            .device
            .receiver
            .get_status()
            .map_err(|e| CastError::Connection(e.to_string()))?;
        let volume = receiver_status.volume.level.unwrap_or(1.0);

        let Some(media_session_id) = self.media_session_id else {
            // Nothing loaded yet: the receiver is idle, volume is all we have.
            return Ok(ReceiverStatus {
                player_state: CastPlayerState::Idle,
                position: None,
                duration: None,
                volume,
            });
        };

        let media_status = self
            .device
            .media
            .get_status(self.transport_id.clone(), Some(media_session_id))
            .map_err(|e| CastError::Connection(e.to_string()))?;

        let Some(entry) = media_status.entries.first() else {
            // The media session is gone (the receiver dropped the media) but the
            // connection survives: report idle rather than ending the session.
            return Ok(ReceiverStatus {
                player_state: CastPlayerState::Idle,
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
fn player_state_from(entry: &rust_cast::channels::media::StatusEntry) -> CastPlayerState {
    use rust_cast::channels::media::{IdleReason, PlayerState};
    match entry.player_state {
        PlayerState::Playing => CastPlayerState::Playing,
        PlayerState::Paused => CastPlayerState::Paused,
        PlayerState::Buffering => CastPlayerState::Buffering,
        PlayerState::Idle => match entry.idle_reason {
            Some(IdleReason::Finished) => CastPlayerState::Finished,
            _ => CastPlayerState::Idle,
        },
    }
}
