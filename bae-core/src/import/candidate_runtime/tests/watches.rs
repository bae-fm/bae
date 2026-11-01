// ── Readers of the current state ────────────────────────────────────────────
//
// Each wakes when the runtime or the table changes and reads what it draws,
// so however many changes land before it looks, it reads where they left off.

fn signals_event(key: &crate::import::CandidateKey, text: &str) -> ImportEvent {
    let mut signals = extracted_signals();
    signals.track_titles = vec![text.to_string()];
    ImportEvent::SignalsUpdated {
        candidate_key: key.clone(),
        run: IdentifyRunId::for_test(1),
        signals,
        artwork: crate::signals::ArtworkScan::Absent,
        priority: CallPriority::Background,
    }
}

/// Wait until the view shows each of `keys` claimed and running. A claim
/// reaches the view queued, then running as a later read of the table says;
/// a reader taking the view before that is told the move later, as if
/// whatever it waits for had moved the key.
async fn wait_until_running(runtime: &CandidateRuntime, keys: &[&crate::import::CandidateKey]) {
    wait_until_at(
        runtime,
        keys,
        ImportStep::Preparing(PrepareStep::ValidatingSourceFiles),
    )
    .await;
}

/// Wait until the view shows each of `keys`' import at `step`, as a read of
/// the table after the step's write says.
async fn wait_until_at(
    runtime: &CandidateRuntime,
    keys: &[&crate::import::CandidateKey],
    step: ImportStep,
) {
    runtime
        .watch_view()
        .wait_for(|view| {
            keys.iter().all(|key| {
                view.snapshots
                    .get(key)
                    .and_then(|snapshot| snapshot.import.as_ref())
                    .map(|import| import.step)
                    == Some(step)
            })
        })
        .await
        .unwrap();
}

/// The live read of every key, kept current for as long as the test runs.
fn keep_current(runtime: &CandidateRuntime) -> tokio::task::JoinHandle<()> {
    tokio::spawn(runtime.clone().keep_view_current())
}

#[tokio::test]
async fn a_snapshots_reader_is_told_everything_then_each_key_that_changed() {
    let runtime = CandidateRuntime::for_test().await;
    let live = keep_current(&runtime);
    claim(&runtime, &crate::import::CandidateKey::test("/watch/a/rel1")).await;
    claim(&runtime, &crate::import::CandidateKey::test("/watch/a/rel2")).await;
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

    measuring(&runtime, &crate::import::CandidateKey::test("/watch/a/rel1"), 40).await;
    runtime.move_import(&crate::import::CandidateKey::test("/watch/a/rel2"), "imp-1", None).await.unwrap();
    let mut changed = Vec::new();
    while !(changed.iter().any(|change| {
        matches!(change, CandidateRuntimeChange::Removed { key } if key == &crate::import::CandidateKey::test("/watch/a/rel2"))
    }) && changed.iter().any(|change| {
        matches!(change, CandidateRuntimeChange::Updated { key, runtime }
            if key == &crate::import::CandidateKey::test("/watch/a/rel1")
                && runtime.import.as_ref().map(|import| import.step)
                    == Some(ImportStep::Running(ImportPhase::MeasuringLoudness)))
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
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    let files = scanned_key(&runtime, key).await;
    let mut watch = RuntimeSnapshotsWatch::of(&runtime);
    watch.next().await.expect("the runtime is there");

    runtime
        .admit(vec![(key.clone(), files.clone())], Admission::Requested)
        .await
        .unwrap();
    let queued = loop {
        let changes = watch.next().await.expect("the runtime is there");
        if let Some(runtime) = changes.into_iter().find_map(|change| match change {
            CandidateRuntimeChange::Updated { key: changed, runtime } if changed == key.clone() => {
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

    // What the run publishes before the table names it is stored nowhere.
    assert!(!runtime
        .publish_candidate_run(key, run(1), &triangulating(), None)
        .await
        .unwrap());
    assert!(runtime
        .run_started(key, &files, run(1), &triangulating())
        .await
        .unwrap());
    let running = loop {
        let changes = watch.next().await.expect("the runtime is there");
        if let Some(runtime) = changes.into_iter().find_map(|change| match change {
            CandidateRuntimeChange::Updated { key: changed, runtime }
                if changed == key.clone()
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
        Some(IdentificationRuntime::Running(published(triangulating())))
    );
    live.abort();
}

#[tokio::test]
async fn a_values_reader_reads_the_signals_as_they_stand() {
    let runtime = CandidateRuntime::for_test().await;
    let mut watch = RuntimeValuesWatch::of(&runtime);
    let key = &crate::import::CandidateKey::test("/watch/a/rel3");
    runtime.record_event(&signals_event(key, "first"));
    runtime.record_event(&signals_event(key, "last"));

    let mut changed = Vec::new();
    while !changed
        .iter()
        .any(|value: &RuntimeValue| value.key == *key && value.signals.track_titles == ["last"])
    {
        changed.extend(watch.next().await.expect("the runtime is there"));
    }
}

/// The import counts' live query delivers each write that moves them: an
/// admission opens the batch at its total, a withdrawal ends one of it, and
/// a claim puts an import in flight.
#[tokio::test]
async fn the_import_counts_query_delivers_each_write_that_moves_them() {
    let runtime = CandidateRuntime::for_test().await;
    let mut counts = runtime.database.subscribe_import_counts();
    let first = &crate::import::CandidateKey::test("/watch/a/rel1");
    let second = &crate::import::CandidateKey::test("/watch/a/rel2");
    let marks = vec![
        (first.clone(), scanned_key(&runtime, first).await),
        (second.clone(), scanned_key(&runtime, second).await),
    ];
    let mut next_counts = async || {
        tokio::time::timeout(std::time::Duration::from_secs(5), counts.next())
            .await
            .expect("the query delivers")
            .expect("the query stays open")
            .expect("the query reads")
    };
    assert_eq!(next_counts().await, crate::import::ImportCounts::default());

    runtime.admit(marks, Admission::Automatic).await.unwrap();
    assert_eq!(
        next_counts().await,
        crate::import::ImportCounts {
            identified: 0,
            admitted: 2,
            imports_in_flight: 0,
        }
    );

    runtime.withdraw(first).await.unwrap();
    assert_eq!(
        next_counts().await,
        crate::import::ImportCounts {
            identified: 1,
            admitted: 2,
            imports_in_flight: 0,
        }
    );

    claim(&runtime, &crate::import::CandidateKey::test("/watch/a/rel3")).await;
    assert_eq!(
        next_counts().await,
        crate::import::ImportCounts {
            identified: 1,
            admitted: 2,
            imports_in_flight: 1,
        }
    );
}

/// A signals change names only the key whose signals moved, not every key
/// the runtime holds signals for.
#[tokio::test]
async fn a_values_reader_is_told_only_the_key_whose_signals_moved() {
    let runtime = CandidateRuntime::for_test().await;
    let live = keep_current(&runtime);
    let mut watch = RuntimeValuesWatch::of(&runtime);
    runtime.record_event(&signals_event(&crate::import::CandidateKey::test("/watch/a/rel1"), "first"));
    runtime.record_event(&signals_event(&crate::import::CandidateKey::test("/watch/a/rel2"), "first"));
    let mut told = Vec::new();
    while told.len() < 2 {
        told.extend(
            watch
                .next()
                .await
                .expect("the runtime is there"),
        );
    }

    runtime.record_event(&signals_event(&crate::import::CandidateKey::test("/watch/a/rel2"), "second"));
    let next = watch.next().await.expect("the runtime is there");
    assert!(
        matches!(
            next.as_slice(),
            [RuntimeValue { key, signals }]
                if key == &crate::import::CandidateKey::test("/watch/a/rel2") && signals.track_titles == ["second"]
        ),
        "{next:?}"
    );
    live.abort();
}

/// A change to one key's runtime names that key alone.
#[tokio::test]
async fn a_snapshots_reader_is_told_only_the_key_that_moved() {
    let runtime = CandidateRuntime::for_test().await;
    let live = keep_current(&runtime);
    claim(&runtime, &crate::import::CandidateKey::test("/watch/a/rel1")).await;
    claim(&runtime, &crate::import::CandidateKey::test("/watch/a/rel2")).await;
    wait_until_running(&runtime, &[&crate::import::CandidateKey::test("/watch/a/rel1"), &crate::import::CandidateKey::test("/watch/a/rel2")]).await;
    let mut watch = RuntimeSnapshotsWatch::of(&runtime);
    watch.next().await.expect("the runtime is there");

    measuring(&runtime, &crate::import::CandidateKey::test("/watch/a/rel1"), 40).await;
    let next = watch.next().await.expect("the runtime is there");
    assert!(
        matches!(
            next.as_slice(),
            [CandidateRuntimeChange::Updated { key, runtime }]
                if key == &crate::import::CandidateKey::test("/watch/a/rel1")
                    && runtime.import.as_ref().map(|import| import.step)
                        == Some(ImportStep::Running(ImportPhase::MeasuringLoudness))
        ),
        "{next:?}"
    );
    live.abort();
}

/// A running import's percent moves in the percents watch by its key; the
/// snapshot stream hears only the step it reached, so a percent tick at the
/// same step tells it nothing.
#[tokio::test]
async fn an_import_percent_moves_the_percents_not_the_snapshots() {
    let runtime = CandidateRuntime::for_test().await;
    let live = keep_current(&runtime);
    claim(&runtime, &crate::import::CandidateKey::test("/watch/a/rel1")).await;
    claim(&runtime, &crate::import::CandidateKey::test("/watch/a/rel2")).await;
    let mut percents = runtime.watch_import_percents();
    wait_until_running(&runtime, &[&crate::import::CandidateKey::test("/watch/a/rel1"), &crate::import::CandidateKey::test("/watch/a/rel2")]).await;
    measuring(&runtime, &crate::import::CandidateKey::test("/watch/a/rel1"), 40).await;
    percents
        .wait_for(|percents| percents.get(&crate::import::CandidateKey::test("/watch/a/rel1")) == Some(&40))
        .await
        .unwrap();
    wait_until_at(
        &runtime,
        &[&crate::import::CandidateKey::test("/watch/a/rel1")],
        ImportStep::Running(ImportPhase::MeasuringLoudness),
    )
    .await;
    let mut watch = RuntimeSnapshotsWatch::of(&runtime);
    watch.next().await.expect("the runtime is there");

    runtime.record_event(&progress(&crate::import::CandidateKey::test("/watch/a/rel1"), 41));
    percents
        .wait_for(|percents| percents.get(&crate::import::CandidateKey::test("/watch/a/rel1")) == Some(&41))
        .await
        .unwrap();
    assert!(
        !percents.borrow().contains_key(&crate::import::CandidateKey::test("/watch/a/rel2")),
        "a claim no report moved has no percent"
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), watch.next())
            .await
            .is_err(),
        "a percent at the same step tells the snapshot stream nothing"
    );

    runtime.move_import(&crate::import::CandidateKey::test("/watch/a/rel1"), "imp-1", None).await.unwrap();
    runtime.record_event(&ImportEvent::ImportProgress {
        candidate_key: crate::import::CandidateKey::test("/watch/a/rel1"),
        progress: ImportProgress::Complete {
            id: "rel1".to_string(),
            import_id: "imp-1".to_string(),
            album_id: "alb".to_string(),
        },
    });
    percents
        .wait_for(|percents| !percents.contains_key(&crate::import::CandidateKey::test("/watch/a/rel1")))
        .await
        .unwrap();
    live.abort();
}
