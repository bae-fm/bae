// Who wrote the candidate's draft, as the row reads it back: the author stored
// on the draft row, and `Nobody` for the blank draft discovery created.

/// A run that settled on one pressing writes the pick, so the row says
/// identification wrote it.
#[tokio::test]
async fn a_verdict_that_picks_names_identification_as_the_author() {
    let (db, _tmp) = empty_db().await;
    fetched(&db, "rel-1").await;
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
    fetched(&db, "rel-1").await;
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

/// A draft applied from releases keeps which releases and which lengths it
/// was read against: the releases are the ones its provenance names, read
/// back from their stored rows, and the lengths are the ones it was laid out
/// against.
#[tokio::test]
async fn an_applied_draft_reads_back_its_releases_and_lengths() {
    let (db, _tmp) = empty_db().await;
    fetched(&db, "rel-applied").await;
    let primary = db
        .load_source_release(&crate::import::MetadataRef::new(
            crate::import::Catalog::MusicBrainz,
            "rel-applied",
        ))
        .await
        .unwrap()
        .expect("the fetched release is stored");
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let mut row = concluding(
        new_candidate_row(&hash, &host_root("/music/Some Album"), &sample_verdict()),
        "rel-applied",
    );
    let applied = crate::import::source_release::AppliedSource {
        primary,
        partners: Vec::new(),
        audio_durations_ms: vec![180_000, 240_000],
    };
    row.metadata
        .as_mut()
        .expect("the concluding verdict carries a draft")
        .assets
        .applied_source = Some(applied.clone());
    store_candidate_state(&db, &candidate, &row.folder_path).await;
    crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .unwrap();

    let preparation = db
        .load_candidate_preparation(&hash)
        .await
        .unwrap()
        .expect("the candidate reads back");
    assert_eq!(preparation.metadata.assets.applied_source, Some(applied));
}

/// An application whose releases are not the ones the provenance names is a
/// draft the rows could not describe, and is refused whole.
#[tokio::test]
async fn an_application_the_provenance_does_not_name_is_refused() {
    let (db, _tmp) = empty_db().await;
    fetched(&db, "rel-named").await;
    fetched(&db, "rel-other").await;
    let other = db
        .load_source_release(&crate::import::MetadataRef::new(
            crate::import::Catalog::MusicBrainz,
            "rel-other",
        ))
        .await
        .unwrap()
        .expect("the fetched release is stored");
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let mut row = concluding(
        new_candidate_row(&hash, &host_root("/music/Some Album"), &sample_verdict()),
        "rel-named",
    );
    row.metadata
        .as_mut()
        .expect("the concluding verdict carries a draft")
        .assets
        .applied_source = Some(crate::import::source_release::AppliedSource {
        primary: other,
        partners: Vec::new(),
        audio_durations_ms: vec![180_000, 240_000],
    });
    store_candidate_state(&db, &candidate, &row.folder_path).await;

    let error = crate::import::CandidatePreparations::new(db.clone())
        .store_verdict(&row)
        .await
        .expect_err("the provenance names another release");
    assert!(
        error.to_string().contains("disagree"),
        "unexpected error: {error}"
    );
}
