/// A binding the user cleared survives a relaunch: it is stored under the
/// candidate's content hash, read back from a cold database, and the scan
/// that follows reports the folder as they settled it rather than as its
/// filenames read.
///
/// The scan is the point — a binding that round-tripped through SQLite but
/// never reached a folder's roles would be a stored value nothing consumes.
#[tokio::test]
async fn a_cleared_binding_survives_a_relaunch() {
    use crate::import::folder_scanner::{
        collect_release_candidate_files_with_scope, CandidateFileEdits, SheetBindingEdits,
        StoredCandidateEdits, UserSheetBinding,
    };

    let (db, _tmp) = empty_db().await;
    let folder = walkthrough_folder();
    let scanned = collect_release_candidate_files_with_scope(
        folder.path(),
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .unwrap();
    assert_eq!(
        scanned.track_count(),
        12,
        "the unique same-stem audio is bound automatically"
    );
    let root = folder.path().to_string_lossy().into_owned();
    db.add_watched_import_folder(&root).await.unwrap();
    let generation = db.begin_folder_scan(&root, crate::import::VolumeKind::Local).await.unwrap();
    let candidate = crate::import::folder_scanner::FolderCandidate {
        path: folder.path().to_path_buf(),
        file_root: folder.path().to_path_buf(),
        name: "Release".to_string(),
        files: scanned.clone(),
        watched_folder_path: root.clone(),
        scope: crate::import::ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: String::new(),
        grouping: None,
    };
    db.save_folder_scan_item(
        &root,
        generation,
        &crate::import::folder_scanner::ScanItem::Valid(candidate),
    )
    .await
    .unwrap();
    db.finish_folder_scan(&root, generation, None)
        .await
        .unwrap();

    let mut edits = SheetBindingEdits::default();
    edits.set_reference(
        "cd.cue".to_string(),
        "cd.wav".to_string(),
        UserSheetBinding::Cleared,
    );
    let candidate_edits = CandidateFileEdits {
        sheet_bindings: edits,
        ..Default::default()
    };
    let mut settled = scanned.clone();
    settled
        .apply_candidate_file_edits(&candidate_edits)
        .unwrap();
    let hash = scanned.content_hash();
    let (metadata_revision, mut mapping_preparation) =
        current_mapping_preparation(&db, &hash).await;
    mapping_preparation.draft.tracks = crate::import::pane::blank_candidate_source(&settled)
        .draft
        .tracks;
    crate::import::CandidatePreparations::new(db.clone())
        .store_file_decisions(
            &as_read(&hash, metadata_revision),
            &folder.path().to_string_lossy(),
            &candidate_edits,
            &[(folder.path().to_string_lossy().into_owned(), settled)],
            &mapping_preparation,
        )
        .await
        .unwrap();

    let current = db
        .load_candidate_file_edits(&scanned.content_hash())
        .await
        .unwrap();
    assert_eq!(current.revision, 1);
    assert_eq!(
        current
            .sheet_bindings
            .get("cd.cue")
            .and_then(|references| references.get("cd.wav")),
        Some(&UserSheetBinding::Cleared)
    );
    assert_eq!(
        db.load_candidate_file_edits("missing").await.unwrap(),
        CandidateFileEdits::default()
    );

    let restored = db.load_folder_scan_snapshots().await.unwrap();
    let crate::import::folder_scanner::ScanItem::Valid(restored_candidate) = &restored[0].items[0]
    else {
        panic!("the persisted candidate keeps its valid variant");
    };
    assert_eq!(restored_candidate.file_edit_revision, 1);
    assert_eq!(restored_candidate.track_count(), 1);
    assert!(restored_candidate.files.bound_sheets().is_empty());

    // A subsequent scan reads the same decisions and derives the same
    // shape as the candidate restored before that scan.
    let stored = db.load_stored_candidate_edits().await.unwrap();
    let reopened = collect_release_candidate_files_with_scope(
        folder.path(),
        crate::import::ReleaseFileScope::Recursive,
        &stored,
    )
    .unwrap();

    assert_eq!(
        reopened.track_count(),
        1,
        "the cleared binding read back from disk is the one the scan applies"
    );
    assert!(reopened.bound_sheets().is_empty());
}

/// The pair that makes re-identification correct rather than incidental:
/// changing a binding leaves the row's key alone, **and** clears the
/// verdict stored under it.
///
/// The hash covers files and never role decisions, so the edit addresses
/// the same row rather than orphaning it — and that row's verdict was
/// derived from the shape the folder no longer has, so the queue must
/// answer the candidate again instead of trusting it.
#[tokio::test]
async fn changing_a_binding_keeps_the_hash_and_clears_the_verdict() {
    use crate::import::folder_scanner::{
        collect_release_candidate_files_with_scope, CandidateFileEdits, SheetBindingEdits,
        StoredCandidateEdits, UserSheetBinding,
    };

    let (db, _tmp) = empty_db().await;
    let folder = walkthrough_folder();
    let proposed = collect_release_candidate_files_with_scope(
        folder.path(),
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .unwrap();
    assert_eq!(proposed.track_count(), 12);
    let hash = proposed.content_hash();
    store_candidate_state(&db, &proposed, &folder.path().to_string_lossy()).await;

    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&new_candidate_row(
            &hash,
            &folder.path().to_string_lossy(),
            &sample_verdict(),
        ))
        .await
        .unwrap();
    assert!(
        db.load_import_candidate_states()
            .await
            .unwrap()
            .get(&hash)
            .expect("the verdict is stored")
            .identify
            .is_some(),
        "the candidate starts out identified"
    );

    let mut edits = SheetBindingEdits::default();
    edits.set_reference(
        "cd.cue".to_string(),
        "cd.wav".to_string(),
        UserSheetBinding::Cleared,
    );
    let mut settled = proposed.clone();
    settled
        .apply_candidate_file_edits(&CandidateFileEdits {
            sheet_bindings: edits.clone(),
            ..Default::default()
        })
        .unwrap();
    let folder_path = folder.path().to_string_lossy().into_owned();
    let (metadata_revision, mut mapping_preparation) =
        current_mapping_preparation(&db, &hash).await;
    mapping_preparation.draft.tracks = crate::import::pane::blank_candidate_source(&settled)
        .draft
        .tracks;
    crate::import::CandidatePreparations::new(db.clone())
        .store_file_decisions(
            &as_read(&hash, metadata_revision),
            &folder_path,
            &CandidateFileEdits {
                sheet_bindings: edits,
                ..Default::default()
            },
            &[(folder_path.clone(), settled)],
            &mapping_preparation,
        )
        .await
        .unwrap();

    let unbound = collect_release_candidate_files_with_scope(
        folder.path(),
        crate::import::ReleaseFileScope::Recursive,
        &db.load_stored_candidate_edits().await.unwrap(),
    )
    .unwrap();
    assert_eq!(
        unbound.track_count(),
        1,
        "the folder really did change shape -- otherwise this proves nothing"
    );
    assert_eq!(
        unbound.content_hash(),
        hash,
        "the hash covers files, never role decisions, so the row stays addressable"
    );

    let row = db
        .load_import_candidate_states()
        .await
        .unwrap()
        .remove(&hash)
        .expect("the row is still found under the unchanged hash");
    assert!(
        row.identify.is_none(),
        "the stored verdict described the folder before the binding; it must be cleared \
         so the queue identifies the candidate again"
    );
    assert_eq!(
        row.file_edits
            .sheet_bindings
            .get("cd.cue")
            .and_then(|references| references.get("cd.wav")),
        Some(&UserSheetBinding::Cleared),
        "the decision that cleared the verdict is what the row now holds"
    );
}

#[tokio::test]
async fn folder_release_decision_is_idempotent_and_root_scoped() {
    use crate::import::folder_scanner::{FolderReleaseDecision, FolderReleaseDecisionKey};

    let (db, _tmp) = empty_db().await;
    let other = host_root("/other/library");
    let key = FolderReleaseDecisionKey {
        watched_folder_path: host_root("/mounted/library"),
        relative_folder_path: "Collection/Release Wrapper".to_string(),
    };
    db.add_watched_import_folder(&key.watched_folder_path)
        .await
        .unwrap();
    db.add_watched_import_folder(&other).await.unwrap();

    store_user_folder_decision(
    &db,
        &key,
        FolderReleaseDecision::CombineAsOneRelease,
    )
    .await
    .unwrap();
    store_user_folder_decision(
    &db,
        &key,
        FolderReleaseDecision::CombineAsOneRelease,
    )
    .await
    .unwrap();
    store_user_folder_decision(
    &db,
        &FolderReleaseDecisionKey {
            watched_folder_path: other,
            relative_folder_path: key.relative_folder_path.clone(),
        },
        FolderReleaseDecision::KeepAsSeparateReleases,
    )
    .await
    .unwrap();

    let decisions = db
        .load_folder_release_decisions(&key.watched_folder_path)
        .await
        .unwrap();
    assert_eq!(
        decisions.get(&key.relative_folder_path)
            .map(|reading| (reading.decision, reading.author)),
        Some((FolderReleaseDecision::CombineAsOneRelease, crate::import::folder_scanner::FolderReleaseDecisionAuthor::User))
    );
}
