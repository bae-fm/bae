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
