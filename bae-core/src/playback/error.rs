//! Playback error types
use thiserror::Error;
#[derive(Error, Debug)]
pub enum PlaybackError {
    #[error("Database error: {0}")]
    Database(String),
    /// A track, file, or other resource the caller named doesn't exist.
    #[error("{0} not found: {1}")]
    NotFound(&'static str, String),
    #[error("Invalid FLAC: {0}")]
    InvalidFlac(String),
    /// A source read failed. `context` names the operation (which file, which
    /// byte range); `source` is the OS error itself, kind intact, so a caller
    /// can tell a missing file from a full disk from a dropped network volume.
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    /// An async task panicked or was cancelled.
    #[error("Task failed: {0}")]
    TaskFailed(String),
    /// A playback invariant failed.
    #[error("Internal playback error: {0}")]
    Internal(String),
    /// A remote track has no local copy and sync is disconnected, so the
    /// audio can't be fetched. The user needs to reconnect cloud sync.
    #[error("Sync is disconnected — reconnect to play this release")]
    SyncDisconnected,
    /// Another device attached the track's file and storage doesn't hold it
    /// yet: its upload is still under way. Keyed so the user waits for the
    /// upload instead of getting a missing-file diagnostic.
    #[error(
        "This release is still uploading to the cloud home — try again when the upload finishes"
    )]
    UploadPending,
    /// A local release imported on another device: its audio stays there
    /// until that device uploads it, so this device has nothing to read.
    #[error("This release's audio is on the device that imported it and isn't uploaded yet")]
    NotUploaded,
    /// A local release's original file on this device is gone or changed
    /// since import.
    #[error("This release's original files are missing or changed on this device")]
    OriginalMissing,
}
impl PlaybackError {
    /// Project this domain error onto the UI-facing reason. The cases where
    /// this device can't reach the audio are user-actionable and keyed; every other mode
    /// is un-enumerable for the UI and goes to the diagnostic arm with the error
    /// chain as opaque, log-only detail.
    pub fn ui_reason(&self) -> crate::ui::PlaybackErrorReason {
        use crate::ui::PlaybackErrorReason;
        match self {
            PlaybackError::SyncDisconnected => PlaybackErrorReason::SyncDisconnected,
            PlaybackError::UploadPending => PlaybackErrorReason::UploadPending,
            PlaybackError::NotUploaded => PlaybackErrorReason::NotUploaded,
            PlaybackError::OriginalMissing => PlaybackErrorReason::OriginalMissing,
            other => PlaybackErrorReason::internal(other),
        }
    }
    pub fn not_found(what: &'static str, id: impl Into<String>) -> Self {
        Self::NotFound(what, id.into())
    }
    pub fn database(e: impl std::fmt::Display) -> Self {
        Self::Database(e.to_string())
    }
    pub fn flac(msg: impl Into<String>) -> Self {
        Self::InvalidFlac(msg.into())
    }
    pub fn task(e: impl std::fmt::Display) -> Self {
        Self::TaskFailed(e.to_string())
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}
