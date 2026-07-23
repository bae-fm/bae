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
pub mod session;

pub use channel::{
    CastChannel, CastError, CastMedia, CastPlayerState, ReceiverStatus, RustCastChannel,
};
pub use discovery::{CastDevice, CastDiscovery};
pub use session::{CastSession, CastSessionStatus, StatusCallback};

#[cfg(test)]
mod tests;
