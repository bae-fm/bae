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
    PlatformAudioFocus::new(Box::new(focus), bae_diagnostics::Diagnostics::noop())
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

/// A throw refuses the request, so playback stays paused instead of the
/// playback service unwinding. uniffi raises it as a panic, which the panic
/// hook reported, so the request doesn't report it again.
#[test]
fn a_platform_that_throws_refuses() {
    let bugs = std::sync::Arc::new(bae_diagnostics::RecordingBugReporter::default());
    let throwing = PlatformAudioFocus::new(
        Box::new(Throwing),
        bae_diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone()),
    );

    assert!(throwing.request().is_err());
    throwing.release();

    assert_eq!(bugs.reports(), Vec::new());
}
