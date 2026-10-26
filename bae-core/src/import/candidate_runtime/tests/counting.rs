// ── The identification count ────────────────────────────────────────────────
//
// What the count does over the writes of each candidate's identification.

/// The count as the sidebar reads it, if it moved since the last read.
fn counts(runtime: &CandidateRuntime, last: &mut (u32, u32)) -> Vec<(u32, u32)> {
    let now = runtime.identification_progress();
    if now == *last {
        return Vec::new();
    }
    *last = now;
    vec![now]
}

/// A Lookup a person starts is an identification like any other: it opens a
/// batch on its own, without a sweep pass behind it.
#[tokio::test]
async fn a_lookup_started_by_hand_opens_a_batch() {
    let runtime = CandidateRuntime::for_test().await;
    let mut last = (0, 0);
    let key = "/watch/a/rel1";
    let files = scanned_key(&runtime, key).await;

    runtime
        .admit(vec![(key.to_string(), files.clone())], Admission::Requested)
        .await
        .unwrap();
    assert_eq!(counts(&runtime, &mut last), vec![(0, 1)]);

    report(&runtime, key, &files, run(1), triangulating()).await;
    assert_eq!(
        counts(&runtime, &mut last),
        Vec::new(),
        "the run starting is the same identification, still waited on"
    );

    runtime.record_run_state(key, run(1), true, &manual_only());
    runtime.run_answered(key, run(1), false).await.unwrap();
    assert_eq!(
        counts(&runtime, &mut last),
        Vec::new(),
        "and so is the answer waiting on its write"
    );

    runtime.end_identification_answer(key, run(1)).await.unwrap();
    assert_eq!(
        counts(&runtime, &mut last),
        vec![(0, 0)],
        "the write landing ends the only identification, and the batch with it"
    );
}

/// The sweep's queue is a batch: what it publishes is admitted, and each
/// verdict advances the count until the last one ends it.
#[tokio::test]
async fn the_sweeps_queue_is_counted_and_drains_to_nothing() {
    let runtime = CandidateRuntime::for_test().await;
    let mut last = (0, 0);
    let first = "/watch/a/rel1";
    let second = "/watch/a/rel2";
    let first_files = scanned_key(&runtime, first).await;
    let second_files = scanned_key(&runtime, second).await;

    runtime
        .admit(
            vec![
                (first.to_string(), first_files.clone()),
                (second.to_string(), second_files.clone()),
            ],
            Admission::Automatic,
        )
        .await
        .unwrap();
    assert_eq!(counts(&runtime, &mut last), vec![(0, 2)]);

    report(&runtime, first, &first_files, run(1), manual_only()).await;
    runtime.end_identification_answer(first, run(1)).await.unwrap();
    assert_eq!(counts(&runtime, &mut last), vec![(1, 2)]);

    report(&runtime, second, &second_files, run(2), manual_only()).await;
    runtime.end_identification_answer(second, run(2)).await.unwrap();
    assert_eq!(counts(&runtime, &mut last), vec![(0, 0)]);
}

/// A key the sweep drops before it runs is an identification that is over —
/// nothing is going to answer it, and the count must not wait on it for ever.
#[tokio::test]
async fn a_key_dropped_from_the_queue_before_it_ran_is_over() {
    let runtime = CandidateRuntime::for_test().await;
    let mut last = (0, 0);
    let first = "/watch/a/rel1";
    let second = "/watch/a/rel2";
    let marks = vec![
        (first.to_string(), scanned_key(&runtime, first).await),
        (second.to_string(), scanned_key(&runtime, second).await),
    ];

    runtime.admit(marks, Admission::Automatic).await.unwrap();
    assert_eq!(counts(&runtime, &mut last), vec![(0, 2)]);

    runtime.withdraw(second).await.unwrap();
    assert_eq!(
        counts(&runtime, &mut last),
        vec![(1, 2)],
        "the dropped key is counted as ended, not taken out of the total"
    );

    runtime.withdraw(first).await.unwrap();
    assert_eq!(counts(&runtime, &mut last), vec![(0, 0)]);
}

/// A write that failed is a run that is over: the row says why, and nothing is
/// waiting on a write that is not coming.
#[tokio::test]
async fn a_failed_write_ends_the_identification() {
    let runtime = CandidateRuntime::for_test().await;
    let mut last = (0, 0);
    let key = "/watch/a/rel1";
    let files = scanned_key(&runtime, key).await;

    runtime
        .admit(vec![(key.to_string(), files.clone())], Admission::Requested)
        .await
        .unwrap();
    report(&runtime, key, &files, run(1), manual_only()).await;
    assert_eq!(counts(&runtime, &mut last), vec![(0, 1)]);

    runtime
        .fail_identification(key, run(1), &files, not_written("disk is full"))
        .await
        .unwrap();
    assert_eq!(counts(&runtime, &mut last), vec![(0, 0)]);
    assert_eq!(
        identification(&runtime, key).await,
        Some(crate::import::IdentificationStatus::FinalizationFailed {
            failure: not_written("disk is full"),
        }),
        "the failure stays on the row after the batch is over"
    );
}

/// A run cancelled because its candidate left the scan ends its
/// identification with its end.
#[tokio::test]
async fn a_cancelled_run_s_end_ends_its_identification() {
    let runtime = CandidateRuntime::for_test().await;
    let mut last = (0, 0);
    let key = "/watch/a/rel1";
    let files = scanned_key(&runtime, key).await;

    runtime
        .admit(vec![(key.to_string(), files.clone())], Admission::Requested)
        .await
        .unwrap();
    report(&runtime, key, &files, run(1), triangulating()).await;
    assert_eq!(counts(&runtime, &mut last), vec![(0, 1)]);

    runtime.run_ended(key, run(1)).await.unwrap();
    assert_eq!(counts(&runtime, &mut last), vec![(0, 0)]);
}

/// A second batch counts only its own work — the first one's total described
/// identifications that are over.
#[tokio::test]
async fn a_batch_after_a_drain_starts_from_zero() {
    let runtime = CandidateRuntime::for_test().await;
    let mut last = (0, 0);
    let first = "/watch/a/rel1";
    let second = "/watch/a/rel2";
    let first_files = scanned_key(&runtime, first).await;
    let second_files = scanned_key(&runtime, second).await;

    runtime
        .admit(
            vec![
                (first.to_string(), first_files.clone()),
                (second.to_string(), second_files),
            ],
            Admission::Automatic,
        )
        .await
        .unwrap();
    assert_eq!(counts(&runtime, &mut last), vec![(0, 2)]);
    runtime.withdraw(first).await.unwrap();
    assert_eq!(counts(&runtime, &mut last), vec![(1, 2)]);
    runtime.withdraw(second).await.unwrap();
    assert_eq!(counts(&runtime, &mut last), vec![(0, 0)]);

    runtime
        .admit(vec![(first.to_string(), first_files)], Admission::Requested)
        .await
        .unwrap();
    assert_eq!(counts(&runtime, &mut last), vec![(0, 1)]);
}

/// A library release being re-identified in its own sheet is not the import
/// queue's work: nothing admits it, its run reaches the sheet through the
/// runtime, and the count never picks it up.
#[tokio::test]
async fn a_release_re_identified_in_its_own_sheet_is_not_counted() {
    let runtime = CandidateRuntime::for_test().await;
    let mut last = (0, 0);
    let key = "reidentify:release-1";

    runtime.record_run_state(key, run(1), false, &triangulating());
    runtime.record_run_state(key, run(1), false, &manual_only());
    assert!(
        runtime.snapshot(key).await.unwrap().is_some(),
        "the sheet reads its run off the runtime"
    );
    assert_eq!(counts(&runtime, &mut last), Vec::new());
}
