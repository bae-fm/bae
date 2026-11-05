//! The lookup choices stored on a candidate: which signals its runs leave out,
//! and which of its barcodes and extracted catalog numbers they ask about.

use crate::tables::import_state::tests::*;
use crate::tables::tests::*;
use crate::tables::watched_path::host_root;
use bae_store::test_fixtures::empty_db;

/// The whole value goes down and comes back, catalog order included: the
/// chosen numbers are asked in the order the person chose them, so their
/// order is part of the value rather than a set the store may reshuffle.
#[tokio::test]
async fn lookup_choices_round_trip_with_their_catalog_order() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let files = track_files_candidate(&[("01 Track.flac", 111), ("02 Track.flac", 222)]);
    let hash = store_candidate_state(&db, &files, &host_root("/music/Album")).await;

    let choices = bae_identify::LookupChoices {
        disc_id_excluded: true,
        excluded_barcodes: Vec::new(),
        chosen_catalogs: vec!["LBL 002".to_string(), "LBL 001".to_string()],
        search_words: None,
        discounted_catalogs: Vec::new(),
    };
    tables.save_import_candidate_lookup_choices(&hash, &choices).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    assert_eq!(loaded.get(&hash).expect("the candidate reads back").lookup_choices, choices);
}

/// A second write is the whole value again: what it no longer names is gone,
/// not merged with what stood before it.
#[tokio::test]
async fn writing_lookup_choices_replaces_what_stood_before() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let files = track_files_candidate(&[("01 Track.flac", 111), ("02 Track.flac", 222)]);
    let hash = store_candidate_state(&db, &files, &host_root("/music/Album")).await;

    tables
        .save_import_candidate_lookup_choices(
            &hash,
            &bae_identify::LookupChoices {
                disc_id_excluded: true,
                excluded_barcodes: vec!["0123456789012".to_string()],
                chosen_catalogs: vec!["LBL 001".to_string(), "LBL 002".to_string()],
                search_words: None,
                discounted_catalogs: Vec::new(),
            },
        )
        .await
        .unwrap();
    let replacement = bae_identify::LookupChoices {
        disc_id_excluded: false,
        excluded_barcodes: vec!["0123456789012".to_string()],
        chosen_catalogs: vec!["LBL 003".to_string()],
        search_words: None,
        discounted_catalogs: Vec::new(),
    };
    tables.save_import_candidate_lookup_choices(&hash, &replacement).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    assert_eq!(loaded.get(&hash).expect("the candidate reads back").lookup_choices, replacement);
}

/// A candidate nobody has chosen anything for reads as the default: nothing
/// excluded, nothing chosen.
#[tokio::test]
async fn a_candidate_with_no_stored_choices_reads_the_default() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let files = track_files_candidate(&[("01 Track.flac", 111), ("02 Track.flac", 222)]);
    let hash = store_candidate_state(&db, &files, &host_root("/music/Album")).await;

    let loaded = tables.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded.get(&hash).expect("the candidate reads back").lookup_choices,
        bae_identify::LookupChoices::default()
    );
}

/// Choices for a hash no candidate row stands under have nothing to belong
/// to. The write says so rather than landing nowhere.
#[tokio::test]
async fn choices_for_an_unknown_candidate_are_refused() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let error = tables
        .save_import_candidate_lookup_choices(
            "0000000000000000000000000000000000000000000000000000000000000000",
            &bae_identify::LookupChoices {
                disc_id_excluded: true,
                excluded_barcodes: Vec::new(),
                chosen_catalogs: Vec::new(),
                search_words: None,
                discounted_catalogs: Vec::new(),
            },
        )
        .await
        .expect_err("a hash with no candidate row cannot hold lookup choices");
    assert!(
        error.to_string().contains("candidate state row"),
        "the error names what is missing: {error}"
    );
}

/// The numbers struck out of the candidate's text go down and come back
/// beside the chosen ones. They are a set — nothing dispatches on their order
/// — so they read back by value, and the two lists never touch: a number can
/// be looked up and struck out at once.
#[tokio::test]
async fn struck_out_catalog_numbers_round_trip_beside_the_chosen_ones() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let files = track_files_candidate(&[("01 Track.flac", 111), ("02 Track.flac", 222)]);
    let hash = store_candidate_state(&db, &files, &host_root("/music/Album")).await;

    let choices = bae_identify::LookupChoices {
        disc_id_excluded: false,
        excluded_barcodes: Vec::new(),
        chosen_catalogs: vec!["LBL 002".to_string()],
        search_words: None,
        discounted_catalogs: vec!["LBL 002".to_string(), "LBL 100".to_string()],
    };
    tables.save_import_candidate_lookup_choices(&hash, &choices).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    assert_eq!(loaded.get(&hash).expect("the candidate reads back").lookup_choices, choices);
}

/// The struck-out numbers are replaced whole like the rest of the value: what
/// the new value does not name is not struck out any more.
#[tokio::test]
async fn writing_the_choices_replaces_what_was_struck_out() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let files = track_files_candidate(&[("01 Track.flac", 111), ("02 Track.flac", 222)]);
    let hash = store_candidate_state(&db, &files, &host_root("/music/Album")).await;

    tables
        .save_import_candidate_lookup_choices(
            &hash,
            &bae_identify::LookupChoices {
                discounted_catalogs: vec!["LBL 001".to_string(), "LBL 002".to_string()],
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let replacement = bae_identify::LookupChoices {
        discounted_catalogs: vec!["LBL 002".to_string()],
        ..Default::default()
    };
    tables.save_import_candidate_lookup_choices(&hash, &replacement).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    assert_eq!(loaded.get(&hash).expect("the candidate reads back").lookup_choices, replacement);
}

/// The barcodes a run leaves out go down and come back as the set they are:
/// a double sleeve's two codes, one asked about and one not, are two separate
/// decisions rather than one flag over the pair.
#[tokio::test]
async fn left_out_barcodes_round_trip_as_a_set() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let files = track_files_candidate(&[("01 Track.flac", 111), ("02 Track.flac", 222)]);
    let hash = store_candidate_state(&db, &files, &host_root("/music/Album")).await;

    let choices = bae_identify::LookupChoices {
        disc_id_excluded: false,
        excluded_barcodes: vec!["0123456789012".to_string(), "9999999999999".to_string()],
        chosen_catalogs: Vec::new(),
        search_words: None,
        discounted_catalogs: Vec::new(),
    };
    tables.save_import_candidate_lookup_choices(&hash, &choices).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    assert_eq!(loaded.get(&hash).expect("the candidate reads back").lookup_choices, choices);
}

/// The left-out barcodes are replaced whole like the rest of the value: a code
/// the new value does not name is asked about again.
#[tokio::test]
async fn writing_the_choices_replaces_the_left_out_barcodes() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let files = track_files_candidate(&[("01 Track.flac", 111), ("02 Track.flac", 222)]);
    let hash = store_candidate_state(&db, &files, &host_root("/music/Album")).await;

    tables
        .save_import_candidate_lookup_choices(
            &hash,
            &bae_identify::LookupChoices {
                excluded_barcodes: vec!["0123456789012".to_string(), "9999999999999".to_string()],
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let replacement = bae_identify::LookupChoices {
        excluded_barcodes: vec!["9999999999999".to_string()],
        ..Default::default()
    };
    tables.save_import_candidate_lookup_choices(&hash, &replacement).await.unwrap();

    let loaded = tables.load_import_candidate_states().await.unwrap();
    assert_eq!(loaded.get(&hash).expect("the candidate reads back").lookup_choices, replacement);
}

/// The words typed for the title search go down and come back with the
/// rest, and a value naming none reads back naming none.
#[tokio::test]
async fn typed_search_words_round_trip() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let files = track_files_candidate(&[("01 Track.flac", 111), ("02 Track.flac", 222)]);
    let hash = store_candidate_state(&db, &files, &host_root("/music/Album")).await;

    let typed = bae_identify::LookupChoices {
        search_words: Some(bae_identify::SearchWords {
            album: "Album Title".to_string(),
            artist: String::new(),
        }),
        ..Default::default()
    };
    tables.save_import_candidate_lookup_choices(&hash, &typed).await.unwrap();
    let loaded = tables.load_import_candidate_states().await.unwrap();
    assert_eq!(loaded.get(&hash).expect("the candidate reads back").lookup_choices, typed);

    tables
        .save_import_candidate_lookup_choices(&hash, &bae_identify::LookupChoices::default())
        .await
        .unwrap();
    let loaded = tables.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded.get(&hash).expect("the candidate reads back").lookup_choices.search_words,
        None
    );
}
