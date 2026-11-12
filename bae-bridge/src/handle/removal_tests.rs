use super::*;
use std::sync::Arc;

/// A removal that stops after leaving the store is reported, read back by
/// the host after the library is gone, and finished there.
#[cfg(unix)]
#[test]
fn an_unfinished_removal_is_reported_and_finished_by_the_host() {
    use std::os::unix::fs::PermissionsExt;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .unwrap();
    let (library, root) = bae_library::test_libraries::setup_fresh_library(&runtime);
    std::fs::create_dir_all(library.manager().library_path().join("cache")).unwrap();
    let services = runtime.block_on(async move {
        bae_app::test_services::start_services(&library)
            .await
            .expect("the test library's services start")
    });
    let host = crate::handle::tests::test_host(root.path());
    let app = Arc::new(
        crate::host::start_handle(host.clone(), bae_diagnostics::Diagnostics::noop())(
            services, runtime,
        )
        .unwrap(),
    );
    let libraries = bae_app_dir::AppDir::under_home(root.path()).unwrap().libraries();
    std::fs::set_permissions(&libraries, std::fs::Permissions::from_mode(0o555)).unwrap();
    let caller = tokio::runtime::Runtime::new().unwrap();
    let result = caller.block_on(app.clone().remove_library(None)).unwrap();
    let BridgeLibraryRemoval::Unfinished { removal } = result else {
        panic!("the directory left behind must be reported");
    };
    assert!(removal.error.to_string().contains("store was removed"), "{}", removal.error);
    drop(app);

    caller.block_on(async {
        assert!(host.clone().unfinished_removal().await.unwrap().is_some());
        assert!(host
            .clone()
            .finish_removals(crate::BridgeLibrarySetup::new(host.clone()))
            .await
            .is_err());
        std::fs::set_permissions(&libraries, std::fs::Permissions::from_mode(0o755)).unwrap();
        let setup = crate::BridgeLibrarySetup::new(host.clone());
        host.clone().finish_removals(setup.clone()).await.unwrap();
        let finishing = |done, total| {
            Some(crate::types::BridgeFastValue::SetupProgress {
                progress: crate::types::BridgeSetupProgress::new(
                    crate::types::BridgeSetupStep::FinishingRemovals,
                    done,
                    total,
                ),
            })
        };
        assert_eq!(
            setup.written(),
            vec![finishing(0, 0), finishing(0, 1), finishing(1, 1), None],
            "finishing counts the removal it finishes"
        );
        assert!(host.clone().unfinished_removal().await.unwrap().is_none());
        host.clone().finish_removals(crate::BridgeLibrarySetup::new(host.clone())).await.unwrap();
    });
}
