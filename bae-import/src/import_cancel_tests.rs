#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use super::*;

#[tokio::test]
async fn a_waiting_import_cancelled_never_runs() {
    let cancels = ImportCancels::new();
    cancels.register(&crate::CandidateKey::test("Album"), "import-1");

    assert_eq!(
        cancels.cancel(&crate::CandidateKey::test("Album")),
        CancelOutcome::CancelledWaiting { import_id: "import-1".to_string() }
    );
    let end = cancels
        .run(&crate::CandidateKey::test("Album"), "import-1", async {
            panic!("a cancelled import never runs")
        })
        .await;
    assert!(matches!(end, ImportRunEnd::<()>::CancelledWaiting));
    assert_eq!(cancels.cancel(&crate::CandidateKey::test("Album")), CancelOutcome::NotImporting);
}

#[tokio::test]
async fn a_running_import_cancelled_is_dropped_before_it_writes() {
    let cancels = ImportCancels::new();
    cancels.register(&crate::CandidateKey::test("Album"), "import-1");
    let (started_tx, started) = tokio::sync::oneshot::channel();
    let run = {
        let cancels = cancels.clone();
        tokio::spawn(async move {
            cancels
                .run(&crate::CandidateKey::test("Album"), "import-1", async {
                    started_tx.send(()).unwrap();
                    std::future::pending::<Result<(), ImportStop>>().await
                })
                .await
        })
    };
    started.await.unwrap();

    assert_eq!(
        cancels.cancel(&crate::CandidateKey::test("Album")),
        CancelOutcome::CancelledRunning
    );
    assert!(matches!(run.await.unwrap(), ImportRunEnd::CancelledRunning));
    assert_eq!(cancels.cancel(&crate::CandidateKey::test("Album")), CancelOutcome::NotImporting);
}

#[tokio::test]
async fn an_import_writing_its_release_is_not_cancelled() {
    let cancels = ImportCancels::new();
    cancels.register(&crate::CandidateKey::test("Album"), "import-1");
    let writing = cancels.clone();
    let end = cancels
        .run(&crate::CandidateKey::test("Album"), "import-1", async move {
            writing.begin_writing(&crate::CandidateKey::test("Album"), "import-1")?;
            Ok(writing.cancel(&crate::CandidateKey::test("Album")))
        })
        .await;

    assert!(matches!(end, ImportRunEnd::Ran(Ok(CancelOutcome::Writing))));
}

#[test]
fn a_cancelled_import_cannot_begin_writing() {
    let cancels = ImportCancels::new();
    cancels.register(&crate::CandidateKey::test("Album"), "import-1");
    cancels.entries.lock().unwrap().get_mut(&crate::CandidateKey::test("Album")).unwrap().stage =
        Stage::Running;

    assert_eq!(
        cancels.cancel(&crate::CandidateKey::test("Album")),
        CancelOutcome::CancelledRunning
    );
    assert!(matches!(
        cancels.begin_writing(&crate::CandidateKey::test("Album"), "import-1"),
        Err(ImportStop::Cancelled)
    ));
}

/// A cancel that lands between the work's last await and its write is
/// refused at the write, and the run ends cancelled, not failed.
#[tokio::test]
async fn a_run_refused_its_write_ends_cancelled() {
    let cancels = ImportCancels::new();
    cancels.register(&crate::CandidateKey::test("Album"), "import-1");
    let writing = cancels.clone();
    let end = cancels
        .run(&crate::CandidateKey::test("Album"), "import-1", async move {
            writing.cancel(&crate::CandidateKey::test("Album"));
            writing.begin_writing(&crate::CandidateKey::test("Album"), "import-1")
        })
        .await;

    assert!(matches!(end, ImportRunEnd::CancelledRunning));
}

#[test]
fn cancelling_everything_spares_only_what_is_writing() {
    let cancels = ImportCancels::new();
    cancels.register(&crate::CandidateKey::test("Album 1"), "import-1");
    cancels.register(&crate::CandidateKey::test("Album 2"), "import-2");
    cancels.register(&crate::CandidateKey::test("Album 3"), "import-3");
    cancels.entries.lock().unwrap().get_mut(&crate::CandidateKey::test("Album 2")).unwrap().stage =
        Stage::Running;
    cancels.entries.lock().unwrap().get_mut(&crate::CandidateKey::test("Album 3")).unwrap().stage =
        Stage::Writing;

    assert_eq!(
        cancels.cancel_all(),
        vec![(crate::CandidateKey::test("Album 1"), "import-1".to_string())]
    );
    assert_eq!(cancels.cancel(&crate::CandidateKey::test("Album 3")), CancelOutcome::Writing);
    assert!(matches!(
        cancels.begin_writing(&crate::CandidateKey::test("Album 2"), "import-2"),
        Err(ImportStop::Cancelled)
    ));
}

/// A command cancelled while it waited finds the entry of the import that
/// followed it and leaves it alone: it is skipped, and the import that
/// holds the candidate now runs when the worker reaches it.
#[tokio::test]
async fn a_command_cancelled_while_waiting_leaves_the_next_import_alone() {
    let cancels = ImportCancels::new();
    cancels.register(&crate::CandidateKey::test("Album"), "import-1");
    cancels.cancel(&crate::CandidateKey::test("Album"));
    cancels.register(&crate::CandidateKey::test("Album"), "import-2");

    let end = cancels
        .run(&crate::CandidateKey::test("Album"), "import-1", async {
            panic!("the cancelled command never runs")
        })
        .await;
    assert!(matches!(end, ImportRunEnd::<()>::CancelledWaiting));
    assert!(matches!(
        cancels.begin_writing(&crate::CandidateKey::test("Album"), "import-1"),
        Err(ImportStop::Cancelled)
    ));
    cancels.forget(&crate::CandidateKey::test("Album"), "import-1");

    let end =
        cancels.run(&crate::CandidateKey::test("Album"), "import-2", async { Ok("ran") }).await;
    assert!(matches!(end, ImportRunEnd::Ran(Ok("ran"))));
}
