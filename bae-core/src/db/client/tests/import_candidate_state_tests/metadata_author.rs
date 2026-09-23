// Who wrote the candidate's draft, as the row reads it back: the author stored
// on the draft row, and `Nobody` for the blank draft discovery created.

/// A run that settled on one pressing writes the pick, so the row says
/// identification wrote it.
#[tokio::test]
async fn a_verdict_that_picks_names_identification_as_the_author() {
    let (db, _tmp) = empty_db().await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let row = concluding(
        new_candidate_row(
            &hash,
            &host_root("/music/Some Album"),
            &sample_verdict(),
        ),
        "rel-1",
    );
    store_candidate_state(&db, &candidate, &row.folder_path).await;

    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded
            .get(&hash)
            .expect("the candidate reads back")
            .metadata_author,
        crate::import::MetadataAuthor::Identification
    );
}

/// A verdict that settled on nothing to pick leaves the draft to whoever wrote
/// it — here nobody.
#[tokio::test]
async fn a_verdict_that_picks_nothing_leaves_the_draft_unclaimed() {
    let (db, _tmp) = empty_db().await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let row = new_candidate_row(
        &hash,
        &host_root("/music/Some Album"),
        &sample_verdict(),
    );
    assert!(row.metadata.is_none(), "it settled on no release to write");
    store_candidate_state(&db, &candidate, &row.folder_path).await;

    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded
            .get(&hash)
            .expect("the candidate reads back")
            .metadata_author,
        crate::import::MetadataAuthor::Nobody
    );
}

/// A candidate discovery stored with a blank draft: nobody wrote it.
#[tokio::test]
async fn a_candidate_with_no_pick_has_no_author() {
    let (db, _tmp) = empty_db().await;
    let files = track_files_candidate(&[("01 Track.flac", 111), ("02 Track.flac", 222)]);
    let hash = store_candidate_state(&db, &files, &host_root("/music/Album")).await;

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded
            .get(&hash)
            .expect("the candidate reads back")
            .metadata_author,
        crate::import::MetadataAuthor::Nobody
    );
}

/// A person editing the draft identification wrote makes it theirs: after the
/// edit it is their answer, not the run's pick waiting on the Ready rule.
#[tokio::test]
async fn an_edit_to_identification_s_draft_makes_the_person_its_author() {
    let (db, _tmp) = empty_db().await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let row = concluding(
        new_candidate_row(
            &hash,
            &host_root("/music/Some Album"),
            &sample_verdict(),
        ),
        "rel-1",
    );
    store_candidate_state(&db, &candidate, &row.folder_path).await;
    let preparations = crate::import::CandidatePreparations::new(db.clone());
    preparations.store_verdict(&row).await.unwrap();

    preparations
        .set_field(
            &hash,
            crate::import::CandidateEditField::AlbumTitle,
            "Album",
        )
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    let stored = loaded.get(&hash).expect("the candidate reads back");
    assert_eq!(stored.metadata_author, crate::import::MetadataAuthor::Person);
    assert!(
        matches!(
            stored.metadata_provenance,
            Some(crate::import::MetadataProvenance::ExternalRelease { .. })
        ),
        "the edit leaves where the draft was read from"
    );
}
