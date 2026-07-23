//! Remote renderers: playing to a device that fetches audio over HTTP itself.
//!
//! bae plays to two flavors of remote renderer behind the one playback queue: a
//! Google Cast receiver ([`crate::cast`]) and a UPnP MediaRenderer
//! ([`crate::dlna`]). Both are the same shape — the device fetches a track's
//! audio over HTTP and is driven by transport commands — so everything but the
//! wire is shared here: the command [`channel`] trait, the [`session`] that
//! drives one connected device from its own thread, the served-[`mod@format`] gate,
//! and the merged [`device`] list.
//!
//! This is decoupled from both the audio URL source (the caller injects one) and
//! the playback service (the session reports through a callback), so bae-core
//! depends on neither bae-subsonic nor a specific renderer flavor.

pub mod channel;
pub mod device;
pub mod format;
pub mod session;

use std::sync::Arc;

pub use channel::{
    ReceiverStatus, RendererChannel, RendererError, RendererMedia, RendererPlayerState,
};
pub use device::{RendererConnection, RendererDevice, RendererKind};
pub use format::{
    cast_stream_format, dlna_stream_format, RendererStreamFormat, TRANSCODE_BITRATE_KBPS,
};
pub use session::{RendererSession, RendererSessionStatus, StatusCallback};

/// Mints the HTTP URL the renderer fetches a track's audio from, given the track
/// id and the stream format to serve it in. The caller (bae-desktop) injects one
/// backed by an ephemeral Subsonic router; the error is a human-readable reason.
/// The format is passed in — not re-derived here — because the service has
/// already resolved the track's content type and this closure runs synchronously
/// on the service thread, where an async lookup can't. Keeps bae-core free of a
/// dependency on bae-subsonic.
pub type MediaUrlProvider =
    Arc<dyn Fn(&str, RendererStreamFormat) -> Result<String, String> + Send + Sync>;

/// Mints the HTTP URL for a track's cover art, given the track id, or `None`
/// when the track has no cover. Injected alongside [`MediaUrlProvider`].
pub type CoverUrlProvider = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Picks the served stream format for a source codec. The flavor-specific gate
/// ([`cast_stream_format`] or [`dlna_stream_format`]) is chosen where the channel
/// is built and injected alongside it, so the service reissues each track through
/// the right safe-set as the queue advances without knowing the flavor.
pub type StreamFormatFn = fn(&crate::util::content_type::ContentType) -> RendererStreamFormat;

#[cfg(test)]
mod tests;
