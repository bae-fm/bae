// ── Readers of the current state ────────────────────────────────────────────
//
// Each wakes when the runtime changes and reads what it draws, so however many
// changes land before it looks, it reads where they left off.

fn signals_event(key: &str, text: &str) -> ImportEvent {
    let mut signals = extracted_signals();
    signals.track_titles = vec![text.to_string()];
    ImportEvent::SignalsUpdated {
        candidate_key: key.to_string(),
        run: IdentifyRunId::for_test(1),
        signals,
        artwork: crate::signals::ArtworkScan::Absent,
        priority: CallPriority::Background,
    }
}

#[tokio::test]
async fn a_facts_reader_behind_by_any_number_of_changes_reads_where_they_left_off() {
    let runtime = CandidateRuntime::default();
    let mut watch = RuntimeFactsWatch::of(&runtime);
    for index in 0..3_000 {
        claim(&runtime, &format!("/watch/a/rel{index}"));
    }

    assert!(watch.changed().await);
    assert_eq!(watch.facts().len(), 3_000);
}

#[tokio::test]
async fn a_facts_reader_passes_over_a_progress_tick() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    claim(&runtime, key);
    runtime.record_event(&progress(key, 10));
    let mut watch = RuntimeFactsWatch::of(&runtime);

    runtime.record_event(&progress(key, 20));
    let woke = tokio::time::timeout(std::time::Duration::from_millis(0), watch.changed()).await;
    assert!(woke.is_err(), "a tick within an import changes no fact");
}

#[tokio::test]
async fn a_snapshots_reader_is_told_everything_then_each_key_that_changed() {
    let runtime = CandidateRuntime::default();
    claim(&runtime, "/watch/a/rel1");
    claim(&runtime, "/watch/a/rel2");
    let mut watch = RuntimeSnapshotsWatch::of(&runtime);

    let first = watch.next().await.expect("the runtime is there");
    assert!(matches!(
        first.as_slice(),
        [CandidateRuntimeChange::Reset { runtimes }] if runtimes.len() == 2
    ));

    runtime.record_event(&progress("/watch/a/rel1", 40));
    runtime.release_import_claim("/watch/a/rel2", "imp-1");
    let mut changed = watch.next().await.expect("the runtime is there");
    changed.sort_by_key(|change| format!("{change:?}"));
    assert!(matches!(
        changed.as_slice(),
        [
            CandidateRuntimeChange::Removed { key: removed },
            CandidateRuntimeChange::Updated { key: updated, .. },
        ] if removed == "/watch/a/rel2" && updated == "/watch/a/rel1"
    ), "{changed:?}");
}

#[tokio::test]
async fn a_values_reader_reads_the_counts_and_signals_as_they_stand() {
    let runtime = CandidateRuntime::default();
    let mut watch = RuntimeValuesWatch::of(&runtime);

    runtime.admit(
        vec!["/watch/a/rel1".to_string(), "/watch/a/rel2".to_string()],
        Admission::Automatic,
    );
    runtime.withdraw("/watch/a/rel1");
    claim(&runtime, "/watch/a/rel3");
    runtime.record_event(&signals_event("/watch/a/rel3", "first"));
    runtime.record_event(&signals_event("/watch/a/rel3", "last"));

    let mut changed = Vec::new();
    while !changed.iter().any(|value| matches!(value, RuntimeValue::Signals { .. })) {
        changed.extend(watch.next().await.expect("the runtime is there"));
    }
    assert!(changed.contains(&RuntimeValue::IdentificationProgress {
        identified: 1,
        total: 2
    }));
    assert!(changed.contains(&RuntimeValue::ImportsInFlight { count: 1 }));
    assert!(changed.iter().any(|value| matches!(
        value,
        RuntimeValue::Signals { key, signals }
            if key == "/watch/a/rel3" && signals.track_titles == ["last"]
    )));
}
