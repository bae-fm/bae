//! Each candidate's standing as the standings table stores and reads it back.

use crate::candidate_runtime::{
    IdentificationChange, IdentificationStage, StoredIdentification, StoredImport, StoredStanding,
};
use crate::folder_scanner::CandidateFileEdits;
use crate::tables::import_state::tests::*;
use crate::tables::tests::*;
use crate::tables::watched_path::host_root;
use crate::IdentifyRunId;
use crate::{Admission, CandidateFiles, SaveFailure};
use bae_store::test_fixtures::empty_db;
use bae_store::Store;

/// `key`'s release scanned under `/music`, as the scan stores it.
async fn scanned(db: &Store, key: &crate::CandidateKey) {
    let tables = crate::tables::ImportTables::new(db.clone());
    scan_key(&tables, std::path::Path::new("/music"), key).await;
}

/// The files the scan identifies `key`'s release as.
fn files_of(key: &crate::CandidateKey) -> CandidateFiles {
    CandidateFiles {
        content_hash: key_candidate(std::path::Path::new("/music"), key).files.content_hash(),
        file_edit_revision: 0,
    }
}

fn run(number: u64) -> IdentifyRunId {
    IdentifyRunId::for_test(number)
}

/// Apply `changes` to `key` one write each.
async fn change(db: &Store, key: &crate::CandidateKey, changes: Vec<IdentificationChange>) {
    let tables = crate::tables::ImportTables::new(db.clone());
    for change in changes {
        tables.change_candidate_identifications(vec![(key.clone(), change)]).await.unwrap();
    }
}

/// Every stage a candidate's identification can reach, and every standing of
/// its import, reads back as the change that reached it left it.
#[tokio::test]
async fn every_standing_reads_back_as_written() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let admit = |key: &str| IdentificationChange::Admit {
        files: files_of(&crate::CandidateKey::test(key)),
        admission: Admission::Requested,
    };
    let start = |key: &str| IdentificationChange::RunStarted {
        files: files_of(&crate::CandidateKey::test(key)),
        run: run(1),
    };
    let answer = |reads_pick| IdentificationChange::RunAnswered { run: run(1), reads_pick };
    let fail = |key: &str, failure| IdentificationChange::AnswerFailed {
        files: files_of(&crate::CandidateKey::test(key)),
        run: run(1),
        failure,
    };
    let not_written = SaveFailure::NotWritten { error: "the disk is full".to_string() };
    let inapplicable = SaveFailure::Inapplicable { error: "a track has no title".to_string() };
    let cases = [
        (
            "/music/Queued",
            vec![admit("/music/Queued")],
            IdentificationStage::Queued { admission: Admission::Requested },
        ),
        (
            "/music/Running",
            vec![admit("/music/Running"), start("/music/Running")],
            IdentificationStage::Running { run: run(1) },
        ),
        (
            "/music/Finalizing",
            vec![admit("/music/Finalizing"), start("/music/Finalizing"), answer(false)],
            IdentificationStage::Finalizing { run: run(1) },
        ),
        (
            "/music/Asked",
            vec![
                admit("/music/Asked"),
                start("/music/Asked"),
                answer(true),
                IdentificationChange::ImportAsked,
            ],
            IdentificationStage::ReadingPick { run: run(1), import_asked: true },
        ),
        (
            "/music/Not Written",
            vec![fail("/music/Not Written", not_written.clone())],
            IdentificationStage::Failed { failure: not_written },
        ),
        (
            "/music/Inapplicable",
            vec![fail("/music/Inapplicable", inapplicable.clone())],
            IdentificationStage::Failed { failure: inapplicable },
        ),
    ];
    for (key, changes, stage) in cases {
        let key = &crate::CandidateKey::test(key);
        scanned(&db, key).await;
        change(&db, key, changes).await;
        assert_eq!(
            tables.load_candidate_standing(key).await.unwrap(),
            StoredStanding {
                identification: Some(StoredIdentification { files: files_of(key), stage }),
                import: None,
            },
            "{key}"
        );
    }

    let key = &crate::CandidateKey::test("/music/Import");
    tables.claim_candidate_import(key, "import-1", None).await.unwrap();
    for step in [
        crate::ImportStep::Preparing(crate::PrepareStep::Queued),
        crate::ImportStep::Preparing(crate::PrepareStep::ValidatingSourceFiles),
        crate::ImportStep::Running(crate::ImportPhase::MeasuringLoudness),
        crate::ImportStep::Running(crate::ImportPhase::Finalizing),
    ] {
        tables.move_candidate_import(key, "import-1", Some(step)).await.unwrap();
        assert_eq!(
            tables.load_candidate_standing(key).await.unwrap(),
            StoredStanding {
                identification: None,
                import: Some(StoredImport { import_id: "import-1".to_string(), step }),
            }
        );
    }
}

/// A candidate nothing is running for has no row: ending the last thing
/// going for it removes the row it had.
#[tokio::test]
async fn nothing_running_removes_the_row() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let key = &crate::CandidateKey::test("/music/Album");
    scanned(&db, key).await;
    change(
        &db,
        key,
        vec![IdentificationChange::Admit { files: files_of(key), admission: Admission::Automatic }],
    )
    .await;
    tables.claim_candidate_import(key, "import-1", None).await.unwrap();

    change(&db, key, vec![IdentificationChange::Withdraw]).await;
    assert_eq!(tables.load_candidate_standing(key).await.unwrap().identification, None);
    tables.move_candidate_import(key, "import-1", None).await.unwrap();

    assert!(tables.load_candidate_standings().await.unwrap().is_empty());
    assert_eq!(tables.load_candidate_standing(key).await.unwrap(), StoredStanding::default());
}

/// A second claim on a candidate an import owns is refused by the row, and
/// changes nothing — the identification change asked with it included.
#[tokio::test]
async fn a_claim_while_another_stands_is_refused_whole() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let key = &crate::CandidateKey::test("/music/Album");
    scanned(&db, key).await;
    change(
        &db,
        key,
        vec![
            IdentificationChange::Admit { files: files_of(key), admission: Admission::Requested },
            IdentificationChange::RunStarted { files: files_of(key), run: run(1) },
        ],
    )
    .await;
    tables.claim_candidate_import(key, "import-1", None).await.unwrap();
    let claimed = tables.load_candidate_standing(key).await.unwrap();

    assert_eq!(
        tables
            .claim_candidate_import(key, "import-2", Some(IdentificationChange::Cancelled))
            .await
            .unwrap(),
        crate::tables::ImportClaim::Refused
    );
    assert_eq!(tables.load_candidate_standing(key).await.unwrap(), claimed);
}

/// Clearing the table, as the library opens, removes every standing.
#[tokio::test]
async fn clearing_removes_every_standing() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    for key in ["/music/One", "/music/Two"] {
        let key = &crate::CandidateKey::test(key);
        scanned(&db, key).await;
        change(
            &db,
            key,
            vec![IdentificationChange::Admit {
                files: files_of(key),
                admission: Admission::Automatic,
            }],
        )
        .await;
    }
    tables
        .claim_candidate_import(&crate::CandidateKey::test("/music/Two"), "import-1", None)
        .await
        .unwrap();
    tables.clear_candidate_standings().await.unwrap();

    assert!(tables.load_candidate_standings().await.unwrap().is_empty());
}

/// How many imports the import counts' live query counts in flight.
async fn imports_in_flight(db: &Store) -> u32 {
    let tables = crate::tables::ImportTables::new(db.clone());
    tables
        .subscribe_import_counts()
        .next()
        .await
        .expect("the query stays open")
        .unwrap()
        .imports_in_flight
}

/// The imports in flight are the rows an import holds, whatever it has
/// reached: a candidate only being identified is not one, and an import's
/// ending takes its row out.
#[tokio::test]
async fn the_imports_in_flight_are_the_rows_an_import_holds() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    for key in ["/music/One", "/music/Two", "/music/Three"] {
        let key = &crate::CandidateKey::test(key);
        scanned(&db, key).await;
        change(
            &db,
            key,
            vec![IdentificationChange::Admit {
                files: files_of(key),
                admission: Admission::Automatic,
            }],
        )
        .await;
    }
    assert_eq!(imports_in_flight(&db).await, 0);

    tables
        .claim_candidate_import(&crate::CandidateKey::test("/music/One"), "import-1", None)
        .await
        .unwrap();
    tables
        .claim_candidate_import(&crate::CandidateKey::test("/music/Two"), "import-2", None)
        .await
        .unwrap();
    tables
        .move_candidate_import(
            &crate::CandidateKey::test("/music/Two"),
            "import-2",
            Some(crate::ImportStep::Running(crate::ImportPhase::Finalizing)),
        )
        .await
        .unwrap();
    assert_eq!(imports_in_flight(&db).await, 2);

    tables
        .move_candidate_import(&crate::CandidateKey::test("/music/One"), "import-1", None)
        .await
        .unwrap();
    assert_eq!(imports_in_flight(&db).await, 1);
}

// ── A candidate's standing ends with the candidate ──────────────────────────
//
// Each scan write that removes a candidate or reads it with other files ends
// that candidate's identification in the same write, by key, and moves the
// identification count with it. An import's claim outlives it.

/// Put the candidate at `key` with `files` on the identification queue.
async fn admitted(db: &Store, key: &crate::CandidateKey, files: crate::CandidateFiles) {
    let tables = crate::tables::ImportTables::new(db.clone());
    tables
        .change_candidate_identifications(vec![(
            key.clone(),
            crate::candidate_runtime::IdentificationChange::Admit {
                files,
                admission: crate::Admission::Automatic,
            },
        )])
        .await
        .unwrap();
    assert!(tables.load_candidate_standing(key).await.unwrap().identification.is_some());
    assert_eq!(tables.load_identification_progress().await.unwrap(), (0, 1));
}

/// The candidate at `key` stands with no identification, and the batch it
/// was counted in is over.
async fn assert_identification_ended(db: &Store, key: &crate::CandidateKey) {
    let tables = crate::tables::ImportTables::new(db.clone());
    assert_eq!(
        tables.load_candidate_standing(key).await.unwrap().identification,
        None,
        "{key}'s identification ended with it"
    );
    assert_eq!(tables.load_identification_progress().await.unwrap(), (0, 0));
}

/// The pane candidate's key and files, stored and admitted.
async fn admitted_pane_candidate(db: &Store) -> (crate::CandidateKey, crate::CandidateFiles) {
    let (_, hash) = stored_pane_candidate(db).await;
    let key = pane_candidate_key();
    let files = crate::CandidateFiles { content_hash: hash, file_edit_revision: 0 };
    admitted(db, &key, files.clone()).await;
    (key, files)
}

#[tokio::test]
async fn a_pass_that_no_longer_finds_a_candidate_ends_its_identification() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (key, _) = admitted_pane_candidate(&db).await;
    let root = host_root("/music");

    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.finish_folder_scan(&root, generation, None).await.unwrap();

    assert!(tables.load_folder_scan_item(&key).await.unwrap().is_none());
    assert_identification_ended(&db, &key).await;
}

#[tokio::test]
async fn an_entry_rewritten_with_other_files_ends_its_identification() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (key, _) = admitted_pane_candidate(&db).await;
    let root = host_root("/music");

    // The same files again keep it.
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(&root, generation, &pane_candidate_item(&root))
        .await
        .unwrap()
        .expect("the current scan accepts the candidate");
    assert!(tables.load_candidate_standing(&key).await.unwrap().identification.is_some());

    let mut reshaped = pane_candidate_item(&root);
    let crate::folder_scanner::ScanItem::Valid(candidate) = &mut reshaped else {
        unreachable!("the fixture creates a valid candidate")
    };
    candidate.files.files[0].file.version.size += 1;
    tables
        .save_folder_scan_item(&root, generation, &reshaped)
        .await
        .unwrap()
        .expect("the current scan accepts the candidate");

    assert_identification_ended(&db, &key).await;
}

/// A typed search is of the files it was started for: the scan writing the
/// candidate with the same files keeps it, and the write that reads it with
/// other files ends it, every row it owns with it.
#[tokio::test]
async fn an_entry_rewritten_with_other_files_ends_its_search() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    stored_pane_candidate(&db).await;
    let key = pane_candidate_key();
    let root = host_root("/music");
    let search = crate::CandidateSearch::started(
        bae_catalogs::search::SearchQuery::Barcode { barcode: "012345678905".to_string() },
        &[bae_preferences::CatalogAvailability {
            catalog: bae_catalogs::catalog::LookupCatalog::Discogs,
            state: bae_preferences::SourceAvailability::On,
        }],
    );
    let run = tables.start_candidate_search(&key, &search).await.unwrap();
    assert!(tables
        .land_candidate_search(
            run,
            bae_catalogs::catalog::LookupCatalog::Discogs,
            Ok(vec![bae_catalogs::search::MetadataResult::for_test(
                bae_catalogs::catalog::LookupCatalog::Discogs,
                "dg-1",
                None,
            )]),
        )
        .await
        .unwrap());
    let searched = |db: Store| async move {
        let tables = crate::tables::ImportTables::new(db);
        tables.load_candidate_runtime(&pane_candidate_key()).await.unwrap().search
    };
    assert!(searched(db.clone()).await.is_some());

    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(&root, generation, &pane_candidate_item(&root))
        .await
        .unwrap()
        .expect("the current scan accepts the candidate");
    assert!(searched(db.clone()).await.is_some(), "the same files keep their search");

    let mut reshaped = pane_candidate_item(&root);
    let crate::folder_scanner::ScanItem::Valid(candidate) = &mut reshaped else {
        unreachable!("the fixture creates a valid candidate")
    };
    candidate.files.files[0].file.version.size += 1;
    tables
        .save_folder_scan_item(&root, generation, &reshaped)
        .await
        .unwrap()
        .expect("the current scan accepts the candidate");

    assert!(searched(db.clone()).await.is_none());
    assert_eq!(
        db.count_rows_for_test("import_candidate_match", "source", "discogs").await.unwrap(),
        0,
        "the releases it held went with it"
    );
}

#[tokio::test]
async fn a_file_decision_ends_its_candidate_s_identification() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let (files, hash) = stored_pane_candidate(&db).await;
    let key = pane_candidate_key();
    admitted(
        &db,
        &key,
        crate::CandidateFiles { content_hash: hash.clone(), file_edit_revision: 0 },
    )
    .await;
    let (metadata_revision, draft) = current_draft(&db, &hash).await;

    crate::tables::ImportTables::new(db.clone())
        .store_file_decisions(
            &as_read(&hash, metadata_revision),
            &key,
            &CandidateFileEdits::default(),
            &[(key.clone(), files)],
            draft,
        )
        .await
        .unwrap();

    assert_identification_ended(&db, &key).await;
}

#[tokio::test]
async fn separating_a_picked_grouping_ends_its_release_s_identification() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = host_root("/music");
    tables.add_watched_import_folder(&root).await.unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    let mut members = Vec::new();
    for name in ["Volume A", "Volume B"] {
        let mut folder = candidate(&root, name);
        // A file of its own per folder, so no two share a path on disk.
        for entry in &mut folder.files.files {
            entry.file.version.path = folder.path.join(&entry.file.relative_path);
        }
        tables
            .save_folder_scan_item(
                &root,
                generation,
                &crate::folder_scanner::ScanItem::Valid(folder.clone()),
            )
            .await
            .unwrap()
            .expect("the scan generation is current");
        members.push(folder);
    }
    tables.finish_folder_scan(&root, generation, None).await.unwrap();
    let key = crate::CandidateKey::grouping("picked");
    tables.combine_releases(key.clone(), members).await.unwrap();
    let release = tables
        .load_release_candidate(&key)
        .await
        .unwrap()
        .unwrap()
        .expect("the grouping's release is stored");
    admitted(&db, &key, crate::CandidateFiles::of(&release)).await;

    tables.separate_picked_grouping(&key).await.unwrap();

    assert_identification_ended(&db, &key).await;
}

#[tokio::test]
async fn removing_a_watched_folder_ends_its_candidates_identification_and_keeps_an_import() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (key, _) = admitted_pane_candidate(&db).await;
    tables.claim_candidate_import(&key, "import-1", None).await.unwrap();

    tables
        .remove_watched_import_folders(vec![host_root("/music")], None)
        .await
        .unwrap()
        .expect("the folder was watched");

    assert_identification_ended(&db, &key).await;
    assert!(
        tables.load_candidate_standing(&key).await.unwrap().import.is_some(),
        "the import outlives its candidate until its own ending"
    );
}
