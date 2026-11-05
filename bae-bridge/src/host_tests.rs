use super::*;
#[test]
fn an_opened_store_keeps_its_original_custody() {
    let root = tempfile::tempdir().unwrap();
    let host = BridgeHost::with_test_keyring(
        BridgeDiagnostics::noop(),
        BridgeAppDir::new(root.path().to_str().unwrap().into()).unwrap(),
    )
    .unwrap();
    let caller = tokio::runtime::Runtime::new().unwrap();
    let opened = caller
        .block_on(host.clone().create_library(
            Some("Session Library".into()),
            crate::BridgeLibrarySetup::new(host.clone()),
        ))
        .unwrap();
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
