use super::LiveReadError;
use std::sync::Arc;

/// A read whose query ended once its store closed ends with it, and a new
/// request to it reads nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_read_whose_query_ended_takes_a_new_request_and_reads_nothing() {
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
    let read = crate::subscribe_queue_upcoming(&manager);
    database.close().await.unwrap();
    // Reads made before the close may still be delivered.
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        while !matches!(read.next().await, Err(LiveReadError::Cancelled)) {}
    })
    .await
    .expect("the read ends with its store");

    read.set([bae_store::LibraryPageWindow { offset: 0, limit: 10 }].into())
        .expect("the read takes the request");

    assert!(
        matches!(read.next().await, Err(LiveReadError::Cancelled)),
        "the ended read reads nothing"
    );
}
