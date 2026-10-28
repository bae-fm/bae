// What an import's claim on a candidate records, and what it refuses.

#[tokio::test]
async fn a_second_claim_on_an_owned_candidate_is_refused_and_changes_nothing() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    runtime.claim_for_import(key, "imp-1", false).await.unwrap();
    let claimed = runtime.snapshot(key).await.unwrap();

    assert!(matches!(
        runtime.claim_for_import(key, "imp-2", false).await,
        Err(crate::import::ImportError::CandidateImportInProgress)
    ));
    assert_eq!(runtime.snapshot(key).await.unwrap(), claimed);
}

/// The refusal is the claim's row: a runtime that holds nothing of the first
/// claim — another one over the same library — is refused all the same.
#[tokio::test]
async fn a_claim_is_refused_by_the_claim_the_table_holds() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    runtime.claim_for_import(key, "imp-1", false).await.unwrap();

    let holding_nothing = runtime.over_the_same_library_for_test();
    assert!(holding_nothing.inner.lock().unwrap().runtime.is_empty());
    assert!(matches!(
        holding_nothing.claim_for_import(key, "imp-2", false).await,
        Err(crate::import::ImportError::CandidateImportInProgress)
    ));
    assert!(holding_nothing.import_claimed(key).await.unwrap());
}

#[tokio::test]
async fn a_claim_is_the_queued_step_until_the_worker_takes_it_up() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";

    runtime.claim_for_import(key, "imp-1", false).await.unwrap();
    assert_eq!(
        runtime.snapshot(key).await.unwrap().and_then(|runtime| runtime.import),
        Some(ImportInFlight {
            step: ImportStep::Preparing(PrepareStep::Queued),
        })
    );

    runtime.move_import(key, "imp-1", None).await.unwrap();
    assert!(
        runtime.snapshot(key).await.unwrap().is_none(),
        "with nothing else running, releasing the claim empties the key"
    );
}

/// Only the import holding a claim moves or ends it, and a report from an
/// import that already ended moves nothing a reader draws.
#[tokio::test]
async fn only_the_claiming_import_moves_its_claim() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    claim(&runtime, key).await;
    runtime.record_event(&progress(key, 60));
    runtime.move_import(key, "imp-1", None).await.unwrap();
    runtime.record_event(&ImportEvent::ImportProgress {
        candidate_key: key.to_string(),
        progress: ImportProgress::Cancelled {
            import_id: "imp-1".to_string(),
        },
    });

    runtime.claim_for_import(key, "imp-2", false).await.unwrap();
    let queued = Some(ImportInFlight {
        step: ImportStep::Preparing(PrepareStep::Queued),
    });
    runtime.record_event(&progress(key, 80));
    assert_eq!(
        runtime.snapshot(key).await.unwrap().and_then(|runtime| runtime.import),
        queued,
        "the ended import's report does not move the new claim"
    );
    assert!(
        runtime
            .move_import(key, "imp-1", Some(ImportStanding::Running))
            .await
            .is_err(),
        "nor may the ended import move it"
    );
    assert!(runtime.move_import(key, "imp-1", None).await.is_err());
    assert_eq!(
        runtime.snapshot(key).await.unwrap().and_then(|runtime| runtime.import),
        queued,
        "nor end it"
    );
}

/// The scan dropping a candidate an import holds — the folder removed, read
/// with other files, or found invalid — does not end the import. The claim
/// stays until the import's own ending, so no second import takes the key
/// meanwhile.
#[tokio::test]
async fn an_import_outlives_its_candidate_leaving_the_scan() {
    let key = "/watch/a/rel1";
    let mut reshaped = folder_candidate(key, "/watch/a");
    reshaped.file_edit_revision = 1;
    let departures = [
        ImportEvent::Scan(ScanEvent::CandidateRemoved {
            candidate_key: key.to_string(),
        }),
        scanned(reshaped),
        ImportEvent::Scan(ScanEvent::InvalidCandidate(InvalidCandidate {
            path: PathBuf::from(key),
            name: "rel1".to_string(),
            watched_folder_path: "/watch/a".to_string(),
            display_path: "rel1".to_string(),
            grouping: None,
            files: Vec::new(),
            tags: None,
        })),
    ];
    for departure in departures {
        let runtime = CandidateRuntime::for_test().await;
        runtime.record_event(&scanned(folder_candidate(key, "/watch/a")));
        claim(&runtime, key).await;
        runtime.record_event(&progress(key, 42));

        runtime.record_event(&departure);
        assert!(runtime.import_claimed(key).await.unwrap(), "{departure:?}");
        assert!(
            matches!(
                runtime.claim_for_import(key, "imp-2", false).await,
                Err(crate::import::ImportError::CandidateImportInProgress)
            ),
            "{departure:?} let a second import claim the key"
        );

        runtime.move_import(key, "imp-1", None).await.unwrap();
        runtime.record_event(&ImportEvent::ImportProgress {
            candidate_key: key.to_string(),
            progress: ImportProgress::Failed {
                error: "the files changed".to_string(),
                import_id: "imp-1".to_string(),
            },
        });
        assert!(runtime.snapshot(key).await.unwrap().is_none(), "{departure:?}");
    }
}

/// Claiming a candidate for an import ends whatever run or answer its
/// identification holds, in the claim's own write.
#[tokio::test]
async fn a_claim_that_ends_identification_ends_it_in_its_own_write() {
    let runtime = CandidateRuntime::for_test().await;
    let key = "/watch/a/rel1";
    let files = scanned_key(&runtime, key).await;
    runtime
        .admit(vec![(key.to_string(), files.clone())], Admission::Requested)
        .await
        .unwrap();
    report(&runtime, key, &files, run(1), triangulating()).await;

    runtime.claim_for_import(key, "imp-1", true).await.unwrap();
    let facts = runtime.standing_facts(key).await.unwrap();
    assert_eq!(facts.identification, None);
    assert_eq!(facts.import, Some(ImportStanding::Queued));
    assert_eq!(runtime.identification_progress().await.unwrap(), (0, 0));
}
