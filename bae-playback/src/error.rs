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
    /// An async task panicked or was cancelled.
    #[error("Task failed: {0}")]
    TaskFailed(String),
    /// A playback invariant failed.
    #[error("Internal playback error: {0}")]
    Internal(String),
    /// The audio output didn't start: the device's, the speaker's, or bae's
    /// own audio path's doing, as the error says.
    #[error("the audio output didn't start: {0}")]
    AudioOutput(#[source] crate::AudioError),
}
impl From<bae_store::DbError> for PlaybackError {
    fn from(error: bae_store::DbError) -> Self {
        Self::database(error)
    }
}

impl From<bae_records::TrackAudioError> for PlaybackError {
    fn from(error: bae_records::TrackAudioError) -> Self {
        match error {
            bae_records::TrackAudioError::TrackNotFound(track_id) => {
                Self::not_found("Track", track_id)
            }
            error => Self::database(error),
        }
    }
}

impl PlaybackError {
    /// Project this domain error onto the UI-facing reason. The cases where
    /// this device can't reach the audio are user-actionable and keyed; every other mode
    /// is un-enumerable for the UI and goes to the diagnostic arm with the error
    /// chain as opaque, log-only detail.
    pub fn ui_reason(&self) -> crate::PlaybackErrorReason {
        use crate::PlaybackErrorReason;
        match self {
            PlaybackError::AudioOutput(error) => {
                use crate::AudioError;
                use crate::AudioOutputProblem;
                let problem = match error {
                    AudioError::DeviceUnavailable => AudioOutputProblem::DeviceUnavailable,
                    AudioError::UnsupportedFormat { .. } => AudioOutputProblem::UnsupportedFormat,
                    AudioError::AirPlayRefused(_) => AudioOutputProblem::AirPlayRefused,
                    AudioError::Unexpected(_) => return PlaybackErrorReason::internal(self),
                };
                PlaybackErrorReason::AudioOutput { problem }
            }
            other => PlaybackErrorReason::internal(other),
        }
    }
    /// Whether this failure is bae's own — a bug for the crash reporter —
    /// rather than the person's, the device's, the speaker's or the network's.
    pub fn is_bug(&self) -> bool {
        matches!(
            self,
            PlaybackError::AudioOutput(crate::AudioError::Unexpected(_))
                | PlaybackError::TaskFailed(_)
                | PlaybackError::Internal(_)
        )
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
}

/// What the person is told when a read of a track's audio fails. Where this
/// device can't reach the audio is theirs to act on and keyed; every other
/// failure goes to the diagnostic arm with the error as log-only detail.
pub fn read_failure_reason(error: &bae_track_audio::TrackReadError) -> crate::PlaybackErrorReason {
    use crate::PlaybackErrorReason;
    use bae_track_audio::TrackReadError;
    match error {
        TrackReadError::SyncDisconnected => PlaybackErrorReason::SyncDisconnected,
        TrackReadError::OnOtherDevice(device) => PlaybackErrorReason::OnOtherDevice(device.clone()),
        TrackReadError::OriginalMissing => PlaybackErrorReason::OriginalMissing,
        other => PlaybackErrorReason::internal(other),
    }
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
