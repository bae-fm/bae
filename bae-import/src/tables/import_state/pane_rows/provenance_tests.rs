//! The draft's provenance, stored and read back.

use crate::tables::import_state::tests::*;
use crate::tables::tests::*;
use crate::tables::watched_path::host_root;
use crate::tables::ImportTables;
use bae_store::test_fixtures::fixed_now;
use coven::FixedClock;
use std::sync::Arc;

#[tokio::test]
async fn every_metadata_provenance_variant_survives_a_database_reopen() {
    let tmp = tempfile::TempDir::new().unwrap();
    let installation =
        coven::Coven::in_memory(bae_app_dir::AppDir::at(tmp.path()).unwrap().coven_app_dir());
    let clock: coven::ClockRef = Arc::new(FixedClock::new(fixed_now().into()));
    let handle = bae_store::configure_store(
        bae_store::TestSources::on(&clock).builder(&installation),
        crate::TEST_SCHEMA,
    )
    .create_store("Test Library", "Test Device", &[], |_, _| Ok(()))
    .await
    .unwrap();
    let id = handle.store_id();
    let db = bae_store::test_devices::store_under(handle, clock.clone());
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
        crate::tables::ImportTables::new(db.clone()).store_verdict(&row).await.unwrap();
        crate::tables::ImportTables::new(db.clone())
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

    let handle = bae_store::configure_store(
        bae_store::TestSources::on(&clock).builder(&installation),
        crate::TEST_SCHEMA,
    )
    .open(id, |_| ())
    .await
    .unwrap();
    let reopened = bae_store::test_devices::store_under(handle, clock);
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
