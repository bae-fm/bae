//! Reading a candidate back under its content hash.

use crate::tables::import_state::tests::*;
use crate::tables::tests::*;
use bae_db::test_fixtures::fixed_now;
use bae_db::watched_path::host_root;
use coven::FixedClock;
use std::sync::Arc;

use crate::tables::ImportTables;
use bae_db::test_fixtures::empty_db;
use bae_db::Database;

/// Resizing a file changes `content_hash`, so the new hash finds no row and the
/// old row stays under its own key.
#[tokio::test]
async fn resizing_a_file_orphans_the_old_row_under_a_new_hash() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let original = track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let original_hash = original.content_hash();
    let row = new_candidate_row(&original_hash, &host_root("/music/Some Album"), &sample_verdict());
    store_candidate_state(&db, &original, &row.candidate_key.test_folder()).await;
    crate::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();

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
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let at_old_location =
        track_files_candidate(&[("01 Track.flac", 123_456), ("02 Track.flac", 234_567)]);
    let hash = at_old_location.content_hash();
    let row =
        new_candidate_row(&hash, &host_root("/music/Old Location/Some Album"), &sample_verdict());
    store_candidate_state(&db, &at_old_location, &row.candidate_key.test_folder()).await;
    crate::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();

    let mut at_new_location = at_old_location.clone();
    for entry in &mut at_new_location.files {
        entry.file.path =
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

#[tokio::test]
async fn every_metadata_provenance_variant_survives_a_database_reopen() {
    let tmp = tempfile::TempDir::new().unwrap();
    let installation = coven::TestCoven::new();
    let layout = bae_app_dir::AppDir::at(tmp.path()).unwrap().store_layout();
    let clock: coven::ClockRef = Arc::new(FixedClock::new(fixed_now().into()));
    let handle = bae_db::configure_store(installation.builder(layout.clone()))
        .clock(clock.clone())
        .create_store("Test Library")
        .await
        .unwrap();
    let id = handle.store_id();
    let db = Database::from_handle(handle, clock.clone(), std::sync::Arc::new(coven::UuidIds));
    fetched(&db, "release-a").await;
    let cases = [
        (
            track_files_candidate(&[("01 Track.flac", 100_001)]).content_hash(),
            &host_root("/music/Candidate A"),
            100_001,
            release_pick("release-a"),
        ),
        (
            track_files_candidate(&[("01 Track.flac", 100_002)]).content_hash(),
            &host_root("/music/Candidate B"),
            100_002,
            crate::MetadataProvenance::FileMetadata,
        ),
    ];

    for (content_hash, folder_path, size, provenance) in &cases {
        let candidate = track_files_candidate(&[("01 Track.flac", *size)]);
        assert_eq!(&candidate.content_hash(), content_hash);
        store_candidate_state(&db, &candidate, folder_path).await;
        let row = new_candidate_row(content_hash, folder_path, &sample_verdict());
        let key = crate::CandidateKey::folder(folder_path);
        crate::CandidatePreparations::new(db.clone()).store_verdict(&row).await.unwrap();
        crate::CandidatePreparations::new(db.clone())
            .replace_metadata(
                content_hash,
                &key,
                &metadata_draft("Album", "Artist"),
                Some(provenance),
                &[],
            )
            .await
            .unwrap();
    }
    db.close().await.unwrap();
    drop(db);

    let handle = bae_db::configure_store(installation.builder(layout))
        .clock(clock.clone())
        .open(id, |_| ())
        .await
        .unwrap();
    let reopened = Database::from_handle(handle, clock, std::sync::Arc::new(coven::UuidIds));
    let loaded = ImportTables::new(reopened).load_import_candidate_states().await.unwrap();

    for (content_hash, _, _, provenance) in &cases {
        assert_eq!(
            loaded
                .get(content_hash)
                .expect("the candidate state survives the database reopen")
                .metadata_provenance
                .as_ref(),
            Some(provenance)
        );
    }
}
