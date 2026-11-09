//! Library writes through the bridge object the platforms hold.

use std::sync::Arc;

/// A handle over a fresh library holding `fixture`, built the way the app
/// builds one.
fn bridge_handle_holding(
    test_name: &str,
    fixture: &bae_library::LibraryFixture,
) -> crate::handle::tests::TestHandle {
    let (runtime, library, root) = crate::handle::tests::fresh_test_library(test_name);
    runtime.block_on(library.manager().write_fixture(fixture)).expect("write the library fixture");
    crate::handle::tests::start_test_handle(runtime, library, root)
}

/// Deleting an album through the handle takes it out of the grid and leaves
/// the other albums; deleting it again is refused, since the library no
/// longer holds it.
#[test]
fn delete_album_takes_the_album_out_of_the_grid() {
    let album = |title: &str| bae_library::FixtureAlbum {
        title: title.to_string(),
        artists: vec!["Artist Name".to_string()],
        tracks: vec!["Track Title".to_string()],
        cover: None,
    };
    let handle = bridge_handle_holding(
        "delete-album",
        &bae_library::LibraryFixture {
            albums: vec![album("Deleted Album"), album("Kept Album")],
            watched_folders: Vec::new(),
            playing: None,
        },
    );
    let subscription = handle.handle().subscribe_album_browse(Vec::new(), false);
    subscription
        .set_windows(vec![crate::types::BridgeLibraryPageWindow { offset: 0, limit: 10 }])
        .expect("request the grid's window");
    let titles = |rows: &[crate::types::BridgeAlbumAppearance]| -> Vec<String> {
        rows.iter().map(|row| row.album.title.clone()).collect()
    };
    let listed = loop {
        let snapshot = handle
            .handle()
            .runtime
            .block_on(subscription.clone().next())
            .expect("album browse snapshot");
        if !snapshot.windows.is_empty() {
            break snapshot;
        }
    };
    assert_eq!(titles(&listed.windows[0].rows), ["Kept Album", "Deleted Album"]);
    let deleted = listed.windows[0]
        .rows
        .iter()
        .find(|row| row.album.title == "Deleted Album")
        .expect("the album to delete is listed")
        .album
        .id
        .clone();

    handle
        .handle()
        .runtime
        .block_on(Arc::clone(handle.handle()).delete_album(deleted.clone()))
        .expect("delete the album");

    let after = loop {
        let snapshot = handle
            .handle()
            .runtime
            .block_on(subscription.clone().next())
            .expect("album browse snapshot");
        if snapshot.total_count == 1 {
            break snapshot;
        }
    };
    assert_eq!(titles(&after.windows[0].rows), ["Kept Album"]);
    let again = handle.handle().runtime.block_on(Arc::clone(handle.handle()).delete_album(deleted));
    assert!(matches!(again, Err(crate::types::BridgeError::Diagnostic { .. })), "{again:?}");
}

/// Setting S3 up as the library's storage writes the steps coven reports into
/// the setup's stream, the last its creating the library in storage, and the
/// stream holds nothing once the setup ends.
#[test]
fn setting_up_s3_storage_writes_its_steps() {
    let root = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()
        .unwrap();
    let (library, storage) = runtime.block_on(bae_library::test_libraries::storage_library(
        root.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    ));
    let coven::StorageConfig::S3 { bucket, region, endpoint, profile } =
        coven::Storage::config(storage.as_ref())
    else {
        panic!("the memory provider uses S3 configuration");
    };
    let services = runtime.block_on(async move {
        bae_app::test_services::start_services(&library)
            .await
            .expect("the test library's services start")
    });
    let host = crate::BridgeHost::with_test_keyring(
        crate::BridgeDiagnostics::noop(),
        crate::BridgeAppDir::new(root.path().join("host").to_str().unwrap().into()).unwrap(),
    )
    .unwrap();
    let app = Arc::new(
        super::AppHandle::start(
            services,
            runtime,
            bae_diagnostics::Diagnostics::noop(),
            #[cfg(feature = "cast")]
            Arc::new(coven::SequentialIds::new()),
            crate::handle::tests::test_host(root.path()),
        )
        .unwrap(),
    );
    let setup = crate::BridgeLibrarySetup::new(host);

    app.runtime
        .block_on(app.clone().save_sync_config(
            crate::types::BridgeSaveSyncConfig {
                bucket,
                region,
                endpoint: endpoint.map(|url| url.to_string()),
                access_key: "test-access".into(),
                secret_key: "test-secret".into(),
                profile: profile.map(crate::types::BridgeS3Profile::from_core),
            },
            "Test Device".into(),
            Some(setup.clone()),
        ))
        .unwrap();

    let written = setup.written();
    assert_eq!(written.last(), Some(&None), "{written:?}");
    assert_eq!(
        written.iter().rev().nth(1),
        Some(&Some(crate::types::BridgeFastValue::StorageSetupProgress {
            progress: crate::types::BridgeStorageSetupProgress {
                step: crate::types::BridgeStorageSetupStep::CreatingStore,
                done: 1,
                total: 1,
            },
        })),
        "{written:?}"
    );
    app.close_library(None).unwrap();
}
