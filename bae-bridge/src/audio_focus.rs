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
            |failed| self.diagnostics.bug(failed),
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
            |failed| self.diagnostics.bug(failed),
            "release audio focus",
            || self.focus.release(),
        ) {
            tracing::warn!("the system may still hold audio focus for bae: {failed}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bae_playback::AudioFocusSystem;

    struct Answering(BridgeAudioFocusAnswer);

    impl BridgeAudioFocus for Answering {
        fn request(&self) -> BridgeAudioFocusAnswer {
            self.0.clone()
        }
        fn release(&self) {}
    }

    fn platform(focus: impl BridgeAudioFocus + 'static) -> PlatformAudioFocus {
        PlatformAudioFocus::new(Box::new(focus), crate::BridgeDiagnostics::noop())
    }

    #[test]
    fn a_refusal_reaches_core_with_the_systems_reason() {
        let refusing = platform(Answering(BridgeAudioFocusAnswer::Refused {
            reason: "audio mode IN_CALL".to_string(),
        }));
        assert_eq!(
            refusing.request(),
            Err(bae_playback::AudioFocusRefused("audio mode IN_CALL".to_string()))
        );
        let granting = platform(Answering(BridgeAudioFocusAnswer::Granted));
        assert_eq!(granting.request(), Ok(()));
    }

    /// The platform's audio focus, throwing on every call as uniffi raises a
    /// host throw: a panic.
    struct Throwing;

    impl BridgeAudioFocus for Throwing {
        fn request(&self) -> BridgeAudioFocusAnswer {
            panic!("Callback interface failure: the host threw");
        }
        fn release(&self) {
            panic!("Callback interface failure: the host threw");
        }
    }

    /// A throw is reported and refuses the request, so playback stays
    /// paused instead of the playback service unwinding.
    #[test]
    fn a_platform_that_throws_is_reported_and_refuses() {
        let bugs = std::sync::Arc::new(bae_diagnostics::RecordingBugReporter::default());
        let throwing = PlatformAudioFocus::new(
            Box::new(Throwing),
            crate::BridgeDiagnostics::reporting_bugs_to(bugs.clone()),
        );

        assert!(throwing.request().is_err());
        throwing.release();

        let operations: Vec<String> = bugs.reports().into_iter().map(|(op, _)| op).collect();
        assert_eq!(operations, ["request audio focus", "release audio focus"]);
    }
}
