use crate::ImportFailure;

#[tokio::test]
async fn an_active_import_omits_its_previous_persisted_failure_from_the_detail() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;
    tables.save_import_candidate_failure(
        &hash,
        0,
        &ImportFailure::error_only("the prior attempt failed", fixed_now()),
    )
    .await
    .unwrap();

    let key = pane_candidate_key();
    tables.claim_candidate_import(&key, "import-1", None).await.unwrap();
    tables.move_candidate_import(
        &key,
        "import-1",
        Some(crate::ImportStep::Running(crate::ImportPhase::ReadingFiles)),
    )
    .await
    .unwrap();
    let detail = tables
        .load_import_candidate(&key)
        .await
        .unwrap()
        .expect("the stored candidate has a detail")
        .resolve();

    assert!(detail.live.facts.importing());
    assert_eq!(
        detail.live.actions,
        vec![
            crate::CandidateAction::CancelImport,
            crate::CandidateAction::RevealFolder
        ]
    );
    assert!(detail.draft().unwrap().failure.is_none());
}

/// The pane draws a queued import differently from a running one, and names
/// a running one's step, so its import status says the step the import is at.
#[tokio::test]
async fn the_pane_s_import_status_says_the_step_the_import_is_at() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    stored_pane_candidate(&db).await;
    let key = pane_candidate_key();

    tables.claim_candidate_import(&key, "import-1", None).await.unwrap();
    for step in [
        crate::ImportStep::Preparing(crate::PrepareStep::Queued),
        crate::ImportStep::Preparing(crate::PrepareStep::ValidatingSourceFiles),
        crate::ImportStep::Running(crate::ImportPhase::MeasuringLoudness),
        crate::ImportStep::Running(crate::ImportPhase::Finalizing),
    ] {
        tables.move_candidate_import(&key, "import-1", Some(step)).await.unwrap();
        let detail = tables
            .load_import_candidate(&key)
            .await
            .unwrap()
            .expect("the stored candidate has a detail")
            .resolve();
        assert_eq!(
            detail.import_status,
            Some(crate::CandidateImportStatus::Importing { step })
        );
    }
}
