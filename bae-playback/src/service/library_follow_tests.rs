use super::*;

/// A follow whose store closes hears why and then ends: its task stops
/// instead of reading the closed store again and again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_follow_ends_once_its_store_closes() {
    let directory = tempfile::TempDir::new().unwrap();
    let path = directory.path().join("library.db");
    let database =
        bae_db::Database::new_test(path.to_str().unwrap(), std::sync::Arc::new(coven::SystemClock))
            .await
            .unwrap();
    let mut follow = LibraryFollow::new(
        crate::tables::subscribe_playback_track_infos(&database, Vec::new()),
        Vec::new(),
        "the tracks' sides",
        &bae_diagnostics::Diagnostics::noop(),
    );

    database.close().await.unwrap();

    follow_ends(&mut follow).await;
}

/// Wait for `follow`'s task to end, seen as its reads closing: the task
/// holds their sender until it ends.
async fn follow_ends<Request, Value>(follow: &mut LibraryFollow<Request, Value>) {
    while tokio::time::timeout(std::time::Duration::from_secs(10), follow.reads.recv())
        .await
        .expect("the follow's task ends")
        .is_some()
    {}
}

/// Pointing a follow whose query ended at another request reads nothing,
/// and leaves the service that holds it running.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_ended_follow_takes_a_new_request() {
    let directory = tempfile::TempDir::new().unwrap();
    let path = directory.path().join("library.db");
    let database =
        bae_db::Database::new_test(path.to_str().unwrap(), std::sync::Arc::new(coven::SystemClock))
            .await
            .unwrap();
    let mut follow = LibraryFollow::new(
        crate::tables::subscribe_playback_track_infos(&database, Vec::new()),
        Vec::new(),
        "the tracks' sides",
        &bae_diagnostics::Diagnostics::noop(),
    );
    database.close().await.unwrap();
    follow_ends(&mut follow).await;

    follow.follow(vec!["another track".to_string()]);

    assert_eq!(follow.request, vec!["another track".to_string()]);
}
