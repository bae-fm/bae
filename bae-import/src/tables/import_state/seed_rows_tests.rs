//! The state a scan creates for the files it finds.

use crate::tables::import_state::tests::*;
use crate::tables::tests::*;
use bae_db::test_fixtures::empty_db;
use bae_db::watched_path::host_root;

/// A candidate discovery stored with a blank draft: nobody wrote it.
#[tokio::test]
async fn a_candidate_with_no_pick_has_no_author() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let files = track_files_candidate(&[("01 Track.flac", 111), ("02 Track.flac", 222)]);
    let hash = store_candidate_state(&db, &files, &host_root("/music/Album")).await;

    let loaded = tables.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded.get(&hash).expect("the candidate reads back").metadata_author,
        crate::MetadataAuthor::Nobody
    );
}
