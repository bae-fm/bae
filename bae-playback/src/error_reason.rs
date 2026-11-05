//! Why playback stopped, in the words the person is shown.

use bae_diagnostics::UiErrorCategory;

/// Why playback couldn't start or continue. The cases where this device can't
/// reach the release's audio are user-actionable and keyed; every in-core
/// failure (decode, audio output, IO) is un-enumerable and routes to
/// `Diagnostic`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackErrorReason {
    /// A remote cloud-only track has no local copy and sync is disconnected —
    /// the user reconnects cloud sync to play it. Actionable, keyed.
    SyncDisconnected,
    /// A local release whose audio is on another device only, the one
    /// named, which hasn't uploaded it. Actionable, keyed.
    OnOtherDevice(bae_records::OtherDevice),
    /// A local release's original files are gone or changed on this device.
    /// Actionable, keyed.
    OriginalMissing,
    /// The audio output didn't start, for a reason of the device's or the
    /// speaker's. Actionable, keyed.
    AudioOutput { problem: AudioOutputProblem },
    /// Any other failure: bae's own (decode, audio output, IO, DB, missing
    /// file) or a renderer refusing a command. The UI renders its generic
    /// per-category line plus `detail`, the opaque, log-only error chain.
    Diagnostic { failure: PlaybackDiagnostic, detail: String },
}

/// Whose failure a diagnostic playback failure is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackDiagnostic {
    /// bae's own.
    Internal,
    /// The device playback was sent to refused a command while the link to
    /// it stayed up.
    Renderer,
}

/// What kept the audio output from starting, as the person can act on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioOutputProblem {
    /// The audio device was disconnected or is no longer available.
    DeviceUnavailable,
    /// The audio device can't play the track's sample rate and channels.
    UnsupportedFormat,
    /// The AirPlay speaker couldn't be reached or didn't accept the session.
    AirPlayRefused,
}

impl PlaybackErrorReason {
    /// An internal-category diagnostic reason carrying the given opaque,
    /// log-only detail. For the in-core failure paths (decode, audio output,
    /// IO) that have no user-actionable distinction.
    pub fn internal(detail: impl std::fmt::Display) -> Self {
        Self::Diagnostic { failure: PlaybackDiagnostic::Internal, detail: detail.to_string() }
    }

    /// A renderer refused a command; `detail` is why, untranslated.
    pub fn renderer(detail: impl std::fmt::Display) -> Self {
        Self::Diagnostic { failure: PlaybackDiagnostic::Renderer, detail: detail.to_string() }
    }
}

impl PlaybackDiagnostic {
    /// The error category the UI renders this failure's line from.
    pub fn category(self) -> UiErrorCategory {
        match self {
            Self::Internal => UiErrorCategory::Internal,
            Self::Renderer => UiErrorCategory::Renderer,
        }
    }
}
