//! One candidate's whole state saved as a unit.

use crate::tables::import_state::tests::*;
use crate::CoverSelection;
use bae_catalogs::catalog::Catalog;
use bae_db::test_fixtures::empty_db;
use bae_db::watched_path::host_root;

#[tokio::test]
async fn a_remote_cover_without_exact_bytes_writes_nothing() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;
    let cover = CoverSelection::Remote(
        bae_catalogs::remote_image::RemoteImageSet::original(
            "https://example.invalid/image".to_string(),
        ),
        Catalog::Discogs,
    );

    crate::CandidatePreparations::new(db.clone())
        .set_prepared_cover(
            &host_root("/music"),
            &pane_candidate_key(),
            &as_read(&hash, 0),
            &cover,
            None,
        )
        .await
        .expect_err("a remote selection requires its exact bytes");

    let state =
        tables.load_import_candidate_state(&hash).await.unwrap().expect("the candidate remains");
    assert_eq!(state.metadata_revision, 0);
    assert_eq!(tables.load_import_candidate_pane_rows(&hash).await.unwrap().cover, None);
}
