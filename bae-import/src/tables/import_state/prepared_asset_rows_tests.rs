//! The bytes prepared for a remote cover, stored with it.

use crate::tables::import_state::tests::*;
use crate::CoverSelection;
use bae_catalogs::catalog::Catalog;
use bae_db::test_fixtures::empty_db;
use bae_db::watched_path::host_root;

#[tokio::test]
async fn a_remote_cover_round_trips_the_exact_prepared_bytes() {
    let (db, _tmp) = empty_db().await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;
    let cover = CoverSelection::Remote(
        bae_catalogs::remote_image::RemoteImageSet::original(
            "https://example.invalid/image".to_string(),
        ),
        Catalog::Discogs,
    );
    let image = bae_catalogs::remote_image::RemoteImage {
        bytes: vec![1, 2, 3, 4],
        content_type: bae_util::content_type::ContentType::Jpeg,
    };

    crate::CandidatePreparations::new(db.clone())
        .set_prepared_cover(
            &host_root("/music"),
            &pane_candidate_key(),
            &as_read(&hash, 0),
            &cover,
            Some(&image),
        )
        .await
        .unwrap();

    assert_eq!(
        tables.load_import_candidate_prepared_assets(&hash).await.unwrap().remote_cover,
        Some(image)
    );
}
