//! Remote renderers: playing to a device that fetches audio over HTTP itself.
//!
//! bae plays to two flavors of remote renderer behind the one playback queue: a
//! Google Cast receiver and a UPnP MediaRenderer. Both are the same shape — the
//! device fetches a track's audio over HTTP and is driven by transport commands
//! — so the playback service drives both through what this module holds: the
//! command [`channel`] trait, the [`session`] that drives one connected device
//! from its own thread, the served-[`mod@format`] decision, and the
//! [`media_source`] the device fetches each track through.
//!
//! The wire of each flavor, and discovery of the devices, live in bae-cast,
//! which connects a device's channel and hands it here. This keeps bae-core free
//! of every device protocol and of the audio URL source (the caller injects
//! one), and the session reports through a callback, so it names no playback
//! type.

pub mod channel;
pub mod format;
pub mod media_source;
pub mod session;

pub use channel::{
    ReceiverStatus, RendererChannel, RendererError, RendererMedia, RendererPlayerState,
};
pub use format::{RendererFlavor, RendererStreamFormat, TRANSCODE_BITRATE_KBPS};
pub use media_source::{CoverUrlProvider, MediaUrlProvider, RendererMediaSource, ServedTrack};
pub use session::{EventCallback, RendererSession, RendererSessionEvent};

#[cfg(test)]
mod tests;
