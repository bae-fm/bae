//! The platform's audio focus (Android) or audio session (iOS), which core
//! asks before every start of playback.

/// The system's answer to a request for the speaker.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeAudioFocusAnswer {
    Granted,
    /// `reason` is the system's own words, for the log.
    Refused {
        reason: String,
    },
}

/// Android's audio focus or iOS's audio session, which the app hands the host
/// at launch. Core calls both methods on its playback thread: `request` before
/// playback starts, `release` once it stops.
#[uniffi::export(callback_interface)]
pub trait BridgeAudioFocus: Send + Sync {
    fn request(&self) -> BridgeAudioFocusAnswer;
    fn release(&self);
}

/// Core's view of the platform's [`BridgeAudioFocus`]. A throw from the
/// platform is reported; a request it threw on is refused, so playback stays
/// paused.
pub(crate) struct PlatformAudioFocus {
    focus: Box<dyn BridgeAudioFocus>,
    diagnostics: std::sync::Arc<crate::BridgeDiagnostics>,
}

impl PlatformAudioFocus {
    pub(crate) fn new(
        focus: Box<dyn BridgeAudioFocus>,
        diagnostics: std::sync::Arc<crate::BridgeDiagnostics>,
    ) -> Self {
        Self { focus, diagnostics }
    }
}

impl bae_playback::AudioFocusSystem for PlatformAudioFocus {
    fn request(&self) -> Result<(), bae_playback::AudioFocusRefused> {
        match crate::host_calls::host_call(
            |bug| self.diagnostics.panicked(bug),
            "request audio focus",
            || self.focus.request(),
        ) {
            Ok(BridgeAudioFocusAnswer::Granted) => Ok(()),
            Ok(BridgeAudioFocusAnswer::Refused { reason }) => {
                Err(bae_playback::AudioFocusRefused(reason))
            }
            Err(failed) => Err(bae_playback::AudioFocusRefused(failed.to_string())),
        }
    }

    fn release(&self) {
        if let Err(failed) = crate::host_calls::host_call(
            |bug| self.diagnostics.panicked(bug),
            "release audio focus",
            || self.focus.release(),
        ) {
            tracing::warn!("the system may still hold audio focus for bae: {failed}");
        }
    }
}

#[cfg(test)]
#[path = "audio_focus_tests.rs"]
mod tests;
