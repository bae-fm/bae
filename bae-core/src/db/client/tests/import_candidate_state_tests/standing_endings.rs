// ── A candidate's standing ends with the candidate ──────────────────────────
//
// Each scan write that removes a candidate or reads it with other files ends
// that candidate's identification in the same write, by key, and moves the
// identification count with it. An import's claim outlives it.

/// Put the candidate at `key` with `files` on the identification queue.
async fn admitted(db: &Database, key: &crate::import::CandidateKey, files: crate::import::CandidateFiles) {
    db.change_candidate_identifications(vec![(
        key.clone(),
        crate::import::candidate_runtime::IdentificationChange::Admit {
            files,
            admission: crate::import::Admission::Automatic,
        },
    )])
    .await
    .unwrap();
    assert!(db
        .load_candidate_standing(key)
        .await
        .unwrap()
        .identification
        .is_some());
    assert_eq!(db.load_identification_progress().await.unwrap(), (0, 1));
}

/// The candidate at `key` stands with no identification, and the batch it
/// was counted in is over.
async fn assert_identification_ended(db: &Database, key: &crate::import::CandidateKey) {
    assert_eq!(
        db.load_candidate_standing(key).await.unwrap().identification,
        None,
        "{key}'s identification ended with it"
    );
    assert_eq!(db.load_identification_progress().await.unwrap(), (0, 0));
}

/// The pane candidate's key and files, stored and admitted.
async fn admitted_pane_candidate(db: &Database) -> (crate::import::CandidateKey, crate::import::CandidateFiles) {
    let (_, hash) = stored_pane_candidate(db).await;
    let key = pane_candidate_key();
    let files = crate::import::CandidateFiles {
        content_hash: hash,
        file_edit_revision: 0,
    };
    admitted(db, &key, files.clone()).await;
    (key, files)
}

#[tokio::test]
async fn a_pass_that_no_longer_finds_a_candidate_ends_its_identification() {
    let (db, _tmp) = empty_db().await;
    let (key, _) = admitted_pane_candidate(&db).await;
    let root = host_root("/music");

    let generation = db.begin_folder_scan(&root, crate::import::VolumeKind::Local).await.unwrap();
    db.finish_folder_scan(&root, generation, None).await.unwrap();

    assert!(db.load_folder_scan_item(&key).await.unwrap().is_none());
    assert_identification_ended(&db, &key).await;
}

#[tokio::test]
async fn an_entry_rewritten_with_other_files_ends_its_identification() {
    let (db, _tmp) = empty_db().await;
    let (key, _) = admitted_pane_candidate(&db).await;
    let root = host_root("/music");

    // The same files again keep it.
    let generation = db.begin_folder_scan(&root, crate::import::VolumeKind::Local).await.unwrap();
    db.save_folder_scan_item(&root, generation, &scanned_candidate(&root, "Album"))
        .await
        .unwrap()
        .expect("the current scan accepts the candidate");
    assert!(db
        .load_candidate_standing(&key)
        .await
        .unwrap()
        .identification
        .is_some());

    let mut reshaped = scanned_candidate(&root, "Album");
    let crate::import::folder_scanner::ScanItem::Valid(candidate) = &mut reshaped else {
        unreachable!("the fixture creates a valid candidate")
    };
    candidate.files.files[0].file.size += 1;
    db.save_folder_scan_item(&root, generation, &reshaped)
        .await
        .unwrap()
        .expect("the current scan accepts the candidate");

    assert_identification_ended(&db, &key).await;
}

#[tokio::test]
async fn a_file_decision_ends_its_candidate_s_identification() {
    let (db, _tmp) = empty_db().await;
    let (files, hash) = stored_pane_candidate(&db).await;
    let key = pane_candidate_key();
    admitted(
        &db,
        &key,
        crate::import::CandidateFiles {
            content_hash: hash.clone(),
            file_edit_revision: 0,
        },
    )
    .await;
    let (metadata_revision, draft) = current_draft(&db, &hash).await;

    crate::import::CandidatePreparations::new(db.clone())
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
    let (db, _tmp) = empty_db().await;
    let root = host_root("/music");
    db.add_watched_import_folder(&root).await.unwrap();
    let generation = db.begin_folder_scan(&root, crate::import::VolumeKind::Local).await.unwrap();
    let mut members = Vec::new();
    for name in ["Volume A", "Volume B"] {
        let mut folder = super::candidate(&root, name);
        // A file of its own per folder, so no two share a path on disk.
        for entry in &mut folder.files.files {
            entry.file.path = folder.path.join(&entry.file.relative_path);
        }
        db.save_folder_scan_item(
            &root,
            generation,
            &crate::import::folder_scanner::ScanItem::Valid(folder.clone()),
        )
        .await
        .unwrap()
        .expect("the scan generation is current");
        members.push(folder);
    }
    db.finish_folder_scan(&root, generation, None).await.unwrap();
    let key = crate::import::CandidateKey::grouping("picked");
    db.combine_releases(key.clone(), members).await.unwrap().unwrap();
    let release = db
        .load_release_candidate(&key)
        .await
        .unwrap()
        .unwrap()
        .expect("the grouping's release is stored");
    admitted(&db, &key, crate::import::CandidateFiles::of(&release)).await;

    db.separate_picked_grouping(&key).await.unwrap();

    assert_identification_ended(&db, &key).await;
}

#[tokio::test]
async fn removing_a_watched_folder_ends_its_candidates_identification_and_keeps_an_import() {
    let (db, _tmp) = empty_db().await;
    let (key, _) = admitted_pane_candidate(&db).await;
    db.claim_candidate_import(&key, "import-1", None).await.unwrap();

    db.remove_watched_import_folders(vec![host_root("/music")], None)
        .await
        .unwrap()
        .expect("the folder was watched");

    assert_identification_ended(&db, &key).await;
    assert!(
        db.load_candidate_standing(&key).await.unwrap().import.is_some(),
        "the import outlives its candidate until its own ending"
    );
}
