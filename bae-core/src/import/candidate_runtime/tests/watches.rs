// ── Readers of the current state ────────────────────────────────────────────
//
// Each wakes when the runtime or the table changes and reads what it draws,
// so however many changes land before it looks, it reads where they left off.

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

/// The live read of every key, kept current for as long as the test runs.
fn keep_current(runtime: &CandidateRuntime) -> tokio::task::JoinHandle<()> {
    tokio::spawn(runtime.clone().keep_view_current())
}

#[tokio::test]
async fn a_snapshots_reader_is_told_everything_then_each_key_that_changed() {
    let runtime = CandidateRuntime::for_test().await;
    let live = keep_current(&runtime);
    claim(&runtime, "/watch/a/rel1").await;
    claim(&runtime, "/watch/a/rel2").await;
    let mut view = runtime.watch_view();
    view.wait_for(|view| view.snapshots.len() == 2)
        .await
        .unwrap();
    let mut watch = RuntimeSnapshotsWatch::of(&runtime);

    let first = watch.next().await.expect("the runtime is there");
    assert!(matches!(
        first.as_slice(),
        [CandidateRuntimeChange::Reset { runtimes }] if runtimes.len() == 2
    ));

    runtime.record_event(&progress("/watch/a/rel1", 40));
    runtime.move_import("/watch/a/rel2", "imp-1", None).await.unwrap();
    let mut changed = Vec::new();
    while !(changed.iter().any(|change| {
        matches!(change, CandidateRuntimeChange::Removed { key } if key == "/watch/a/rel2")
    }) && changed.iter().any(|change| {
        matches!(change, CandidateRuntimeChange::Updated { key, runtime }
            if key == "/watch/a/rel1"
                && runtime.import.as_ref().and_then(|import| import.progress_percent) == Some(40))
    })) {
        changed.extend(watch.next().await.expect("the runtime is there"));
    }
    live.abort();
}

/// The pane's runtime stream draws Queued, then Running, from the table, with
/// the state the run published beside it.
#[tokio::test]
async fn the_runtime_stream_shows_queued_then_running_from_the_table() {
    let runtime = CandidateRuntime::for_test().await;
    let live = keep_current(&runtime);
    let key = "/watch/a/rel1";
    let files = scanned_key(&runtime, key).await;
    let mut watch = RuntimeSnapshotsWatch::of(&runtime);
    watch.next().await.expect("the runtime is there");

    runtime
        .admit(vec![(key.to_string(), files.clone())], Admission::Requested)
        .await
        .unwrap();
    let queued = loop {
        let changes = watch.next().await.expect("the runtime is there");
        if let Some(runtime) = changes.into_iter().find_map(|change| match change {
            CandidateRuntimeChange::Updated { key: changed, runtime } if changed == key => {
                Some(runtime)
            }
            _ => None,
        }) {
            break runtime;
        }
    };
    assert_eq!(
        queued.identification,
        Some(IdentificationRuntime::Queued(Admission::Requested))
    );

    // What the run publishes, held before the table names the run, draws
    // nothing on its own.
    runtime.record_run_state(key, run(1), true, &triangulating());
    assert_eq!(
        runtime
            .latest_view()
            .snapshots
            .get(key)
            .and_then(|snapshot| snapshot.identification_in_flight()),
        Some(crate::import::IdentificationInFlight::Queued)
    );
    assert!(runtime.run_started(key, &files, run(1)).await.unwrap());
    let running = loop {
        let changes = watch.next().await.expect("the runtime is there");
        if let Some(runtime) = changes.into_iter().find_map(|change| match change {
            CandidateRuntimeChange::Updated { key: changed, runtime }
                if changed == key
                    && matches!(runtime.identification, Some(IdentificationRuntime::Running(_))) =>
            {
                Some(runtime)
            }
            _ => None,
        }) {
            break runtime;
        }
    };
    assert_eq!(
        running.identification,
        Some(IdentificationRuntime::Running(triangulating()))
    );
    live.abort();
}

#[tokio::test]
async fn a_values_reader_reads_the_counts_and_signals_as_they_stand() {
    let runtime = CandidateRuntime::for_test().await;
    let live = keep_current(&runtime);
    let mut watch = RuntimeValuesWatch::of(&runtime);
    let first = "/watch/a/rel1";
    let second = "/watch/a/rel2";
    let marks = vec![
        (first.to_string(), scanned_key(&runtime, first).await),
        (second.to_string(), scanned_key(&runtime, second).await),
    ];

    runtime.admit(marks, Admission::Automatic).await.unwrap();
    runtime.withdraw(first).await.unwrap();
    claim(&runtime, "/watch/a/rel3").await;
    runtime.record_event(&signals_event("/watch/a/rel3", "first"));
    runtime.record_event(&signals_event("/watch/a/rel3", "last"));

    let mut changed = Vec::new();
    while !(changed.contains(&RuntimeValue::ImportsInFlight { count: 1 })
        && changed.iter().any(|value| {
            matches!(value, RuntimeValue::Signals { signals, .. } if signals.track_titles == ["last"])
        })
        && changed.contains(&RuntimeValue::IdentificationProgress {
            identified: 1,
            total: 2,
        }))
    {
        changed.extend(watch.next().await.expect("the runtime is there"));
    }
    live.abort();
}
