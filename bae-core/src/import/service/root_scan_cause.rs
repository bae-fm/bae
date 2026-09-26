//! Why a root scan was asked for, for the log line every request writes.

/// Why a root scan was asked for.
///
/// Logged wherever one is requested, because "the scans never stop" is a
/// question only the thing that keeps asking for them can answer — and until
/// now nothing recorded that. A watched network share whose own reads come
/// back as writes would look exactly like a folder somebody keeps editing.
pub(super) enum RootScanCause {
    /// The filesystem reported changes under the root: the events that passed
    /// the change filter, kind and path, and how many were filtered out.
    FsChange(String),
    /// The watcher itself failed, so the root is re-read to catch up on
    /// whatever it missed.
    WatchError,
    /// The filesystem watch said it lost track of changes — FSEvents dropping
    /// events, inotify's queue overflowing — so what it could have missed is
    /// read again: the folder it named, or the root when it named none.
    EventsDropped,
    /// The periodic check of a network folder found a directory that moved.
    /// Such a folder has no watch worth the name, so this is the only thing
    /// that notices a change made on the server or by another machine.
    NetworkFolderMoved,
    /// Something a person did — naming which.
    Asked(&'static str),
}

impl std::fmt::Display for RootScanCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FsChange(events) => write!(f, "filesystem change ({events})"),
            Self::WatchError => write!(f, "the folder watcher reported an error"),
            Self::EventsDropped => write!(f, "the folder watch lost track of changes"),
            Self::NetworkFolderMoved => {
                write!(f, "the periodic check found a directory that moved")
            }
            Self::Asked(what) => write!(f, "{what}"),
        }
    }
}
