//! The command wire to one connected remote renderer.
//!
//! A *remote renderer* is a device that fetches a track's audio over HTTP itself
//! and is driven by transport commands — a Cast receiver or a UPnP MediaRenderer.
//! [`RendererChannel`] is the protocol-agnostic seam the session drives: connect
//! once, then LOAD/PLAY/PAUSE/SEEK/volume/STOP and read status. The two real
//! implementations — Cast over CASTV2 (`crate::cast`) and UPnP over SOAP
//! (`crate::dlna`) — differ only in their wire; the session, poll loop, and
//! everything above them are shared. Tests supply a fake implementation.
//!
//! This trait is specifically the "the device fetches a URL and is driven by
//! commands" shape. A push-audio renderer like AirPlay — where bae keeps
//! decoding and streams the audio to the receiver, which never fetches a URL — is
//! a different renderer flavor and is deliberately outside this trait; it belongs
//! beside `Renderer::Remote` as its own variant, not as a third channel here.

use std::fmt;
use std::time::Duration;

/// The media a LOAD hands the renderer: the URL it fetches over HTTP and the
/// metadata it shows on-screen. Both a Cast LOAD and a UPnP `SetAVTransportURI`
/// are built from these fields.
#[derive(Debug, Clone, PartialEq)]
pub struct RendererMedia {
    /// The HTTP URL the renderer fetches the audio from.
    pub url: String,
    /// The MIME type of `url`'s bytes (`audio/flac`, `audio/mpeg`, …).
    pub content_type: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// An HTTP URL to cover art, shown on renderers with a screen. `None` when
    /// the track has no cover.
    pub cover_url: Option<String>,
    /// The track's total duration, when known.
    pub duration: Option<Duration>,
}

/// What the renderer's player is doing, read from its status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererPlayerState {
    /// No media is playing for a non-terminal reason — the player is idle before
    /// the first LOAD, or between a LOAD and its buffering, or a load was
    /// cancelled/interrupted. Not a queue-advance signal.
    Idle,
    /// The loaded media reached its natural end. This is the queue-advance
    /// signal (Cast IDLE/FINISHED, or a UPnP transport that went STOPPED after
    /// playing through to the end).
    Finished,
    /// The player is filling its buffer; position is not advancing yet.
    Buffering,
    Playing,
    Paused,
}

/// A snapshot of the renderer's state from one `poll_status`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReceiverStatus {
    pub player_state: RendererPlayerState,
    /// The renderer's playback position into the current media, when reported.
    pub position: Option<Duration>,
    /// The current media's duration, when the renderer reports it.
    pub duration: Option<Duration>,
    /// Renderer volume level (0.0–1.0), or `None` when it wasn't read — carried
    /// as absent rather than invented, so a UI slider isn't jumped to a
    /// fabricated level.
    pub volume: Option<f32>,
}

/// A failure talking to the renderer. `Connection` is terminal — it means the
/// link to the device is gone (the Cast app was stopped, the UPnP renderer went
/// unreachable, the network dropped), which the session reads as the remote
/// session ending.
#[derive(Debug)]
pub enum RendererError {
    /// The link could not be established or was lost.
    Connection(String),
    /// Preparing the renderer to receive media failed (Cast app launch).
    Launch(String),
    /// A media/transport command (load, play, seek, …) failed.
    Command(String),
}

impl fmt::Display for RendererError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RendererError::Connection(detail) => {
                write!(f, "renderer connection failed: {detail}")
            }
            RendererError::Launch(detail) => write!(f, "renderer launch failed: {detail}"),
            RendererError::Command(detail) => write!(f, "renderer command failed: {detail}"),
        }
    }
}

impl std::error::Error for RendererError {}

/// The operations the session performs against a connected renderer. Each call
/// blocks on the wire, so the session drives it from its own thread. A
/// [`RendererError::Connection`] from any call is terminal (the session ends);
/// tests fake this to record commands and hand back canned statuses.
pub trait RendererChannel: Send {
    /// Load `media` on the renderer and start it playing.
    fn load(&mut self, media: &RendererMedia) -> Result<(), RendererError>;
    fn play(&mut self) -> Result<(), RendererError>;
    fn pause(&mut self) -> Result<(), RendererError>;
    fn seek(&mut self, position: Duration) -> Result<(), RendererError>;
    /// Set the renderer volume to `level` (0.0–1.0).
    fn set_volume(&mut self, level: f32) -> Result<(), RendererError>;
    /// Stop playback on the renderer (the loaded media is unloaded).
    fn stop(&mut self) -> Result<(), RendererError>;
    /// Read the renderer's current status.
    fn poll_status(&mut self) -> Result<ReceiverStatus, RendererError>;
}
