//! Playback error types

use thiserror::Error;

/// Errors that can occur during audio playback operations
#[derive(Error, Debug)]
pub enum PlaybackError {
    /// Database query failed
    #[error("Database error: {0}")]
    Database(String),

    /// Requested resource not found (track, file, chunk, etc.)
    #[error("{0} not found: {1}")]
    NotFound(&'static str, String),

    /// Cloud storage download failed
    #[error("Cloud download failed: {0}")]
    CloudDownload(String),

    /// Decryption failed
    #[error("Decryption failed: {0}")]
    Decryption(String),

    /// Invalid or corrupt FLAC data
    #[error("Invalid FLAC: {0}")]
    InvalidFlac(String),

    /// File system IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Async task panicked or was cancelled
    #[error("Task failed: {0}")]
    TaskFailed(String),
}

impl PlaybackError {
    pub fn not_found(what: &'static str, id: impl Into<String>) -> Self {
        Self::NotFound(what, id.into())
    }

    pub fn database(e: impl std::fmt::Display) -> Self {
        Self::Database(e.to_string())
    }

    pub fn cloud(e: impl std::fmt::Display) -> Self {
        Self::CloudDownload(e.to_string())
    }

    pub fn decrypt(e: impl std::fmt::Display) -> Self {
        Self::Decryption(e.to_string())
    }

    pub fn flac(msg: impl Into<String>) -> Self {
        Self::InvalidFlac(msg.into())
    }

    pub fn task(e: impl std::fmt::Display) -> Self {
        Self::TaskFailed(e.to_string())
    }
}
