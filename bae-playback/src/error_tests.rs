use super::*;
use crate::AudioError;
use crate::{AudioOutputProblem, PlaybackErrorReason};

fn kitchen() -> crate::NamedReceiver {
    crate::NamedReceiver { id: "58:55:CA:1A:2B:3C".to_string(), name: "Kitchen".to_string() }
}

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
        (
            AudioError::AirPlayPairingLost(kitchen()),
            AudioOutputProblem::AirPlayPairingLost { receiver: kitchen() },
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
    assert_eq!(failure, crate::PlaybackDiagnostic::Internal);
    assert!(detail.contains("RAOP key wrap"), "{detail}");
}
