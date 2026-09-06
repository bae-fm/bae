//! Remote renderers: playing to a device that fetches audio over HTTP itself.
//!
//! bae plays to two flavors of remote renderer behind the one playback queue: a
//! Google Cast receiver ([`crate::cast`]) and a UPnP MediaRenderer
//! ([`crate::dlna`]). Both are the same shape — the device fetches a track's
//! audio over HTTP and is driven by transport commands — so everything but the
//! wire is shared here: the command [`channel`] trait, the [`session`] that
//! drives one connected device from its own thread, the served-[`mod@format`]
//! gate, the [`media_source`] the device fetches each track through, and the
//! merged [`device`] list.
//!
//! This is decoupled from both the audio URL source (the caller injects one) and
//! the playback service (the session reports through a callback), so bae-core
//! depends on neither bae-subsonic nor a specific renderer flavor.
//!
//! [`discovery`] is where the picker's list comes from — bae's own browsing, or
//! services a host's browser reports in when bae may not read the network
//! itself.

pub mod channel;
pub mod device;
pub mod discovery;
pub mod format;
pub mod media_source;
pub(crate) mod published_devices;
pub mod session;

pub use channel::{
    ReceiverStatus, RendererChannel, RendererError, RendererMedia, RendererPlayerState,
};
pub use device::{RendererConnection, RendererDevice, RendererKind};
pub use discovery::{
    RendererDiscovery, RendererServiceType, ReportedRenderer, RENDERER_SERVICE_TYPES,
};
pub use format::{
    cast_stream_format, dlna_stream_format, RendererStreamFormat, TRANSCODE_BITRATE_KBPS,
};
pub use media_source::{
    CoverUrlProvider, MediaUrlProvider, RendererMediaSource, ServedTrack, StreamFormatFn,
};
pub use session::{RendererSession, RendererSessionStatus, StatusCallback};

#[cfg(test)]
mod tests;
