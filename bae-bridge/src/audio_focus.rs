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

/// Core's view of the platform's [`BridgeAudioFocus`].
pub(crate) struct PlatformAudioFocus(pub(crate) Box<dyn BridgeAudioFocus>);

impl bae_core::playback::AudioFocusSystem for PlatformAudioFocus {
    fn request(&self) -> Result<(), bae_core::playback::AudioFocusRefused> {
        match self.0.request() {
            BridgeAudioFocusAnswer::Granted => Ok(()),
            BridgeAudioFocusAnswer::Refused { reason } => {
                Err(bae_core::playback::AudioFocusRefused(reason))
            }
        }
    }

    fn release(&self) {
        self.0.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bae_core::playback::AudioFocusSystem;

    struct Answering(BridgeAudioFocusAnswer);

    impl BridgeAudioFocus for Answering {
        fn request(&self) -> BridgeAudioFocusAnswer {
            self.0.clone()
        }
        fn release(&self) {}
    }

    #[test]
    fn a_refusal_reaches_core_with_the_systems_reason() {
        let refusing = PlatformAudioFocus(Box::new(Answering(BridgeAudioFocusAnswer::Refused {
            reason: "audio mode IN_CALL".to_string(),
        })));
        assert_eq!(
            refusing.request(),
            Err(bae_core::playback::AudioFocusRefused(
                "audio mode IN_CALL".to_string()
            ))
        );
        let granting = PlatformAudioFocus(Box::new(Answering(BridgeAudioFocusAnswer::Granted)));
        assert_eq!(granting.request(), Ok(()));
    }
}
