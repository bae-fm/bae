//! Library writes through the bridge object the platforms hold.

use std::sync::Arc;

/// A handle over a fresh library holding `fixture`, built the way the app
/// builds one.
fn bridge_handle_holding(
    fixture: &bae_library::LibraryFixture,
) -> crate::handle::tests::TestHandle {
    let (runtime, library, root) = crate::handle::tests::fresh_test_library();
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
    let handle = bridge_handle_holding(&bae_library::LibraryFixture {
        albums: vec![album("Deleted Album"), album("Kept Album")],
        watched_folders: Vec::new(),
        playing: None,
    });
    let list = crate::types::BridgeList::AlbumGrid { sort: Vec::new(), group_by_artist: false };
    let subscription = handle.handle().subscribe_list_view(list.clone(), 0, 9).unwrap();
    let counts = handle.handle().subscribe_list_count(list).unwrap();
    let titles = |value: &crate::types::BridgeListView| -> Vec<String> {
        let crate::types::BridgeListRows::Albums { rows } = &value.rows else {
            panic!("the album grid's window holds albums");
        };
        rows.iter().map(|row| row.album.title.clone()).collect()
    };
    let listed =
        handle.handle().runtime.block_on(subscription.clone().next()).expect("album window value");
    assert_eq!(titles(&listed), ["Kept Album", "Deleted Album"]);
    let crate::types::BridgeListRows::Albums { rows } = &listed.rows else {
        panic!("the album grid's window holds albums");
    };
    let deleted = rows
        .iter()
        .find(|row| row.album.title == "Deleted Album")
        .expect("the album to delete is listed")
        .album
        .id
        .clone();
    let count = handle.handle().runtime.block_on(counts.clone().next()).expect("album count");
    assert_eq!(count.rows, 2);

    handle
        .handle()
        .runtime
        .block_on(Arc::clone(handle.handle()).delete_album(deleted.clone()))
        .expect("delete the album");

    let after =
        handle.handle().runtime.block_on(subscription.clone().next()).expect("album window value");
    assert_eq!(titles(&after), ["Kept Album"]);
    let count = handle.handle().runtime.block_on(counts.clone().next()).expect("album count");
    assert_eq!(count.rows, 1);
    let again = handle.handle().runtime.block_on(Arc::clone(handle.handle()).delete_album(deleted));
    assert!(matches!(again, Err(crate::types::BridgeError::Diagnostic { .. })), "{again:?}");
}

/// The queue's upcoming tracks are a list of the bridge like any other: with
/// nothing playing its view holds no rows and its count is zero, and the
/// view's rows come as queue entries.
#[test]
fn the_upcoming_list_of_an_idle_queue_is_empty() {
    let handle = bridge_handle_holding(&bae_library::LibraryFixture {
        albums: Vec::new(),
        watched_folders: Vec::new(),
        playing: None,
    });
    let list = crate::types::BridgeList::Upcoming { shuffled: false };
    let window = handle.handle().subscribe_list_view(list.clone(), 0, 9).unwrap();
    let counts = handle.handle().subscribe_list_count(list).unwrap();
    let runtime = &handle.handle().runtime;

    let listed = runtime.block_on(window.clone().next()).expect("upcoming window value");
    let crate::types::BridgeListRows::Upcoming { rows } = &listed.rows else {
        panic!("the upcoming list's window holds queue entries");
    };
    assert!(rows.is_empty() && listed.start == 0);
    assert_eq!(runtime.block_on(counts.clone().next()).expect("upcoming count").rows, 0);
}

/// Setting S3 up as the library's storage writes the steps coven reports into
/// the setup's stream, the last its creating the library in storage, and the
/// stream holds nothing once the setup ends.
#[test]
fn setting_up_s3_storage_writes_its_steps() {
    let root = bae_util::test_files::TestFolder::new();
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
        crate::host::start_handle(
            crate::handle::tests::test_host(root.path()),
            bae_diagnostics::Diagnostics::noop(),
        )(services, runtime)
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
            Some(setup.clone()),
        ))
        .unwrap();

    let written = setup.written();
    assert_eq!(written.last(), Some(&None), "{written:?}");
    assert_eq!(
        written.iter().rev().nth(1),
        Some(&Some(crate::types::BridgeFastValue::StorageSetupProgress {
            progress: crate::types::BridgeStorageSetupProgress::from_core(coven::Progress {
                step: coven::StorageSetupStep::CreatingStore,
                done: 1,
                total: 1,
            }),
        })),
        "{written:?}"
    );
    app.close_library(None).unwrap();
}
