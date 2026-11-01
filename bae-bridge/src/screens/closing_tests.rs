//! A library's close writing its steps into the stream the app reads.

use super::*;

/// Closing a library writes each step as it starts, in order, and the
/// stream holds nothing once the close ends.
#[test]
fn a_close_writes_each_step_it_runs() {
    let root = tempfile::tempdir().unwrap();
    let host = crate::BridgeHost::with_test_keyring(
        crate::BridgeDiagnostics::noop(),
        crate::BridgeAppDir::new(root.path().to_str().unwrap().into()).unwrap(),
    )
    .unwrap();
    let caller = tokio::runtime::Runtime::new().unwrap();
    let opened = caller
        .block_on(host.clone().create_library(
            Some("Closed Library".into()),
            crate::BridgeLibrarySetup::new(host.clone()),
        ))
        .unwrap();
    let app = opened.open(50, false).unwrap();
    let closing = BridgeLibraryClosing::new(host);
    let stream = closing.feed_closing_steps();

    app.close_library(Some(closing.clone())).unwrap();

    let mut expected = Vec::new();
    #[cfg(feature = "desktop")]
    expected.push(Some(BridgeClosingStep::StoppingServers));
    expected.push(Some(BridgeClosingStep::StoppingPlayback));
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    expected.push(Some(BridgeClosingStep::StoppingImports));
    expected.extend([
        Some(BridgeClosingStep::StoppingSync),
        Some(BridgeClosingStep::FinishingWork),
        Some(BridgeClosingStep::ClosingStore),
        None,
    ]);
    assert_eq!(closing.written(), expected);
    assert_eq!(stream.latest(Vec::new()), None);
}
