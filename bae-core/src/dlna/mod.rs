//! UPnP/DLNA control point.
//!
//! bae plays to UPnP MediaRenderers — AV receivers, smart TVs, network streamers
//! — as a second flavor of remote renderer beside Cast. Same shape: discover
//! devices ([`discovery`]), hand the renderer a URL it fetches over HTTP, and
//! drive it with transport commands, polling for position. The renderer fetches
//! and decodes the audio itself; bae is the control point and the HTTP source.
//!
//! Control is SOAP over HTTP: the four AVTransport transport actions plus
//! RenderingControl volume, with position read by polling `GetPositionInfo`.
//! [`soap`] builds each envelope and parses the poll responses as pure string
//! work; [`channel::DlnaChannel`] carries them over blocking HTTP and presents
//! the renderer as the shared [`crate::renderer::RendererChannel`], including the
//! end-of-track inference UPnP's reason-less STOPPED requires.

pub mod channel;
pub mod discovery;
pub mod soap;

pub use channel::DlnaChannel;
pub use discovery::DlnaDiscovery;

#[cfg(test)]
mod tests;
