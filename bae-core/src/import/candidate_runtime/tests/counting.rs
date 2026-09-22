// ── The identification count ────────────────────────────────────────────────
//
// What the count does over the queue operations this map already exposes.

/// A runtime and the bus its identification count is announced on.
fn counted_runtime() -> (CandidateRuntime, broadcast::Receiver<ImportEvent>) {
    let runtime = CandidateRuntime::default();
    let bus = crate::import::ImportEventBus::new(64, runtime.clone());
    (runtime, bus.subscribe())
}

/// Every count announced since the last read, in order.
fn counts(events: &mut broadcast::Receiver<ImportEvent>) -> Vec<(u32, u32)> {
    let mut counts = Vec::new();
    loop {
        match events.try_recv() {
            Ok(ImportEvent::IdentificationProgress { identified, total }) => {
                counts.push((identified, total));
            }
            Ok(_) => continue,
            Err(broadcast::error::TryRecvError::Empty) => return counts,
            Err(error) => panic!("the import bus failed while draining: {error}"),
        }
    }
}

/// A Lookup a person starts is an identification like any other: it opens a
/// batch on its own, without a sweep pass behind it.
#[test]
fn a_lookup_started_by_hand_opens_a_batch() {
    let (runtime, mut events) = counted_runtime();
    let key = "/watch/a/rel1";

    runtime.queue_explicit_identification(key);
    assert_eq!(counts(&mut events), vec![(0, 1)]);

    runtime.record_event(&identify(key, 1, triangulating()));
    assert_eq!(
        counts(&mut events),
        Vec::new(),
        "the run starting is the same identification, still waited on"
    );

    runtime.record_event(&identify(key, 1, manual_only()));
    assert_eq!(
        counts(&mut events),
        Vec::new(),
        "and so is the answer waiting on its write"
    );

    runtime.finish_identification_save(key, run(1));
    assert_eq!(
        counts(&mut events),
        vec![(0, 0)],
        "the write landing ends the only identification, and the batch with it"
    );
}

/// The run reporting is what takes its key off the queue, so the key is never
/// neither waiting nor running — which would read as an identification that
/// ended and started again.
#[test]
fn a_run_reporting_takes_its_key_off_the_queue() {
    let (runtime, mut events) = counted_runtime();
    let key = "/watch/a/rel1";

    runtime.queue_explicit_identification(key);
    assert_eq!(
        runtime.get(key).and_then(|state| state.queued),
        Some(IdentifyQueueOwner::ExplicitLookup)
    );

    runtime.record_event(&identify(key, 1, triangulating()));
    let state = runtime.get(key).expect("the run is in flight");
    assert_eq!(state.queued, None, "it is not waiting any more");
    assert!(state.running.is_some());
    assert_eq!(counts(&mut events), vec![(0, 1)], "one identification, once");
}

/// The sweep's queue is a batch: what it publishes is admitted, and each
/// verdict advances the count until the last one ends it.
#[test]
fn the_sweeps_queue_is_counted_and_drains_to_nothing() {
    let (runtime, mut events) = counted_runtime();
    let first = "/watch/a/rel1";
    let second = "/watch/a/rel2";

    runtime.replace_automatic_identification_queue([first.to_string(), second.to_string()]);
    assert_eq!(counts(&mut events), vec![(0, 2)]);

    runtime.record_event(&identify(first, 1, manual_only()));
    runtime.finish_identification_save(first, run(1));
    assert_eq!(counts(&mut events), vec![(1, 2)]);

    runtime.record_event(&identify(second, 2, manual_only()));
    runtime.finish_identification_save(second, run(2));
    assert_eq!(counts(&mut events), vec![(0, 0)]);
}

/// A key the sweep drops before it runs is an identification that is over —
/// nothing is going to answer it, and the count must not wait on it for ever.
#[test]
fn a_key_dropped_from_the_queue_before_it_ran_is_over() {
    let (runtime, mut events) = counted_runtime();
    let first = "/watch/a/rel1";
    let second = "/watch/a/rel2";

    runtime.replace_automatic_identification_queue([first.to_string(), second.to_string()]);
    assert_eq!(counts(&mut events), vec![(0, 2)]);

    runtime.replace_automatic_identification_queue([first.to_string()]);
    assert_eq!(
        counts(&mut events),
        vec![(1, 2)],
        "the dropped key is counted as ended, not taken out of the total"
    );

    runtime.clear_automatic_identification(first);
    assert_eq!(counts(&mut events), vec![(0, 0)]);
}

/// A Lookup that never reached a run still ends: whoever queued it clears the
/// mark, and that is the identification over.
#[test]
fn a_lookup_that_never_ran_ends_when_its_mark_is_cleared() {
    let (runtime, mut events) = counted_runtime();
    let key = "/watch/a/rel1";

    runtime.queue_explicit_identification(key);
    runtime.clear_explicit_identification(key);
    assert_eq!(counts(&mut events), vec![(0, 1), (0, 0)]);
}

/// A write that failed is a run that is over: the row says why, and nothing is
/// waiting on a write that is not coming.
#[test]
fn a_failed_write_ends_the_identification() {
    let (runtime, mut events) = counted_runtime();
    let key = "/watch/a/rel1";

    runtime.queue_explicit_identification(key);
    runtime.record_event(&identify(key, 1, manual_only()));
    assert_eq!(counts(&mut events), vec![(0, 1)]);

    runtime.fail_identification(key, run(1), "disk is full".to_string());
    assert_eq!(counts(&mut events), vec![(0, 0)]);
    assert_eq!(
        runtime.get(key).and_then(|state| state.save_failed),
        Some("disk is full".to_string()),
        "the failure stays on the row after the batch is over"
    );
}

/// A candidate whose folder left the scan mid-run takes its identification
/// with it.
#[test]
fn a_candidate_that_leaves_the_scan_ends_its_identification() {
    let (runtime, mut events) = counted_runtime();
    let key = "/watch/a/rel1";

    runtime.queue_explicit_identification(key);
    runtime.record_event(&identify(key, 1, triangulating()));
    assert_eq!(counts(&mut events), vec![(0, 1)]);

    runtime.record_event(&ImportEvent::Scan(ScanEvent::CandidateRemoved {
        candidate_key: key.to_string(),
    }));
    assert_eq!(counts(&mut events), vec![(0, 0)]);
}

/// A second batch counts only its own work — the first one's total described
/// identifications that are over.
#[test]
fn a_batch_after_a_drain_starts_from_zero() {
    let (runtime, mut events) = counted_runtime();
    let first = "/watch/a/rel1";
    let second = "/watch/a/rel2";

    runtime.replace_automatic_identification_queue([first.to_string(), second.to_string()]);
    runtime.clear_automatic_identification(first);
    runtime.clear_automatic_identification(second);
    assert_eq!(counts(&mut events), vec![(0, 2), (1, 2), (0, 0)]);

    runtime.queue_explicit_identification(first);
    assert_eq!(counts(&mut events), vec![(0, 1)]);
}

/// A library release being re-identified in its own sheet is not the import
/// queue's work: nothing admits it, its run reaches the sheet through the same
/// map, and the filter row's count never picks it up — which is also what
/// keeps the count off a verdict that sits there for as long as the sheet is
/// open, with no write to end it.
#[test]
fn a_release_re_identified_in_its_own_sheet_is_not_counted() {
    let (runtime, mut events) = counted_runtime();
    let key = "reidentify:release-1";

    runtime.record_event(&identify(key, 1, triangulating()));
    runtime.record_event(&identify(key, 1, manual_only()));
    assert!(
        runtime.get(key).is_some(),
        "the sheet reads its run off the same map"
    );
    assert_eq!(counts(&mut events), Vec::new());
}
