// The lookup choices stored on a candidate: which signals its runs leave out,
// and which of the extracted catalog numbers they ask about.

/// The whole value goes down and comes back, catalog order included: the
/// chosen numbers are asked in the order the person chose them, so their
/// order is part of the value rather than a set the store may reshuffle.
#[tokio::test]
async fn lookup_choices_round_trip_with_their_catalog_order() {
    let (db, _tmp) = empty_db().await;
    let files = track_files_candidate(&[("01 Track.flac", 111), ("02 Track.flac", 222)]);
    let hash = store_candidate_state(&db, &files, &host_root("/music/Album")).await;

    let choices = crate::import::LookupChoices {
        disc_id_excluded: true,
        barcode_excluded: false,
        chosen_catalogs: vec!["LBL 002".to_string(), "LBL 001".to_string()],
    };
    db.save_import_candidate_lookup_choices(&hash, &choices)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded
            .get(&hash)
            .expect("the candidate reads back")
            .lookup_choices,
        choices
    );
}

/// A second write is the whole value again: what it no longer names is gone,
/// not merged with what stood before it.
#[tokio::test]
async fn writing_lookup_choices_replaces_what_stood_before() {
    let (db, _tmp) = empty_db().await;
    let files = track_files_candidate(&[("01 Track.flac", 111), ("02 Track.flac", 222)]);
    let hash = store_candidate_state(&db, &files, &host_root("/music/Album")).await;

    db.save_import_candidate_lookup_choices(
        &hash,
        &crate::import::LookupChoices {
            disc_id_excluded: true,
            barcode_excluded: true,
            chosen_catalogs: vec!["LBL 001".to_string(), "LBL 002".to_string()],
        },
    )
    .await
    .unwrap();
    let replacement = crate::import::LookupChoices {
        disc_id_excluded: false,
        barcode_excluded: true,
        chosen_catalogs: vec!["LBL 003".to_string()],
    };
    db.save_import_candidate_lookup_choices(&hash, &replacement)
        .await
        .unwrap();

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded
            .get(&hash)
            .expect("the candidate reads back")
            .lookup_choices,
        replacement
    );
}

/// A candidate nobody has chosen anything for reads as the default: nothing
/// excluded, nothing chosen.
#[tokio::test]
async fn a_candidate_with_no_stored_choices_reads_the_default() {
    let (db, _tmp) = empty_db().await;
    let files = track_files_candidate(&[("01 Track.flac", 111), ("02 Track.flac", 222)]);
    let hash = store_candidate_state(&db, &files, &host_root("/music/Album")).await;

    let loaded = db.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded
            .get(&hash)
            .expect("the candidate reads back")
            .lookup_choices,
        crate::import::LookupChoices::default()
    );
}

/// Choices for a hash no candidate row stands under have nothing to belong
/// to. The write says so rather than landing nowhere.
#[tokio::test]
async fn choices_for_an_unknown_candidate_are_refused() {
    let (db, _tmp) = empty_db().await;
    let error = db
        .save_import_candidate_lookup_choices(
            "0000000000000000000000000000000000000000000000000000000000000000",
            &crate::import::LookupChoices {
                disc_id_excluded: true,
                barcode_excluded: false,
                chosen_catalogs: Vec::new(),
            },
        )
        .await
        .expect_err("a hash with no candidate row cannot hold lookup choices");
    assert!(
        error.to_string().contains("candidate state row"),
        "the error names what is missing: {error}"
    );
}
