// ── What identification is doing, as the pane shows it ──────────────────────

use crate::import::IdentificationInFlight;

/// What the runtime says identification is doing for `key`.
fn in_flight(runtime: &CandidateRuntime, key: &str) -> Option<IdentificationInFlight> {
    runtime.get(key)?.identification()
}

/// A key waiting on the queue reads as queued rather than as nothing started;
/// its run's first report makes it the run, whose answer shows while it is
/// written.
#[test]
fn a_queued_key_reads_as_queued_until_its_run_reports() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";

    runtime.admit(vec![key.to_string()], Admission::Automatic);
    assert_eq!(
        in_flight(&runtime, key),
        Some(IdentificationInFlight::Queued)
    );

    runtime.record_event(&identify(key, 1, triangulating()));
    assert_eq!(
        in_flight(&runtime, key),
        Some(IdentificationInFlight::Run(triangulating()))
    );

    runtime.record_event(&identify(key, 1, manual_only()));
    assert_eq!(
        in_flight(&runtime, key),
        Some(IdentificationInFlight::Run(manual_only()))
    );

    runtime.end_identification_answer(key, run(1));
    assert_eq!(in_flight(&runtime, key), None, "nothing is identifying it");
}

/// A key asked for again while its last answer is being written shows that
/// answer until the write ends, then the wait for its next run.
#[test]
fn an_answer_being_written_shows_before_the_next_wait() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    runtime.admit(vec![key.to_string()], Admission::Automatic);
    runtime.record_event(&identify(key, 1, manual_only()));

    runtime.admit(vec![key.to_string()], Admission::Requested);
    assert_eq!(
        in_flight(&runtime, key),
        Some(IdentificationInFlight::Run(manual_only()))
    );

    runtime.end_identification_answer(key, run(1));
    assert_eq!(
        in_flight(&runtime, key),
        Some(IdentificationInFlight::Queued)
    );
}
