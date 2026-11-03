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
