//! A setup call's steps, as the app reads them off the call's stream.

use super::*;
use crate::types::{
    BridgeBootstrapState, BridgeBootstrapStatus, BridgeBootstrapStep, BridgeLoadProgress,
    BridgeLoadStep, BridgeSetupProgress, BridgeSetupStep,
};
use bae_util::test_channels::HANG_GUARD;

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
    let steps = crate::screens::SetupFeed::new(Some(setup.clone())).steps();

    steps.step(BridgeSetupStep::RemovingLibraries, 1, 3);
    assert_eq!(
        stream.latest(Vec::new()),
        Some(BridgeFastValue::SetupProgress {
            progress: BridgeSetupProgress::new(BridgeSetupStep::RemovingLibraries, 1, 3),
        })
    );
    assert_eq!(runtime.block_on(stream.clone().next()), Ok(vec![Vec::new()]), "the key started");

    drop(steps);
    assert_eq!(stream.latest(Vec::new()), None);
    assert_eq!(runtime.block_on(stream.clone().next()), Ok(vec![Vec::new()]), "the key stopped");
}

/// Creating a library runs one step, which ends with the call.
#[test]
fn creating_a_library_reports_its_step() {
    let root = tempfile::tempdir().unwrap();
    let host = crate::handle::tests::test_host(root.path());
    let setup = BridgeLibrarySetup::new(host.clone());
    let caller = tokio::runtime::Runtime::new().unwrap();
    caller
        .block_on(host.create_library(
            Some("Created".into()),
            "Creator".into(),
            "Test Device".into(),
            setup.clone(),
        ))
        .unwrap();
    assert_eq!(
        setup.written(),
        vec![
            Some(BridgeFastValue::SetupProgress {
                progress: BridgeSetupProgress::new(BridgeSetupStep::CreatingLibrary, 0, 0),
            }),
            None,
        ]
    );
}

/// A blank creator's name fails the call before anything is made: no library
/// is listed and no step ran.
#[test]
fn a_blank_creator_name_creates_nothing() {
    let root = tempfile::tempdir().unwrap();
    let host = crate::handle::tests::test_host(root.path());
    let setup = BridgeLibrarySetup::new(host.clone());
    let caller = tokio::runtime::Runtime::new().unwrap();
    let error = caller
        .block_on(host.clone().create_library(
            None,
            "  ".into(),
            "Test Device".into(),
            setup.clone(),
        ))
        .err()
        .expect("a blank name is refused");
    assert!(matches!(error, crate::BridgeError::Diagnostic { .. }), "{error:?}");
    assert!(caller.block_on(host.list_libraries()).unwrap().is_empty());
    assert!(setup.written().is_empty());
}

/// Where a restore stands reaches its subscription as each state changes,
/// the latest first; the counts of a step go to the stream alone, so a
/// count moving wakes no pull, and a step without counts empties the stream.
#[test]
fn a_restore_s_states_are_pulled_and_its_counts_are_streamed() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let setup = library_setup(OperationRuntime::new(
        runtime.handle().clone(),
        bae_diagnostics::Diagnostics::noop(),
    ));
    let stream = setup.feed_setup_steps();
    let states = setup.subscribe_states();
    let steps = crate::screens::SetupFeed::new(Some(setup.clone())).steps();
    let next_state = || {
        runtime.block_on(async {
            tokio::time::timeout(HANG_GUARD, states.clone().next())
                .await
                .expect("a changed state is pulled")
        })
    };
    let loading = |done| {
        coven::BootstrapStatus::Loading(coven::Progress {
            step: coven::LoadStep::Reading,
            done,
            total: 8,
        })
    };

    steps.bootstrap(coven::BootstrapStatus::Connecting, true);
    assert_eq!(
        next_state(),
        Ok(BridgeBootstrapState { status: BridgeBootstrapStatus::Connecting, cancellable: true })
    );
    assert_eq!(stream.latest(Vec::new()), None, "connecting counts nothing");

    steps.bootstrap(loading(2), true);
    steps.bootstrap(loading(3), true);
    assert_eq!(
        stream.latest(Vec::new()),
        Some(BridgeFastValue::BootstrapStep {
            step: BridgeBootstrapStep::Loading {
                progress: BridgeLoadProgress::new(BridgeLoadStep::Reading, 3, 8),
            },
        })
    );
    assert_eq!(
        next_state(),
        Ok(BridgeBootstrapState { status: BridgeBootstrapStatus::Loading, cancellable: true }),
        "two counts are one loading state"
    );

    let watched = setup.states.states.subscribe();
    steps.bootstrap(loading(4), true);
    assert!(!watched.has_changed().unwrap(), "a count moving changes no state");
    steps.bootstrap(coven::BootstrapStatus::Registering, true);
    assert_eq!(stream.latest(Vec::new()), None, "registering counts nothing");
    assert_eq!(
        next_state(),
        Ok(BridgeBootstrapState { status: BridgeBootstrapStatus::Registering, cancellable: true }),
        "the count that moved woke no pull: the next is the next state"
    );
}

/// Whether a storage setup is past its point of no return reaches its
/// subscription as it changes: not while it connects or loads, whose counts
/// go to the stream alone, then once it creates the library in storage.
#[test]
fn a_storage_setup_s_finishing_is_pulled_as_it_changes() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let setup = library_setup(OperationRuntime::new(
        runtime.handle().clone(),
        bae_diagnostics::Diagnostics::noop(),
    ));
    let finishing = setup.subscribe_storage_finishing();
    let steps = crate::screens::SetupFeed::new(Some(setup.clone())).steps();
    let next = || {
        runtime.block_on(async {
            tokio::time::timeout(HANG_GUARD, finishing.clone().next())
                .await
                .expect("a changed finishing is pulled")
        })
    };
    let step = |step, done, total| coven::Progress { step, done, total };

    steps.storage(step(coven::StorageSetupStep::Connecting, 0, 1));
    assert_eq!(next(), Ok(false));

    let watched = setup.storage_finishing.states.subscribe();
    let loading = || coven::StorageSetupStep::Loading(coven::LoadStep::Reading);
    steps.storage(step(loading(), 2, 8));
    steps.storage(step(loading(), 3, 8));
    assert!(!watched.has_changed().unwrap(), "loading is not past the point of no return");

    steps.storage(step(coven::StorageSetupStep::CreatingStore, 0, 1));
    assert_eq!(next(), Ok(true));
}
