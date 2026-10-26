// ── A write of a standing that fails ────────────────────────────────────────

/// Make every write that would store `stage` as a candidate's identification
/// fail, as a full disk fails it.
async fn fail_writes_of(runtime: &CandidateRuntime, stage: &str) {
    let when = format!("NEW.identification = '{stage}'");
    fail_standing_writes(
        runtime
            ._test_directory
            .as_ref()
            .expect("a test runtime keeps its directory")
            .path(),
        &[("INSERT", when.as_str()), ("UPDATE", when.as_str())],
    )
    .await;
}

/// A run's start that cannot be written fails to the driver that reported
/// it, and the table, the count and what the runtime draws stay as they were.
#[tokio::test]
async fn a_start_that_cannot_be_written_fails_and_leaves_the_table_as_it_was() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    let files = scanned_key(&runtime, key).await;
    runtime
        .admit(vec![(key.to_string(), files.clone())], Admission::Requested)
        .await
        .unwrap();
    let before = runtime.stored_standing_for_test(key).await;
    fail_writes_of(&runtime, "running").await;

    let error = runtime.run_started(key, &files, run(1)).await.unwrap_err();
    assert!(error.to_string().contains("the disk is full"), "{error}");

    assert_eq!(
        runtime.stored_standing_for_test(key).await,
        before
    );
    assert_eq!(runtime.identification_progress(), (0, 1));
    assert_eq!(
        runtime.snapshot(key).await.unwrap().and_then(|runtime| runtime.identification),
        Some(IdentificationRuntime::Queued(Admission::Requested))
    );
}

/// An admission that cannot be written fails whole: none of its candidates
/// is marked, and the count does not open.
#[tokio::test]
async fn an_admission_that_cannot_be_written_marks_nothing() {
    let runtime = CandidateRuntime::for_test().await;
    let first = "/watch/a/rel1";
    let second = "/watch/a/rel2";
    let marks = vec![
        (first.to_string(), scanned_key(&runtime, first).await),
        (second.to_string(), scanned_key(&runtime, second).await),
    ];
    fail_writes_of(&runtime, "queued").await;

    let error = runtime.admit(marks, Admission::Automatic).await.unwrap_err();
    assert!(error.to_string().contains("the disk is full"), "{error}");

    assert!(runtime.snapshots().await.unwrap().is_empty());
    assert_eq!(runtime.identification_progress(), (0, 0));
}
