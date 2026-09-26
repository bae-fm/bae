// What an import's claim on a candidate records, and what it refuses.

#[test]
fn a_second_claim_on_an_owned_candidate_is_refused_and_changes_nothing() {
    let runtime = CandidateRuntime::default();
    let key = "/watch/a/rel1";
    runtime.claim_for_import(key).unwrap();
    let claimed = runtime.get(key);
    let mut changes = runtime.subscribe();

    assert!(matches!(
        runtime.claim_for_import(key),
        Err(crate::import::ImportError::CandidateImportInProgress)
    ));
    assert_eq!(runtime.get(key), claimed);
    assert!(drain(&mut changes).is_empty());
}

#[test]
fn a_claim_is_the_queued_step_until_the_worker_reports() {
    let runtime = CandidateRuntime::default();
    let mut changes = runtime.subscribe();
    let key = "/watch/a/rel1";

    runtime.claim_for_import(key).unwrap();
    assert_eq!(
        runtime.get(key).and_then(|runtime| runtime.import),
        Some(ImportInFlight {
            progress_percent: None,
            step: Some(ImportStep::Preparing(PrepareStep::Queued)),
        })
    );
    drain(&mut changes);

    runtime.release_import_claim(key);
    assert!(runtime.get(key).is_none());
    assert_eq!(
        drain(&mut changes),
        vec![CandidateRuntimeChange::Removed {
            key: key.to_string()
        }],
        "with nothing else running, releasing the claim empties the key"
    );
}
