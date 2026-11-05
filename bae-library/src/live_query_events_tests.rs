use std::sync::Arc;

/// A new request to a query that ended once its store closed reads
/// nothing, and leaves the subscription that sent it running.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_ended_query_takes_a_new_request() {
    let directory = tempfile::TempDir::new().unwrap();
    let path = directory.path().join("library.db");
    let database = bae_store::Store::new_test(
        crate::SCHEMA,
        path.to_str().unwrap(),
        Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    let manager = crate::test_libraries::TestLibrary::open(
        database.clone(),
        bae_app_dir::AppDir::under_home(directory.path()).unwrap(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let (reader, _read) = tokio::sync::mpsc::unbounded_channel::<Result<(), crate::LibraryError>>();
    let mut events = super::reconfigurable_live_query_events(
        &manager,
        manager.subscribe_queue_upcoming(bae_store::LibraryPageWindows::new()),
        reader,
    );
    database.close().await.unwrap();
    // Results read before the close may still be waiting to be taken.
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        while events.recv().await.is_some() {}
    })
    .await
    .expect("the query ends with its store");

    events.set([bae_store::LibraryPageWindow { offset: 0, limit: 10 }].into());

    assert!(events.recv().await.is_none(), "the ended query reads nothing");
}
