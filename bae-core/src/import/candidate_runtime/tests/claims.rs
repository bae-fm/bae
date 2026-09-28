// What an import's claim on a candidate records, and what it refuses.

#[test]
fn a_second_claim_on_an_owned_candidate_is_refused_and_changes_nothing() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    runtime.claim_for_import(key, "imp-1").unwrap();
    let claimed = runtime.get(key);
    let mut changes = runtime.every_change();

    assert!(matches!(
        runtime.claim_for_import(key, "imp-2"),
        Err(crate::import::ImportError::CandidateImportInProgress)
    ));
    assert_eq!(runtime.get(key), claimed);
    assert!(drain(&mut changes).is_empty());
}

#[test]
fn a_claim_is_the_queued_step_until_the_worker_reports() {
    let runtime = CandidateRuntime::default();
    let mut changes = runtime.every_change();
    let key = "/watch/a/rel1";

    runtime.claim_for_import(key, "imp-1").unwrap();
    assert_eq!(
        runtime.get(key).and_then(|runtime| runtime.import),
        Some(ImportInFlight {
            progress_percent: None,
            step: ImportStep::Preparing(PrepareStep::Queued),
        })
    );
    drain(&mut changes);

    runtime.release_import_claim(key, "imp-1");
    assert!(runtime.get(key).is_none());
    assert_eq!(
        drain(&mut changes),
        vec![CandidateRuntimeChange::Removed {
            key: key.to_string()
        }],
        "with nothing else running, releasing the claim empties the key"
    );
}

/// A claim starts from the queue whatever the import before it had reached,
/// and only its own import's reports move it: work an ended import left
/// running can still report, about an import the key no longer has.
#[test]
fn only_the_claiming_imports_reports_move_its_claim() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    runtime.claim_for_import(key, "imp-1").unwrap();
    runtime.record_event(&progress(key, 60));
    runtime.record_event(&ImportEvent::ImportProgress {
        candidate_key: key.to_string(),
        progress: ImportProgress::Cancelled {
            import_id: "imp-1".to_string(),
        },
    });
    let mut changes = runtime.every_change();

    runtime.record_event(&progress(key, 70));
    assert!(
        runtime.get(key).is_none(),
        "a report from the ended import puts no import back on the key"
    );
    assert!(drain(&mut changes).is_empty());

    runtime.claim_for_import(key, "imp-2").unwrap();
    let queued = Some(ImportInFlight {
        progress_percent: None,
        step: ImportStep::Preparing(PrepareStep::Queued),
    });
    assert_eq!(runtime.get(key).and_then(|runtime| runtime.import), queued);
    runtime.record_event(&progress(key, 80));
    runtime.record_event(&ImportEvent::ImportProgress {
        candidate_key: key.to_string(),
        progress: ImportProgress::Failed {
            error: "the disk filled".to_string(),
            import_id: "imp-1".to_string(),
        },
    });
    assert_eq!(
        runtime.get(key).and_then(|runtime| runtime.import),
        queued,
        "neither the ended import's progress nor its ending moves the new claim"
    );
    runtime.release_import_claim(key, "imp-1");
    assert_eq!(
        runtime.get(key).and_then(|runtime| runtime.import),
        queued,
        "nor does releasing the ended import's claim"
    );
}
