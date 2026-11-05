//! A setup call's steps, as the app reads them off the call's stream.

use super::*;
use crate::types::{
    BridgeBootstrapState, BridgeBootstrapStatus, BridgeSetupProgress, BridgeSetupStep,
};

/// The stream's one key holds where the call is while it runs, and nothing
/// once the call's writing ends.
#[test]
fn the_stream_holds_the_running_step_until_the_call_ends() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let setup = library_setup(OperationRuntime::new(
        runtime.handle().clone(),
        bae_diagnostics::Diagnostics::noop(),
    ));
    let stream = setup.feed_setup_steps();
    let steps = setup.writer();

    steps.step(BridgeSetupStep::RemovingLibraries, 1, 3);
    assert_eq!(
        stream.latest(Vec::new()),
        Some(BridgeFastValue::SetupProgress {
            progress: BridgeSetupProgress {
                step: BridgeSetupStep::RemovingLibraries,
                done: 1,
                total: 3,
            },
        })
    );
    assert_eq!(runtime.block_on(stream.clone().next()), Ok(vec![Vec::new()]), "the key started");
    steps.bootstrap(coven::BootstrapStatus::Connecting, false);
    assert_eq!(
        stream.latest(Vec::new()),
        Some(BridgeFastValue::BootstrapState {
            state: BridgeBootstrapState {
                status: BridgeBootstrapStatus::Connecting,
                cancellable: false,
            },
        })
    );

    drop(steps);
    assert_eq!(stream.latest(Vec::new()), None);
    assert_eq!(runtime.block_on(stream.clone().next()), Ok(vec![Vec::new()]), "the key stopped");
}

/// Creating a library runs one step, which ends with the call.
#[test]
fn creating_a_library_reports_its_step() {
    let root = tempfile::tempdir().unwrap();
    let host = crate::BridgeHost::with_test_keyring(
        crate::BridgeDiagnostics::noop(),
        crate::BridgeAppDir::new(root.path().to_str().unwrap().into()).unwrap(),
    )
    .unwrap();
    let setup = BridgeLibrarySetup::new(host.clone());
    let caller = tokio::runtime::Runtime::new().unwrap();
    caller.block_on(host.create_library(Some("Created".into()), setup.clone())).unwrap();
    assert_eq!(
        setup.written(),
        vec![
            Some(BridgeFastValue::SetupProgress {
                progress: BridgeSetupProgress {
                    step: BridgeSetupStep::CreatingLibrary,
                    done: 0,
                    total: 0,
                },
            }),
            None,
        ]
    );
}
