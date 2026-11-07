use super::*;
use bae_util::test_channels::HANG_GUARD;

/// A follow whose store closes hears why and then ends, reporting no bug:
/// its task stops instead of reading the closed store again and again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_follow_ends_once_its_store_closes() {
    let directory = tempfile::TempDir::new().unwrap();
    let db = bae_store::Store::new_test(
        crate::TEST_SCHEMA,
        directory.path(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    let (heard, mut reports) = tokio::sync::mpsc::unbounded_channel();
    let mut follow = LibraryFollow::new(
        crate::tables::subscribe_playback_track_infos(&db, Vec::new()),
        Vec::new(),
        "the tracks' sides",
        &bae_diagnostics::Diagnostics::noop()
            .reporting_bugs_to(std::sync::Arc::new(HeardBugs(heard))),
    );

    db.close().await.unwrap();

    follow_ends(&mut follow).await;
    assert_eq!(reports.try_recv().ok(), None, "closing the store is no bug");
}

/// Wait for `follow`'s task to end, seen as its reads closing: the task
/// holds their sender until it ends.
async fn follow_ends<Request, Value>(follow: &mut LibraryFollow<Request, Value>) {
    while tokio::time::timeout(HANG_GUARD, follow.reads.recv())
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
    let db = bae_store::Store::new_test(
        crate::TEST_SCHEMA,
        directory.path(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    let mut follow = LibraryFollow::new(
        crate::tables::subscribe_playback_track_infos(&db, Vec::new()),
        Vec::new(),
        "the tracks' sides",
        &bae_diagnostics::Diagnostics::noop(),
    );
    db.close().await.unwrap();
    follow_ends(&mut follow).await;

    follow.follow(vec!["another track".to_string()]);

    assert_eq!(follow.request, vec!["another track".to_string()]);
}

/// Sends the operation of each bug reported, for a test to wait on.
struct HeardBugs(tokio::sync::mpsc::UnboundedSender<(String, String)>);

impl bae_diagnostics::BugReporter for HeardBugs {
    fn report(&self, operation: &str, detail: &str) {
        self.0
            .send((operation.to_string(), detail.to_string()))
            .expect("the test hears the report");
    }
}

/// A read that fails while its store is open is a bug, reported through
/// diagnostics: playback learns of a deleted track only from these reads.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_read_is_reported_as_a_bug() {
    let directory = tempfile::TempDir::new().unwrap();
    let db = bae_store::Store::new_test(
        crate::TEST_SCHEMA,
        directory.path(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    let (heard, mut reports) = tokio::sync::mpsc::unbounded_channel();
    let diagnostics = bae_diagnostics::Diagnostics::noop()
        .reporting_bugs_to(std::sync::Arc::new(HeardBugs(heard)));
    let query = db.subscribe_reconfigurable(Vec::<String>::new(), |_, sql| {
        sql.query_row("SELECT id FROM no_such_table", [], |row| row.get::<_, String>(0))
            .map_err(bae_store::DbError::from)
    });
    let _follow = LibraryFollow::new(query, Vec::new(), "the missing table", &diagnostics);

    let (operation, detail) = tokio::time::timeout(HANG_GUARD, reports.recv())
        .await
        .expect("the failed read is reported")
        .expect("the reporter stays open");
    assert_eq!(operation, "read the missing table for playback");
    assert!(detail.contains("no_such_table"), "{detail}");
    db.close().await.unwrap();
}

/// A store that failed on an earlier bug repeats it to every read as
/// `StoreFailed`; coven reported that bug as it happened, so the follow's
/// read reports it no more. Another failed read is reported.
#[test]
fn a_store_failed_on_an_earlier_bug_is_not_reported_again() {
    let reporter = std::sync::Arc::new(bae_diagnostics::RecordingBugReporter::default());
    let diagnostics = bae_diagnostics::Diagnostics::noop().reporting_bugs_to(reporter.clone());
    let bug = coven::task::Bug { operation: "authorize a statement", message: "boom".into() };

    failed_read(
        &diagnostics,
        "read the tracks for playback",
        "the tracks",
        &Vec::<String>::new(),
        &bae_store::DbError::from(coven::DbError::StoreFailed(bug)),
    );
    assert_eq!(reporter.reports(), Vec::new());

    failed_read(
        &diagnostics,
        "read the tracks for playback",
        "the tracks",
        &Vec::<String>::new(),
        &bae_store::DbError::from(coven::DbError::StatementFailed),
    );
    assert_eq!(reporter.reports().len(), 1);
}

/// A run that panicked carries its bug back to the follow, which awaits the
/// query, so the follow reports it once, as the panic it is.
#[test]
fn a_run_that_panicked_is_reported_as_its_bug() {
    let reporter = std::sync::Arc::new(bae_diagnostics::RecordingBugReporter::default());
    let diagnostics = bae_diagnostics::Diagnostics::noop().reporting_bugs_to(reporter.clone());
    let bug = coven::task::Bug { operation: "live query", message: "a run broke".into() };

    failed_read(
        &diagnostics,
        "read the tracks for playback",
        "the tracks",
        &Vec::<String>::new(),
        &bae_store::DbError::from(coven::DbError::Bug(bug)),
    );

    assert_eq!(
        reporter.reports(),
        vec![("live query".to_string(), "live query panicked: a run broke".to_string())]
    );
}
