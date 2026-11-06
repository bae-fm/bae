//! Reading a candidate back under its content hash.

use crate::tables::import_state::tests::*;
use crate::tables::tests::*;
use crate::tables::watched_path::host_root;
use bae_store::test_fixtures::empty_db;

/// Resizing a file changes `content_hash`, so the new hash finds no row and the
/// old row stays under its own key.
#[tokio::test]
async fn resizing_a_file_orphans_the_old_row_under_a_new_hash() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let original = track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let original_hash = original.content_hash();
    let row = new_candidate_row(&original_hash, &host_root("/music/Some Album"), &sample_verdict());
    store_candidate_state(&db, &original, &row.candidate_key.test_folder()).await;
    crate::tables::ImportTables::new(db.clone()).store_verdict(&row).await.unwrap();

    let resized = track_files_candidate(&[("01 Track.flac", 999_999), ("02 Track.flac", 234_567)]);
    let resized_hash = resized.content_hash();
    assert_ne!(original_hash, resized_hash, "resizing a file must change the hash");

    let loaded = tables.load_import_candidate_states().await.unwrap();
    assert!(loaded.contains_key(&original_hash), "the old row is still present under its own key");
    assert!(
        !loaded.contains_key(&resized_hash),
        "the new hash must find no row -- the candidate needs re-identifying"
    );
}

/// A moved folder hashes the same, so its saved row is still found.
#[tokio::test]
async fn a_moved_folder_hashes_identically_and_keeps_its_row() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let at_old_location =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = at_old_location.content_hash();
    let row =
        new_candidate_row(&hash, &host_root("/music/Old Location/Some Album"), &sample_verdict());
    store_candidate_state(&db, &at_old_location, &row.candidate_key.test_folder()).await;
    crate::tables::ImportTables::new(db.clone()).store_verdict(&row).await.unwrap();

    let mut at_new_location = at_old_location.clone();
    for entry in &mut at_new_location.files {
        entry.file.version.path =
            host_root("/music/New Location/Some Album").join(&entry.file.relative_path);
    }
    assert_eq!(
        hash,
        at_new_location.content_hash(),
        "a moved folder must hash identically to itself before the move"
    );

    let loaded = tables.load_import_candidate_states().await.unwrap();
    assert!(
        loaded.contains_key(&at_new_location.content_hash()),
        "the row saved before the move must still be reachable after it"
    );
}
