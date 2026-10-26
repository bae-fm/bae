// ── What identification is doing, as the pane shows it ──────────────────────

use crate::import::IdentificationInFlight;

/// What the runtime says identification is doing for `key`.
async fn in_flight(runtime: &CandidateRuntime, key: &str) -> Option<IdentificationInFlight> {
    runtime
        .snapshot(key)
        .await
        .unwrap()?
        .identification_in_flight()
}

/// A key waiting on the queue reads as queued rather than as nothing started;
/// its run's first report makes it the run, whose answer shows while it is
/// written.
#[tokio::test]
async fn a_queued_key_reads_as_queued_until_its_run_reports() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    let files = scanned_key(&runtime, key).await;

    runtime
        .admit(vec![(key.to_string(), files.clone())], Admission::Automatic)
        .await
        .unwrap();
    assert_eq!(
        in_flight(&runtime, key).await,
        Some(IdentificationInFlight::Queued)
    );

    report(&runtime, key, &files, run(1), triangulating()).await;
    assert_eq!(
        in_flight(&runtime, key).await,
        Some(IdentificationInFlight::Run(triangulating()))
    );

    runtime.record_run_state(key, run(1), true, &manual_only());
    runtime.run_answered(key, run(1), false).await.unwrap();
    assert_eq!(
        in_flight(&runtime, key).await,
        Some(IdentificationInFlight::Run(manual_only()))
    );

    runtime.end_identification_answer(key, run(1)).await.unwrap();
    assert_eq!(in_flight(&runtime, key).await, None, "nothing is identifying it");
}

/// A key asked for again while its last answer is being written waits for
/// its next run: the request supersedes the answer, whose write then ends
/// nothing.
#[tokio::test]
async fn a_request_over_an_answer_being_written_waits_for_the_next_run() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    let files = scanned_key(&runtime, key).await;
    runtime
        .admit(vec![(key.to_string(), files.clone())], Admission::Automatic)
        .await
        .unwrap();
    report(&runtime, key, &files, run(1), manual_only()).await;

    runtime
        .admit(vec![(key.to_string(), files.clone())], Admission::Requested)
        .await
        .unwrap();
    assert_eq!(
        in_flight(&runtime, key).await,
        Some(IdentificationInFlight::Queued)
    );

    runtime.end_identification_answer(key, run(1)).await.unwrap();
    assert_eq!(
        in_flight(&runtime, key).await,
        Some(IdentificationInFlight::Queued)
    );
}

/// A library release's re-identify run is no list row's: the runtime holds it
/// whole, its run in flight and then its answer, until its sheet closes.
#[tokio::test]
async fn a_re_identify_run_is_held_by_the_runtime_alone() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "reidentify:release-1";

    runtime.record_run_state(key, run(1), false, &triangulating());
    assert_eq!(
        in_flight(&runtime, key).await,
        Some(IdentificationInFlight::Run(triangulating()))
    );
    runtime.record_run_state(key, run(1), false, &manual_only());
    assert_eq!(
        in_flight(&runtime, key).await,
        Some(IdentificationInFlight::Run(manual_only()))
    );
    assert_eq!(runtime.standing_facts(key).await.unwrap(), Default::default());

    runtime.cancel_identification(key).await.unwrap();
    assert_eq!(in_flight(&runtime, key).await, None);
}
