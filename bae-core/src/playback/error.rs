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
    /// A managed track has no local copy and sync is disconnected, so the
    /// audio can't be fetched. The user needs to reconnect cloud sync.
    #[error("Sync is disconnected — reconnect to play this release")]
    SyncDisconnected,
}
impl PlaybackError {
    /// Project this domain error onto the UI-facing reason. The cloud-only
    /// "not playable yet" case is user-actionable and keyed; every other mode is
    /// un-enumerable for the UI and goes to the diagnostic arm with the error
    /// chain as opaque, log-only detail.
    pub fn into_ui_reason(self) -> crate::ui::PlaybackErrorReason {
        use crate::ui::PlaybackErrorReason;
        match self {
            PlaybackError::SyncDisconnected => PlaybackErrorReason::SyncDisconnected,
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
    pub fn io(msg: impl Into<String>) -> Self {
        Self::Io(std::io::Error::other(msg.into()))
    }
}
