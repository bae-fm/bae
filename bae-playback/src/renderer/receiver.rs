//! A receiver: a remote renderer playback can be cast to, named the same
//! wherever bae holds one. Discovery finds it, the picker lists it, playback
//! moves to it and remembers it, and a lost link names it.

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

/// A receiver: its stable id, which casting to it is asked by, its name,
/// which the picker and the cast surfaces show, and its kind. Names need not
/// be unique across receivers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastReceiver {
    pub id: String,
    pub name: String,
    pub kind: RendererKind,
}
