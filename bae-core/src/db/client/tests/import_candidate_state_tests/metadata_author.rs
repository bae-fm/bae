// Who wrote the candidate's draft, as the row reads it back: the author the
// provenance was written with, and `Nobody` for a draft nothing picked for.

/// A run that settled on one pressing writes the pick, so the row says
/// identification wrote it.
#[tokio::test]
async fn a_verdict_that_picks_names_identification_as_the_author() {
    let (db, _tmp) = empty_db().await;
    let candidate =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = candidate.content_hash();
    let mut row = new_candidate_row(
        &hash,
        &host_root("/music/Some Album"),
        &sample_verdict(),
        2_700_000,
    );
    row.metadata.provenance = Some(crate::import::MetadataProvenance::ExternalRelease {
        source: MetadataSource::MusicBrainz,
        release_id: "rel-1".to_string(),
        partners: Vec::new(),
    });
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

/// A verdict that settled on nothing to pick leaves the draft unclaimed.
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
        2_700_000,
    );
    assert!(row.metadata.provenance.is_none());
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

/// A candidate nothing has written a draft for at all: no provenance row, so
/// no author.
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
