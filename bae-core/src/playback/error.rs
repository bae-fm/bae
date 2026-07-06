//! Playback error types
use thiserror::Error;
/// Errors that can occur during audio playback operations
#[derive(Error, Debug)]
pub enum PlaybackError {
    /// Database query failed
    #[error("Database error: {0}")]
    Database(String),
    /// Requested resource not found (track, file, etc.)
    #[error("{0} not found: {1}")]
    NotFound(&'static str, String),
    /// Invalid or corrupt FLAC data
    #[error("Invalid FLAC: {0}")]
    InvalidFlac(String),
    /// File system IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// Async task panicked or was cancelled
    #[error("Task failed: {0}")]
    TaskFailed(String),
    /// Internal playback invariant failed
    #[error("Internal playback error: {0}")]
    Internal(String),
    /// A remote track has no local copy and sync is disconnected, so the
    /// audio can't be fetched. The user needs to reconnect cloud sync.
    #[error("Sync is disconnected — reconnect to play this release")]
    SyncDisconnected,
    /// A remote track whose cloud upload is still queued has no readable
    /// source left (the original file is gone), so the cloud object may not
    /// exist yet. The user waits for the upload rather than getting a
    /// not-found error for an internal storage key.
    #[error(
        "This release is still uploading to the cloud home — try again when the upload finishes"
    )]
    UploadPending,
}
impl PlaybackError {
    /// Project this domain error onto the UI-facing reason. The two cloud-only
    /// "not playable yet" cases are user-actionable and keyed; every other mode
    /// is un-enumerable for the UI and goes to the diagnostic arm with the error
    /// chain as opaque, log-only detail.
    pub fn into_ui_reason(self) -> crate::ui::PlaybackErrorReason {
        use crate::ui::PlaybackErrorReason;
        match self {
            PlaybackError::SyncDisconnected => PlaybackErrorReason::SyncDisconnected,
            PlaybackError::UploadPending => PlaybackErrorReason::UploadPending,
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
    pub fn io(msg: impl Into<String>) -> Self {
        Self::Io(std::io::Error::other(msg.into()))
    }
}
