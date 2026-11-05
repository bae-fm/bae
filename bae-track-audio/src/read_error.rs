//! Why a read of a track's stored audio failed.

use thiserror::Error;

/// A track's audio could not be read: what playback, a save and import's
/// loudness pass each learn when a fill of a source's bytes fails.
#[derive(Error, Debug)]
pub enum TrackReadError {
    /// A source read failed. `context` names the operation (which file, which
    /// byte range); `source` is the OS error itself, kind intact, so a caller
    /// can tell a missing file from a full disk from a dropped network volume.
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    /// The fill's task panicked.
    #[error("Task failed: {0}")]
    TaskFailed(String),
    /// A read invariant failed.
    #[error("Internal playback error: {0}")]
    Internal(String),
    /// A remote track has no local copy and sync is disconnected, so the
    /// audio can't be fetched. The user needs to reconnect cloud sync.
    #[error("Sync is disconnected — reconnect to play this release")]
    SyncDisconnected,
    /// A local release's audio is on another device only, the one named:
    /// it stays there until that device uploads it, so this device has
    /// nothing to read.
    #[error("This release's audio is only on another device: {0:?}")]
    OnOtherDevice(bae_records::OtherDevice),
    /// A local release's original file on this device is gone or changed
    /// since import.
    #[error("This release's original files are missing or changed on this device")]
    OriginalMissing,
}

impl TrackReadError {
    pub(crate) fn task(e: impl std::fmt::Display) -> Self {
        Self::TaskFailed(e.to_string())
    }
    pub(crate) fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
    pub(crate) fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io { context: context.into(), source }
    }
}

/// A fill whose source came back short is a broken read path: bae's own
/// failure, not the device's or the network's.
impl From<bae_audio::sparse_buffer::ShortRead> for TrackReadError {
    fn from(short: bae_audio::sparse_buffer::ShortRead) -> Self {
        Self::internal(short.to_string())
    }
}
