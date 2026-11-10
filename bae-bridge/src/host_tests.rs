#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use super::*;
#[test]
fn an_opened_store_keeps_its_original_custody() {
    let root = tempfile::tempdir().unwrap();
    let host = crate::handle::tests::test_host(root.path());
    let caller = tokio::runtime::Runtime::new().unwrap();
    let opened = crate::handle::tests::create_opened_library(&host, &caller, "Session Library");
    let app = opened.open(50, false).unwrap();
    let id = opened.library_id();
    drop(opened);
    drop(host);
    assert_eq!(caller.block_on(app.subscribe_config().next()).unwrap().library_id, id);
    assert_eq!(caller.block_on(app.clone().library_name()).unwrap(), "Session Library");
    app.close_library(None).unwrap();
}

/// A host over a temporary bae directory, as a test or a preview starts one,
/// takes the directory with it when it ends.
#[test]
fn a_temporary_bae_directory_ends_with_its_host() {
    let app_dir = BridgeAppDir::temporary().unwrap();
    let path = app_dir.core().libraries().parent().unwrap().to_path_buf();
    let host = BridgeHost::with_test_keyring(BridgeDiagnostics::noop(), app_dir).unwrap();
    assert!(path.is_dir(), "{path:?} is the host's bae directory while it runs");
    drop(host);
    assert!(!path.exists(), "{path:?} outlived its host");
}

#[test]
fn bootstrap_error_maps_to_failure_kind() {
    // Every BootstrapError variant maps to its app_start_failed kind; this is
    // the seam init_app emits through before returning the bridge error.
    assert_eq!(
        app_start_failure_kind(&BootstrapError::LibraryNotFound("x".to_string())),
        AppStartFailureKind::LibraryNotFound
    );
    assert_eq!(
        app_start_failure_kind(&BootstrapError::Config("x".to_string())),
        AppStartFailureKind::Config
    );
    assert_eq!(
        app_start_failure_kind(&BootstrapError::Database("x".to_string())),
        AppStartFailureKind::Database
    );
    assert_eq!(
        app_start_failure_kind(&BootstrapError::Internal("x".to_string())),
        AppStartFailureKind::Internal
    );
    assert_eq!(
        app_start_failure_kind(&BootstrapError::KeyringUnavailable),
        AppStartFailureKind::KeyringUnavailable
    );
    assert_eq!(
        app_start_failure_kind(&BootstrapError::KeychainEntitlementMissing(
            keyring_core::Error::BadStoreFormat("errSecMissingEntitlement".into()).into()
        )),
        AppStartFailureKind::KeychainEntitlementMissing
    );
    assert_eq!(
        app_start_failure_kind(&BootstrapError::ReplacementUnfinished(coven::ReplacedBy::Restore)),
        AppStartFailureKind::ReplacementUnfinished
    );
    assert_eq!(
        app_start_failure_kind(&BootstrapError::DeletionUnfinished),
        AppStartFailureKind::DeletionUnfinished
    );
}

/// A build without its keychain entitlement crosses as its own line, not
/// as a broken keyring or a locked one.
#[test]
fn a_missing_keychain_entitlement_crosses_as_its_own_category() {
    let error = bootstrap_error_to_bridge(BootstrapError::KeychainEntitlementMissing(
        keyring_core::Error::BadStoreFormat("errSecMissingEntitlement".into()).into(),
    ));
    let BridgeError::Diagnostic { category, .. } = &error else {
        panic!("a missing entitlement is a diagnostic: {error:?}");
    };
    assert_eq!(*category, crate::BridgeErrorCategory::KeychainEntitlementMissing);
    assert_eq!(
        crate::bridge_error_category_key(category.clone()),
        "core.error.keyring.entitlement_missing"
    );
}

/// An open that stopped on an unfinished replacement crosses as the call
/// that finishes it, each to its own line.
#[test]
fn an_unfinished_replacement_crosses_as_the_call_that_finishes_it() {
    for (core, bridge, key) in [
        (
            coven::ReplacedBy::Restore,
            crate::BridgeReplacedBy::Restore,
            "core.error.replacement_unfinished.restore",
        ),
        (
            coven::ReplacedBy::Reset,
            crate::BridgeReplacedBy::Reset,
            "core.error.replacement_unfinished.reset",
        ),
    ] {
        let error = bootstrap_error_to_bridge(BootstrapError::ReplacementUnfinished(core));
        let BridgeError::Diagnostic { category, .. } = &error else {
            panic!("an unfinished replacement is a diagnostic: {error:?}");
        };
        assert_eq!(*category, crate::BridgeErrorCategory::ReplacementUnfinished { by: bridge });
        assert_eq!(crate::types::bridge_error_line_key(&error).as_deref(), Some(key));
    }
}

/// An open of a library this device is removing crosses as its own line,
/// which says the removal didn't finish.
#[test]
fn an_unfinished_deletion_crosses_as_its_own_category() {
    let error = bootstrap_error_to_bridge(BootstrapError::DeletionUnfinished);
    let BridgeError::Diagnostic { category, .. } = &error else {
        panic!("an unfinished deletion is a diagnostic: {error:?}");
    };
    assert_eq!(*category, crate::BridgeErrorCategory::DeletionUnfinished);
    assert_eq!(
        crate::types::bridge_error_line_key(&error).as_deref(),
        Some("core.error.deletion_unfinished")
    );
}

/// A join or restore refused because another call holds the library's first
/// installation on this device crosses as that call to go on with, not as a
/// failure: the app resumes it, as at launch.
#[test]
fn another_call_under_way_crosses_as_that_call() {
    let home = tempfile::tempdir().unwrap();
    let store = coven::StoreId("00000000-0000-4000-8000-000000000001".parse().unwrap());
    let refused =
        bae_library::RestoreFromCodeError::Restore(Box::new(coven::BootstrapError::Directory(
            coven::BootstrapDirectoryError::Lock(coven::StoreLockError::BootstrapPending(store)),
        )));

    let host = crate::handle::tests::test_host(home.path());

    let crossed = crate::types::BridgeBootstrapResult::failure(refused, host);

    assert!(matches!(crossed, Ok(crate::types::BridgeBootstrapResult::CallUnderWay)));
}

/// The preset a person adds is core's FLAC preset, under an id of the host's
/// making that differs on every call.
#[test]
fn a_new_save_preset_is_cores_flac_preset_with_its_own_id() {
    use crate::types::{
        bridge_save_format_default_codec, bridge_save_format_name, BridgeSaveFormat,
        BridgeSavePregapPlacement,
    };
    let root = tempfile::tempdir().unwrap();
    let host = crate::handle::tests::test_host(root.path());
    let preset = host.new_save_preset();
    assert_eq!(preset.name, bridge_save_format_name(BridgeSaveFormat::Flac));
    assert_eq!(preset.codec, bridge_save_format_default_codec(BridgeSaveFormat::Flac));
    assert_eq!(preset.pregap_placement, BridgeSavePregapPlacement::AppendToPreviousExceptHtoa);
    assert!(preset.applies_to_track && preset.applies_to_release && preset.embed_cover);
    assert_ne!(preset.id, host.new_save_preset().id);
}

/// A library the host has open stands by its open session's live sync
/// status: coven's read of a closed store refuses a store a handle runs, so
/// only the open library can answer. Once its handle is released, the library
/// is read as it last stood.
#[test]
fn an_open_library_stands_by_its_open_session() {
    let root = tempfile::tempdir().unwrap();
    let host = crate::handle::tests::test_host(root.path());
    let caller = tokio::runtime::Runtime::new().unwrap();
    let opened = crate::handle::tests::create_opened_library(&host, &caller, "Open Library");
    let app = opened.open(50, false).unwrap();
    let id = opened.library_id();
    drop(opened);

    let open = caller.block_on(host.clone().library_standing(id.clone())).unwrap();

    assert!(
        matches!(
            &open.state,
            crate::BridgeStandingState::Sync { shown }
                if shown.status == crate::BridgeSyncStatusWord::NotSynced
        ),
        "{open:?}"
    );
    assert_eq!(open.last_synced, None);

    drop(app);
    let closed = caller.block_on(host.clone().library_standing(id)).unwrap();

    assert_eq!(closed, open);
}

/// A library closed while the app still holds its handle has left the host's
/// record of open libraries: its standing is read from the closed store, never
/// asked of the closed handle.
#[test]
fn a_closed_library_whose_handle_is_still_held_is_read_closed() {
    let root = tempfile::tempdir().unwrap();
    let host = crate::handle::tests::test_host(root.path());
    let caller = tokio::runtime::Runtime::new().unwrap();
    let opened = crate::handle::tests::create_opened_library(&host, &caller, "Closed Library");
    let app = opened.open(50, false).unwrap();
    let id = opened.library_id();
    let store = crate::from_app::store(&id).unwrap();
    drop(opened);

    app.close_library(None).unwrap();
    let standing = caller.block_on(host.clone().library_standing(id)).unwrap();

    let closed = caller
        .block_on(bae_library::closed_library_standing(&host.coven, store, &host.diagnostics))
        .unwrap();
    assert_eq!(standing, crate::BridgeLibraryStanding::from_core(closed));
    drop(app);
}

/// A standing asked while the library closes waits for the close to end,
/// then reads the closed store: the library left the record as its close was
/// recorded, so the call finds the close and never the half-closed library.
#[test]
fn a_standing_asked_during_a_close_waits_for_it_then_reads_the_closed_store() {
    let root = tempfile::tempdir().unwrap();
    let host = crate::handle::tests::test_host(root.path());
    let caller = tokio::runtime::Runtime::new().unwrap();
    let opened = crate::handle::tests::create_opened_library(&host, &caller, "Closing Library");
    let app = opened.open(50, false).unwrap();
    let id = opened.library_id();
    let store = crate::from_app::store(&id).unwrap();
    drop(opened);
    let closing = crate::BridgeLibraryClosing::new(host.clone());
    let (reached, release) = closing.hold_at(crate::types::BridgeClosingStep::StoppingPlayback);
    let closer = std::thread::spawn({
        let app = app.clone();
        move || app.close_library(Some(closing))
    });
    reached.recv_timeout(bae_util::test_channels::HANG_GUARD).expect("the close starts");

    let standing = caller.spawn(host.clone().library_standing(id));
    host.library_sessions.wait_for_waiter(store);
    release.send(()).unwrap();
    closer.join().unwrap().unwrap();
    let standing = caller.block_on(standing).unwrap().unwrap();

    let closed = caller
        .block_on(bae_library::closed_library_standing(&host.coven, store, &host.diagnostics))
        .unwrap();
    assert_eq!(standing, crate::BridgeLibraryStanding::from_core(closed));
    drop(app);
}

/// A standing call that found the library open holds its session open: a
/// close that begins while the call runs waits for it, so the call answers
/// from the open library rather than failing on a runtime shutting down.
#[test]
fn a_standing_running_as_its_library_closes_answers_before_the_close_runs() {
    /// What the test hears first once the close has begun.
    enum Heard {
        CloseWaits,
        CloseEnded(Result<(), crate::BridgeError>),
    }
    let root = tempfile::tempdir().unwrap();
    let host = crate::handle::tests::test_host(root.path());
    let caller = tokio::runtime::Runtime::new().unwrap();
    let opened = crate::handle::tests::create_opened_library(&host, &caller, "Busy Library");
    let app = opened.open(50, false).unwrap();
    let id = opened.library_id();
    drop(opened);
    let (reached, release) = host.library_sessions.hold_next_call();
    let standing = caller.spawn(host.clone().library_standing(id));
    reached.recv_timeout(bae_util::test_channels::HANG_GUARD).expect("the call finds the library");

    let (telling, heard) = std::sync::mpsc::channel();
    let closer = std::thread::spawn({
        let app = app.clone();
        let telling = telling.clone();
        move || telling.send(Heard::CloseEnded(app.close_library(None))).unwrap()
    });
    std::thread::spawn({
        let host = host.clone();
        move || {
            host.library_sessions.wait_for_close_waiting_on_calls();
            // The test has gone on once the close ended first.
            drop(telling.send(Heard::CloseWaits));
        }
    });
    let first = heard.recv_timeout(bae_util::test_channels::HANG_GUARD).unwrap();
    release.send(()).unwrap();
    let standing = caller.block_on(standing).unwrap();

    let standing = standing.expect("the call answers from the open library");
    assert!(
        matches!(
            &standing.state,
            crate::BridgeStandingState::Sync { shown }
                if shown.status == crate::BridgeSyncStatusWord::NotSynced
        ),
        "{standing:?}"
    );
    assert!(matches!(first, Heard::CloseWaits), "the close ran while a call on its library ran");
    closer.join().unwrap();
    let Ok(Heard::CloseEnded(closed)) = heard.recv() else {
        panic!("the close ends after the call");
    };
    closed.unwrap();
    drop(app);
}
