use super::*;
use crate::types::{BridgeAirPlayCode, BridgeAirPlayCodeAsked};
use bae_cast::CodePrompt as _;

/// A sheet that answers with the PIN it was asked for, once it sees the
/// request core sent.
struct Answering;
#[async_trait::async_trait]
impl AirPlayCodePromptCallback for Answering {
    async fn code(&self, request: BridgeAirPlayCodeRequest) -> String {
        assert_eq!(
            request,
            BridgeAirPlayCodeRequest {
                code: BridgeAirPlayCode::Pin { digits: 4 },
                why: BridgeAirPlayCodeAsked::PairingLost,
                last_try: None,
            }
        );
        "0482".into()
    }

    async fn owner_deciding(&self, deciding: bool) {
        assert!(deciding);
    }
}

fn pin_request() -> bae_cast::CodeRequest {
    bae_cast::CodeRequest {
        code: bae_cast::CodeKind::Pin { digits: 4 },
        why: bae_cast::CodeAsked::PairingLost,
        last_try: None,
    }
}

/// The sheet sees core's request and its answer reaches the cast.
#[tokio::test]
async fn the_sheet_answers_the_request_core_sent() {
    let prompt = NativeCodePrompt::new(Box::new(Answering), bae_diagnostics::Diagnostics::noop());
    assert_eq!(prompt.code(pin_request()).await.unwrap(), "0482");
}

/// The app hears that the receiver's owner is deciding.
#[tokio::test]
async fn the_sheet_hears_that_an_owner_is_deciding() {
    let prompt = NativeCodePrompt::new(Box::new(Answering), bae_diagnostics::Diagnostics::noop());
    prompt.owner_deciding(true).await.unwrap();
}

/// A sheet whose host throws something its callback doesn't declare, which
/// uniffi raises in Rust as a panic.
struct Throwing;
#[async_trait::async_trait]
impl AirPlayCodePromptCallback for Throwing {
    async fn code(&self, _: BridgeAirPlayCodeRequest) -> String {
        panic!("Callback interface failure: the host threw");
    }

    async fn owner_deciding(&self, _: bool) {
        panic!("Callback interface failure: the host threw");
    }
}

/// The throw comes back as the bug that fails the cast, instead of unwinding
/// through it. uniffi raises it as a panic, which the panic hook reported, so
/// the prompt doesn't report it again.
#[tokio::test]
async fn a_sheet_that_throws_fails_the_cast_as_a_bug() {
    let bugs = std::sync::Arc::new(bae_diagnostics::RecordingBugReporter::default());
    let prompt = NativeCodePrompt::new(
        Box::new(Throwing),
        bae_diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone()),
    );
    assert!(prompt.code(pin_request()).await.is_err());
    assert_eq!(bugs.reports(), Vec::new());
}
