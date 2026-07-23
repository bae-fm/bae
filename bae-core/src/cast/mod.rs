//! Google Cast sender.
//!
//! bae plays to Chromecast/Nest devices as a second renderer behind the
//! playback service: the receiver fetches audio over HTTP itself, while bae
//! discovers devices ([`discovery`]), opens a control channel ([`channel`]), and
//! drives one connected device from its own thread ([`session`]).
//!
//! The module is decoupled from both the audio URL source (the caller injects
//! one) and the playback service (the session reports through a callback), so
//! bae-core depends on neither bae-subsonic nor a specific renderer.

pub mod channel;
pub mod discovery;
pub mod format;
pub mod session;

use std::sync::Arc;

pub use channel::{
    CastChannel, CastError, CastMedia, CastPlayerState, ReceiverStatus, RustCastChannel,
};
pub use discovery::{CastDevice, CastDiscovery};
pub use format::{cast_stream_format, CastStreamFormat, CAST_TRANSCODE_BITRATE_KBPS};
pub use session::{CastSession, CastSessionStatus, StatusCallback};

/// Mints the HTTP URL the receiver fetches a track's audio from, given the track
/// id and the stream format to serve it in. The caller (bae-desktop) injects one
/// backed by an ephemeral Subsonic router; the error is a human-readable reason.
/// The format is passed in — not re-derived here — because the service has
/// already resolved the track's content type and this closure runs synchronously
/// on the service thread, where an async lookup can't. Keeps bae-core free of a
/// dependency on bae-subsonic.
pub type MediaUrlProvider =
    Arc<dyn Fn(&str, CastStreamFormat) -> Result<String, String> + Send + Sync>;

/// Mints the HTTP URL for a track's cover art, given the track id, or `None`
/// when the track has no cover. Injected alongside [`MediaUrlProvider`].
pub type CoverUrlProvider = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

#[cfg(test)]
mod tests;
