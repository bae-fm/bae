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
    /// Another device attached the track's file and storage doesn't hold it
    /// yet: its upload is still under way. Keyed so the user waits for the
    /// upload instead of getting a missing-file diagnostic.
    #[error(
        "This release is still uploading to the cloud home — try again when the upload finishes"
    )]
    UploadPending,
    /// The audio output didn't start: the device's, the speaker's, or bae's
    /// own audio path's doing, as the error says.
    #[error("the audio output didn't start: {0}")]
    AudioOutput(#[source] crate::playback::AudioError),
}
impl PlaybackError {
    /// Project this domain error onto the UI-facing reason. The cases where
    /// this device can't reach the audio are user-actionable and keyed; every other mode
    /// is un-enumerable for the UI and goes to the diagnostic arm with the error
    /// chain as opaque, log-only detail.
    pub fn ui_reason(&self) -> crate::playback::PlaybackErrorReason {
        use crate::playback::PlaybackErrorReason;
        match self {
            PlaybackError::UploadPending => PlaybackErrorReason::UploadPending,
            PlaybackError::AudioOutput(error) => {
                use crate::playback::AudioError;
                use crate::playback::AudioOutputProblem;
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
            PlaybackError::AudioOutput(crate::playback::AudioError::Unexpected(_))
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
pub(crate) fn read_failure_reason(
    error: &crate::playback::TrackReadError,
) -> crate::playback::PlaybackErrorReason {
    use crate::playback::{PlaybackErrorReason, TrackReadError};
    match error {
        TrackReadError::SyncDisconnected => PlaybackErrorReason::SyncDisconnected,
        TrackReadError::NotUploaded => PlaybackErrorReason::NotUploaded,
        TrackReadError::OriginalMissing => PlaybackErrorReason::OriginalMissing,
        other => PlaybackErrorReason::internal(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::AudioError;
    use crate::playback::{AudioOutputProblem, PlaybackErrorReason};

    /// A device's or a speaker's problem reaches the person by name and is no
    /// bug; bae's own failure is a bug and reads as one.
    #[test]
    fn an_audio_output_failure_names_its_reason() {
        for (error, problem) in [
            (AudioError::DeviceUnavailable, AudioOutputProblem::DeviceUnavailable),
            (
                AudioError::UnsupportedFormat { sample_rate: 352_800, channels: 2 },
                AudioOutputProblem::UnsupportedFormat,
            ),
            (
                AudioError::AirPlayRefused("connection refused".to_string()),
                AudioOutputProblem::AirPlayRefused,
            ),
        ] {
            let error = PlaybackError::AudioOutput(error);
            assert!(!error.is_bug(), "{error}");
            assert!(
                matches!(error.ui_reason(), PlaybackErrorReason::AudioOutput { problem: p } if p == problem),
                "{error}"
            );
        }

        let error = PlaybackError::AudioOutput(AudioError::Unexpected("RAOP key wrap".to_string()));
        assert!(error.is_bug());
        let PlaybackErrorReason::Diagnostic { failure, detail } = error.ui_reason() else {
            panic!("an unexpected failure is a diagnostic");
        };
        assert_eq!(failure, crate::playback::PlaybackDiagnostic::Internal);
        assert!(detail.contains("RAOP key wrap"), "{detail}");
    }
}
