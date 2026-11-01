//! The receivers the picker lists, as the library's device-local
//! `cast_device` table holds them: what discovery last found, written by
//! bae-cast in one write per change.

/// Which flavor of remote renderer a device is. Shown as a tag; the picker
/// does not segregate the list by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererKind {
    Cast,
    Dlna,
    /// An AirPlay receiver (RAOP or AirPlay 2). Unlike Cast/DLNA it does not
    /// fetch a URL — bae pushes decoded audio to it — but it shares the one
    /// device list.
    AirPlay,
}

/// One receiver the picker lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastDevice {
    /// The id casting to it is asked by.
    pub id: String,
    /// The name the picker shows.
    pub name: String,
    pub kind: RendererKind,
}
